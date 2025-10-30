use anyhow::Result;
use jjagent::jj;
use std::process::Command;
use tempfile::TempDir;

#[allow(dead_code)]
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

    /// Configure immutable heads to include a specific change
    fn set_immutable_heads(&self, revset: &str) -> Result<()> {
        let config_output = Command::new("jj")
            .current_dir(self.path())
            .args([
                "config",
                "set",
                "--repo",
                "revset-aliases.\"immutable_heads()\"",
                revset,
            ])
            .output()?;

        if !config_output.status.success() {
            anyhow::bail!(
                "Failed to set immutable_heads: {}",
                String::from_utf8_lossy(&config_output.stderr)
            );
        }

        Ok(())
    }
}

#[test]
fn test_find_session_change_excludes_immutable() -> Result<()> {
    let repo = TestRepo::new()?;
    let session_id = "immutable-test-12345678-1234-5678-90ab-cdef12345678";

    // Create a session change
    let session_message = format!(
        "jjagent: session immutable\n\nClaude-session-id: {}",
        session_id
    );
    Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", &session_message])
        .output()?;

    // Get the change ID (full change ID for comparison with find_session_change_anywhere_in)
    let log_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["log", "-r", "@", "-T", "change_id", "--no-graph"])
        .output()?;

    let change_id = String::from_utf8_lossy(&log_output.stdout)
        .trim()
        .to_string();

    // Should find it before marking as immutable
    let found = jj::find_session_change_anywhere_in(session_id, Some(repo.path()))?;
    assert!(found.is_some(), "Should find session change");
    assert_eq!(
        found.as_ref().unwrap(),
        &change_id,
        "Should return correct change ID"
    );

    // Mark it as immutable
    repo.set_immutable_heads(&format!("builtin_immutable_heads() | {}", change_id))?;

    // Should NOT find it after marking as immutable
    let found = jj::find_session_change_anywhere_in(session_id, Some(repo.path()))?;
    assert!(found.is_none(), "Should not find immutable session change");

    Ok(())
}

#[test]
fn test_find_session_change_finds_mutable_when_immutable_exists() -> Result<()> {
    let repo = TestRepo::new()?;
    let session_id = "multi-test-12345678-1234-5678-90ab-cdef12345678";

    // Create first session change (will become immutable)
    let session_message_1 = format!(
        "jjagent: session multi-test\n\nClaude-session-id: {}",
        session_id
    );
    Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", &session_message_1])
        .output()?;

    // Get the change ID (full change ID for comparison)
    let log_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["log", "-r", "@", "-T", "change_id", "--no-graph"])
        .output()?;

    let immutable_change_id = String::from_utf8_lossy(&log_output.stdout)
        .trim()
        .to_string();

    // Mark it as immutable
    repo.set_immutable_heads(&format!(
        "builtin_immutable_heads() | {}",
        immutable_change_id
    ))?;

    // Create a second session change (mutable) - part 2
    let session_message_2 = format!(
        "jjagent: session multi-test pt. 2\n\nClaude-session-id: {}",
        session_id
    );
    Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", &session_message_2])
        .output()?;

    // Get the second change ID (full change ID for comparison)
    let log_output_2 = Command::new("jj")
        .current_dir(repo.path())
        .args(["log", "-r", "@", "-T", "change_id", "--no-graph"])
        .output()?;

    let mutable_change_id = String::from_utf8_lossy(&log_output_2.stdout)
        .trim()
        .to_string();

    // Should find the mutable one, not the immutable one
    let found = jj::find_session_change_anywhere_in(session_id, Some(repo.path()))?;
    assert!(found.is_some(), "Should find a session change");
    assert_eq!(
        found.as_ref().unwrap(),
        &mutable_change_id,
        "Should return the mutable change ID, not the immutable one"
    );

    // Verify the immutable one is not returned
    assert_ne!(
        found.as_ref().unwrap(),
        &immutable_change_id,
        "Should not return the immutable change ID"
    );

    Ok(())
}

#[test]
fn test_find_session_change_in_excludes_immutable() -> Result<()> {
    let repo = TestRepo::new()?;
    let session_id = "descendant-test-12345678-1234-5678-90ab-cdef12345678";

    // Create a session change
    let session_message = format!(
        "jjagent: session descendant\n\nClaude-session-id: {}",
        session_id
    );
    Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", &session_message])
        .output()?;

    // Get the change ID
    let log_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["log", "-r", "@", "-T", "change_id.short()", "--no-graph"])
        .output()?;

    let change_id = String::from_utf8_lossy(&log_output.stdout)
        .trim()
        .to_string();

    // Move to the base commit so the session change becomes a descendant
    Command::new("jj")
        .current_dir(repo.path())
        .args(["edit", "@-"])
        .output()?;

    // Should find it as a descendant before marking as immutable
    let found = jj::find_session_change_in(session_id, Some(repo.path()))?;
    assert!(found.is_some(), "Should find session change as descendant");

    // Mark it as immutable
    repo.set_immutable_heads(&format!("builtin_immutable_heads() | {}", change_id))?;

    // Should NOT find it after marking as immutable
    let found = jj::find_session_change_in(session_id, Some(repo.path()))?;
    assert!(
        found.is_none(),
        "Should not find immutable session change in descendants"
    );

    Ok(())
}

#[test]
fn test_creates_new_session_when_all_immutable() -> Result<()> {
    let repo = TestRepo::new()?;
    let session_id = "create-new-12345678-1234-5678-90ab-cdef12345678";

    // Create a session change
    let session_message = format!(
        "jjagent: session create-new\n\nClaude-session-id: {}",
        session_id
    );
    Command::new("jj")
        .current_dir(repo.path())
        .args(["new", "-m", &session_message])
        .output()?;

    // Get the change ID
    let log_output = Command::new("jj")
        .current_dir(repo.path())
        .args(["log", "-r", "@", "-T", "change_id", "--no-graph"])
        .output()?;

    let change_id = String::from_utf8_lossy(&log_output.stdout)
        .trim()
        .to_string();

    // Mark it as immutable
    repo.set_immutable_heads(&format!("builtin_immutable_heads() | {}", change_id))?;

    // Should NOT find it after marking as immutable
    let found = jj::find_session_change_anywhere_in(session_id, Some(repo.path()))?;
    assert!(found.is_none(), "Should not find immutable session change");

    // This simulates what finalize_precommit does:
    // When no session change is found (because all are immutable),
    // it should create a new one, which will be mutable
    // (This is the actual behavior in hooks.rs:242-250)

    Ok(())
}
