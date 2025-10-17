use anyhow::Result;
use std::process::Command;
use tempfile::TempDir;

struct TestRepo {
    dir: TempDir,
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

    fn path(&self) -> &std::path::Path {
        self.dir.path()
    }
}

#[test]
fn test_change_id_command_finds_session() -> Result<()> {
    let repo = TestRepo::new()?;
    let session_id = "test-session-12345678-1234-5678-90ab-cdef12345678";

    // Create a session change with the session ID in the trailer
    let session_message = format!(
        "jjagent: session test-ses\n\nClaude-session-id: {}",
        session_id
    );
    let new_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", &session_message])
        .output()?;

    if !new_output.status.success() {
        anyhow::bail!(
            "Failed to create session change: {}",
            String::from_utf8_lossy(&new_output.stderr)
        );
    }

    // Get the change ID that was created
    let log_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["log", "-r", "@", "-T", "change_id", "--no-graph"])
        .output()?;

    let expected_change_id = String::from_utf8_lossy(&log_output.stdout)
        .trim()
        .to_string();

    // Run the change-id command
    let output = Command::new(env!("CARGO_BIN_EXE_jjagent"))
        .current_dir(repo.path())
        .env_remove("JJAGENT_DISABLE")
        .args(["change-id", session_id])
        .output()?;

    assert!(
        output.status.success(),
        "change-id command should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let actual_change_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert_eq!(
        actual_change_id, expected_change_id,
        "change-id command should return the correct change ID"
    );

    Ok(())
}

#[test]
fn test_change_id_command_not_found() -> Result<()> {
    let repo = TestRepo::new()?;
    let session_id = "nonexistent-session-12345678";

    // Run the change-id command for a session that doesn't exist
    let output = Command::new(env!("CARGO_BIN_EXE_jjagent"))
        .current_dir(repo.path())
        .env_remove("JJAGENT_DISABLE")
        .args(["change-id", session_id])
        .output()?;

    assert!(
        !output.status.success(),
        "change-id command should fail when session not found"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("No change found for session ID"),
        "Error message should mention that no change was found, got: {}",
        stderr
    );

    Ok(())
}

#[test]
fn test_change_id_command_multiple_parts() -> Result<()> {
    let repo = TestRepo::new()?;
    let session_id = "multi-part-12345678-1234-5678-90ab-cdef12345678";

    // Create session change (part 1)
    let session_message_1 = format!(
        "jjagent: session multi-pa\n\nClaude-session-id: {}",
        session_id
    );
    Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", &session_message_1])
        .output()?;

    // Create session part 2 with the same session ID
    let session_message_2 = format!(
        "jjagent: session multi-pa pt. 2\n\nClaude-session-id: {}",
        session_id
    );
    Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", &session_message_2])
        .output()?;

    // Run the change-id command - should find one of the commits (first match)
    let output = Command::new(env!("CARGO_BIN_EXE_jjagent"))
        .current_dir(repo.path())
        .env_remove("JJAGENT_DISABLE")
        .args(["change-id", session_id])
        .output()?;

    assert!(
        output.status.success(),
        "change-id command should succeed even with multiple parts, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let change_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert!(
        !change_id.is_empty(),
        "change-id should return a non-empty change ID"
    );

    Ok(())
}

#[test]
fn test_change_id_command_finds_in_history() -> Result<()> {
    let repo = TestRepo::new()?;
    let session_id = "history-test-12345678-1234-5678-90ab-cdef12345678";

    // Create a session change
    let session_message = format!(
        "jjagent: session history-\n\nClaude-session-id: {}",
        session_id
    );
    Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", &session_message])
        .output()?;

    // Get the session change ID
    let log_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["log", "-r", "@", "-T", "change_id", "--no-graph"])
        .output()?;

    let session_change_id = String::from_utf8_lossy(&log_output.stdout)
        .trim()
        .to_string();

    // Create more commits on top
    Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", "another commit"])
        .output()?;

    Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", "yet another commit"])
        .output()?;

    // Run the change-id command - should still find the session change in history
    let output = Command::new(env!("CARGO_BIN_EXE_jjagent"))
        .current_dir(repo.path())
        .env_remove("JJAGENT_DISABLE")
        .args(["change-id", session_id])
        .output()?;

    assert!(
        output.status.success(),
        "change-id command should find session in history, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let found_change_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert_eq!(
        found_change_id, session_change_id,
        "change-id should find the correct session change even with commits on top"
    );

    Ok(())
}
