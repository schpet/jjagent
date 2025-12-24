use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

struct TestRepo {
    dir: TempDir,
}

/// Simulates a Claude Code session for testing
struct ClaudeSimulator {
    session_id: String,
    jjagent_binary: &'static str,
    repo_path: PathBuf,
}

impl ClaudeSimulator {
    fn new(repo_path: &Path, session_id: &str) -> Self {
        Self {
            session_id: session_id.to_string(),
            jjagent_binary: env!("CARGO_BIN_EXE_jjagent"),
            repo_path: repo_path.to_path_buf(),
        }
    }

    /// Simulate a Write tool call with PreToolUse and PostToolUse hooks
    fn write_file(&self, path: &str, content: &str) -> Result<()> {
        self.tool_call("Write", || {
            fs::write(self.repo_path.join(path), content)?;
            Ok(())
        })
    }

    /// Simulate an Edit tool call with PreToolUse and PostToolUse hooks
    fn edit_file(&self, path: &str, content: &str) -> Result<()> {
        self.tool_call("Edit", || {
            fs::write(self.repo_path.join(path), content)?;
            Ok(())
        })
    }

    /// Simulate any tool call with a custom action
    fn tool_call<F>(&self, tool_name: &str, action: F) -> Result<()>
    where
        F: FnOnce() -> Result<()>,
    {
        self.run_hook("PreToolUse", tool_name)?;
        action()?;
        self.run_hook("PostToolUse", tool_name)?;
        Ok(())
    }

    fn run_hook(&self, hook_name: &str, tool_name: &str) -> Result<()> {
        let hook_input = format!(
            r#"{{"session_id":"{}","tool_name":"{}"}}"#,
            self.session_id, tool_name
        );

        let mut child = Command::new(self.jjagent_binary)
            .current_dir(&self.repo_path)
            .env_remove("JJAGENT_DISABLE")
            .env_remove("JJAGENT_LOG")
            .env_remove("JJAGENT_LOG_FILE")
            .args(["claude", "hooks", hook_name])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        if let Some(stdin) = child.stdin.take() {
            use std::io::Write;
            let mut stdin = stdin;
            stdin.write_all(hook_input.as_bytes())?;
            stdin.flush()?;
            // stdin is dropped here, which closes it
        }

        let output = child.wait_with_output()?;

        // Debug: Print hook output
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stdout.is_empty() {
            eprintln!("HOOK {} STDOUT: {}", hook_name, stdout);
        }
        if !stderr.is_empty() {
            eprintln!("HOOK {} STDERR: {}", hook_name, stderr);
        }

        assert!(
            output.status.success(),
            "{} hook failed: {}",
            hook_name,
            String::from_utf8_lossy(&output.stderr)
        );

        Ok(())
    }

    /// Run a hook and return the output for testing error cases
    fn run_hook_raw(&self, hook_name: &str, tool_name: &str) -> Result<std::process::Output> {
        let hook_input = format!(
            r#"{{"session_id":"{}","tool_name":"{}"}}"#,
            self.session_id, tool_name
        );

        let mut child = Command::new(self.jjagent_binary)
            .current_dir(&self.repo_path)
            .env_remove("JJAGENT_DISABLE")
            .env_remove("JJAGENT_LOG")
            .env_remove("JJAGENT_LOG_FILE")
            .args(["claude", "hooks", hook_name])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        if let Some(stdin) = child.stdin.take() {
            use std::io::Write;
            let mut stdin = stdin;
            stdin.write_all(hook_input.as_bytes())?;
            stdin.flush()?;
        }

        child
            .wait_with_output()
            .context("Failed to wait for hook output")
    }

    /// Simulate Claude stopping (Stop hook)
    fn stop(&self) -> Result<()> {
        self.run_hook("Stop", "")
    }
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

        // Disable watchman for tests
        let config_output = Command::new("jj")
            .current_dir(dir.path())
            .args(["config", "set", "--repo", "fsmonitor.backend", "none"])
            .output()?;

        if !config_output.status.success() {
            anyhow::bail!(
                "Failed to disable watchman: {}",
                String::from_utf8_lossy(&config_output.stderr)
            );
        }

        Ok(Self { dir })
    }

    /// Create a repo with a realistic initial state (base + uwc)
    /// matching the workflow documentation
    fn new_with_uwc() -> Result<Self> {
        let repo = Self::new()?;

        // Rename initial commit to "base"
        let desc_output = Command::new("jj")
            .current_dir(repo.path())
            .args(["describe", "-m", "base"])
            .output()?;

        if !desc_output.status.success() {
            anyhow::bail!(
                "Failed to describe base: {}",
                String::from_utf8_lossy(&desc_output.stderr)
            );
        }

        // Create uwc on top
        let new_output = Command::new("jj")
            .current_dir(repo.path())
            .args(["new", "-m", "uwc"])
            .output()?;

        if !new_output.status.success() {
            anyhow::bail!(
                "Failed to create uwc: {}",
                String::from_utf8_lossy(&new_output.stderr)
            );
        }

        Ok(repo)
    }

    fn path(&self) -> &std::path::Path {
        self.dir.path()
    }

    /// Get a deterministic snapshot of the repo state (log + all changes)
    fn snapshot(&self) -> Result<String> {
        let template =
            r#"if(current_working_copy, "@", if(root, "◆", "○")) ++ "  " ++ description ++ "\n""#;

        let output = Command::new("jj")
            .current_dir(self.path())
            .env("JJ_CONFIG", "/dev/null")
            .args(["log", "--no-graph", "-T", template, "-p"])
            .output()
            .context("Failed to run jj log")?;

        if !output.status.success() {
            anyhow::bail!("jj log failed: {}", String::from_utf8_lossy(&output.stderr));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

#[test]
fn test_pretool_hook_creates_precommit() -> Result<()> {
    let repo = TestRepo::new()?;
    let session_id = "test-session-12345678";
    let simulator = ClaudeSimulator::new(repo.path(), session_id);

    // Simulate a tool call (Write in this case)
    simulator.write_file("test.txt", "hello world")?;

    // Verify the snapshot
    let snapshot = repo.snapshot()?;
    insta::assert_snapshot!("pretool_creates_precommit", snapshot);

    Ok(())
}

#[test]
fn test_pretool_hook_multiple_calls() -> Result<()> {
    let repo = TestRepo::new()?;
    let session_id = "multi-test-87654321";
    let simulator = ClaudeSimulator::new(repo.path(), session_id);

    // Simulate multiple tool calls
    simulator.write_file("file1.txt", "first file")?;
    simulator.write_file("file2.txt", "second file")?;
    simulator.edit_file("file1.txt", "edited first file")?;

    // Verify the snapshot
    let snapshot = repo.snapshot()?;
    insta::assert_snapshot!("pretool_multiple_calls", snapshot);

    Ok(())
}

#[test]
fn test_pretool_hook_with_uwc() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;
    let session_id = "uwc-test-12345678";
    let simulator = ClaudeSimulator::new(repo.path(), session_id);

    // Simulate a tool call - should create precommit on top of uwc
    simulator.write_file("claude.txt", "claude's changes")?;

    // Verify the snapshot shows: @ precommit, uwc, base, root
    let snapshot = repo.snapshot()?;
    insta::assert_snapshot!("pretool_with_uwc", snapshot);

    Ok(())
}

#[test]
fn test_session_change_detection_finds_existing() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;
    let session_id = "detect-test-12345678";

    // Manually create a session change with trailer
    let session_message = format!(
        "jjagent: session detect-t\n\nClaude-session-id: {}",
        session_id
    );
    let new_output = Command::new("jj")
        .current_dir(repo.path())
        .args([
            "new",
            "--insert-before",
            "@",
            "--no-edit",
            "-m",
            &session_message,
        ])
        .output()?;

    if !new_output.status.success() {
        anyhow::bail!(
            "Failed to create session change: {}",
            String::from_utf8_lossy(&new_output.stderr)
        );
    }

    // Verify the snapshot shows the session change
    let snapshot = repo.snapshot()?;
    insta::assert_snapshot!("session_change_exists", snapshot);

    Ok(())
}

