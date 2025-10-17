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
fn test_describe_preserves_trailers() -> Result<()> {
    let repo = TestRepo::new()?;
    let session_id = "test-session-12345678-1234-5678-90ab-cdef12345678";

    // Create a session change with trailer
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

    // Update the description with a new message
    let new_message = "Add new feature for testing\n\nThis is a more detailed commit message\nthat spans multiple lines.";
    let output = Command::new(env!("CARGO_BIN_EXE_jjagent"))
        .current_dir(repo.path())
        .env_remove("JJAGENT_DISABLE")
        .args(["describe", session_id, "-m", new_message])
        .output()?;

    assert!(
        output.status.success(),
        "describe command should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify the description was updated but trailer preserved
    let desc_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["log", "-r", "@", "-T", "description", "--no-graph"])
        .output()?;

    let desc_text = String::from_utf8_lossy(&desc_output.stdout);

    // Check that the new message is present
    assert!(
        desc_text.contains("Add new feature for testing"),
        "New commit message should be present, got: {}",
        desc_text
    );
    assert!(
        desc_text.contains("This is a more detailed commit message"),
        "New commit body should be present, got: {}",
        desc_text
    );

    // Check that the trailer is still there
    assert!(
        desc_text.contains(&format!("Claude-session-id: {}", session_id)),
        "Trailer should be preserved, got: {}",
        desc_text
    );

    // Make sure old message is gone
    assert!(
        !desc_text.contains("jjagent: session test-ses"),
        "Old message should be replaced"
    );

    Ok(())
}

#[test]
fn test_describe_with_multiple_trailers() -> Result<()> {
    let repo = TestRepo::new()?;
    let session_id = "multi-trailer-12345678-1234-5678-90ab-cdef12345678";

    // Create a session change with multiple trailers
    let session_message = format!(
        "Original message\n\nSigned-off-by: Test User <test@example.com>\nClaude-session-id: {}",
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

    // Update the description
    let new_message = "Updated commit message";
    let output = Command::new(env!("CARGO_BIN_EXE_jjagent"))
        .current_dir(repo.path())
        .env_remove("JJAGENT_DISABLE")
        .args(["describe", session_id, "-m", new_message])
        .output()?;

    assert!(
        output.status.success(),
        "describe command should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify both trailers are preserved
    let show_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["show", "@"])
        .output()?;

    let show_text = String::from_utf8_lossy(&show_output.stdout);

    assert!(
        show_text.contains("Updated commit message"),
        "New message should be present"
    );
    assert!(
        show_text.contains("Signed-off-by: Test User <test@example.com>"),
        "First trailer should be preserved"
    );
    assert!(
        show_text.contains(&format!("Claude-session-id: {}", session_id)),
        "Second trailer should be preserved"
    );

    Ok(())
}

#[test]
fn test_describe_session_not_found() -> Result<()> {
    let repo = TestRepo::new()?;
    let session_id = "nonexistent-session-12345678";

    // Try to describe a session that doesn't exist
    let output = Command::new(env!("CARGO_BIN_EXE_jjagent"))
        .current_dir(repo.path())
        .env_remove("JJAGENT_DISABLE")
        .args(["describe", session_id, "-m", "New message"])
        .output()?;

    assert!(
        !output.status.success(),
        "describe command should fail when session not found"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("No change found"),
        "Error message should mention that no change was found, got: {}",
        stderr
    );

    Ok(())
}

#[test]
fn test_describe_without_existing_trailers() -> Result<()> {
    let repo = TestRepo::new()?;
    let session_id = "no-trailers-12345678-1234-5678-90ab-cdef12345678";

    // Create a regular commit first, then manually add a trailer to make it a session
    let new_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", "Initial message"])
        .output()?;

    if !new_output.status.success() {
        anyhow::bail!(
            "Failed to create commit: {}",
            String::from_utf8_lossy(&new_output.stderr)
        );
    }

    // Add the session trailer
    let message_with_trailer = format!("Initial message\n\nClaude-session-id: {}", session_id);
    let describe_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["describe", "-m", &message_with_trailer])
        .output()?;

    if !describe_output.status.success() {
        anyhow::bail!(
            "Failed to add trailer: {}",
            String::from_utf8_lossy(&describe_output.stderr)
        );
    }

    // Now use jjagent describe
    let new_message = "New message without trailers in input";
    let output = Command::new(env!("CARGO_BIN_EXE_jjagent"))
        .current_dir(repo.path())
        .env_remove("JJAGENT_DISABLE")
        .args(["describe", session_id, "-m", new_message])
        .output()?;

    assert!(
        output.status.success(),
        "describe command should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify the trailer is still there
    let show_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["show", "@"])
        .output()?;

    let show_text = String::from_utf8_lossy(&show_output.stdout);

    assert!(
        show_text.contains(new_message),
        "New message should be present"
    );
    assert!(
        show_text.contains(&format!("Claude-session-id: {}", session_id)),
        "Trailer should be added/preserved"
    );

    Ok(())
}

#[test]
fn test_describe_finds_session_in_history() -> Result<()> {
    let repo = TestRepo::new()?;
    let session_id = "history-session-12345678-1234-5678-90ab-cdef12345678";

    // Create a session change
    let session_message = format!(
        "Initial session message\n\nClaude-session-id: {}",
        session_id
    );
    Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", &session_message])
        .output()?;

    // Create more commits on top
    Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", "another commit"])
        .output()?;

    // Now describe the session change that's in history
    let new_message = "Updated session message";
    let output = Command::new(env!("CARGO_BIN_EXE_jjagent"))
        .current_dir(repo.path())
        .env_remove("JJAGENT_DISABLE")
        .args(["describe", session_id, "-m", new_message])
        .output()?;

    assert!(
        output.status.success(),
        "describe command should find session in history, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify the session change was updated
    let log_output = Command::new("jj")
        .current_dir(repo.path())
        .args([
            "log",
            "-r",
            &format!(r#"description(glob:"*{}*")"#, session_id),
            "-T",
            "description",
            "--no-graph",
        ])
        .output()?;

    let log_text = String::from_utf8_lossy(&log_output.stdout);

    assert!(
        log_text.contains(new_message),
        "Updated message should be in the session change, got: {}",
        log_text
    );
    assert!(
        log_text.contains(&format!("Claude-session-id: {}", session_id)),
        "Trailer should be preserved, got: {}",
        log_text
    );

    Ok(())
}
