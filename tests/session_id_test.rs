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
fn test_session_id_command_finds_session() -> Result<()> {
    let repo = TestRepo::new()?;
    let session_id = "test-session-12345678-1234-5678-90ab-cdef12345678";

    // Create a commit with a Claude-session-id trailer
    let message = format!(
        "jjagent: session test-ses\n\nClaude-session-id: {}",
        session_id
    );
    let new_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", &message])
        .output()?;

    if !new_output.status.success() {
        anyhow::bail!(
            "Failed to create session change: {}",
            String::from_utf8_lossy(&new_output.stderr)
        );
    }

    // Run the session-id command
    let output = Command::new(env!("CARGO_BIN_EXE_jjagent"))
        .current_dir(repo.path())
        .env_remove("JJAGENT_DISABLE")
        .args(["session-id", "@"])
        .output()?;

    assert!(
        output.status.success(),
        "session-id command should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let actual_session_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert_eq!(
        actual_session_id, session_id,
        "session-id command should return the correct session ID"
    );

    Ok(())
}

#[test]
fn test_session_id_command_not_found() -> Result<()> {
    let repo = TestRepo::new()?;

    // Create a commit without a Claude-session-id trailer
    Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", "regular commit without trailer"])
        .output()?;

    // Run the session-id command
    let output = Command::new(env!("CARGO_BIN_EXE_jjagent"))
        .current_dir(repo.path())
        .env_remove("JJAGENT_DISABLE")
        .args(["session-id", "@"])
        .output()?;

    assert!(
        !output.status.success(),
        "session-id command should fail when no trailer found"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("No Claude-session-id trailer found"),
        "Error message should mention that no trailer was found, got: {}",
        stderr
    );

    Ok(())
}

#[test]
fn test_session_id_command_multiple_trailers_returns_last() -> Result<()> {
    let repo = TestRepo::new()?;
    let session_id_1 = "first-session-11111111-1111-1111-1111-111111111111";
    let session_id_2 = "second-session-22222222-2222-2222-2222-222222222222";
    let session_id_3 = "third-session-33333333-3333-3333-3333-333333333333";

    // Create a commit with multiple Claude-session-id trailers
    let message = format!(
        "jjagent: session multi\n\nClaude-session-id: {}\nClaude-session-id: {}\nClaude-session-id: {}",
        session_id_1, session_id_2, session_id_3
    );
    Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", &message])
        .output()?;

    // Run the session-id command
    let output = Command::new(env!("CARGO_BIN_EXE_jjagent"))
        .current_dir(repo.path())
        .env_remove("JJAGENT_DISABLE")
        .args(["session-id", "@"])
        .output()?;

    assert!(
        output.status.success(),
        "session-id command should succeed with multiple trailers, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let actual_session_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert_eq!(
        actual_session_id, session_id_3,
        "session-id command should return the LAST session ID when multiple exist"
    );

    Ok(())
}

#[test]
fn test_session_id_command_with_change_id() -> Result<()> {
    let repo = TestRepo::new()?;
    let session_id = "changeid-test-12345678-1234-5678-90ab-cdef12345678";

    // Create a commit with a Claude-session-id trailer
    let message = format!(
        "jjagent: session change\n\nClaude-session-id: {}",
        session_id
    );
    Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", &message])
        .output()?;

    // Get the change ID
    let log_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["log", "-r", "@", "-T", "change_id.short()", "--no-graph"])
        .output()?;
    let change_id = String::from_utf8_lossy(&log_output.stdout)
        .trim()
        .to_string();

    // Create another commit on top
    Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", "another commit"])
        .output()?;

    // Run the session-id command with the change ID
    let output = Command::new(env!("CARGO_BIN_EXE_jjagent"))
        .current_dir(repo.path())
        .env_remove("JJAGENT_DISABLE")
        .args(["session-id", &change_id])
        .output()?;

    assert!(
        output.status.success(),
        "session-id command should work with change ID, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let actual_session_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert_eq!(
        actual_session_id, session_id,
        "session-id command should return correct session ID when using change ID"
    );

    Ok(())
}

#[test]
fn test_session_id_command_default_rev() -> Result<()> {
    let repo = TestRepo::new()?;
    let session_id = "default-test-12345678-1234-5678-90ab-cdef12345678";

    // Create a commit with a Claude-session-id trailer
    let message = format!(
        "jjagent: session default\n\nClaude-session-id: {}",
        session_id
    );
    Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", &message])
        .output()?;

    // Run the session-id command without specifying a rev (should default to @)
    let output = Command::new(env!("CARGO_BIN_EXE_jjagent"))
        .current_dir(repo.path())
        .env_remove("JJAGENT_DISABLE")
        .args(["session-id"])
        .output()?;

    assert!(
        output.status.success(),
        "session-id command should succeed with default rev, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let actual_session_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert_eq!(
        actual_session_id, session_id,
        "session-id command should return correct session ID with default rev (@)"
    );

    Ok(())
}