#[test]
fn test_session_change_detection_multiple_descendants() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;
    let session_id = "multi-desc-12345678";

    // Create base commit
    let desc_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["edit", "@-"])
        .output()?;

    if !desc_output.status.success() {
        anyhow::bail!(
            "Failed to edit base: {}",
            String::from_utf8_lossy(&desc_output.stderr)
        );
    }

    // Create first descendant with session ID
    let session_message = format!(
        "jjagent: session multi-de\n\nClaude-session-id: {}",
        session_id
    );
    let new1_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", &session_message])
        .output()?;

    if !new1_output.status.success() {
        anyhow::bail!(
            "Failed to create first descendant: {}",
            String::from_utf8_lossy(&new1_output.stderr)
        );
    }

    // Create second descendant without session ID
    let new2_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", "another commit"])
        .output()?;

    if !new2_output.status.success() {
        anyhow::bail!(
            "Failed to create second descendant: {}",
            String::from_utf8_lossy(&new2_output.stderr)
        );
    }

    // Create third descendant with different session ID
    let other_message = "jjagent: session other-se\n\nClaude-session-id: other-session-87654321";
    let new3_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", other_message])
        .output()?;

    if !new3_output.status.success() {
        anyhow::bail!(
            "Failed to create third descendant: {}",
            String::from_utf8_lossy(&new3_output.stderr)
        );
    }

    // Verify the snapshot shows all descendants
    let snapshot = repo.snapshot()?;
    insta::assert_snapshot!("session_change_multiple_descendants", snapshot);

    Ok(())
}

#[test]
fn test_find_session_change_integration() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;
    let session_id = "find-me-12345678";

    // Create session change with trailer
    let msg = format!(
        "jjagent: session find-me\n\nClaude-session-id: {}",
        session_id
    );
    let new_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "--insert-before", "@", "--no-edit", "-m", &msg])
        .output()?;

    if !new_output.status.success() {
        anyhow::bail!(
            "Failed to create session change: {}",
            String::from_utf8_lossy(&new_output.stderr)
        );
    }

    // Move to base commit - session change should be in descendants
    let edit_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["edit", "@--"])
        .output()?;

    if !edit_output.status.success() {
        anyhow::bail!(
            "Failed to edit base: {}",
            String::from_utf8_lossy(&edit_output.stderr)
        );
    }

    // Use find_session_change from base - should find the session
    let result = jjagent::jj::find_session_change_in(session_id, Some(repo.path()))?;
    assert!(
        result.is_some(),
        "Should find session change in descendants"
    );
    let _found = result.unwrap();
    // The commit was found successfully - that's all we need to verify
    // (The filtering is done by jj's template, so if we got a result, it matches)

    // Verify the final repo state
    let snapshot = repo.snapshot()?;
    insta::assert_snapshot!("find_session_change_integration", snapshot);

    Ok(())
}

#[test]
fn test_create_session_change() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;
    let session_id = jjagent::session::SessionId::from_full("create-test-12345678");

    // Simulate pretool hook: create precommit on top of uwc
    let precommit_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", "jjagent: precommit create-t"])
        .output()?;

    if !precommit_output.status.success() {
        anyhow::bail!(
            "Failed to create precommit: {}",
            String::from_utf8_lossy(&precommit_output.stderr)
        );
    }

    // Now @ is at precommit, create session change
    // Should insert between uwc and base
    jjagent::jj::create_session_change_in(&session_id, Some(repo.path()))?;

    // Verify the structure: @ precommit -> uwc -> session -> base -> root
    let snapshot = repo.snapshot()?;
    insta::assert_snapshot!("create_session_change", snapshot);

    Ok(())
}

#[test]
fn test_create_session_change_verifies_position() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;
    let session_id = jjagent::session::SessionId::from_full("position-test-87654321");

    // Add a file to uwc to make it non-empty
    std::fs::write(repo.path().join("user_file.txt"), "user's work")?;

    // Simulate pretool hook: create precommit on top of uwc
    let precommit_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", "jjagent: precommit position"])
        .output()?;

    if !precommit_output.status.success() {
        anyhow::bail!(
            "Failed to create precommit: {}",
            String::from_utf8_lossy(&precommit_output.stderr)
        );
    }

    // Now @ is at precommit, create session change
    jjagent::jj::create_session_change_in(&session_id, Some(repo.path()))?;

    // Verify that:
    // 1. Session change is between uwc and base
    // 2. User's file is in uwc (not in session)
    // 3. Structure is correct
    let snapshot = repo.snapshot()?;
    insta::assert_snapshot!("create_session_change_with_user_content", snapshot);

    // Additionally, verify we can find the session change from base
    // Structure is: @ precommit -> uwc -> session -> base -> root
    let edit_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["edit", "@---"]) // Go to base (3 steps back from precommit)
        .output()?;

    if !edit_output.status.success() {
        anyhow::bail!(
            "Failed to edit base: {}",
            String::from_utf8_lossy(&edit_output.stderr)
        );
    }

    // From base, descendants should include session, uwc, and precommit
    let found = jjagent::jj::find_session_change_in("position-test-87654321", Some(repo.path()))?;
    assert!(
        found.is_some(),
        "Should find session change in descendants from base"
    );

    Ok(())
}

#[test]
fn test_count_conflicts_no_conflicts() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;

    // Get the change ID of uwc
    let log_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["log", "-r", "@", "-T", "change_id.short()", "--no-graph"])
        .output()?;

    if !log_output.status.success() {
        anyhow::bail!(
            "Failed to get change ID: {}",
            String::from_utf8_lossy(&log_output.stderr)
        );
    }

    let change_id = String::from_utf8_lossy(&log_output.stdout)
        .trim()
        .to_string();

    // Count conflicts - should be 0
    let count = jjagent::jj::count_conflicts_in(&change_id, Some(repo.path()))?;
    assert_eq!(count, 0, "Should have no conflicts");

    Ok(())
}

#[test]
fn test_count_conflicts_with_conflict() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;

    // Create a file in uwc
    std::fs::write(repo.path().join("conflict.txt"), "original content")?;

    // Commit uwc
    let desc_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["describe", "-m", "uwc with file"])
        .output()?;

    if !desc_output.status.success() {
        anyhow::bail!(
            "Failed to describe uwc: {}",
            String::from_utf8_lossy(&desc_output.stderr)
        );
    }

    // Get the change ID of uwc before creating parallel change
    let log_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["log", "-r", "@", "-T", "change_id.short()", "--no-graph"])
        .output()?;

    if !log_output.status.success() {
        anyhow::bail!(
            "Failed to get uwc change ID: {}",
            String::from_utf8_lossy(&log_output.stderr)
        );
    }

    let uwc_change_id = String::from_utf8_lossy(&log_output.stdout)
        .trim()
        .to_string();

    // Go back to base and create a parallel change
    let edit_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["edit", "@-"])
        .output()?;

    if !edit_output.status.success() {
        anyhow::bail!(
            "Failed to edit base: {}",
            String::from_utf8_lossy(&edit_output.stderr)
        );
    }

    // Create parallel change with conflicting content
    let new_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", "parallel change"])
        .output()?;

    if !new_output.status.success() {
        anyhow::bail!(
            "Failed to create parallel change: {}",
            String::from_utf8_lossy(&new_output.stderr)
        );
    }

    std::fs::write(repo.path().join("conflict.txt"), "conflicting content")?;

    // Rebase uwc onto parallel change to create conflict
    let rebase_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["rebase", "-s", &uwc_change_id, "-d", "@"])
        .output()?;

    if !rebase_output.status.success() {
        anyhow::bail!(
            "Failed to rebase: {}",
            String::from_utf8_lossy(&rebase_output.stderr)
        );
    }

    // Count conflicts on uwc - should be at least 1
    let count = jjagent::jj::count_conflicts_in(&uwc_change_id, Some(repo.path()))?;
    assert!(count >= 1, "Should have at least one conflicted change");

    // Verify the snapshot shows the conflict
    let snapshot = repo.snapshot()?;
    insta::assert_snapshot!("count_conflicts_with_conflict", snapshot);

    Ok(())
}

