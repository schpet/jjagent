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

    fn run_hook(&self, hook: &str, tool_name: Option<&str>) -> Result<String> {
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

        let mut cmd = Command::new(jjagent_binary);
        cmd.current_dir(self.dir.path())
            .env_remove("JJAGENT_DISABLE")
            .args(["claude", "hooks", hook])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn()?;

        // Write input
        if let Some(stdin) = child.stdin.as_mut() {
            use std::io::Write;
            stdin.write_all(serde_json::to_string(&input)?.as_bytes())?;
        }

        let output = child.wait_with_output()?;

        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("Hook stderr: {}", stderr);

        if !output.status.success() {
            eprintln!("Hook failed with status: {:?}", output.status);
            eprintln!("Stdout: {}", String::from_utf8_lossy(&output.stdout));
        }

        Ok(stderr.to_string())
    }

    fn get_current_description(&self) -> Result<String> {
        let output = Command::new("jj")
            .current_dir(self.dir.path())
            .args(["log", "-r", "@", "--no-graph", "-T", "description"])
            .output()?;

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn is_on_temporary_change(&self) -> Result<bool> {
        let desc = self.get_current_description()?;
        Ok(desc.contains("Claude-temp-change:"))
    }
}

#[test]
fn test_file_modifying_tools_create_temp_change() -> Result<()> {
    let repo = TestRepo::new()?;

    // Test Edit tool - should create temp change
    repo.run_hook("PreToolUse", Some("Edit"))?;
    assert!(
        repo.is_on_temporary_change()?,
        "Edit tool should create temporary change"
    );

    Ok(())
}

#[test]
fn test_non_file_modifying_tools_skip_temp_change() -> Result<()> {
    let repo = TestRepo::new()?;

    // Test Bash tool - should NOT create temp change
    let stderr = repo.run_hook("PreToolUse", Some("Bash"))?;
    assert!(
        stderr.contains("Skipping temporary change for non-file-modifying tool: Bash"),
        "Bash tool should skip temporary change creation"
    );
    assert!(
        !repo.is_on_temporary_change()?,
        "Bash tool should not create temporary change"
    );

    Ok(())
}

#[test]
fn test_read_tool_skips_temp_change() -> Result<()> {
    let repo = TestRepo::new()?;

    // Test Read tool - should NOT create temp change
    let stderr = repo.run_hook("PreToolUse", Some("Read"))?;
    assert!(
        stderr.contains("Skipping temporary change for non-file-modifying tool: Read"),
        "Read tool should skip temporary change creation"
    );
    assert!(
        !repo.is_on_temporary_change()?,
        "Read tool should not create temporary change"
    );

    Ok(())
}

#[test]
fn test_grep_tool_skips_temp_change() -> Result<()> {
    let repo = TestRepo::new()?;

    // Test Grep tool - should NOT create temp change
    let stderr = repo.run_hook("PreToolUse", Some("Grep"))?;
    assert!(
        stderr.contains("Skipping temporary change for non-file-modifying tool: Grep"),
        "Grep tool should skip temporary change creation"
    );
    assert!(
        !repo.is_on_temporary_change()?,
        "Grep tool should not create temporary change"
    );

    Ok(())
}

#[test]
fn test_write_tool_creates_temp_change() -> Result<()> {
    let repo = TestRepo::new()?;

    // Test Write tool - should create temp change
    repo.run_hook("PreToolUse", Some("Write"))?;
    assert!(
        repo.is_on_temporary_change()?,
        "Write tool should create temporary change"
    );

    Ok(())
}

#[test]
fn test_multi_edit_tool_creates_temp_change() -> Result<()> {
    let repo = TestRepo::new()?;

    // Test MultiEdit tool - should create temp change
    repo.run_hook("PreToolUse", Some("MultiEdit"))?;
    assert!(
        repo.is_on_temporary_change()?,
        "MultiEdit tool should create temporary change"
    );

    Ok(())
}

#[test]
fn test_notebook_edit_tool_creates_temp_change() -> Result<()> {
    let repo = TestRepo::new()?;

    // Test NotebookEdit tool - should create temp change
    repo.run_hook("PreToolUse", Some("NotebookEdit"))?;
    assert!(
        repo.is_on_temporary_change()?,
        "NotebookEdit tool should create temporary change"
    );

    Ok(())
}

#[test]
fn test_post_tool_use_skips_for_non_file_tools() -> Result<()> {
    let repo = TestRepo::new()?;

    // Run PostToolUse for Bash - should skip processing
    let stderr = repo.run_hook("PostToolUse", Some("Bash"))?;
    assert!(
        stderr.contains("Skipping post-processing for non-file-modifying tool: Bash"),
        "PostToolUse should skip for Bash tool"
    );

    // Verify we're still on original commit (not temporary)
    assert!(
        !repo.is_on_temporary_change()?,
        "Should not be on temporary change after Bash PostToolUse"
    );

    Ok(())
}

#[test]
fn test_unknown_tool_defaults_to_creating_temp_change() -> Result<()> {
    let repo = TestRepo::new()?;

    // Test with no tool_name - should create temp change (default behavior)
    repo.run_hook("PreToolUse", None)?;
    assert!(
        repo.is_on_temporary_change()?,
        "Missing tool_name should default to creating temporary change"
    );

    Ok(())
}
