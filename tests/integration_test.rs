use anyhow::Result;
use serde_json::json;
use std::fs;
use std::process::Command;
use tempfile::TempDir;

struct TestRepo {
    dir: TempDir,
    session_id: String,
}

impl TestRepo {
    fn new() -> Result<Self> {
        let dir = TempDir::new()?;

        // Initialize jj repo
        let init_output = Command::new("jj")
            .current_dir(dir.path())
            .args(["git", "init"])
            .output()?;

        if !init_output.status.success() {
            anyhow::bail!(
                "Failed to init jj repo: {}",
                String::from_utf8_lossy(&init_output.stderr)
            );
        }

        // Create initial commit
        fs::write(dir.path().join("initial.txt"), "initial content")?;

        let desc_output = Command::new("jj")
            .current_dir(dir.path())
            .args(["describe", "-m", "Initial commit"])
            .output()?;

        if !desc_output.status.success() {
            anyhow::bail!(
                "Failed to describe commit: {}",
                String::from_utf8_lossy(&desc_output.stderr)
            );
        }

        // Verify jj root works
        let root_output = Command::new("jj")
            .current_dir(dir.path())
            .args(["root"])
            .output()?;

        if !root_output.status.success() {
            anyhow::bail!("jj root failed - repo not properly initialized");
        }

        Ok(Self {
            dir,
            session_id: uuid::Uuid::new_v4().to_string(),
        })
    }

    fn run_hook(&self, hook: &str, tool_name: Option<&str>) -> Result<()> {
        self.run_hook_with_env(hook, tool_name, vec![])
    }

    fn run_hook_with_env(
        &self,
        hook: &str,
        tool_name: Option<&str>,
        env_vars: Vec<(&str, &str)>,
    ) -> Result<()> {
        let input = if let Some(tool) = tool_name {
            json!({
                "session_id": self.session_id,
                "tool_name": tool
            })
        } else {
            json!({
                "session_id": self.session_id,
                "prompt": "Test prompt"
            })
        };

        // Get the path to the jjagent binary built by cargo
        let jjagent_binary = env!("CARGO_BIN_EXE_jjagent");

        // Build jjagent command - need to execute it with jj repo as working directory
        let mut cmd = Command::new(jjagent_binary);
        cmd.current_dir(self.dir.path())
            .env_remove("JJAGENT_DISABLE")
            .args(["claude", "hooks", hook])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        for (key, value) in env_vars {
            cmd.env(key, value);
        }

        let mut child = cmd.spawn()?;

        // Write input
        if let Some(stdin) = child.stdin.as_mut() {
            use std::io::Write;
            stdin.write_all(input.to_string().as_bytes())?;
        }

        // Wait and check output
        let output = child.wait_with_output()?;

        // Always print stderr for debugging
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.is_empty() {
            eprintln!("jjagent stderr: {}", stderr);
        }

        // Print stderr for debugging if command fails
        if !output.status.success() {
            eprintln!("jjagent command failed with status: {:?}", output.status);
        }

        Ok(())
    }