#[test]
fn test_count_conflicts_after_change() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;

    // Create a file in base
    let edit_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["edit", "@-"])
        .output()?;

    if !edit_output.status.success() {
        anyhow::bail!(
            "Failed to edit base: {}",
            String::from_utf8_lossy(&edit_output.stderr)
        );
    }

    std::fs::write(repo.path().join("base.txt"), "base content")?;

    let base_desc_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["describe", "-m", "base with file"])
        .output()?;

    if !base_desc_output.status.success() {
        anyhow::bail!(
            "Failed to describe base: {}",
            String::from_utf8_lossy(&base_desc_output.stderr)
        );
    }

    // Get base change ID
    let base_log_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["log", "-r", "@", "-T", "change_id.short()", "--no-graph"])
        .output()?;

    if !base_log_output.status.success() {
        anyhow::bail!(
            "Failed to get base change ID: {}",
            String::from_utf8_lossy(&base_log_output.stderr)
        );
    }

    let base_change_id = String::from_utf8_lossy(&base_log_output.stdout)
        .trim()
        .to_string();

    // Go to uwc and modify the file differently
    let edit_uwc_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["edit", "@+"])
        .output()?;

    if !edit_uwc_output.status.success() {
        anyhow::bail!(
            "Failed to edit uwc: {}",
            String::from_utf8_lossy(&edit_uwc_output.stderr)
        );
    }

    std::fs::write(repo.path().join("base.txt"), "uwc modified content")?;

    // Create another change on top of uwc
    let new_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", "change on top"])
        .output()?;

    if !new_output.status.success() {
        anyhow::bail!(
            "Failed to create change on top: {}",
            String::from_utf8_lossy(&new_output.stderr)
        );
    }

    // Count conflicts from base - should include descendants if any
    let count = jjagent::jj::count_conflicts_in(&base_change_id, Some(repo.path()))?;
    assert_eq!(count, 0, "Should have no conflicts in this scenario");

    Ok(())
}

#[test]
fn test_squash_happy_path() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;
    let session_id = jjagent::session::SessionId::from_full("squash-test-12345678");

    // Add some content to uwc
    std::fs::write(repo.path().join("uwc_file.txt"), "user's work")?;

    // Simulate pretool hook: create precommit on top of uwc
    let precommit_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", "jjagent: precommit squash-t"])
        .output()?;

    if !precommit_output.status.success() {
        anyhow::bail!(
            "Failed to create precommit: {}",
            String::from_utf8_lossy(&precommit_output.stderr)
        );
    }

    // Add Claude's changes to precommit
    std::fs::write(repo.path().join("claude_file.txt"), "claude's work")?;

    // Get precommit change ID (current @)
    let precommit_id = jjagent::jj::get_change_id_in("@", Some(repo.path()))?;

    // Create session change
    jjagent::jj::create_session_change_in(&session_id, Some(repo.path()))?;

    // Get uwc and session change IDs
    let uwc_id = jjagent::jj::get_change_id_in("@-", Some(repo.path()))?;
    let session_change_id =
        jjagent::jj::find_session_change_anywhere_in("squash-test-12345678", Some(repo.path()))?
            .expect("Session change should exist");

    // Attempt squash (should succeed without introducing conflicts)
    let new_conflicts = jjagent::jj::squash_precommit_into_session_in(
        &precommit_id,
        &session_change_id,
        &uwc_id,
        Some(repo.path()),
    )?;

    assert!(!new_conflicts, "Should not introduce new conflicts");

    // Verify final state: @ uwc -> session -> base -> root
    let snapshot = repo.snapshot()?;
    insta::assert_snapshot!("squash_happy_path", snapshot);

    Ok(())
}

#[test]
fn test_squash_with_changes() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;
    let session_id = jjagent::session::SessionId::from_full("squash-changes-12345678");

    // Simulate pretool hook: create precommit on top of uwc
    let precommit_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", "jjagent: precommit squash-c"])
        .output()?;

    if !precommit_output.status.success() {
        anyhow::bail!(
            "Failed to create precommit: {}",
            String::from_utf8_lossy(&precommit_output.stderr)
        );
    }

    // Add multiple changes to precommit
    std::fs::write(repo.path().join("file1.txt"), "first change")?;
    std::fs::write(repo.path().join("file2.txt"), "second change")?;
    std::fs::create_dir_all(repo.path().join("subdir"))?;
    std::fs::write(repo.path().join("subdir/file3.txt"), "third change")?;

    // Get precommit change ID
    let precommit_id = jjagent::jj::get_change_id_in("@", Some(repo.path()))?;

    // Create session change
    jjagent::jj::create_session_change_in(&session_id, Some(repo.path()))?;

    // Get uwc and session change IDs
    let uwc_id = jjagent::jj::get_change_id_in("@-", Some(repo.path()))?;
    let session_change_id =
        jjagent::jj::find_session_change_anywhere_in("squash-changes-12345678", Some(repo.path()))?
            .expect("Session change should exist");

    // Attempt squash
    let new_conflicts = jjagent::jj::squash_precommit_into_session_in(
        &precommit_id,
        &session_change_id,
        &uwc_id,
        Some(repo.path()),
    )?;

    assert!(!new_conflicts, "Should not introduce new conflicts");

    // Verify that changes were squashed into session
    let snapshot = repo.snapshot()?;
    insta::assert_snapshot!("squash_with_changes", snapshot);

    Ok(())
}

#[test]
fn test_handle_squash_conflicts() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;
    let session_id = jjagent::session::SessionId::from_full("conflict-test-12345678");

    // Create a file in uwc
    std::fs::write(repo.path().join("conflict.txt"), "original content")?;

    // Simulate pretool hook: create precommit on top of uwc
    let precommit_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", "jjagent: precommit conflict-"])
        .output()?;

    if !precommit_output.status.success() {
        anyhow::bail!(
            "Failed to create precommit: {}",
            String::from_utf8_lossy(&precommit_output.stderr)
        );
    }

    // Modify the same file in precommit to create potential conflict
    std::fs::write(repo.path().join("conflict.txt"), "claude's changes")?;

    // Get precommit change ID
    let precommit_id = jjagent::jj::get_change_id_in("@", Some(repo.path()))?;

    // Create session change
    jjagent::jj::create_session_change_in(&session_id, Some(repo.path()))?;

    // Get uwc and session change IDs
    let uwc_id = jjagent::jj::get_change_id_in("@-", Some(repo.path()))?;
    let session_change_id =
        jjagent::jj::find_session_change_anywhere_in("conflict-test-12345678", Some(repo.path()))?
            .expect("Session change should exist");

    // Attempt squash (should introduce conflicts due to same file modification)
    let _new_conflicts = jjagent::jj::squash_precommit_into_session_in(
        &precommit_id,
        &session_change_id,
        &uwc_id,
        Some(repo.path()),
    )?;

    // For this test, we'll handle conflicts regardless of whether they were introduced
    // (simulating the conflict path from the workflow)
    jjagent::jj::handle_squash_conflicts_in(&session_id, 2, Some(repo.path()))?;

    // Verify final state: @ new wc -> pt. 2 -> uwc -> session -> base -> root
    let snapshot = repo.snapshot()?;
    insta::assert_snapshot!("handle_squash_conflicts", snapshot);

    Ok(())
}

