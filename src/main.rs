use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::Deserialize;
use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "jjcc")]
#[command(about = "JJ Claude Code - Manage jj changesets for Claude sessions")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Claude Code hooks for jj integration
    #[command(subcommand)]
    Hooks(HookCommands),
    /// Session management commands
    #[command(subcommand)]
    Session(SessionCommands),
}

#[derive(Subcommand)]
enum SessionCommands {
    /// Split a session commit to continue work in a new commit
    Split {
        /// The session UUID to split
        session_id: String,
        /// Custom description for the new split commit
        #[arg(short = 'm', long = "description", value_name = "MESSAGE")]
        description: Option<String>,
    },
}

#[derive(Subcommand)]
enum HookCommands {
    /// Handle UserPromptSubmit hook
    #[command(name = "UserPromptSubmit")]
    UserPromptSubmit,
    /// Handle PreToolUse hook
    #[command(name = "PreToolUse")]
    PreToolUse,
    /// Handle PostToolUse hook
    #[command(name = "PostToolUse")]
    PostToolUse,
    /// Handle Stop hook (no-op for backwards compatibility)
    #[command(name = "Stop")]
    Stop,
    /// Handle SessionEnd hook
    #[command(name = "SessionEnd")]
    SessionEnd,
}

#[derive(Debug, Deserialize)]
struct HookInput {
    session_id: String,
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    _tool_name: Option<String>,
    #[serde(default)]
    _tool_input: Option<serde_json::Value>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if env::var("JJCC_DISABLE").unwrap_or_default() == "1" {
        eprintln!("jjcc: Disabled via JJCC_DISABLE=1");
        return Ok(());
    }

    match cli.command {
        Commands::Session(session_cmd) => {
            // Check if we're in a jj repository
            if !is_jj_repo() {
                anyhow::bail!("Not in a jj repository");
            }

            match session_cmd {
                SessionCommands::Split {
                    session_id,
                    description,
                } => {
                    jjcc::session_split(&session_id, description.as_deref())?;
                }
            }
        }
        Commands::Hooks(hook_cmd) => {
            // Check if we're in a jj repository
            if !is_jj_repo() {
                // Not in a jj repo - silently exit with success
                // This allows global configuration without errors in non-jj directories
                eprintln!("jjcc: Not in a jj repository, skipping");
                return Ok(());
            }

            // Read JSON input from stdin
            let mut buffer = String::new();
            io::stdin().read_to_string(&mut buffer)?;

            let input: HookInput =
                serde_json::from_str(&buffer).context("Failed to parse JSON input")?;

            match hook_cmd {
                HookCommands::UserPromptSubmit => handle_user_prompt_submit(input)?,
                HookCommands::PreToolUse => handle_pre_tool_use(input)?,
                HookCommands::PostToolUse => handle_post_tool_use(input)?,
                HookCommands::Stop => {
                    // Stop hook - no-op, just acknowledge
                    eprintln!("Session {} stopped", input.session_id);
                }
                HookCommands::SessionEnd => handle_session_end(input)?,
            }
        }
    }

    Ok(())
}

fn get_temp_file_path(session_id: &str, suffix: &str) -> PathBuf {
    let temp_dir = env::temp_dir();
    temp_dir.join(format!("claude-session-{}-{}", session_id, suffix))
}

fn extract_session_id_from_temp_workspace(desc: &str) -> Option<String> {
    if !desc.contains("[Claude Workspace]") {
        return None;
    }
    let lines: Vec<&str> = desc.lines().collect();
    for line in lines {
        if line.contains("session") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if let Some(last) = parts.last() {
                return Some(last.to_string());
            }
        }
    }
    None
}

