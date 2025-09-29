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
            .args(["config", "set", "--repo", "core.fsmonitor", "none"])
            .output()?;

        if !config_output.status.success() {
            anyhow::bail!(
                "Failed to disable watchman: {}",
                String::from_utf8_lossy(&config_output.stderr)
            );
        }

        Ok(Self { dir })
    }

    fn run_jjagent(&self, args: &[&str]) -> Result<std::process::Output> {
        let jjagent_binary = env!("CARGO_BIN_EXE_jjagent");

        Ok(Command::new(jjagent_binary)
            .current_dir(self.dir.path())
            .args(args)
            .output()?)
    }

    fn get_current_description(&self) -> Result<String> {
        let output = Command::new("jj")
            .current_dir(self.dir.path())
            .args(["log", "-r", "@", "--no-graph", "-T", "description"])
            .output()?;

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

#[test]
fn test_issue_command_requires_message() -> Result<()> {
    let repo = TestRepo::new()?;

    // Run issue command without message should fail
    let output = repo.run_jjagent(&["claude", "issue"])?;

    assert!(
        !output.status.success(),
        "Issue command should fail without message"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("required") || stderr.contains("message"),
        "Error should mention required message"
    );

    Ok(())
}

#[test]
fn test_issue_command_with_message() -> Result<()> {
    let repo = TestRepo::new()?;

    // Run issue command with message
    let output = repo.run_jjagent(&["claude", "issue", "-m", "Test feature work"])?;

    assert!(output.status.success(), "Issue command should succeed");

    // Get the session ID
    let session_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert_eq!(session_id.len(), 36, "Session ID should be a UUID");

    // The current working copy should remain unchanged (original), but we should
    // have created a new change before it
    // Let's check that the session commit exists
    let log_output = Command::new("jj")
        .current_dir(repo.dir.path())
        .args([
            "log",
            "-r",
            &format!("description(glob:'*Claude-session-id: {}*')", session_id),
            "--no-graph",
            "-T",
            "description",
        ])
        .output()?;

    assert!(log_output.status.success());
    let found_desc = String::from_utf8_lossy(&log_output.stdout);
    assert!(
        found_desc.contains("Test feature work"),
        "Description should contain the message"
    );
    assert!(
        found_desc.contains(&format!("Claude-session-id: {}", session_id)),
        "Description should contain session ID trailer"
    );

    Ok(())
}

#[test]
fn test_settings_command() -> Result<()> {
    let repo = TestRepo::new()?;

    // Run settings command
    let output = repo.run_jjagent(&["settings"])?;

    assert!(output.status.success(), "Settings command should succeed");

    // Parse the JSON output
    let settings_json = String::from_utf8_lossy(&output.stdout);
    let settings: serde_json::Value = serde_json::from_str(&settings_json)?;

    // Check that it has the hooks structure
    assert!(
        settings.get("hooks").is_some(),
        "Settings should contain hooks"
    );

    let hooks = &settings["hooks"];
    assert!(
        hooks.get("UserPromptSubmit").is_some(),
        "Should have UserPromptSubmit hook"
    );
    assert!(
        hooks.get("PreToolUse").is_some(),
        "Should have PreToolUse hook"
    );
    assert!(
        hooks.get("PostToolUse").is_some(),
        "Should have PostToolUse hook"
    );
    assert!(
        hooks.get("SessionEnd").is_some(),
        "Should have SessionEnd hook"
    );

    // Verify that each hook contains the jjagent command
    for hook_name in &[
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
        "SessionEnd",
    ] {
        let hook_config = &hooks[hook_name][0]["hooks"][0];
        assert_eq!(hook_config["type"], "command");
        let command = hook_config["command"].as_str().unwrap();
        assert!(
            command.contains("jjagent"),
            "Hook command should contain jjagent"
        );
        assert!(
            command.contains("claude hooks"),
            "Hook command should contain 'claude hooks'"
        );
        assert!(
            command.contains(hook_name),
            "Hook command should contain the hook name"
        );
    }

    Ok(())
}

#[test]
fn test_settings_command_outside_jj_repo() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let jjagent_binary = env!("CARGO_BIN_EXE_jjagent");

    // Run settings command outside a jj repo
    let output = Command::new(jjagent_binary)
        .current_dir(temp_dir.path())
        .args(["settings"])
        .output()?;

    // Settings should work even outside a jj repo
    assert!(
        output.status.success(),
        "Settings command should succeed outside jj repo"
    );

    // Should still output valid JSON
    let settings_json = String::from_utf8_lossy(&output.stdout);
    let _settings: serde_json::Value = serde_json::from_str(&settings_json)?;

    Ok(())
}

#[test]
fn test_issue_command_outside_jj_repo() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let jjagent_binary = env!("CARGO_BIN_EXE_jjagent");

    // Run issue command outside a jj repo
    let output = Command::new(jjagent_binary)
        .current_dir(temp_dir.path())
        .args(["claude", "issue", "-m", "Test message"])
        .output()?;

    // Issue command should exit with error message but success code due to claude subcommand behavior
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Not in a jj repository"),
        "Should indicate not in jj repo"
    );

    Ok(())
}

#[test]
fn test_issue_command_with_multiline_message() -> Result<()> {
    let repo = TestRepo::new()?;

    // Run issue command with multiline message
    let output = repo.run_jjagent(&[
        "claude",
        "issue",
        "-m",
        "Feature: User authentication\nTask: Add login endpoint",
    ])?;

    assert!(output.status.success(), "Issue command should succeed");

    let session_id = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // Check that the commit was created with proper formatting
    let log_output = Command::new("jj")
        .current_dir(repo.dir.path())
        .args([
            "log",
            "-r",
            &format!("description(glob:'*Claude-session-id: {}*')", session_id),
            "--no-graph",
            "-T",
            "description",
        ])
        .output()?;

    let found_desc = String::from_utf8_lossy(&log_output.stdout);
    assert!(
        found_desc.contains("Feature: User authentication"),
        "Should contain first line"
    );
    assert!(
        found_desc.contains("Task: Add login endpoint"),
        "Should contain second line"
    );
    assert!(
        found_desc.contains(&format!("Claude-session-id: {}", session_id)),
        "Should contain session ID trailer"
    );

    Ok(())
}

#[test]
fn test_issue_command_respects_jjagent_disable() -> Result<()> {
    let repo = TestRepo::new()?;
    let jjagent_binary = env!("CARGO_BIN_EXE_jjagent");

    // Run issue command with JJAGENT_DISABLE=1
    let output = Command::new(jjagent_binary)
        .current_dir(repo.dir.path())
        .env("JJAGENT_DISABLE", "1")
        .args(["claude", "issue", "-m", "Test message"])
        .output()?;

    assert!(
        output.status.success(),
        "Should exit successfully when disabled"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Disabled via JJAGENT_DISABLE=1"));

    // Should not output a session ID
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.trim().is_empty(),
        "Should not output anything when disabled"
    );

    Ok(())
}