#[test]
fn test_conflict_path_multiple_parts() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;
    let session_id = jjagent::session::SessionId::from_full("multipart-test-12345678");

    // Simulate pretool hook: create precommit on top of uwc
    let precommit_output = Command::new("jj")
        .current_dir(repo.path())
        .args([
            "new",
            "-m",
            "jjagent: precommit multipar",
            "--ignore-working-copy",
        ])
        .output()?;

    if !precommit_output.status.success() {
        anyhow::bail!(
            "Failed to create precommit: {}",
            String::from_utf8_lossy(&precommit_output.stderr)
        );
    }

    // Add changes to precommit
    std::fs::write(repo.path().join("part1.txt"), "first part")?;

    // Get precommit change ID
    let precommit_id = jjagent::jj::get_change_id_in("@", Some(repo.path()))?;

    // Create session change
    jjagent::jj::create_session_change_in(&session_id, Some(repo.path()))?;

    // Get uwc and session change IDs
    let uwc_id = jjagent::jj::get_change_id_in("@-", Some(repo.path()))?;
    let session_change_id =
        jjagent::jj::find_session_change_anywhere_in("multipart-test-12345678", Some(repo.path()))?
            .expect("Session change should exist");

    // Attempt squash
    jjagent::jj::squash_precommit_into_session_in(
        &precommit_id,
        &session_change_id,
        &uwc_id,
        Some(repo.path()),
    )?;

    // Simulate conflict path for part 2
    jjagent::jj::handle_squash_conflicts_in(&session_id, 2, Some(repo.path()))?;

    // Verify we can create part 3 as well
    // Add more changes
    std::fs::write(repo.path().join("part2.txt"), "second part")?;

    // Simulate another pretool -> posttool cycle
    let precommit2_output = Command::new("jj")
        .current_dir(repo.path())
        .args([
            "new",
            "-m",
            "jjagent: precommit multipar",
            "--ignore-working-copy",
        ])
        .output()?;

    if !precommit2_output.status.success() {
        anyhow::bail!(
            "Failed to create second precommit: {}",
            String::from_utf8_lossy(&precommit2_output.stderr)
        );
    }

    std::fs::write(repo.path().join("part3.txt"), "third part")?;

    // Handle conflicts again for part 3
    jjagent::jj::handle_squash_conflicts_in(&session_id, 3, Some(repo.path()))?;

    // Verify final state shows multiple parts
    let snapshot = repo.snapshot()?;
    insta::assert_snapshot!("conflict_path_multiple_parts", snapshot);

    Ok(())
}

#[test]
fn test_integration_full_workflow_happy_path() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;
    let session_id = "integration-happy-12345678";
    let simulator = ClaudeSimulator::new(repo.path(), session_id);

    // User has some existing work in uwc
    std::fs::write(repo.path().join("user_work.txt"), "user's code")?;

    // Simulate Claude making changes via Write tool
    simulator.write_file("feature.txt", "new feature")?;

    // Verify final state: @ uwc -> session (with feature.txt) -> base -> root
    let snapshot = repo.snapshot()?;
    insta::assert_snapshot!("integration_full_workflow_happy_path", snapshot);

    Ok(())
}

#[test]
fn test_integration_multiple_tool_uses() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;
    let session_id = "integration-multi-12345678";
    let simulator = ClaudeSimulator::new(repo.path(), session_id);

    // Simulate multiple tool calls in the same session
    simulator.write_file("file1.txt", "first change")?;
    simulator.write_file("file2.txt", "second change")?;
    simulator.edit_file("file1.txt", "edited first change")?;

    // Verify final state: all changes squashed into session
    let snapshot = repo.snapshot()?;
    insta::assert_snapshot!("integration_multiple_tool_uses", snapshot);

    Ok(())
}

#[test]
fn test_integration_conflict_path() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;
    let session_id = "integration-conflict-12345678";
    let simulator = ClaudeSimulator::new(repo.path(), session_id);

    // User has a file in uwc
    std::fs::write(repo.path().join("shared.txt"), "user's version")?;

    // Simulate Claude modifying the same file (will cause conflict on squash)
    // First tool call creates precommit, modifies file, then posttool tries to squash
    simulator.write_file("shared.txt", "claude's version")?;

    // The posttool hook should have detected conflict and created pt. 2
    // Verify final state: @ new wc -> pt. 2 -> uwc -> session -> base -> root
    let snapshot = repo.snapshot()?;
    insta::assert_snapshot!("integration_conflict_path", snapshot);

    Ok(())
}

#[test]
fn test_integration_multiple_conflicts() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;
    let session_id = "integration-multiconf-12345678";
    let simulator = ClaudeSimulator::new(repo.path(), session_id);

    // User has a file
    std::fs::write(repo.path().join("conflict_file.txt"), "original")?;

    // First tool call - creates conflict, becomes pt. 2
    simulator.write_file("conflict_file.txt", "change 1")?;

    // Add more user work to the new working copy
    std::fs::write(
        repo.path().join("conflict_file.txt"),
        "user change after pt 2",
    )?;

    // Second tool call - creates another conflict, becomes pt. 3
    simulator.write_file("conflict_file.txt", "change 2")?;

    // Verify final state shows pt. 2 and pt. 3
    let snapshot = repo.snapshot()?;
    insta::assert_snapshot!("integration_multiple_conflicts", snapshot);

    Ok(())
}

#[test]
fn test_empty_changes() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;
    let session_id = "empty-test-12345678";
    let simulator = ClaudeSimulator::new(repo.path(), session_id);

    // User has some work
    std::fs::write(repo.path().join("user_file.txt"), "user's content")?;

    // Simulate a tool call that doesn't actually modify any files
    // (pretool creates precommit, but no changes are made before posttool)
    simulator.tool_call("Read", || Ok(()))?;

    // Verify the workflow handles empty precommit correctly
    // Should still create session change and squash (even though empty)
    let snapshot = repo.snapshot()?;
    insta::assert_snapshot!("empty_changes", snapshot);

    Ok(())
}

#[test]
fn test_multiple_concurrent_sessions() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;

    // Session 1 makes changes
    let session1_id = "session1-12345678";
    let simulator1 = ClaudeSimulator::new(repo.path(), session1_id);
    simulator1.write_file("session1_file.txt", "session 1 work")?;

    // Session 2 makes different changes
    let session2_id = "session2-87654321";
    let simulator2 = ClaudeSimulator::new(repo.path(), session2_id);
    simulator2.write_file("session2_file.txt", "session 2 work")?;

    // Session 1 makes more changes
    simulator1.write_file("session1_more.txt", "more session 1 work")?;

    // Verify both sessions have their own session changes
    // Should show: @ uwc -> session2 pt.2 OR session1 pt.2 -> ... -> session2 -> session1 -> base
    let snapshot = repo.snapshot()?;
    insta::assert_snapshot!("multiple_concurrent_sessions", snapshot);

    Ok(())
}

#[test]
fn test_linear_history_maintained() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;
    let session_id = "linear-test-12345678";
    let simulator = ClaudeSimulator::new(repo.path(), session_id);

    // Make several changes
    simulator.write_file("file1.txt", "first")?;
    simulator.write_file("file2.txt", "second")?;
    simulator.write_file("file3.txt", "third")?;

    // Verify history is linear (no branches)
    // Use jj log to check for any branches
    let log_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["log", "-r", "all()"])
        .output()?;

    if !log_output.status.success() {
        anyhow::bail!(
            "jj log failed: {}",
            String::from_utf8_lossy(&log_output.stderr)
        );
    }

    let log_str = String::from_utf8_lossy(&log_output.stdout);
    // Check that there are no branch symbols (│ or branches indicated by multiple commits at same level)
    // In linear history, each commit should have exactly one parent (except root)

    // Verify via snapshot
    let snapshot = repo.snapshot()?;
    insta::assert_snapshot!("linear_history_maintained", snapshot);

    // Additional verification: count the number of commits
    let all_commits = log_str
        .lines()
        .filter(|line| line.starts_with("◆") || line.starts_with("○") || line.starts_with("@"))
        .count();

    // We should have: @ (current), session changes, uwc, base, root
    assert!(
        all_commits >= 4,
        "Should have at least 4 commits in linear history"
    );

    Ok(())
}