    fn get_current_change_id(&self) -> Result<String> {
        let output = Command::new("jj")
            .current_dir(self.dir.path())
            .args(["log", "-r", "@", "--no-graph", "-T", "change_id"])
            .output()?;

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn get_change_description(&self, change_id: &str) -> Result<String> {
        let output = Command::new("jj")
            .current_dir(self.dir.path())
            .args(["log", "-r", change_id, "--no-graph", "-T", "description"])
            .output()?;

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    #[allow(dead_code)]
    fn get_log(&self) -> Result<String> {
        let output = Command::new("jj")
            .current_dir(self.dir.path())
            .args(["log", "--limit", "10"])
            .output()?;

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn create_file(&self, name: &str, content: &str) -> Result<()> {
        fs::write(self.dir.path().join(name), content)?;
        Ok(())
    }

    fn is_on_temp_workspace(&self) -> Result<bool> {
        let desc = self.get_change_description("@")?;
        // Check for the new trailer-based temporary change marker
        for line in desc.lines().rev() {
            if line.trim().is_empty() {
                break;
            }
            if line.starts_with("Jjagent-claude-temp-change:") {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn find_claude_change(&self) -> Result<Option<String>> {
        let output = Command::new("jj")
            .current_dir(self.dir.path())
            .args([
                "log",
                "-r",
                &format!(
                    "description(glob:'*Jjagent-claude-session-id: {}*')",
                    self.session_id
                ),
                "--no-graph",
                "-T",
                "change_id",
                "--limit",
                "1",
            ])
            .output()?;

        if output.status.success() && !output.stdout.is_empty() {
            Ok(Some(
                String::from_utf8_lossy(&output.stdout).trim().to_string(),
            ))
        } else {
            Ok(None)
        }
    }

    fn run_session_split(&self, session_id: &str) -> Result<std::process::ExitStatus> {
        let jjagent_binary = env!("CARGO_BIN_EXE_jjagent");

        let output = Command::new(jjagent_binary)
            .current_dir(self.dir.path())
            .env_remove("JJAGENT_DISABLE")
            .args(["claude", "session", "split", session_id])
            .output()?;

        // Print stderr for debugging
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.is_empty() {
            eprintln!("jjagent session split stderr: {}", stderr);
        }

        Ok(output.status)
    }

    fn run_session_split_with_description(
        &self,
        session_id: &str,
        description: &str,
    ) -> Result<std::process::ExitStatus> {
        let jjagent_binary = env!("CARGO_BIN_EXE_jjagent");

        let output = Command::new(jjagent_binary)
            .current_dir(self.dir.path())
            .env_remove("JJAGENT_DISABLE")
            .args(["claude", "session", "split", session_id, "-m", description])
            .output()?;

        // Print stderr for debugging
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.is_empty() {
            eprintln!("jjagent session split stderr: {}", stderr);
        }

        Ok(output.status)
    }

    fn create_commit_with_session_id(&self, session_id: &str) -> Result<String> {
        // Create a new commit
        Command::new("jj")
            .current_dir(self.dir.path())
            .args(["new"])
            .output()?;

        // Add description with session trailer
        let description = format!(
            "Claude Code Session {}\n\nJjagent-claude-session-id: {}",
            session_id, session_id
        );

        Command::new("jj")
            .current_dir(self.dir.path())
            .args(["describe", "-m", &description])
            .output()?;

        self.get_current_change_id()
    }

    fn count_commits_with_session_id(&self, session_id: &str) -> Result<usize> {
        let output = Command::new("jj")
            .current_dir(self.dir.path())
            .args([
                "log",
                "-r",
                &format!(
                    "description(glob:'*Jjagent-claude-session-id: {}*')",
                    session_id
                ),
                "--no-graph",
                "-T",
                "change_id ++ \"\\n\"",
            ])
            .output()?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            Ok(stdout.lines().filter(|line| !line.is_empty()).count())
        } else {
            Ok(0)
        }
    }
}

#[test]
fn test_first_tool_use() -> Result<()> {
    let repo = TestRepo::new()?;
    let initial_change = repo.get_current_change_id()?;

    // First tool use should create temporary workspace
    repo.run_hook("UserPromptSubmit", None)?;
    repo.run_hook("PreToolUse", Some("Write"))?;

    // We should be on a temporary workspace
    let current = repo.get_current_change_id()?;
    assert_ne!(current, initial_change, "Should have moved to a new change");

    let desc = repo.get_change_description(&current)?;
    let has_temp_change_trailer = desc
        .lines()
        .rev()
        .take_while(|line| !line.trim().is_empty())
        .any(|line| line.starts_with("Jjagent-claude-temp-change:"));
    assert!(has_temp_change_trailer, "Should be on temporary workspace");

    // Simulate edit
    repo.create_file("test1.txt", "First edit")?;

    repo.run_hook("PostToolUse", Some("Write"))?;

    // After PostToolUse, we should be back on original working copy
    let final_change = repo.get_current_change_id()?;
    assert_eq!(
        final_change, initial_change,
        "Should be back on original working copy"
    );

    Ok(())
}

#[test]
fn test_subsequent_tool_uses() -> Result<()> {
    let repo = TestRepo::new()?;
    let initial_change = repo.get_current_change_id()?;

    // First tool use
    repo.run_hook("UserPromptSubmit", None)?;
    repo.run_hook("PreToolUse", Some("Write"))?;
    repo.create_file("test1.txt", "First edit")?;
    repo.run_hook("PostToolUse", Some("Write"))?;

    // Verify we're back on original
    assert_eq!(repo.get_current_change_id()?, initial_change);

    // Second tool use in same session
    repo.run_hook("UserPromptSubmit", None)?;
    repo.run_hook("PreToolUse", Some("Write"))?;

    // Should be on temp workspace now
    assert!(
        repo.is_on_temp_workspace()?,
        "Should be on temporary workspace"
    );

    let current = repo.get_current_change_id()?;
    assert_ne!(
        current, initial_change,
        "Should not be on original working copy"
    );

    // The Claude change should exist
    let claude_change = repo
        .find_claude_change()?
        .expect("Claude change should exist");
    assert_ne!(
        current, claude_change,
        "Should not be on Claude change directly"
    );

    repo.create_file("test2.txt", "Second edit")?;
    repo.run_hook("PostToolUse", Some("Write"))?;

    // Should be back on original
    assert_eq!(
        repo.get_current_change_id()?,
        initial_change,
        "Should be back on original"
    );

    Ok(())
}

#[test]
fn test_multiple_messages_same_session() -> Result<()> {
    let repo = TestRepo::new()?;
    let initial_change = repo.get_current_change_id()?;

    // Simulate multiple back-and-forth messages in same session
    for i in 1..=3 {
        repo.run_hook("UserPromptSubmit", None)?;
        repo.run_hook("PreToolUse", Some("Write"))?;

        // After first, should always be on temp workspace
        if i > 1 {
            assert!(
                repo.is_on_temp_workspace()?,
                "Should be on temp workspace for subsequent edits"
            );
        }

        repo.create_file(&format!("test{}.txt", i), &format!("Edit {}", i))?;
        repo.run_hook("PostToolUse", Some("Write"))?;

        // Always end up back on original
        assert_eq!(
            repo.get_current_change_id()?,
            initial_change,
            "Should be back on original"
        );
    }

    // All edits should be in single Claude change
    let claude_change = repo
        .find_claude_change()?
        .expect("Claude change should exist");

    // Verify files exist in Claude change
    let output = Command::new("jj")
        .current_dir(repo.dir.path())
        .args(["diff", "-r", &claude_change])
        .output()?;

    let diff = String::from_utf8_lossy(&output.stdout);
    assert!(diff.contains("test1.txt"));
    assert!(diff.contains("test2.txt"));
    assert!(diff.contains("test3.txt"));

    Ok(())
}

#[test]
fn test_never_stay_on_claude_change() -> Result<()> {
    let repo = TestRepo::new()?;
    let initial_change = repo.get_current_change_id()?;

    // Multiple operations - we should never be left on the Claude change itself
    for i in 1..=5 {
        repo.run_hook("UserPromptSubmit", None)?;
        repo.run_hook("PreToolUse", Some("Write"))?;

        let current = repo.get_current_change_id()?;

        // Always on temp workspace now
        assert!(repo.is_on_temp_workspace()?, "Should be on temp workspace");

        // Never on the Claude change directly
        let claude_change = repo.find_claude_change()?;
        if i > 1 && claude_change.is_some() {
            let claude_id = claude_change.unwrap();
            assert_ne!(current, claude_id, "Should never be left on Claude change");
        }

        repo.create_file(&format!("file{}.txt", i), "content")?;
        repo.run_hook("PostToolUse", Some("Write"))?;

        // Always end on original working copy
        assert_eq!(
            repo.get_current_change_id()?,
            initial_change,
            "Should always return to original"
        );
    }

    Ok(())
}
#[test]
fn test_never_left_on_claude_change_after_operations() -> Result<()> {
    let repo = TestRepo::new()?;
    let initial_change = repo.get_current_change_id()?;

    // This test specifically verifies we never leave the user on the Claude change
    // It simulates the exact scenario: create, edit, then follow-up message

    // First message and edit
    repo.run_hook("UserPromptSubmit", None)?;
    repo.run_hook("PreToolUse", Some("Write"))?;
    repo.create_file("file1.txt", "content")?;
    repo.run_hook("PostToolUse", Some("Write"))?;

    // Verify we're on original working copy
    let after_first = repo.get_current_change_id()?;
    assert_eq!(
        after_first, initial_change,
        "Should be on original after first message"
    );

    // Get the Claude change ID
    let claude_change = repo
        .find_claude_change()?
        .expect("Claude change should exist");

    // Follow-up message without tool use (just prompt)
    repo.run_hook("UserPromptSubmit", None)?;

    // Still should be on original, not Claude change
    let after_prompt = repo.get_current_change_id()?;
    assert_eq!(
        after_prompt, initial_change,
        "Should still be on original after prompt"
    );
    assert_ne!(
        after_prompt, claude_change,
        "Should NOT be on Claude change"
    );

    // Now another tool use
    repo.run_hook("PreToolUse", Some("Write"))?;

    // Should be on temp workspace, not Claude change
    let during_edit = repo.get_current_change_id()?;
    assert_ne!(
        during_edit, claude_change,
        "Should NOT be on Claude change during edit"
    );
    assert!(repo.is_on_temp_workspace()?, "Should be on temp workspace");

    repo.create_file("file2.txt", "more content")?;
    repo.run_hook("PostToolUse", Some("Write"))?;

    // Final check - must be on original
    let final_pos = repo.get_current_change_id()?;
    assert_eq!(
        final_pos, initial_change,
        "Must end on original working copy"
    );
    assert_ne!(final_pos, claude_change, "Must NOT be on Claude change");

    Ok(())
}

#[test]
fn test_interrupted_operation_recovery() -> Result<()> {
    let repo = TestRepo::new()?;
    let _initial_change = repo.get_current_change_id()?;

    // Simulate an interrupted operation - PreToolUse without PostToolUse
    repo.run_hook("UserPromptSubmit", None)?;
    repo.run_hook("PreToolUse", Some("Write"))?;
    repo.create_file("file1.txt", "content")?;
    // Oops, PostToolUse never called (simulating crash/interrupt)

    // Now a new operation starts
    repo.run_hook("UserPromptSubmit", None)?;
    repo.run_hook("PreToolUse", Some("Write"))?;

    // Should handle this gracefully
    let current = repo.get_current_change_id()?;

    // Should either be on a temp workspace or have recovered somehow
    // but definitely not stuck on the Claude change
    let claude_change = repo.find_claude_change()?;
    if let Some(claude_id) = claude_change {
        assert_ne!(
            current, claude_id,
            "Should not be stuck on Claude change after interrupted op"
        );
    }

    Ok(())
}

#[test]
fn test_changes_not_abandoned_when_present() -> Result<()> {
    let repo = TestRepo::new()?;
    let initial_change = repo.get_current_change_id()?;

    // First create the scenario where Claude makes changes
    repo.run_hook("UserPromptSubmit", None)?;

    eprintln!("Initial change ID: {}", initial_change);

    repo.run_hook("PreToolUse", Some("Write"))?;

    // Check if we're on a workspace now
    let after_pre = repo.get_current_change_id()?;
    eprintln!("After PreToolUse change ID: {}", after_pre);
    let desc = repo.get_change_description(&after_pre)?;
    eprintln!("After PreToolUse description: {}", desc);

    // Claude creates a file (simulating actual work)
    repo.create_file("claude_file.txt", "Claude made this change")?;

    // Get workspace ID before PostToolUse
    let workspace_id = repo.get_current_change_id()?;
    eprintln!("Workspace ID: {}", workspace_id);

    // Check status before PostToolUse
    let status_output = Command::new("jj")
        .current_dir(repo.dir.path())
        .args(["status", "--no-pager"])
        .output()?;
    eprintln!(
        "Status before PostToolUse: {}",
        String::from_utf8_lossy(&status_output.stdout)
    );

    // Run PostToolUse - this should NOT abandon the workspace
    repo.run_hook("PostToolUse", Some("Write"))?;

    // Verify we're back on original
    let after_post = repo.get_current_change_id()?;
    assert_eq!(after_post, initial_change, "Should be back on original");

    // Verify the Claude change exists and contains our file
    let claude_change = repo
        .find_claude_change()?
        .expect("Claude change should exist - not abandoned!");

    // The workspace ID becomes the Claude change ID (it gets rebased and renamed)
    assert_eq!(
        workspace_id, claude_change,
        "Workspace becomes the Claude change"
    );

    // Verify the file is in the Claude change
    let diff_output = Command::new("jj")
        .current_dir(repo.dir.path())
        .args(["diff", "-r", &claude_change])
        .output()?;

    let diff = String::from_utf8_lossy(&diff_output.stdout);
    assert!(
        diff.contains("claude_file.txt"),
        "File should be in Claude change, not abandoned. Diff: {}",
        diff
    );

    Ok(())
}

#[test]
fn test_empty_commit_with_working_copy_changes_not_abandoned() -> Result<()> {
    // This test verifies that when the commit is (empty) but there are working copy changes,
    // we don't abandon the workspace
    let repo = TestRepo::new()?;
    let initial_change = repo.get_current_change_id()?;

    repo.run_hook("UserPromptSubmit", None)?;
    repo.run_hook("PreToolUse", Some("Write"))?;

    // Create a file in working copy (not committed)
    repo.create_file("new_file.txt", "content")?;

    // Verify the commit is empty but there are working copy changes
    let status_output = Command::new("jj")
        .current_dir(repo.dir.path())
        .args(["status", "--no-pager"])
        .output()?;
    let status = String::from_utf8_lossy(&status_output.stdout);

    // Should show working copy changes
    assert!(
        status.contains("Working copy changes:"),
        "Should have working copy changes"
    );

    // Run PostToolUse - should NOT abandon despite (empty) marker
    repo.run_hook("PostToolUse", Some("Write"))?;

    // Should be back on original
    assert_eq!(repo.get_current_change_id()?, initial_change);

    // Claude change should exist with our changes
    let claude_change = repo
        .find_claude_change()?
        .expect("Claude change should NOT be abandoned!");

    // Verify file is in Claude change
    let diff_output = Command::new("jj")
        .current_dir(repo.dir.path())
        .args(["diff", "-r", &claude_change])
        .output()?;
    let diff = String::from_utf8_lossy(&diff_output.stdout);
    assert!(
        diff.contains("new_file.txt"),
        "File should be in Claude change. Diff: {}",
        diff
    );

    Ok(())
}

#[test]
fn test_workspace_abandoned_when_no_changes() -> Result<()> {
    // This test verifies that when there are truly no changes, the workspace is abandoned
    let repo = TestRepo::new()?;
    let initial_change = repo.get_current_change_id()?;

    repo.run_hook("UserPromptSubmit", None)?;
    repo.run_hook("PreToolUse", Some("Read"))?;

    // Don't create any files - simulate a read-only operation

    // Run PostToolUse - should abandon the empty workspace
    repo.run_hook("PostToolUse", Some("Read"))?;

    // After abandon with explicit edit, we should be back on the original
    let current = repo.get_current_change_id()?;
    assert_eq!(
        current, initial_change,
        "Should be back on original after abandon"
    );

    // Claude change should NOT exist
    let claude_change = repo.find_claude_change()?;
    assert!(
        claude_change.is_none(),
        "Claude change should not exist when no changes were made"
    );

    Ok(())
}

#[test]
fn test_git_interpret_trailers_compatibility() -> Result<()> {
    let repo = TestRepo::new()?;

    // Create a Claude change with session ID trailer
    repo.run_hook("UserPromptSubmit", None)?;
    repo.run_hook("PreToolUse", Some("Write"))?;
    repo.create_file("test.txt", "content")?;
    repo.run_hook("PostToolUse", Some("Write"))?;

    // Find the Claude change after PostToolUse (when it's been created)
    let claude_change = repo
        .find_claude_change()?
        .expect("Claude change should exist");

    // Get the commit message directly from jj using the specific change ID
    let desc_output = Command::new("jj")
        .current_dir(repo.dir.path())
        .args([
            "log",
            "-r",
            &claude_change,
            "--no-graph",
            "-T",
            "description",
        ])
        .output()?;

    if !desc_output.status.success() {
        anyhow::bail!(
            "Failed to get jj description: {}",
            String::from_utf8_lossy(&desc_output.stderr)
        );
    }

    let commit_message = String::from_utf8_lossy(&desc_output.stdout);

    // Write the commit message to a temp file
    let message_file = repo.dir.path().join("commit_message.txt");
    fs::write(&message_file, commit_message.as_bytes())?;

    // Run git interpret-trailers on the commit message
    let trailers_output = Command::new("git")
        .current_dir(repo.dir.path())
        .args([
            "interpret-trailers",
            "--only-trailers",
            message_file.to_str().unwrap(),
        ])
        .output()?;

    if !trailers_output.status.success() {
        anyhow::bail!(
            "git interpret-trailers failed: {}",
            String::from_utf8_lossy(&trailers_output.stderr)
        );
    }

    let trailers = String::from_utf8_lossy(&trailers_output.stdout);

    // Verify the Claude-Session-Id trailer was parsed correctly
    assert!(
        trailers.contains(&format!("Jjagent-claude-session-id: {}", repo.session_id)),
        "Claude-Session-Id trailer should be parsed by git interpret-trailers. Got: '{}'",
        trailers
    );

    Ok(())
}

#[test]
fn test_multiple_commits_same_session_id_uses_furthest_descendant() -> Result<()> {
    let repo = TestRepo::new()?;
    let initial_change = repo.get_current_change_id()?;

    // First tool use - creates first Claude change
    repo.run_hook("UserPromptSubmit", None)?;
    repo.run_hook("PreToolUse", Some("Write"))?;
    repo.create_file("file1.txt", "first change")?;
    repo.run_hook("PostToolUse", Some("Write"))?;

    // Get the first Claude change
    let first_claude = repo
        .find_claude_change()?
        .expect("First Claude change should exist");

    // Manually create a descendant commit with the same session ID
    // This simulates a scenario where multiple commits might have the same session ID
    Command::new("jj")
        .current_dir(repo.dir.path())
        .args(["new", &first_claude])
        .output()?;

    // Add a file to the new commit
    repo.create_file("file2.txt", "second change")?;

    // Add the same Claude-Session-Id trailer to this commit
    let desc_with_trailer = format!(
        "Another Claude commit\n\nJjagent-claude-session-id: {}",
        repo.session_id
    );

    let mut child = Command::new("jj")
        .current_dir(repo.dir.path())
        .args(["describe", "--stdin"])
        .stdin(std::process::Stdio::piped())
        .spawn()?;

    if let Some(stdin) = child.stdin.as_mut() {
        use std::io::Write;
        stdin.write_all(desc_with_trailer.as_bytes())?;
    }
    child.wait()?;

    // Get the second commit's ID
    let second_claude = repo.get_current_change_id()?;

    // Navigate back to initial to simulate starting a new operation
    Command::new("jj")
        .current_dir(repo.dir.path())
        .args(["edit", &initial_change])
        .output()?;

    // Verify that find_claude_change returns the furthest descendant
    let found_claude = repo
        .find_claude_change()?
        .expect("Should find a Claude change");

    assert_eq!(
        found_claude, second_claude,
        "Should find the furthest descendant (second_claude), not the first one"
    );

    // Now do another tool use - it should find and use the furthest descendant
    repo.run_hook("UserPromptSubmit", None)?;
    repo.run_hook("PreToolUse", Some("Write"))?;
    repo.create_file("file3.txt", "third change")?;
    repo.run_hook("PostToolUse", Some("Write"))?;

    // Verify that the changes were squashed into the furthest descendant (second_claude)
    // not the first one
    let diff_output = Command::new("jj")
        .current_dir(repo.dir.path())
        .args(["diff", "-r", &second_claude])
        .output()?;

    let diff = String::from_utf8_lossy(&diff_output.stdout);

    // The second commit should now have all three files
    assert!(
        diff.contains("file2.txt"),
        "Should have file2.txt from original second commit"
    );
    assert!(
        diff.contains("file3.txt"),
        "Should have file3.txt from new changes"
    );

    // The first commit should only have file1.txt
    let first_diff_output = Command::new("jj")
        .current_dir(repo.dir.path())
        .args(["diff", "-r", &first_claude])
        .output()?;

    let first_diff = String::from_utf8_lossy(&first_diff_output.stdout);
    assert!(
        first_diff.contains("file1.txt"),
        "First should have file1.txt"
    );
    assert!(
        !first_diff.contains("file3.txt"),
        "First should NOT have file3.txt"
    );

    Ok(())
}

#[test]
fn test_session_split_basic() -> Result<()> {
    let repo = TestRepo::new()?;
    let session_id = uuid::Uuid::new_v4().to_string();

    // Create a commit with a session ID
    let _session_commit = repo.create_commit_with_session_id(&session_id)?;

    // Move to a new working copy commit on top
    Command::new("jj")
        .current_dir(repo.dir.path())
        .args(["new"])
        .output()?;

    let working_copy_before = repo.get_current_change_id()?;

    // Run session split
    let status = repo.run_session_split(&session_id)?;
    assert!(status.success(), "Session split should succeed");

    // Should still be on the same working copy
    let working_copy_after = repo.get_current_change_id()?;
    assert_eq!(
        working_copy_before, working_copy_after,
        "Should remain on working copy"
    );

    // Should now have 2 commits with the session ID
    assert_eq!(
        repo.count_commits_with_session_id(&session_id)?,
        2,
        "Should have 2 commits with session ID"
    );

    // The new commit should be between session and working copy
    // Check by verifying @- has the session ID
    let parent_desc = repo.get_change_description("@-")?;
    assert!(
        parent_desc.contains(&format!("Jjagent-claude-session-id: {}", session_id)),
        "Parent of @ should have session ID"
    );
    assert!(
        parent_desc.contains("(split "),
        "Parent of @ should have split timestamp"
    );

    Ok(())
}

#[test]
fn test_session_split_not_found() -> Result<()> {
    let repo = TestRepo::new()?;
    let non_existent_id = uuid::Uuid::new_v4().to_string();

    // Try to split a non-existent session
    let status = repo.run_session_split(&non_existent_id)?;
    assert!(!status.success(), "Should fail when session not found");

    Ok(())
}

#[test]
fn test_session_split_multiple_sessions() -> Result<()> {
    let repo = TestRepo::new()?;
    let session_id = uuid::Uuid::new_v4().to_string();

    // Create first commit with session ID
    let _first_commit = repo.create_commit_with_session_id(&session_id)?;

    // Create second commit with same session ID (descendant)
    Command::new("jj")
        .current_dir(repo.dir.path())
        .args(["new"])
        .output()?;

    let second_commit = repo.create_commit_with_session_id(&session_id)?;

    // Move to a new working copy on top
    Command::new("jj")
        .current_dir(repo.dir.path())
        .args(["new"])
        .output()?;

    // Run session split - should use furthest descendant
    let status = repo.run_session_split(&session_id)?;
    assert!(status.success(), "Should succeed with multiple sessions");

    // Should now have 3 commits with the session ID
    assert_eq!(
        repo.count_commits_with_session_id(&session_id)?,
        3,
        "Should have 3 commits with session ID"
    );

    // The new commit should be on top of the second commit (furthest descendant)
    let parent_id = Command::new("jj")
        .current_dir(repo.dir.path())
        .args(["log", "-r", "@--", "--no-graph", "-T", "change_id"])
        .output()?;

    let parent = String::from_utf8_lossy(&parent_id.stdout)
        .trim()
        .to_string();
    assert_eq!(
        parent, second_commit,
        "Should be on top of furthest descendant"
    );

    Ok(())
}

#[test]
fn test_session_split_preserves_original() -> Result<()> {
    let repo = TestRepo::new()?;
    let session_id = uuid::Uuid::new_v4().to_string();

    // Create a commit with session ID
    let session_commit = repo.create_commit_with_session_id(&session_id)?;
    let original_desc = repo.get_change_description(&session_commit)?;

    // Move to new working copy
    Command::new("jj")
        .current_dir(repo.dir.path())
        .args(["new"])
        .output()?;

    // Run session split
    repo.run_session_split(&session_id)?;

    // Original commit should be unchanged
    let after_desc = repo.get_change_description(&session_commit)?;
    assert_eq!(
        original_desc, after_desc,
        "Original commit should be unchanged"
    );

    Ok(())
}

#[test]
fn test_session_split_empty_commit() -> Result<()> {
    let repo = TestRepo::new()?;
    let session_id = uuid::Uuid::new_v4().to_string();

    // Create a commit with session ID
    repo.create_commit_with_session_id(&session_id)?;

    // Move to new working copy
    Command::new("jj")
        .current_dir(repo.dir.path())
        .args(["new"])
        .output()?;

    // Add a file to working copy (uncommitted changes)
    repo.create_file("working.txt", "uncommitted")?;

    // Run session split
    repo.run_session_split(&session_id)?;

    // The new split commit (@-) should be empty
    let diff_output = Command::new("jj")
        .current_dir(repo.dir.path())
        .args(["diff", "-r", "@-"])
        .output()?;

    let diff = String::from_utf8_lossy(&diff_output.stdout);
    assert!(
        diff.trim().is_empty() || diff.contains("0 files changed"),
        "Split commit should be empty"
    );

    // Working copy should still have its changes
    let wc_diff = Command::new("jj")
        .current_dir(repo.dir.path())
        .args(["diff"])
        .output()?;

    let wc_diff_str = String::from_utf8_lossy(&wc_diff.stdout);
    assert!(
        wc_diff_str.contains("working.txt"),
        "Working copy should still have uncommitted changes"
    );

    Ok(())
}

#[test]
fn test_session_split_working_copy_unchanged() -> Result<()> {
    let repo = TestRepo::new()?;
    let session_id = uuid::Uuid::new_v4().to_string();

    // Create session commit
    repo.create_commit_with_session_id(&session_id)?;

    // Move to new working copy
    Command::new("jj")
        .current_dir(repo.dir.path())
        .args(["new"])
        .output()?;

    let working_copy_id = repo.get_current_change_id()?;

    // Run session split
    repo.run_session_split(&session_id)?;

    // Should still be on same working copy
    assert_eq!(
        repo.get_current_change_id()?,
        working_copy_id,
        "Working copy should remain unchanged"
    );

    Ok(())
}

#[test]
fn test_session_split_description() -> Result<()> {
    let repo = TestRepo::new()?;
    let session_id = uuid::Uuid::new_v4().to_string();

    // Create session commit with specific description
    Command::new("jj")
        .current_dir(repo.dir.path())
        .args(["new"])
        .output()?;

    let description = format!(
        "My Custom Session Title\nWith multiple lines\n\nJjagent-claude-session-id: {}",
        session_id
    );

    Command::new("jj")
        .current_dir(repo.dir.path())
        .args(["describe", "-m", &description])
        .output()?;

    // Move to new working copy
    Command::new("jj")
        .current_dir(repo.dir.path())
        .args(["new"])
        .output()?;

    // Run session split
    repo.run_session_split(&session_id)?;

    // Get the new split commit's description
    let split_desc = repo.get_change_description("@-")?;

    // Should have first line + suffix + trailer
    assert!(
        split_desc.starts_with("My Custom Session Title (split "),
        "Should copy first line with split suffix"
    );
    assert!(
        split_desc.contains(&format!("Jjagent-claude-session-id: {}", session_id)),
        "Should have session ID trailer"
    );
    assert!(
        !split_desc.contains("With multiple lines"),
        "Should not copy additional lines"
    );

    Ok(())
}

#[test]
fn test_session_split_working_copy_is_session() -> Result<()> {
    let repo = TestRepo::new()?;
    let session_id = uuid::Uuid::new_v4().to_string();

    // Create session commit and stay on it
    repo.create_commit_with_session_id(&session_id)?;
    let session_commit = repo.get_current_change_id()?;

    // Run session split while @ IS the session commit
    let status = repo.run_session_split(&session_id)?;
    assert!(status.success(), "Should handle @ = session commit");

    // Should now be on a new commit
    let new_commit = repo.get_current_change_id()?;
    assert_ne!(new_commit, session_commit, "Should be on new commit");

    // Should have 2 commits with session ID
    assert_eq!(
        repo.count_commits_with_session_id(&session_id)?,
        2,
        "Should have 2 commits with session ID"
    );

    // New commit should be child of session commit
    let parent_id = Command::new("jj")
        .current_dir(repo.dir.path())
        .args(["log", "-r", "@-", "--no-graph", "-T", "change_id"])
        .output()?;

    let parent = String::from_utf8_lossy(&parent_id.stdout)
        .trim()
        .to_string();
    assert_eq!(
        parent, session_commit,
        "New commit should be child of session"
    );

    Ok(())
}

#[test]
fn test_session_split_custom_description() -> Result<()> {
    let repo = TestRepo::new()?;
    let session_id = uuid::Uuid::new_v4().to_string();

    // Create a commit with session ID
    repo.create_commit_with_session_id(&session_id)?;

    // Move to new working copy
    Command::new("jj")
        .current_dir(repo.dir.path())
        .args(["new"])
        .output()?;

    // Run session split with custom description
    let custom_desc = "Custom split description for feature work";
    let status = repo.run_session_split_with_description(&session_id, custom_desc)?;
    assert!(
        status.success(),
        "Session split with custom description should succeed"
    );

    // Get the new split commit's description
    let split_desc = repo.get_change_description("@-")?;

    // Should have the custom description, NOT the original first line
    assert!(
        split_desc.starts_with(custom_desc),
        "Should use custom description. Got: {}",
        split_desc
    );

    // Should still have the session ID trailer
    assert!(
        split_desc.contains(&format!("Jjagent-claude-session-id: {}", session_id)),
        "Should have session ID trailer"
    );

    // Should NOT have the (split timestamp) suffix when using custom description
    assert!(
        !split_desc.contains("(split "),
        "Should not have (split timestamp) suffix with custom description"
    );

    Ok(())
}

#[test]
fn test_session_split_diverged_working_copy() -> Result<()> {
    let repo = TestRepo::new()?;
    let session_id = uuid::Uuid::new_v4().to_string();

    // Create session commit
    let _session_commit = repo.create_commit_with_session_id(&session_id)?;

    // Go back to parent and create a divergent branch
    Command::new("jj")
        .current_dir(repo.dir.path())
        .args(["edit", "@-"])
        .output()?;

    Command::new("jj")
        .current_dir(repo.dir.path())
        .args(["new"])
        .output()?;

    // Now @ is not a descendant of session commit
    // Try to split - should fail
    let status = repo.run_session_split(&session_id)?;
    assert!(
        !status.success(),
        "Should fail when @ is not descendant of session"
    );

    Ok(())
}

#[test]
fn test_concurrent_session_on_temp_workspace() -> Result<()> {
    let repo = TestRepo::new()?;
    let initial_change = repo.get_current_change_id()?;

    repo.run_hook("PreToolUse", Some("Edit"))?;

    let current_change = repo.get_current_change_id()?;
    assert_ne!(
        current_change, initial_change,
        "Should be on temp workspace"
    );

    let desc = repo.get_change_description("@")?;
    let has_temp_change_trailer = desc
        .lines()
        .rev()
        .take_while(|line| !line.trim().is_empty())
        .any(|line| line.starts_with("Jjagent-claude-temp-change:"));
    assert!(has_temp_change_trailer, "Should be on temp workspace");

    let session_b_id = uuid::Uuid::new_v4().to_string();
    let input = json!({
        "session_id": session_b_id,
        "tool_name": "Edit"
    });

    let jjagent_binary = env!("CARGO_BIN_EXE_jjagent");
    let handle = std::thread::spawn({
        let repo_dir = repo.dir.path().to_path_buf();
        let input = input.clone();
        move || -> Result<std::process::Output> {
            let mut child = Command::new(jjagent_binary)
                .current_dir(&repo_dir)
                .env_remove("JJAGENT_DISABLE")
                .args(["claude", "hooks", "PreToolUse"])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()?;

            if let Some(stdin) = child.stdin.as_mut() {
                use std::io::Write;
                stdin.write_all(input.to_string().as_bytes())?;
            }

            Ok(child.wait_with_output()?)
        }
    });

    std::thread::sleep(std::time::Duration::from_millis(1500));

    repo.create_file("test.txt", "content")?;
    repo.run_hook("PostToolUse", Some("Edit"))?;

    let output = handle.join().unwrap()?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    if !output.status.success() {
        eprintln!("Session B stderr: {}", stderr);
        eprintln!("Session B stdout: {}", stdout);
    }

    assert!(
        output.status.success(),
        "Session B should succeed after Session A completes. Stderr: {}",
        stderr
    );

    Ok(())
}

#[test]
fn test_concurrent_session_on_claude_change() -> Result<()> {
    let repo = TestRepo::new()?;

    repo.run_hook("PreToolUse", Some("Edit"))?;
    repo.create_file("test.txt", "content")?;
    repo.run_hook("PostToolUse", None)?;

    let claude_change = repo
        .find_claude_change()?
        .expect("Should have found Claude change");

    Command::new("jj")
        .current_dir(repo.dir.path())
        .args(["edit", &claude_change])
        .output()?;

    let current = repo.get_current_change_id()?;
    assert_eq!(
        current, claude_change,
        "Should be on Claude change from session A"
    );

    let session_b_id = uuid::Uuid::new_v4().to_string();
    let input = json!({
        "session_id": session_b_id,
        "tool_name": "Edit"
    });

    let jjagent_binary = env!("CARGO_BIN_EXE_jjagent");
    let mut child = Command::new(jjagent_binary)
        .current_dir(repo.dir.path())
        .env_remove("JJAGENT_DISABLE")
        .args(["claude", "hooks", "PreToolUse"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    if let Some(stdin) = child.stdin.as_mut() {
        use std::io::Write;
        stdin.write_all(input.to_string().as_bytes())?;
    }

    let output = child.wait_with_output()?;

    assert!(
        !output.status.success(),
        "Session B should fail to run PreToolUse on Session A's Claude change"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Concurrent") || stderr.contains("session"),
        "Error should mention concurrent session. Got: {}",
        stderr
    );

    Ok(())
}

#[test]
fn test_poisoned_original_working_copy() -> Result<()> {
    let repo = TestRepo::new()?;
    let original = repo.get_current_change_id()?;

    repo.run_hook("PreToolUse", Some("Edit"))?;

    let original_file = std::env::temp_dir().join(format!(
        "claude-session-{}-original-working-copy.txt",
        repo.session_id
    ));
    assert!(original_file.exists(), "Original file should be stored");

    let other_session_id = uuid::Uuid::new_v4().to_string();
    Command::new("jj")
        .current_dir(repo.dir.path())
        .args([
            "describe",
            "-r",
            &original,
            "-m",
            &format!(
                "Corrupted\n\nJjagent-claude-session-id: {}",
                other_session_id
            ),
        ])
        .output()?;

    repo.create_file("test.txt", "content")?;

    let input = json!({
        "session_id": repo.session_id,
    });

    let jjagent_binary = env!("CARGO_BIN_EXE_jjagent");
    let mut child = Command::new(jjagent_binary)
        .current_dir(repo.dir.path())
        .env_remove("JJAGENT_DISABLE")
        .args(["claude", "hooks", "PostToolUse"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    if let Some(stdin) = child.stdin.as_mut() {
        use std::io::Write;
        stdin.write_all(input.to_string().as_bytes())?;
    }

    let output = child.wait_with_output()?;

    assert!(
        !output.status.success(),
        "PostToolUse should fail when original is poisoned"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Concurrent") || stderr.contains("session"),
        "Error should mention concurrent session issue. Got: {}",
        stderr
    );

    Ok(())
}

#[test]
fn test_sequential_sessions() -> Result<()> {
    let repo = TestRepo::new()?;
    let initial = repo.get_current_change_id()?;

    repo.run_hook("PreToolUse", Some("Edit"))?;
    repo.create_file("file_a.txt", "session a")?;
    repo.run_hook("PostToolUse", None)?;

    let after_session_a = repo.get_current_change_id()?;
    assert_eq!(
        after_session_a, initial,
        "Should return to original after session A"
    );

    let session_b_id = uuid::Uuid::new_v4().to_string();
    let input = json!({
        "session_id": session_b_id,
        "tool_name": "Edit"
    });

    let jjagent_binary = env!("CARGO_BIN_EXE_jjagent");
    let mut child = Command::new(jjagent_binary)
        .current_dir(repo.dir.path())
        .env_remove("JJAGENT_DISABLE")
        .args(["claude", "hooks", "PreToolUse"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    if let Some(stdin) = child.stdin.as_mut() {
        use std::io::Write;
        stdin.write_all(input.to_string().as_bytes())?;
    }

    let output = child.wait_with_output()?;

    assert!(
        output.status.success(),
        "Session B should succeed after Session A completes. Stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    Ok(())
}

#[test]
fn test_same_session_continuation() -> Result<()> {
    let repo = TestRepo::new()?;
    let initial = repo.get_current_change_id()?;

    repo.run_hook("PreToolUse", Some("Edit"))?;
    repo.create_file("file1.txt", "first")?;
    repo.run_hook("PostToolUse", None)?;

    assert_eq!(
        repo.get_current_change_id()?,
        initial,
        "Should return to original"
    );

    let result = repo.run_hook("PreToolUse", Some("Edit"));
    assert!(
        result.is_ok(),
        "Second tool use in same session should succeed"
    );

    Ok(())
}

#[test]
fn test_session_with_own_claude_change() -> Result<()> {
    let repo = TestRepo::new()?;
    let initial = repo.get_current_change_id()?;

    repo.run_hook("PreToolUse", Some("Edit"))?;
    repo.create_file("file1.txt", "first")?;
    repo.run_hook("PostToolUse", None)?;

    let claude_change = repo
        .find_claude_change()?
        .expect("Should have found Claude change");

    repo.run_hook("PreToolUse", Some("Edit"))?;
    repo.create_file("file2.txt", "second")?;

    let result = repo.run_hook("PostToolUse", None);
    assert!(
        result.is_ok(),
        "Should succeed squashing into own Claude change"
    );

    assert_eq!(
        repo.get_current_change_id()?,
        initial,
        "Should return to original"
    );

    let files_output = Command::new("jj")
        .current_dir(repo.dir.path())
        .args(["log", "-r", &claude_change, "-T", "empty", "--summary"])
        .output()?;

    let files = String::from_utf8_lossy(&files_output.stdout);
    assert!(files.contains("file1.txt"), "Should have first file");
    assert!(files.contains("file2.txt"), "Should have second file");

    Ok(())
}

#[test]
fn test_concurrent_edits_with_waiting() -> Result<()> {
    let repo = TestRepo::new()?;
    let initial_change = repo.get_current_change_id()?;

    repo.run_hook("PreToolUse", Some("Edit"))?;
    let _workspace_a = repo.get_current_change_id()?;
    assert!(
        repo.is_on_temp_workspace()?,
        "Session A should be on temp workspace"
    );

    let session_b_id = uuid::Uuid::new_v4().to_string();

    std::thread::spawn({
        let repo_dir = repo.dir.path().to_path_buf();
        let session_b_id = session_b_id.clone();
        move || -> Result<()> {
            std::thread::sleep(std::time::Duration::from_millis(500));

            let input = json!({
                "session_id": session_b_id,
                "tool_name": "Edit"
            });

            let jjagent_binary = env!("CARGO_BIN_EXE_jjagent");
            let mut child = Command::new(jjagent_binary)
                .current_dir(&repo_dir)
                .env_remove("JJAGENT_DISABLE")
                .args(["claude", "hooks", "PreToolUse"])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()?;

            if let Some(stdin) = child.stdin.as_mut() {
                use std::io::Write;
                stdin.write_all(input.to_string().as_bytes())?;
            }

            let _output = child.wait_with_output()?;
            Ok(())
        }
    });

    std::thread::sleep(std::time::Duration::from_millis(1000));

    repo.create_file("file_a.txt", "session a content")?;
    repo.run_hook("PostToolUse", Some("Edit"))?;

    assert_eq!(
        repo.get_current_change_id()?,
        initial_change,
        "Session A should return to initial"
    );

    std::thread::sleep(std::time::Duration::from_millis(2000));

    Ok(())
}

#[test]
fn test_same_session_can_continue_on_own_workspace() -> Result<()> {
    let repo = TestRepo::new()?;

    repo.run_hook("PreToolUse", Some("Edit"))?;
    let workspace_id = repo.get_current_change_id()?;

    let result = repo.run_hook("PreToolUse", Some("Edit"));
    assert!(
        result.is_ok(),
        "Same session should be able to continue on its own workspace"
    );

    assert_eq!(
        repo.get_current_change_id()?,
        workspace_id,
        "Should still be on same workspace"
    );

    Ok(())
}

#[test]
fn test_timeout_when_session_never_completes() -> Result<()> {
    let repo = TestRepo::new()?;

    repo.run_hook("PreToolUse", Some("Edit"))?;

    assert!(
        repo.is_on_temp_workspace()?,
        "Session A should be on temp workspace"
    );

    let session_b_id = uuid::Uuid::new_v4().to_string();
    let input = json!({
        "session_id": session_b_id,
        "tool_name": "Edit"
    });

    let jjagent_binary = env!("CARGO_BIN_EXE_jjagent");
    let start = std::time::Instant::now();
    let mut child = Command::new(jjagent_binary)
        .current_dir(repo.dir.path())
        .env_remove("JJAGENT_DISABLE")
        .args(["claude", "hooks", "PreToolUse"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    if let Some(stdin) = child.stdin.as_mut() {
        use std::io::Write;
        stdin.write_all(input.to_string().as_bytes())?;
    }

    let output = child.wait_with_output()?;
    let elapsed = start.elapsed();

    assert!(
        !output.status.success(),
        "Session B should timeout and fail"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("60 seconds") || stderr.contains("abandoned"),
        "Error should mention timeout. Got: {}",
        stderr
    );

    assert!(
        elapsed.as_secs() >= 60 && elapsed.as_secs() < 65,
        "Should timeout after approximately 60 seconds, got {} seconds",
        elapsed.as_secs()
    );

    Ok(())
}

#[test]
fn test_bash_tool_creates_files() -> Result<()> {
    let repo = TestRepo::new()?;
    let initial_change = repo.get_current_change_id()?;

    // First bash tool use should create temporary workspace
    repo.run_hook("UserPromptSubmit", None)?;
    repo.run_hook("PreToolUse", Some("Bash"))?;

    // We should be on a temporary workspace
    let current = repo.get_current_change_id()?;
    assert_ne!(current, initial_change, "Should have moved to a new change");

    let desc = repo.get_change_description(&current)?;
    let has_temp_change_trailer = desc
        .lines()
        .rev()
        .take_while(|line| !line.trim().is_empty())
        .any(|line| line.starts_with("Jjagent-claude-temp-change:"));
    assert!(has_temp_change_trailer, "Should be on temporary workspace");

    // Simulate bash command that creates a file
    repo.create_file("bash_created.txt", "Created by bash command")?;

    repo.run_hook("PostToolUse", Some("Bash"))?;

    // After PostToolUse, we should be back on original working copy
    let final_change = repo.get_current_change_id()?;
    assert_eq!(
        final_change, initial_change,
        "Should be back on original working copy"
    );

    // Claude change should exist with our file
    let claude_change = repo
        .find_claude_change()?
        .expect("Claude change should exist");

    // Verify the file is in the Claude change
    let diff_output = Command::new("jj")
        .current_dir(repo.dir.path())
        .args(["diff", "-r", &claude_change])
        .output()?;

    let diff = String::from_utf8_lossy(&diff_output.stdout);
    assert!(
        diff.contains("bash_created.txt"),
        "File should be in Claude change, not abandoned. Diff: {}",
        diff
    );

    Ok(())
}

#[test]
fn test_bash_tool_modifies_existing_files() -> Result<()> {
    let repo = TestRepo::new()?;
    let initial_change = repo.get_current_change_id()?;

    // Create an initial file
    repo.create_file("existing.txt", "Initial content")?;
    Command::new("jj")
        .current_dir(repo.dir.path())
        .args(["describe", "-m", "Add existing file"])
        .output()?;

    repo.run_hook("UserPromptSubmit", None)?;
    repo.run_hook("PreToolUse", Some("Bash"))?;

    // Simulate bash command modifying the file (like sed, awk, etc.)
    fs::write(repo.dir.path().join("existing.txt"), "Modified by bash")?;

    repo.run_hook("PostToolUse", Some("Bash"))?;

    // Should be back on original
    assert_eq!(
        repo.get_current_change_id()?,
        initial_change,
        "Should be back on original working copy"
    );

    // Claude change should exist with the modification
    let claude_change = repo
        .find_claude_change()?
        .expect("Claude change should exist");

    let diff_output = Command::new("jj")
        .current_dir(repo.dir.path())
        .args(["diff", "-r", &claude_change])
        .output()?;

    let diff = String::from_utf8_lossy(&diff_output.stdout);
    assert!(
        diff.contains("Modified by bash"),
        "Modified content should be in Claude change. Diff: {}",
        diff
    );

    Ok(())
}

#[test]
fn test_bash_tool_no_changes_abandoned() -> Result<()> {
    let repo = TestRepo::new()?;
    let initial_change = repo.get_current_change_id()?;

    repo.run_hook("UserPromptSubmit", None)?;
    repo.run_hook("PreToolUse", Some("Bash"))?;

    // Don't create or modify any files - simulate a read-only bash command
    // like `ls`, `grep`, `cat`, etc.

    repo.run_hook("PostToolUse", Some("Bash"))?;

    // Should be back on original
    assert_eq!(
        repo.get_current_change_id()?,
        initial_change,
        "Should be back on original after abandon"
    );

    // Claude change should NOT exist when no changes were made
    let claude_change = repo.find_claude_change()?;
    assert!(
        claude_change.is_none(),
        "Claude change should not exist when no changes were made by bash"
    );

    Ok(())
}

#[test]
fn test_mixed_edit_and_bash_workflow() -> Result<()> {
    let repo = TestRepo::new()?;
    let initial_change = repo.get_current_change_id()?;

    // First operation: file edit
    repo.run_hook("UserPromptSubmit", None)?;
    repo.run_hook("PreToolUse", Some("Edit"))?;
    repo.create_file("edit_file.txt", "Created by edit")?;
    repo.run_hook("PostToolUse", Some("Edit"))?;

    assert_eq!(
        repo.get_current_change_id()?,
        initial_change,
        "Should be back on original after edit"
    );

    // Second operation: bash command in same session
    repo.run_hook("UserPromptSubmit", None)?;
    repo.run_hook("PreToolUse", Some("Bash"))?;
    repo.create_file("bash_file.txt", "Created by bash")?;
    repo.run_hook("PostToolUse", Some("Bash"))?;

    assert_eq!(
        repo.get_current_change_id()?,
        initial_change,
        "Should be back on original after bash"
    );

    // Both files should be in the same Claude change
    let claude_change = repo
        .find_claude_change()?
        .expect("Claude change should exist");

    let diff_output = Command::new("jj")
        .current_dir(repo.dir.path())
        .args(["diff", "-r", &claude_change])
        .output()?;

    let diff = String::from_utf8_lossy(&diff_output.stdout);
    assert!(
        diff.contains("edit_file.txt"),
        "Edit file should be in Claude change"
    );
    assert!(
        diff.contains("bash_file.txt"),
        "Bash file should be in Claude change"
    );

    Ok(())
}

#[test]
fn test_bash_tool_subsequent_operations() -> Result<()> {
    let repo = TestRepo::new()?;
    let initial_change = repo.get_current_change_id()?;

    // First bash operation
    repo.run_hook("UserPromptSubmit", None)?;
    repo.run_hook("PreToolUse", Some("Bash"))?;
    repo.create_file("first_bash.txt", "First bash operation")?;
    repo.run_hook("PostToolUse", Some("Bash"))?;

    // Second bash operation in same session
    repo.run_hook("UserPromptSubmit", None)?;
    repo.run_hook("PreToolUse", Some("Bash"))?;

    // Should be on temp workspace now
    assert!(
        repo.is_on_temp_workspace()?,
        "Should be on temporary workspace for second bash"
    );

    repo.create_file("second_bash.txt", "Second bash operation")?;
    repo.run_hook("PostToolUse", Some("Bash"))?;

    // Should be back on original
    assert_eq!(
        repo.get_current_change_id()?,
        initial_change,
        "Should be back on original"
    );

    // Both files should be in the Claude change
    let claude_change = repo
        .find_claude_change()?
        .expect("Claude change should exist");

    let diff_output = Command::new("jj")
        .current_dir(repo.dir.path())
        .args(["diff", "-r", &claude_change])
        .output()?;

    let diff = String::from_utf8_lossy(&diff_output.stdout);
    assert!(
        diff.contains("first_bash.txt"),
        "First bash file should be in Claude change"
    );
    assert!(
        diff.contains("second_bash.txt"),
        "Second bash file should be in Claude change"
    );

    Ok(())
}

#[test]
fn test_bash_tool_with_working_copy_changes() -> Result<()> {
    // Test the scenario where bash command creates working copy changes but commit is empty
    let repo = TestRepo::new()?;
    let initial_change = repo.get_current_change_id()?;

    repo.run_hook("UserPromptSubmit", None)?;
    repo.run_hook("PreToolUse", Some("Bash"))?;

    // Create a file in working copy (not committed)
    repo.create_file("bash_working_copy.txt", "Bash created this")?;

    // Verify the commit is empty but there are working copy changes
    let status_output = Command::new("jj")
        .current_dir(repo.dir.path())
        .args(["status", "--no-pager"])
        .output()?;
    let status = String::from_utf8_lossy(&status_output.stdout);

    // Should show working copy changes
    assert!(
        status.contains("Working copy changes:"),
        "Should have working copy changes from bash command"
    );

    // Run PostToolUse - should NOT abandon despite (empty) marker in jj status
    repo.run_hook("PostToolUse", Some("Bash"))?;

    // Should be back on original
    assert_eq!(repo.get_current_change_id()?, initial_change);

    // Claude change should exist with our changes
    let claude_change = repo
        .find_claude_change()?
        .expect("Claude change should NOT be abandoned!");

    // Verify file is in Claude change
    let diff_output = Command::new("jj")
        .current_dir(repo.dir.path())
        .args(["diff", "-r", &claude_change])
        .output()?;
    let diff = String::from_utf8_lossy(&diff_output.stdout);
    assert!(
        diff.contains("bash_working_copy.txt"),
        "File should be in Claude change. Diff: {}",
        diff
    );

    Ok(())
}