fn wait_for_other_session(other_session_id: &str, current_session_id: &str) -> Result<()> {
    let timeout_secs = 60;
    let poll_interval = Duration::from_secs(2);
    let start = std::time::Instant::now();
    let mut last_message_at = start;
    let message_interval = Duration::from_secs(30);

    eprintln!(
        "⏳ Waiting for another editing session ({}) to complete...",
        &other_session_id[..8.min(other_session_id.len())]
    );

    loop {
        let elapsed = start.elapsed();
        let now = std::time::Instant::now();

        if elapsed.as_secs() > timeout_secs {
            let current_change = get_current_change_id()?;
            anyhow::bail!(
                "Error: Another editing session appears abandoned\n\n\
                 Session {} created a temporary workspace but has not completed\n\
                 within 60 seconds. It may have crashed or been force-quit.\n\n\
                 To recover:\n\
                 1. Run `jj edit <your-work>` to return to your working copy\n\
                 2. Run `jj abandon <temp-workspace-id>` to clean up\n\
                 3. Retry this session\n\n\
                 Current change: {}\n\
                 This session:   {}\n\
                 Other session:  {}",
                &other_session_id[..8.min(other_session_id.len())],
                &current_change[..12.min(current_change.len())],
                &current_session_id[..8.min(current_session_id.len())],
                &other_session_id[..8.min(other_session_id.len())],
            );
        }

        if now.duration_since(last_message_at) > message_interval {
            eprintln!(
                "⏳ Waiting for editing session {} ({}s)...",
                &other_session_id[..8.min(other_session_id.len())],
                elapsed.as_secs()
            );
            last_message_at = now;
        }

        let current_desc = get_current_description()?;
        if let Some(workspace_session_id) = extract_session_id_from_temp_workspace(&current_desc) {
            if workspace_session_id == other_session_id {
                thread::sleep(poll_interval);
                continue;
            }
        }

        eprintln!("✓ Other session complete, proceeding...");
        break;
    }

    Ok(())
}

fn handle_user_prompt_submit(input: HookInput) -> Result<()> {
    let session_id = input.session_id;

    if let Some(prompt) = input.prompt {
        eprintln!("Session {}: {}", session_id, prompt);
    }
    Ok(())
}

fn handle_pre_tool_use(input: HookInput) -> Result<()> {
    let session_id = input.session_id;

    // Invariant: The hook should handle any tool type (Edit, Write, MultiEdit, Bash, etc.)
    // This is a critical design principle that allows for universal change attribution

    let current_desc = get_current_description()?;
    if current_desc.contains("[Claude Workspace]") && current_desc.contains(&session_id) {
        eprintln!("Already on temporary workspace, continuing");
        return Ok(());
    }

    if current_desc.contains("[Claude Workspace]") {
        if let Some(other_session_id) = extract_session_id_from_temp_workspace(&current_desc) {
            if other_session_id != session_id {
                wait_for_other_session(&other_session_id, &session_id)?;

                let current_desc_after_wait = get_current_description()?;
                if current_desc_after_wait.contains("[Claude Workspace]") {
                    let current_change_id_after_wait = get_current_change_id()?;
                    anyhow::bail!(
                        "Error: Still on temporary workspace after waiting\n\n\
                         The working copy is still a temporary Claude workspace.\n\
                         This indicates the session did not complete properly.\n\n\
                         To fix:\n\
                         1. Run `jj edit <your-work>` to return to your working copy\n\
                         2. Optionally abandon the temp workspace: `jj abandon {}`\n\
                         3. Retry this session\n\n\
                         Current change: {}\n\
                         This session:   {}",
                        &current_change_id_after_wait[..12.min(current_change_id_after_wait.len())],
                        &current_change_id_after_wait[..12.min(current_change_id_after_wait.len())],
                        &session_id[..8.min(session_id.len())],
                    );
                }
            }
        }
    }

    let current_change_id = get_current_change_id()?;
    verify_change_safe_for_session(&current_change_id, &session_id, "working copy")?;

    let original_working_copy_file = get_temp_file_path(&session_id, "original-working-copy.txt");
    fs::write(&original_working_copy_file, &current_change_id)?;

    eprintln!("Creating temporary workspace for Claude edits");
    run_jj_command(&["new"])?;

    run_jj_command(&[
        "describe",
        "-m",
        &format!(
            "[Claude Workspace] Temporary workspace for session {}",
            session_id
        ),
    ])?;

    Ok(())
}