#[test]
fn test_conflict_resolution_maintains_user_changes() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;
    let session_id = "preserve-user-12345678";
    let simulator = ClaudeSimulator::new(repo.path(), session_id);

    // User creates a file
    std::fs::write(repo.path().join("important.txt"), "user's important data")?;

    // Claude modifies the same file (creates conflict)
    simulator.write_file("important.txt", "claude's changes")?;

    // Verify user's version is preserved in uwc
    let snapshot = repo.snapshot()?;
    insta::assert_snapshot!("conflict_preserves_user_changes", snapshot);

    // The user's version should still be in uwc, Claude's in pt. 2
    Ok(())
}

#[test]
fn test_stop_hook_finalizes_interrupted_session() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;
    let session_id = "interrupted-session-12345678";
    let simulator = ClaudeSimulator::new(repo.path(), session_id);

    // Simulate an interrupted session: pretool runs but posttool doesn't
    simulator.run_hook("PreToolUse", "Write")?;

    // User makes a change while on precommit
    std::fs::write(repo.path().join("interrupted.txt"), "interrupted work")?;

    // Claude stops (Stop hook runs)
    simulator.stop()?;

    // Verify the precommit was finalized and user is back on uwc
    let snapshot = repo.snapshot()?;
    insta::assert_snapshot!("stop_finalizes_interrupted", snapshot);

    Ok(())
}

#[test]
fn test_stop_hook_noop_on_uwc() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;
    let session_id = "noop-test-12345678";
    let simulator = ClaudeSimulator::new(repo.path(), session_id);

    // Complete a normal tool call
    simulator.write_file("normal.txt", "normal work")?;

    // @ should now be on uwc, not precommit
    // Stop hook should be a noop
    simulator.stop()?;

    // Verify nothing changed
    let snapshot = repo.snapshot()?;
    insta::assert_snapshot!("stop_noop_on_uwc", snapshot);

    Ok(())
}

#[test]
fn test_stop_hook_noop_on_session_mismatch() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;
    let session_id_a = "session-a-12345678";
    let session_id_b = "session-b-87654321";

    let simulator_a = ClaudeSimulator::new(repo.path(), session_id_a);
    let simulator_b = ClaudeSimulator::new(repo.path(), session_id_b);

    // Session A starts a tool call (pretool)
    simulator_a.run_hook("PreToolUse", "Write")?;

    // Session B tries to stop (should be noop because @ is session A's precommit)
    simulator_b.stop()?;

    // Verify @ is still on session A's precommit (session B's stop should be noop)
    let log_output = Command::new("jj")
        .current_dir(repo.path())
        .args([
            "log",
            "-r",
            "@",
            "-T",
            r#"description ++ "\n" ++ trailers.map(|t| if(t.key() == "Claude-precommit-session-id", t.value(), "")).join("")"#,
            "--no-graph"
        ])
        .output()?;

    let output_str = String::from_utf8_lossy(&log_output.stdout);
    eprintln!("Current @ state:\n{}", output_str);

    // Check that @ is on a precommit for session A
    assert!(
        output_str.contains("session-a-12345678"),
        "Expected @ to still be on session A's precommit, got: {}",
        output_str
    );

    Ok(())
}

#[test]
fn test_hook_noops_in_non_jj_repo() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;
    let session_id = "error-test-12345678";
    let simulator = ClaudeSimulator::new(repo.path(), session_id);

    // Destroy the jj repo - this simulates running in a non-jj repo
    let jj_dir = repo.path().join(".jj");
    std::fs::remove_dir_all(&jj_dir)?;

    // Run a hook - it should noop when not in a jj repo
    let output = simulator.run_hook_raw("PreToolUse", "Write")?;

    // The hook should succeed with a noop (not fail)
    assert!(
        output.status.success(),
        "Hook should succeed (noop) when not in a jj repo"
    );

    // Check that stdout contains JSON with continue: true (allowing execution to proceed)
    let stdout = String::from_utf8_lossy(&output.stdout);
    eprintln!("Hook stdout: {}", stdout);

    // Parse the JSON
    let json: serde_json::Value =
        serde_json::from_str(&stdout).context("Hook stdout should contain valid JSON")?;

    // Verify the JSON structure - should continue execution
    assert_eq!(
        json.get("continue"),
        Some(&serde_json::Value::Bool(true)),
        "JSON should have continue: true for non-jj repos"
    );

    // stopReason should not be present for successful hooks
    assert!(
        json.get("stopReason").is_none(),
        "JSON should not have a stopReason for successful noop"
    );

    Ok(())
}

#[test]
fn test_hook_does_not_create_jj_dir_in_git_repo() -> Result<()> {
    use tempfile::TempDir;

    // Create a temporary directory with just a git repo (no jj)
    let temp_dir = TempDir::new()?;
    let git_repo_path = temp_dir.path();

    // Initialize a git repo
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(git_repo_path)
        .output()?;

    // Verify .jj doesn't exist
    let jj_dir = git_repo_path.join(".jj");
    assert!(!jj_dir.exists(), ".jj should not exist initially");

    // Run the hook
    let session_id = "git-only-test-12345678";
    let simulator = ClaudeSimulator::new(git_repo_path, session_id);
    let output = simulator.run_hook_raw("PreToolUse", "Write")?;

    // Hook should succeed
    assert!(output.status.success(), "Hook should succeed in git repo");

    // IMPORTANT: Verify .jj directory was NOT created
    assert!(
        !jj_dir.exists(),
        ".jj directory should NOT be created in git-only repos"
    );

    Ok(())
}

#[test]
fn test_two_sessions_conflict_same_file() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;

    // User creates a change with user.txt
    std::fs::write(repo.path().join("user.txt"), "user's content")?;
    let user_desc_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["describe", "-m", "user"])
        .output()?;

    if !user_desc_output.status.success() {
        anyhow::bail!(
            "Failed to describe user change: {}",
            String::from_utf8_lossy(&user_desc_output.stderr)
        );
    }

    // Create a new uwc on top
    let new_uwc_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", "uwc"])
        .output()?;

    if !new_uwc_output.status.success() {
        anyhow::bail!(
            "Failed to create new uwc: {}",
            String::from_utf8_lossy(&new_uwc_output.stderr)
        );
    }

    // Session 1 edits claude.txt
    let session1_id = "session1-conflict-12345678";
    let simulator1 = ClaudeSimulator::new(repo.path(), session1_id);
    simulator1.write_file("claude.txt", "session 1 version")?;

    // Session 2 edits the same file (claude.txt)
    let session2_id = "session2-conflict-87654321";
    let simulator2 = ClaudeSimulator::new(repo.path(), session2_id);
    simulator2.write_file("claude.txt", "session 2 version")?;

    // Session 1 makes another update to claude.txt
    simulator1.write_file("claude.txt", "session 1 second version")?;

    // Capture the final state showing the conflict
    let snapshot = repo.snapshot()?;
    insta::assert_snapshot!("two_sessions_conflict_same_file", snapshot);

    Ok(())
}

#[test]
fn test_two_sessions_separate_files() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;

    // User creates a change with user.txt
    std::fs::write(repo.path().join("user.txt"), "user's content")?;
    let user_desc_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["describe", "-m", "user"])
        .output()?;

    if !user_desc_output.status.success() {
        anyhow::bail!(
            "Failed to describe user change: {}",
            String::from_utf8_lossy(&user_desc_output.stderr)
        );
    }

    // Create a new uwc on top
    let new_uwc_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", "uwc"])
        .output()?;

    if !new_uwc_output.status.success() {
        anyhow::bail!(
            "Failed to create new uwc: {}",
            String::from_utf8_lossy(&new_uwc_output.stderr)
        );
    }

    // Session 1 edits session1.txt
    let session1_id = "session1-separate-12345678";
    let simulator1 = ClaudeSimulator::new(repo.path(), session1_id);
    simulator1.write_file("session1.txt", "session 1 content")?;

    // Session 2 edits session2.txt (different file, no conflict)
    let session2_id = "session2-separate-87654321";
    let simulator2 = ClaudeSimulator::new(repo.path(), session2_id);
    simulator2.write_file("session2.txt", "session 2 content")?;

    // Capture the final state showing no conflict
    let snapshot = repo.snapshot()?;
    insta::assert_snapshot!("two_sessions_separate_files", snapshot);

    Ok(())
}

