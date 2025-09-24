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

        // Build jjcc command - need to execute it with jj repo as working directory
        let mut child = Command::new("jjcc")
            .current_dir(self.dir.path())
            .args(["hooks", hook])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        // Write input
        if let Some(stdin) = child.stdin.as_mut() {
            use std::io::Write;
            stdin.write_all(input.to_string().as_bytes())?;
        }

        // Wait and check output
        let output = child.wait_with_output()?;

        // Print stderr for debugging if command fails
        if !output.status.success() {
            eprintln!(
                "jjcc command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
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
        Ok(desc.contains("[Claude PreToolUse]"))
    }

    fn find_claude_change(&self) -> Result<Option<String>> {
        let output = Command::new("jj")
            .current_dir(self.dir.path())
            .args([
                "log",
                "-r",
                &format!(
                    "description(glob:'*Claude-Session-Id: {}*')",
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
}

#[test]
fn test_first_tool_use() -> Result<()> {
    let repo = TestRepo::new()?;
    let initial_change = repo.get_current_change_id()?;

    // First tool use should create Claude change and leave us on it
    repo.run_hook("UserPromptSubmit", None)?;
    repo.run_hook("PreToolUse", Some("Write"))?;

    // We should be on the Claude change itself (first time)
    let current = repo.get_current_change_id()?;
    assert_ne!(current, initial_change, "Should have moved to a new change");

    let desc = repo.get_change_description(&current)?;
    assert!(
        desc.contains("Claude Code Session"),
        "Should be on Claude change"
    );
    assert!(
        !desc.contains("[Claude PreToolUse]"),
        "Should NOT be on temp workspace"
    );

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

        if i == 1 {
            // First time, we ARE on Claude change temporarily
            let desc = repo.get_change_description(&current)?;
            assert!(
                desc.contains("Claude Code Session"),
                "First time should be on Claude change"
            );
        } else {
            // Subsequent times, should be on temp workspace
            assert!(repo.is_on_temp_workspace()?, "Should be on temp workspace");

            // But never on the Claude change directly
            let claude_change = repo.find_claude_change()?;
            if let Some(claude_id) = claude_change {
                assert_ne!(
                    current, claude_id,
                    "Should never be left on Claude change for subsequent edits"
                );
            }
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
    assert_eq!(after_first, initial_change, "Should be on original after first message");

    // Get the Claude change ID
    let claude_change = repo.find_claude_change()?.expect("Claude change should exist");

    // Follow-up message without tool use (just prompt)
    repo.run_hook("UserPromptSubmit", None)?;

    // Still should be on original, not Claude change
    let after_prompt = repo.get_current_change_id()?;
    assert_eq!(after_prompt, initial_change, "Should still be on original after prompt");
    assert_ne!(after_prompt, claude_change, "Should NOT be on Claude change");

    // Now another tool use
    repo.run_hook("PreToolUse", Some("Write"))?;

    // Should be on temp workspace, not Claude change
    let during_edit = repo.get_current_change_id()?;
    assert_ne!(during_edit, claude_change, "Should NOT be on Claude change during edit");
    assert!(repo.is_on_temp_workspace()?, "Should be on temp workspace");

    repo.create_file("file2.txt", "more content")?;
    repo.run_hook("PostToolUse", Some("Write"))?;

    // Final check - must be on original
    let final_pos = repo.get_current_change_id()?;
    assert_eq!(final_pos, initial_change, "Must end on original working copy");
    assert_ne!(final_pos, claude_change, "Must NOT be on Claude change");

    Ok(())
}

#[test]
fn test_interrupted_operation_recovery() -> Result<()> {
    let repo = TestRepo::new()?;
    let initial_change = repo.get_current_change_id()?;

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
        assert_ne!(current, claude_id, "Should not be stuck on Claude change after interrupted op");
    }

    Ok(())
}