fn handle_post_tool_use(input: HookInput) -> Result<()> {
    let session_id = input.session_id;
    eprintln!("PostToolUse: Starting for session {}", session_id);

    // Invariant: PostToolUse must handle all tool types for proper change attribution
    // Whether changes come from Edit, Write, MultiEdit, or Bash commands, the detection
    // mechanism using `jj diff --stat` works universally

    let original_working_copy_file = get_temp_file_path(&session_id, "original-working-copy.txt");
    if !original_working_copy_file.exists() {
        eprintln!("PostToolUse: No original working copy file found");
        return Ok(());
    }

    let original_working_copy_id = fs::read_to_string(&original_working_copy_file)?
        .trim()
        .to_string();

    verify_change_safe_for_session(
        &original_working_copy_id,
        &session_id,
        "stored original working copy",
    )?;

    // Get current workspace change ID
    let workspace_change_id = get_current_change_id()?;

    // Check if there are any changes to move using jj diff --stat
    // Look for the "0 files changed" pattern to determine if workspace is empty
    let diff_stat = Command::new("jj").args(["diff", "--stat"]).output()?;
    let diff_output = String::from_utf8_lossy(&diff_stat.stdout);

    // Invariant: Change detection must be tool-agnostic
    // `jj diff --stat` detects file modifications regardless of whether they came from
    // file editing tools (Edit, Write, MultiEdit) or bash commands that modify files
    debug_assert!(
        diff_stat.status.success(),
        "jj diff --stat must succeed for proper change attribution"
    );

    // More robust check for no changes - look for the exact pattern
    let has_no_changes = diff_output.trim() == "0 files changed, 0 insertions(+), 0 deletions(-)";

    if has_no_changes {
        eprintln!("PostToolUse: No changes made, abandoning workspace");
        run_jj_command(&["abandon", &workspace_change_id])?;
        // After abandon, we need to explicitly return to original
        run_jj_command(&["edit", &original_working_copy_id])?;
        return Ok(());
    }

    // Check if Claude change already exists for this session
    // When multiple commits have the same session ID, jj returns them in
    // topological order (descendants first), so we get the furthest descendant
    let search_output = Command::new("jj")
        .args([
            "log",
            "-r",
            &format!("description(glob:'*Claude-Session-Id: {}*')", session_id),
            "--no-graph",
            "-T",
            "change_id",
            "--limit",
            "1",
        ])
        .output()?;

    if search_output.status.success() && !search_output.stdout.is_empty() {
        // Claude change exists, we'll squash into it
        let existing_id = String::from_utf8_lossy(&search_output.stdout)
            .trim()
            .to_string();
        eprintln!(
            "PostToolUse: Found existing Claude change {}",
            &existing_id[0..12.min(existing_id.len())]
        );

        // Squash workspace changes into the existing Claude change
        run_jj_command(&[
            "squash",
            "--from",
            &workspace_change_id,
            "--into",
            &existing_id,
            "--use-destination-message",
        ])?;
    } else {
        // First tool use - insert workspace before original
        eprintln!("PostToolUse: Creating Claude change before original");

        // Use jj rebase to move our workspace change before the original
        // This rebases original on top of our changes
        run_jj_command(&[
            "rebase",
            "-r",
            &workspace_change_id,
            "--insert-before",
            &original_working_copy_id,
        ])?;

        // Add Claude description with session trailer
        let description = format!("Claude Code Session {}", session_id);
        let trailer = format!("Claude-Session-Id: {}", session_id);
        let message = format!("{}\n\n{}", description, trailer);

        // Describe the workspace (which is now the Claude change)
        let mut child = Command::new("jj")
            .args(["describe", "-r", &workspace_change_id, "--stdin"])
            .stdin(std::process::Stdio::piped())
            .spawn()?;

        if let Some(stdin) = child.stdin.as_mut() {
            use std::io::Write;
            stdin.write_all(message.as_bytes())?;
        }
        child.wait()?;
    }

    eprintln!("PostToolUse: Switching back to original working copy");
    run_jj_command(&["edit", &original_working_copy_id])?;

    eprintln!("PostToolUse: Back on original working copy");
    Ok(())
}