#[test]
fn test_pretool_hook_fails_when_not_at_head() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;
    let session_id = "not-at-head-12345678";
    let simulator = ClaudeSimulator::new(repo.path(), session_id);

    // Create a descendant of @ so @ is not at a head
    // Structure: new_commit -> @ (uwc) -> base -> root
    let new_commit = Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", "descendant commit"])
        .output()?;

    if !new_commit.status.success() {
        anyhow::bail!(
            "Failed to create descendant: {}",
            String::from_utf8_lossy(&new_commit.stderr)
        );
    }

    // Move @ back to uwc so it has a descendant
    let edit_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["edit", "@-"])
        .output()?;

    if !edit_output.status.success() {
        anyhow::bail!(
            "Failed to edit uwc: {}",
            String::from_utf8_lossy(&edit_output.stderr)
        );
    }

    // Verify @ now has a descendant (not at head)
    let is_at_head = jjagent::jj::is_at_head_in(Some(repo.path()))?;
    assert!(
        !is_at_head,
        "@ should not be at a head for this test to be valid"
    );

    // Try to run PreToolUse hook - it should fail
    let output = simulator.run_hook_raw("PreToolUse", "Write")?;

    // The hook should fail
    assert!(
        !output.status.success(),
        "PreToolUse hook should fail when @ is not at a head"
    );

    // Check the error message in stdout (JSON response)
    let stdout = String::from_utf8_lossy(&output.stdout);
    eprintln!("Hook stdout: {}", stdout);

    // Parse the JSON
    let json: serde_json::Value =
        serde_json::from_str(&stdout).context("Hook stdout should contain valid JSON")?;

    // Verify the JSON structure - should not continue execution
    assert_eq!(
        json.get("continue"),
        Some(&serde_json::Value::Bool(false)),
        "JSON should have continue: false when not at head"
    );

    // stopReason should be present
    let stop_reason = json
        .get("stopReason")
        .and_then(|v| v.as_str())
        .context("stopReason should be present")?;

    assert!(
        stop_reason.contains("not at a head"),
        "stopReason should mention 'not at a head', got: {}",
        stop_reason
    );

    Ok(())
}

#[test]
fn test_pretool_hook_fails_with_conflicts() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;
    let session_id = "conflict-test-12345678";
    let simulator = ClaudeSimulator::new(repo.path(), session_id);

    // Create a file
    fs::write(repo.path().join("test.txt"), "original content")?;

    // Commit it to the working copy
    let describe_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["describe", "-m", "add test file"])
        .output()?;

    if !describe_output.status.success() {
        anyhow::bail!(
            "Failed to describe: {}",
            String::from_utf8_lossy(&describe_output.stderr)
        );
    }

    // Create a descendant with different content
    let new_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", "modify test file"])
        .output()?;

    if !new_output.status.success() {
        anyhow::bail!(
            "Failed to create new: {}",
            String::from_utf8_lossy(&new_output.stderr)
        );
    }

    // Modify the file in the new change
    fs::write(repo.path().join("test.txt"), "descendant content")?;

    // Describe to save changes
    let describe_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["describe", "-m", "descendant modification"])
        .output()?;

    if !describe_output.status.success() {
        anyhow::bail!(
            "Failed to describe descendant: {}",
            String::from_utf8_lossy(&describe_output.stderr)
        );
    }

    // Go back to the parent and modify it differently to create conflict
    let edit_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["edit", "@-"])
        .output()?;

    if !edit_output.status.success() {
        anyhow::bail!(
            "Failed to edit parent: {}",
            String::from_utf8_lossy(&edit_output.stderr)
        );
    }

    // Modify the same file differently
    fs::write(repo.path().join("test.txt"), "parent content")?;

    // Describe to save changes
    let describe_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["describe", "-m", "parent modification"])
        .output()?;

    if !describe_output.status.success() {
        anyhow::bail!(
            "Failed to describe parent: {}",
            String::from_utf8_lossy(&describe_output.stderr)
        );
    }

    // Try to rebase descendant onto parent - this will create a conflict
    let rebase_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["rebase", "-s", "@+", "-d", "@"])
        .output()?;

    // Rebase command may succeed but create conflicts
    eprintln!(
        "Rebase output: {}",
        String::from_utf8_lossy(&rebase_output.stderr)
    );

    // Edit the conflicted change
    let edit_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["edit", "@+"])
        .output()?;

    if !edit_output.status.success() {
        anyhow::bail!(
            "Failed to edit conflicted change: {}",
            String::from_utf8_lossy(&edit_output.stderr)
        );
    }

    // Verify we have conflicts
    let has_conflicts = jjagent::jj::has_conflicts_in(Some(repo.path()))?;
    assert!(
        has_conflicts,
        "@ should have conflicts for this test to be valid"
    );

    // Try to run PreToolUse hook - it should fail
    let output = simulator.run_hook_raw("PreToolUse", "Write")?;

    // The hook should fail
    assert!(
        !output.status.success(),
        "PreToolUse hook should fail when @ has conflicts"
    );

    // Check the error message in stdout (JSON response)
    let stdout = String::from_utf8_lossy(&output.stdout);
    eprintln!("Hook stdout: {}", stdout);

    // Parse the JSON
    let json: serde_json::Value =
        serde_json::from_str(&stdout).context("Hook stdout should contain valid JSON")?;

    // Verify the JSON structure - should not continue execution
    assert_eq!(
        json.get("continue"),
        Some(&serde_json::Value::Bool(false)),
        "JSON should have continue: false when there are conflicts"
    );

    // stopReason should be present and mention conflicts
    let stop_reason = json
        .get("stopReason")
        .and_then(|v| v.as_str())
        .context("stopReason should be present")?;

    assert!(
        stop_reason.contains("conflicts"),
        "stopReason should mention 'conflicts', got: {}",
        stop_reason
    );

    Ok(())
}

#[test]
fn test_posttool_hook_fails_with_conflicts() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;
    let session_id = "posttool-conflict-12345678";
    let simulator = ClaudeSimulator::new(repo.path(), session_id);

    // Run PreToolUse to create a precommit
    simulator.run_hook("PreToolUse", "Write")?;

    // Create a file
    fs::write(repo.path().join("test.txt"), "original content")?;

    // Create a conflict scenario similar to the pretool test
    // Create a new change with content
    let new_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", "conflicting change"])
        .output()?;

    if !new_output.status.success() {
        anyhow::bail!(
            "Failed to create new: {}",
            String::from_utf8_lossy(&new_output.stderr)
        );
    }

    // Modify the file in the new change
    fs::write(repo.path().join("test.txt"), "descendant content")?;

    // Describe to save changes
    let describe_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["describe", "-m", "descendant modification"])
        .output()?;

    if !describe_output.status.success() {
        anyhow::bail!(
            "Failed to describe descendant: {}",
            String::from_utf8_lossy(&describe_output.stderr)
        );
    }

    // Go back to the precommit and modify it differently
    let edit_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["edit", "@-"])
        .output()?;

    if !edit_output.status.success() {
        anyhow::bail!(
            "Failed to edit precommit: {}",
            String::from_utf8_lossy(&edit_output.stderr)
        );
    }

    // Modify the same file differently
    fs::write(repo.path().join("test.txt"), "precommit content")?;

    // Describe to save changes
    let describe_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["describe", "-m", "precommit modification"])
        .output()?;

    if !describe_output.status.success() {
        anyhow::bail!(
            "Failed to describe precommit: {}",
            String::from_utf8_lossy(&describe_output.stderr)
        );
    }

    // Try to rebase descendant onto precommit - this will create a conflict
    let rebase_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["rebase", "-s", "@+", "-d", "@"])
        .output()?;

    eprintln!(
        "Rebase output: {}",
        String::from_utf8_lossy(&rebase_output.stderr)
    );

    // Edit the conflicted change (which should be the precommit now)
    let edit_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["edit", "@+"])
        .output()?;

    if !edit_output.status.success() {
        anyhow::bail!(
            "Failed to edit conflicted change: {}",
            String::from_utf8_lossy(&edit_output.stderr)
        );
    }

    // Verify we have conflicts
    let has_conflicts = jjagent::jj::has_conflicts_in(Some(repo.path()))?;
    assert!(
        has_conflicts,
        "@ should have conflicts for this test to be valid"
    );

    // Try to run PostToolUse hook - it should fail
    let output = simulator.run_hook_raw("PostToolUse", "Write")?;

    // The hook should fail
    assert!(
        !output.status.success(),
        "PostToolUse hook should fail when @ has conflicts"
    );

    // Check the error in stderr
    let stderr = String::from_utf8_lossy(&output.stderr);
    eprintln!("Hook stderr: {}", stderr);

    assert!(
        stderr.contains("conflicts"),
        "Error message should mention conflicts, got: {}",
        stderr
    );

    Ok(())
}

