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

        let jjagent_binary = env!("CARGO_BIN_EXE_jjagent");

        let mut cmd = Command::new(jjagent_binary);
        cmd.current_dir(self.dir.path())
            .env_remove("JJAGENT_DISABLE")
            .args(["claude", "hooks", hook])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn()?;

        if let Some(stdin) = child.stdin.as_mut() {
            use std::io::Write;
            stdin.write_all(input.to_string().as_bytes())?;
        }

        let output = child.wait_with_output()?;

        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.is_empty() {
            eprintln!("jjagent stderr: {}", stderr);
        }

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

    fn count_all_commits(&self) -> Result<usize> {
        let output = Command::new("jj")
            .current_dir(self.dir.path())
            .args(["log", "--no-graph", "-T", "change_id ++ \"\\n\""])
            .output()?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            Ok(stdout.lines().filter(|line| !line.is_empty()).count())
        } else {
            Ok(0)
        }
    }

    fn find_empty_commits(&self) -> Result<Vec<String>> {
        let output = Command::new("jj")
            .current_dir(self.dir.path())
            .args([
                "log",
                "-r",
                "all() & empty()",
                "--no-graph",
                "-T",
                "change_id ++ \"\\n\"",
            ])
            .output()?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            Ok(stdout
                .lines()
                .filter(|line| !line.is_empty())
                .map(|s| s.to_string())
                .collect())
        } else {
            Ok(vec![])
        }
    }

    fn get_change_description(&self, change_id: &str) -> Result<String> {
        let output = Command::new("jj")
            .current_dir(self.dir.path())
            .args(["log", "-r", change_id, "--no-graph", "-T", "description"])
            .output()?;

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn find_session_change(&self) -> Result<Option<String>> {
        let output = Command::new("jj")
            .current_dir(self.dir.path())
            .args([
                "log",
                "-r",
                &format!(
                    "description(glob:'*Claude-session-id: {}*')",
                    self.session_id
                ),
                "--no-graph",
                "-T",
                "change_id ++ \"\\n\"",
            ])
            .output()?;

        if output.status.success() && !output.stdout.is_empty() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let first = stdout.lines().next();
            Ok(first.map(|s| s.to_string()))
        } else {
            Ok(None)
        }
    }

    fn count_session_commits(&self) -> Result<usize> {
        let output = Command::new("jj")
            .current_dir(self.dir.path())
            .args([
                "log",
                "-r",
                &format!(
                    "description(glob:'*Claude-session-id: {}*')",
                    self.session_id
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

    fn count_temp_changes(&self) -> Result<usize> {
        let output = Command::new("jj")
            .current_dir(self.dir.path())
            .args([
                "log",
                "-r",
                "description(glob:'*Claude-temp-change:*')",
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
fn test_single_tool_use_commit_count() -> Result<()> {
    let repo = TestRepo::new()?;
    let initial_change = repo.get_current_change_id()?;
    let commit_count_before = repo.count_all_commits()?;

    eprintln!("=== BEFORE ===");
    eprintln!("Initial change: {}", initial_change);
    eprintln!("Commit count: {}", commit_count_before);
    eprintln!("{}", repo.get_log()?);

    repo.run_hook("UserPromptSubmit", None)?;
    repo.run_hook("PreToolUse", Some("Write"))?;

    eprintln!("\n=== AFTER PreToolUse ===");
    let after_pre = repo.get_current_change_id()?;
    eprintln!("Current change: {}", after_pre);
    eprintln!("{}", repo.get_log()?);

    repo.create_file("test.txt", "content")?;
    repo.run_hook("PostToolUse", Some("Write"))?;

    eprintln!("\n=== AFTER PostToolUse ===");
    let after_post = repo.get_current_change_id()?;
    eprintln!("Current change: {}", after_post);
    eprintln!("{}", repo.get_log()?);

    let commit_count_after = repo.count_all_commits()?;
    let empty_commits = repo.find_empty_commits()?;
    let temp_changes = repo.count_temp_changes()?;

    eprintln!("\n=== ANALYSIS ===");
    eprintln!("Commit count before: {}", commit_count_before);
    eprintln!("Commit count after:  {}", commit_count_after);
    eprintln!("Empty commits: {:?}", empty_commits);
    eprintln!("Temp changes: {}", temp_changes);
    eprintln!("Session commits: {}", repo.count_session_commits()?);

    // Should have created exactly 1 new commit (the Claude change)
    assert_eq!(
        commit_count_after,
        commit_count_before + 1,
        "Should create exactly 1 new commit (Claude change), not {}",
        commit_count_after - commit_count_before
    );

    // Temp change should have been cleaned up
    assert_eq!(temp_changes, 0, "Temp changes should be cleaned up");

    // Should be back on original
    assert_eq!(after_post, initial_change, "Should be back on original");

    Ok(())
}

#[test]
fn test_multiple_tool_uses_commit_count() -> Result<()> {
    let repo = TestRepo::new()?;
    let initial_change = repo.get_current_change_id()?;
    let commit_count_before = repo.count_all_commits()?;

    eprintln!("=== INITIAL ===");
    eprintln!("Starting commit count: {}", commit_count_before);

    // First tool use
    repo.run_hook("UserPromptSubmit", None)?;
    repo.run_hook("PreToolUse", Some("Write"))?;
    eprintln!("\n=== After 1st PreToolUse ===");
    eprintln!("{}", repo.get_log()?);

    repo.create_file("file1.txt", "content1")?;
    repo.run_hook("PostToolUse", Some("Write"))?;
    eprintln!("\n=== After 1st PostToolUse ===");
    eprintln!("{}", repo.get_log()?);

    let after_first = repo.count_all_commits()?;
    eprintln!("Commit count after 1st: {}", after_first);

    // Second tool use
    repo.run_hook("UserPromptSubmit", None)?;
    repo.run_hook("PreToolUse", Some("Write"))?;
    eprintln!("\n=== After 2nd PreToolUse ===");
    eprintln!("{}", repo.get_log()?);

    repo.create_file("file2.txt", "content2")?;
    repo.run_hook("PostToolUse", Some("Write"))?;
    eprintln!("\n=== After 2nd PostToolUse ===");
    eprintln!("{}", repo.get_log()?);

    let after_second = repo.count_all_commits()?;
    eprintln!("Commit count after 2nd: {}", after_second);

    // Third tool use
    repo.run_hook("UserPromptSubmit", None)?;
    repo.run_hook("PreToolUse", Some("Write"))?;
    eprintln!("\n=== After 3rd PreToolUse ===");
    eprintln!("{}", repo.get_log()?);

    repo.create_file("file3.txt", "content3")?;
    repo.run_hook("PostToolUse", Some("Write"))?;
    eprintln!("\n=== After 3rd PostToolUse ===");
    eprintln!("{}", repo.get_log()?);

    let commit_count_final = repo.count_all_commits()?;
    let empty_commits = repo.find_empty_commits()?;
    let temp_changes = repo.count_temp_changes()?;
    let session_commits = repo.count_session_commits()?;

    eprintln!("\n=== FINAL ANALYSIS ===");
    eprintln!("Commit count before:   {}", commit_count_before);
    eprintln!("Commit count after:    {}", commit_count_final);
    eprintln!(
        "New commits created:   {}",
        commit_count_final - commit_count_before
    );
    eprintln!("Empty commits:         {:?}", empty_commits);
    eprintln!("Temp changes:          {}", temp_changes);
    eprintln!("Session commits:       {}", session_commits);

    // Should have created exactly 1 new commit (the Claude change)
    // All 3 tool uses should squash into the same Claude change
    assert_eq!(
        commit_count_final,
        commit_count_before + 1,
        "Should create exactly 1 new commit (Claude change), not {}",
        commit_count_final - commit_count_before
    );

    // Should have exactly 1 session commit
    assert_eq!(session_commits, 1, "Should have exactly 1 session commit");

    // Temp changes should all be cleaned up
    assert_eq!(temp_changes, 0, "Temp changes should be cleaned up");

    // Should be back on original
    let final_change = repo.get_current_change_id()?;
    assert_eq!(final_change, initial_change, "Should be back on original");

    Ok(())
}

#[test]
fn test_no_extra_empty_commits_on_repeated_user_prompt_submit() -> Result<()> {
    let repo = TestRepo::new()?;
    let commit_count_before = repo.count_all_commits()?;

    eprintln!("=== INITIAL ===");
    eprintln!("Commit count: {}", commit_count_before);

    // Multiple UserPromptSubmit calls without tool use
    // This simulates back-and-forth conversation
    for i in 1..=5 {
        repo.run_hook("UserPromptSubmit", None)?;
        eprintln!("\n=== After UserPromptSubmit #{} ===", i);
        eprintln!("{}", repo.get_log()?);

        let commit_count = repo.count_all_commits()?;
        eprintln!("Commit count: {}", commit_count);

        assert_eq!(
            commit_count, commit_count_before,
            "UserPromptSubmit should not create commits"
        );
    }

    Ok(())
}

#[test]
fn test_no_empty_commits_from_interrupted_session() -> Result<()> {
    let repo = TestRepo::new()?;
    let commit_count_before = repo.count_all_commits()?;

    eprintln!("=== INITIAL ===");
    eprintln!("Commit count: {}", commit_count_before);

    // Start a tool use but don't complete it (simulating interruption)
    repo.run_hook("UserPromptSubmit", None)?;
    repo.run_hook("PreToolUse", Some("Write"))?;

    eprintln!("\n=== After interrupted PreToolUse ===");
    eprintln!("{}", repo.get_log()?);

    // No PostToolUse called - session is interrupted
    // A temp change was created but not cleaned up

    let commit_count_after = repo.count_all_commits()?;
    let temp_changes = repo.count_temp_changes()?;

    eprintln!("\n=== ANALYSIS ===");
    eprintln!("Commit count before: {}", commit_count_before);
    eprintln!("Commit count after:  {}", commit_count_after);
    eprintln!("Temp changes:        {}", temp_changes);

    // Should have created 1 temp change
    assert_eq!(
        commit_count_after,
        commit_count_before + 1,
        "Should have 1 temp change"
    );
    assert_eq!(temp_changes, 1, "Should have 1 temp change");

    Ok(())
}

#[test]
fn test_commit_count_matches_expected_after_various_operations() -> Result<()> {
    let repo = TestRepo::new()?;
    let commit_count_before = repo.count_all_commits()?;

    eprintln!("=== INITIAL STATE ===");
    eprintln!("Commits: {}", commit_count_before);
    eprintln!("{}", repo.get_log()?);

    // Operation 1: Tool use with changes
    repo.run_hook("UserPromptSubmit", None)?;
    repo.run_hook("PreToolUse", Some("Write"))?;
    repo.create_file("file1.txt", "content1")?;
    repo.run_hook("PostToolUse", Some("Write"))?;

    eprintln!("\n=== AFTER OP1 (tool use with changes) ===");
    eprintln!("{}", repo.get_log()?);
    let after_op1 = repo.count_all_commits()?;
    eprintln!(
        "Commits: {} (expected: {})",
        after_op1,
        commit_count_before + 1
    );

    // Operation 2: Tool use with no changes (read-only)
    repo.run_hook("UserPromptSubmit", None)?;
    repo.run_hook("PreToolUse", Some("Read"))?;
    // No file creation - read-only operation
    repo.run_hook("PostToolUse", Some("Read"))?;

    eprintln!("\n=== AFTER OP2 (read-only tool use) ===");
    eprintln!("{}", repo.get_log()?);
    let after_op2 = repo.count_all_commits()?;
    eprintln!(
        "Commits: {} (expected: {})",
        after_op2,
        commit_count_before + 1
    );

    // Operation 3: Another tool use with changes
    repo.run_hook("UserPromptSubmit", None)?;
    repo.run_hook("PreToolUse", Some("Write"))?;
    repo.create_file("file2.txt", "content2")?;
    repo.run_hook("PostToolUse", Some("Write"))?;

    eprintln!("\n=== AFTER OP3 (tool use with changes) ===");
    eprintln!("{}", repo.get_log()?);
    let after_op3 = repo.count_all_commits()?;
    eprintln!(
        "Commits: {} (expected: {})",
        after_op3,
        commit_count_before + 1
    );

    let final_commits = repo.count_all_commits()?;
    let empty_commits = repo.find_empty_commits()?;
    let temp_changes = repo.count_temp_changes()?;

    eprintln!("\n=== FINAL ANALYSIS ===");
    eprintln!("Initial commits:  {}", commit_count_before);
    eprintln!("Final commits:    {}", final_commits);
    eprintln!("New commits:      {}", final_commits - commit_count_before);
    eprintln!("Empty commits:    {:?}", empty_commits);
    eprintln!("Temp changes:     {}", temp_changes);

    // Should have exactly 1 new commit (the Claude change)
    assert_eq!(
        final_commits,
        commit_count_before + 1,
        "Should have exactly 1 new commit"
    );

    // No temp changes should remain
    assert_eq!(temp_changes, 0, "No temp changes should remain");

    Ok(())
}

#[test]
fn test_identify_which_hook_creates_extra_commits() -> Result<()> {
    let repo = TestRepo::new()?;

    eprintln!("=== BASELINE ===");
    let before = repo.count_all_commits()?;
    eprintln!("Commits: {}", before);

    eprintln!("\n=== STEP 1: UserPromptSubmit ===");
    repo.run_hook("UserPromptSubmit", None)?;
    let after_user_prompt = repo.count_all_commits()?;
    eprintln!(
        "Commits: {} (delta: {})",
        after_user_prompt,
        after_user_prompt - before
    );
    eprintln!("{}", repo.get_log()?);

    eprintln!("\n=== STEP 2: PreToolUse ===");
    repo.run_hook("PreToolUse", Some("Write"))?;
    let after_pre = repo.count_all_commits()?;
    eprintln!(
        "Commits: {} (delta: {})",
        after_pre,
        after_pre - after_user_prompt
    );
    eprintln!("{}", repo.get_log()?);

    eprintln!("\n=== STEP 3: Create file ===");
    repo.create_file("test.txt", "content")?;
    let after_file = repo.count_all_commits()?;
    eprintln!(
        "Commits: {} (delta: {})",
        after_file,
        after_file - after_pre
    );

    eprintln!("\n=== STEP 4: PostToolUse ===");
    repo.run_hook("PostToolUse", Some("Write"))?;
    let after_post = repo.count_all_commits()?;
    eprintln!(
        "Commits: {} (delta: {})",
        after_post,
        after_post - after_file
    );
    eprintln!("{}", repo.get_log()?);

    eprintln!("\n=== STEP 5: Second UserPromptSubmit ===");
    repo.run_hook("UserPromptSubmit", None)?;
    let after_user_prompt2 = repo.count_all_commits()?;
    eprintln!(
        "Commits: {} (delta: {})",
        after_user_prompt2,
        after_user_prompt2 - after_post
    );
    eprintln!("{}", repo.get_log()?);

    eprintln!("\n=== STEP 6: Second PreToolUse ===");
    repo.run_hook("PreToolUse", Some("Write"))?;
    let after_pre2 = repo.count_all_commits()?;
    eprintln!(
        "Commits: {} (delta: {})",
        after_pre2,
        after_pre2 - after_user_prompt2
    );
    eprintln!("{}", repo.get_log()?);

    eprintln!("\n=== STEP 7: Second file ===");
    repo.create_file("test2.txt", "content2")?;
    let after_file2 = repo.count_all_commits()?;
    eprintln!(
        "Commits: {} (delta: {})",
        after_file2,
        after_file2 - after_pre2
    );

    eprintln!("\n=== STEP 8: Second PostToolUse ===");
    repo.run_hook("PostToolUse", Some("Write"))?;
    let final_count = repo.count_all_commits()?;
    eprintln!(
        "Commits: {} (delta from file creation: {})",
        final_count,
        final_count as i32 - after_file2 as i32
    );
    eprintln!("{}", repo.get_log()?);

    eprintln!("\n=== SUMMARY ===");
    eprintln!("Total commits created: {}", final_count - before);
    eprintln!("Expected: 1 (Claude session commit)");

    Ok(())
}