fn handle_session_end(input: HookInput) -> Result<()> {
    let session_id = input.session_id;
    eprintln!("Session {} ended", session_id);

    let original_working_copy_file = get_temp_file_path(&session_id, "original-working-copy.txt");
    let _ = fs::remove_file(original_working_copy_file);

    Ok(())
}

fn get_current_description() -> Result<String> {
    let output = Command::new("jj")
        .args(["log", "-r", "@", "--no-graph", "-T", "description"])
        .output()
        .context("Failed to run jj log")?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn get_current_change_id() -> Result<String> {
    let output = Command::new("jj")
        .args(["log", "-r", "@", "--no-graph", "-T", "change_id"])
        .output()
        .context("Failed to get current change id")?;

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn get_session_id_from_change(change_id: &str) -> Result<Option<String>> {
    let output = Command::new("jj")
        .args(["log", "-r", change_id, "--no-graph", "-T", "description"])
        .output()
        .context("Failed to get commit description")?;

    let description = String::from_utf8_lossy(&output.stdout);

    for line in description.lines().rev() {
        if line.trim().is_empty() {
            break;
        }
        if let Some(session_id) = line.strip_prefix("Claude-Session-Id:") {
            return Ok(Some(session_id.trim().to_string()));
        }
    }

    Ok(None)
}

fn is_temp_workspace(change_id: &str) -> Result<bool> {
    let output = Command::new("jj")
        .args(["log", "-r", change_id, "--no-graph", "-T", "description"])
        .output()
        .context("Failed to get commit description")?;

    let description = String::from_utf8_lossy(&output.stdout);
    Ok(description.contains("[Claude Workspace]"))
}

fn verify_change_safe_for_session(
    change_id: &str,
    current_session_id: &str,
    context: &str,
) -> Result<()> {
    if let Some(found_session_id) = get_session_id_from_change(change_id)? {
        if found_session_id != current_session_id {
            anyhow::bail!(
                "Error: Concurrent Claude session detected\n\n\
                 The {} is a Claude change from another session.\n\
                 Another Claude Code session is likely active in this repo.\n\n\
                 To fix:\n\
                 1. Complete or cancel the other Claude session\n\
                 2. Run `jj edit <your-work>` to return to your working copy\n\
                 3. Retry this session\n\n\
                 Current change: {}\n\
                 This session:   {}\n\
                 Other session:  {}",
                context,
                &change_id[..12.min(change_id.len())],
                &current_session_id[..8.min(current_session_id.len())],
                &found_session_id[..8.min(found_session_id.len())],
            );
        }
    }

    if is_temp_workspace(change_id)? {
        anyhow::bail!(
            "Error: Temporary workspace detected\n\n\
             The {} is a temporary Claude workspace.\n\
             This indicates an interrupted or concurrent session.\n\n\
             To fix:\n\
             1. Run `jj edit <your-work>` to return to your working copy\n\
             2. Optionally abandon the temp workspace: `jj abandon {}`\n\
             3. Retry this session\n\n\
             Current change: {}\n\
             This session:   {}",
            context,
            &change_id[..12.min(change_id.len())],
            &change_id[..12.min(change_id.len())],
            &current_session_id[..8.min(current_session_id.len())],
        );
    }

    Ok(())
}

fn is_jj_repo() -> bool {
    // Check if .jj directory exists in current directory or any parent
    let output = Command::new("jj").args(["root"]).output();

    match output {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

fn run_jj_command(args: &[&str]) -> Result<()> {
    let output = Command::new("jj")
        .args(args)
        .stderr(std::process::Stdio::inherit())
        .output()
        .context("Failed to run jj command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("jj command failed: {}", stderr);
    }

    // Print stdout if there's any output
    if !output.stdout.is_empty() {
        print!("{}", String::from_utf8_lossy(&output.stdout));
    }

    Ok(())
}