#[test]
fn test_pretool_hook_fails_on_session_change() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;
    let session_id = "session-change-test-12345678";
    let simulator = ClaudeSimulator::new(repo.path(), session_id);

    // Create a session change
    let session_id_struct = jjagent::session::SessionId::from_full(session_id);
    jjagent::jj::create_session_change_in(&session_id_struct, Some(repo.path()))?;

    // Find the session change and edit to it
    let session_change_id =
        jjagent::jj::find_session_change_anywhere_in(session_id, Some(repo.path()))?
            .context("Session change should exist")?;

    let edit_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["edit", &session_change_id])
        .output()?;

    if !edit_output.status.success() {
        anyhow::bail!(
            "Failed to edit session change: {}",
            String::from_utf8_lossy(&edit_output.stderr)
        );
    }

    // Verify we're on a session change
    let current_session_id = jjagent::jj::get_current_commit_session_id_in(Some(repo.path()))?;
    assert!(
        current_session_id.is_some(),
        "@ should be a session change for this test to be valid"
    );

    // Try to run PreToolUse hook - it should fail
    let output = simulator.run_hook_raw("PreToolUse", "Write")?;

    // The hook should fail
    assert!(
        !output.status.success(),
        "PreToolUse hook should fail when @ is a session change"
    );

    // Check the error message in stdout (JSON response)
    let stdout = String::from_utf8_lossy(&output.stdout);
    eprintln!("Hook stdout: {}", stdout);

    // Parse the JSON
    let json: serde_json::Value =
        serde_json::from_str(&stdout).context("Hook stdout should contain valid JSON")?;

    // Verify the JSON structure - should not continue execution
    assert_eq!(
        json.get("continue"),
        Some(&serde_json::Value::Bool(false)),
        "JSON should have continue: false when @ is a session change"
    );

    // stopReason should be present and mention session change
    let stop_reason = json
        .get("stopReason")
        .and_then(|v| v.as_str())
        .context("stopReason should be present")?;

    assert!(
        stop_reason.contains("session change") && stop_reason.contains("Claude-session-id"),
        "stopReason should mention 'session change' and 'Claude-session-id', got: {}",
        stop_reason
    );

    Ok(())
}

#[test]
fn test_split_change_basic() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;
    let session_id = jjagent::session::SessionId::from_full("split-basic-12345678");

    // Create a session change
    jjagent::jj::create_session_change_in(&session_id, Some(repo.path()))?;

    // Get the session change ID
    let log_output = Command::new("jj")
        .current_dir(repo.path())
        .args([
            "log",
            "-r",
            &format!("description(glob:\"*{}*\")", session_id.short()),
            "--no-graph",
            "-T",
            "change_id.short()",
        ])
        .output()?;

    let session_change_id = String::from_utf8_lossy(&log_output.stdout)
        .trim()
        .to_string();

    // Create a commit on the session (will become the parent of @)
    Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", "commit1", &session_change_id])
        .output()?;

    std::fs::write(repo.path().join("file1.txt"), "content1")?;

    // Split at session, inserting a new change before @ (which is currently at commit1)
    jjagent::jj::split_change(&session_change_id, Some(repo.path()))?;

    // Verify: @ should have a new session part inserted between session and commit1
    let snapshot = repo.snapshot()?;
    insta::assert_snapshot!("split_change_basic", snapshot);

    Ok(())
}

#[test]
fn test_split_change_not_ancestor() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;

    // Try to split on a non-existent/non-ancestor change
    let result = jjagent::jj::split_change("nonexistent", Some(repo.path()));

    // Should fail
    assert!(
        result.is_err(),
        "split_change should fail for non-ancestor reference"
    );

    let err = result.unwrap_err();
    let err_msg = err.to_string();
    assert!(
        err_msg.contains("not an ancestor") || err_msg.contains("Failed to check ancestry"),
        "Error should mention ancestry check failure, got: {}",
        err_msg
    );

    Ok(())
}

#[test]
fn test_split_change_with_session_id() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;
    let session_id = jjagent::session::SessionId::from_full("split-sid-test-12345678");

    // Create a session change
    jjagent::jj::create_session_change_in(&session_id, Some(repo.path()))?;

    // Create a commit on the session (will become the parent of @)
    let log_output = Command::new("jj")
        .current_dir(repo.path())
        .args([
            "log",
            "-r",
            &format!("description(glob:\"*{}*\")", session_id.short()),
            "--no-graph",
            "-T",
            "change_id.short()",
        ])
        .output()?;

    let session_change_id = String::from_utf8_lossy(&log_output.stdout)
        .trim()
        .to_string();

    Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", "commit1", &session_change_id])
        .output()?;

    std::fs::write(repo.path().join("file1.txt"), "content1")?;

    // Split using the FULL SESSION ID instead of change ID
    // This tests that session ID lookup works
    jjagent::jj::split_change(session_id.full(), Some(repo.path()))?;

    // Verify: @ should have a new session part inserted between session and commit1
    let snapshot = repo.snapshot()?;
    insta::assert_snapshot!("split_change_with_session_id", snapshot);

    Ok(())
}

#[test]
fn test_split_change_with_session() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;
    let session_id = jjagent::session::SessionId::from_full("split-test-12345678");

    // Create a session change
    jjagent::jj::create_session_change_in(&session_id, Some(repo.path()))?;

    // Get the session change ID
    let log_output = Command::new("jj")
        .current_dir(repo.path())
        .args([
            "log",
            "-r",
            &format!("description(glob:\"*{}*\")", session_id.short()),
            "--no-graph",
            "-T",
            "change_id.short()",
        ])
        .output()?;

    let session_change_id = String::from_utf8_lossy(&log_output.stdout)
        .trim()
        .to_string();

    // Create a commit on the session (makes session a direct parent of @)
    Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", "commit on session", &session_change_id])
        .output()?;

    std::fs::write(repo.path().join("session_file.txt"), "session content")?;

    // Split at the session change
    jjagent::jj::split_change(&session_change_id, Some(repo.path()))?;

    // Verify the new structure
    let snapshot = repo.snapshot()?;
    insta::assert_snapshot!("split_change_with_session", snapshot);

    Ok(())
}

#[test]
fn test_move_session_into_basic() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;
    let session_id = "into-basic-12345678";

    // Create some commits: @ -> commit1 -> base
    let commit1_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", "commit1", "@-"])
        .output()?;

    if !commit1_output.status.success() {
        anyhow::bail!(
            "Failed to create commit1: {}",
            String::from_utf8_lossy(&commit1_output.stderr)
        );
    }

    // Create @ on top of commit1
    let at_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", "current"])
        .output()?;

    if !at_output.status.success() {
        anyhow::bail!(
            "Failed to create @: {}",
            String::from_utf8_lossy(&at_output.stderr)
        );
    }

    // Move session into commit1 (using @-)
    jjagent::jj::move_session_into(session_id, "@-", Some(repo.path()))?;

    // Verify: commit1 should now have the session trailer
    let snapshot = repo.snapshot()?;
    insta::assert_snapshot!("move_session_into_basic", snapshot);

    Ok(())
}

#[test]
fn test_move_session_into_ancestor() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;
    let session_id = "into-ancestor-87654321";

    // Create a deeper history: @ -> commit2 -> commit1 -> uwc -> base
    let commit1_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", "commit1", "@-"])
        .output()?;

    if !commit1_output.status.success() {
        anyhow::bail!(
            "Failed to create commit1: {}",
            String::from_utf8_lossy(&commit1_output.stderr)
        );
    }

    let commit2_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", "commit2"])
        .output()?;

    if !commit2_output.status.success() {
        anyhow::bail!(
            "Failed to create commit2: {}",
            String::from_utf8_lossy(&commit2_output.stderr)
        );
    }

    let at_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", "current"])
        .output()?;

    if !at_output.status.success() {
        anyhow::bail!(
            "Failed to create @: {}",
            String::from_utf8_lossy(&at_output.stderr)
        );
    }

    // Move session into commit1 (using @--)
    jjagent::jj::move_session_into(session_id, "@--", Some(repo.path()))?;

    // Verify: commit1 should now have the session trailer
    let snapshot = repo.snapshot()?;
    insta::assert_snapshot!("move_session_into_ancestor", snapshot);

    Ok(())
}

#[test]
fn test_move_session_into_replaces_existing_trailer() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;
    let old_session_id = "old-session-12345678";
    let new_session_id = "new-session-87654321";

    // Create a commit with an existing session trailer
    let commit_message = format!(
        "commit with old session\n\nClaude-session-id: {}",
        old_session_id
    );
    let commit_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", &commit_message, "@-"])
        .output()?;

    if !commit_output.status.success() {
        anyhow::bail!(
            "Failed to create commit: {}",
            String::from_utf8_lossy(&commit_output.stderr)
        );
    }

    // Create @ on top
    let at_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", "current"])
        .output()?;

    if !at_output.status.success() {
        anyhow::bail!(
            "Failed to create @: {}",
            String::from_utf8_lossy(&at_output.stderr)
        );
    }

    // Move new session into the commit that already has a session trailer
    jjagent::jj::move_session_into(new_session_id, "@-", Some(repo.path()))?;

    // Verify: the old session ID should be replaced with the new one
    let snapshot = repo.snapshot()?;
    insta::assert_snapshot!("move_session_into_replaces_trailer", snapshot);

    // Additionally verify the trailer was actually replaced
    let desc = jjagent::jj::get_commit_description_in("@-", Some(repo.path()))?;
    assert!(
        desc.contains(new_session_id),
        "Description should contain new session ID"
    );
    assert!(
        !desc.contains(old_session_id),
        "Description should not contain old session ID"
    );

    Ok(())
}

#[test]
fn test_move_session_into_preserves_other_trailers() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;
    let session_id = "preserve-test-12345678";

    // Create a commit with multiple trailers
    let commit_message = "commit with trailers\n\nSigned-off-by: Test User <test@example.com>\nReviewed-by: Reviewer <reviewer@example.com>";
    let commit_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", commit_message, "@-"])
        .output()?;

    if !commit_output.status.success() {
        anyhow::bail!(
            "Failed to create commit: {}",
            String::from_utf8_lossy(&commit_output.stderr)
        );
    }

    // Create @ on top
    let at_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", "current"])
        .output()?;

    if !at_output.status.success() {
        anyhow::bail!(
            "Failed to create @: {}",
            String::from_utf8_lossy(&at_output.stderr)
        );
    }

    // Move session into the commit
    jjagent::jj::move_session_into(session_id, "@-", Some(repo.path()))?;

    // Verify: other trailers should be preserved
    let snapshot = repo.snapshot()?;
    insta::assert_snapshot!("move_session_into_preserves_trailers", snapshot);

    // Additionally verify trailers are still present
    let desc = jjagent::jj::get_commit_description_in("@-", Some(repo.path()))?;
    assert!(
        desc.contains("Signed-off-by: Test User"),
        "Should preserve Signed-off-by trailer"
    );
    assert!(
        desc.contains("Reviewed-by: Reviewer"),
        "Should preserve Reviewed-by trailer"
    );
    assert!(
        desc.contains(&format!("Claude-session-id: {}", session_id)),
        "Should add Claude-session-id trailer"
    );

    Ok(())
}

#[test]
fn test_move_session_into_not_ancestor() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;
    let session_id = "fail-test-12345678";

    // Try to move session into @ itself (not an ancestor)
    let result = jjagent::jj::move_session_into(session_id, "@", Some(repo.path()));

    // Should fail
    assert!(
        result.is_err(),
        "move_session_into should fail when ref is not an ancestor"
    );

    let err = result.unwrap_err();
    let err_msg = err.to_string();
    assert!(
        err_msg.contains("not an ancestor"),
        "Error should mention 'not an ancestor', got: {}",
        err_msg
    );

    Ok(())
}

#[test]
fn test_move_session_into_with_change_id() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;
    let session_id = "changeid-test-12345678";

    // Create a commit
    let commit_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", "target commit", "@-"])
        .output()?;

    if !commit_output.status.success() {
        anyhow::bail!(
            "Failed to create commit: {}",
            String::from_utf8_lossy(&commit_output.stderr)
        );
    }

    // Get the change ID of the commit
    let change_id = jjagent::jj::get_change_id_in("@", Some(repo.path()))?;

    // Create @ on top
    let at_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", "current"])
        .output()?;

    if !at_output.status.success() {
        anyhow::bail!(
            "Failed to create @: {}",
            String::from_utf8_lossy(&at_output.stderr)
        );
    }

    // Move session using the change ID
    jjagent::jj::move_session_into(session_id, &change_id, Some(repo.path()))?;

    // Verify: the commit should now have the session trailer
    let snapshot = repo.snapshot()?;
    insta::assert_snapshot!("move_session_into_with_change_id", snapshot);

    Ok(())
}

#[test]
fn test_move_session_into_integration() -> Result<()> {
    let repo = TestRepo::new_with_uwc()?;
    let session1_id = "integration1-12345678";
    let session2_id = "integration2-87654321";

    // Simulate session 1 making changes
    let simulator1 = ClaudeSimulator::new(repo.path(), session1_id);
    simulator1.write_file("session1.txt", "session 1 content")?;

    // Simulate session 2 making changes
    let simulator2 = ClaudeSimulator::new(repo.path(), session2_id);
    simulator2.write_file("session2.txt", "session 2 content")?;

    // Now we have: @ uwc -> session2 -> session1 -> base
    // Let's say we want to retroactively mark an older commit as belonging to a new session

    // First, let's get the base commit change ID while we're at @
    let base_change_id = jjagent::jj::get_change_id_in("@---", Some(repo.path()))?;

    // Create a new commit inserted AFTER base (using --insert-after to keep it in the chain)
    // This inserts the commit between base and session1, keeping the linear history
    let commit_output = Command::new("jj")
        .current_dir(repo.path())
        .args([
            "new",
            "--insert-after",
            &base_change_id,
            "--no-edit",
            "-m",
            "manual commit",
        ])
        .output()?;

    if !commit_output.status.success() {
        anyhow::bail!(
            "Failed to create commit: {}",
            String::from_utf8_lossy(&commit_output.stderr)
        );
    }

    // Find the manual commit we just created (it's now between base and session1)
    let manual_output = Command::new("jj")
        .current_dir(repo.path())
        .args([
            "log",
            "-r",
            "description(substring:\"manual commit\")",
            "--no-graph",
            "-T",
            "change_id.short()",
        ])
        .output()?;

    let manual_change_id = String::from_utf8_lossy(&manual_output.stdout)
        .trim()
        .to_string();

    // Move session into the manual commit
    let new_session_id = "retroactive-12345678";
    jjagent::jj::move_session_into(new_session_id, &manual_change_id, Some(repo.path()))?;

    // Verify the final state
    let snapshot = repo.snapshot()?;
    insta::assert_snapshot!("move_session_into_integration", snapshot);

    Ok(())
}
