use anyhow::{Context, Result};
use chrono::Local;
use clap::{Parser, Subcommand};
use serde::Deserialize;
use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process::Command;

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

fn handle_user_prompt_submit(input: HookInput) -> Result<()> {
    let session_id = input.session_id;

    if let Some(prompt) = input.prompt {
        eprintln!("Session {}: {}", session_id, prompt);

        // Store the prompt for later use when creating the Claude change
        let prompt_file = get_temp_file_path(&session_id, "prompts.txt");
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
        let prompt_entry = format!("## {}\n\n{}", timestamp, prompt);

        // Append to prompts file
        if prompt_file.exists() {
            let existing = fs::read_to_string(&prompt_file)?;
            fs::write(&prompt_file, format!("{}\n\n{}", existing, prompt_entry))?;
        } else {
            fs::write(&prompt_file, prompt_entry)?;
        }

        // Check if this session already has a change (from previous tool use)
        // Search for the session ID trailer which is always present
        // When multiple commits have the same session ID, jj returns them in
        // topological order (descendants first), so we get the furthest descendant
        let output = Command::new("jj")
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

        if output.status.success() && !output.stdout.is_empty() {
            // Session exists - update its description with the new prompt
            let session_change = String::from_utf8_lossy(&output.stdout).trim().to_string();
            eprintln!("Found existing Claude change, updating description");

            // Get all prompts
            let all_prompts = fs::read_to_string(&prompt_file)?;

            // Use env var or existing description (minus trailer)
            let description = if let Ok(custom_desc) = env::var("JJCC_DESC") {
                custom_desc
            } else {
                // Get existing description to preserve it
                let desc_output = Command::new("jj")
                    .args([
                        "log",
                        "-r",
                        &session_change,
                        "--no-graph",
                        "-T",
                        "description",
                    ])
                    .output()?;
                let existing = String::from_utf8_lossy(&desc_output.stdout);
                // Extract just the first line (before prompts and trailer)
                existing
                    .lines()
                    .next()
                    .unwrap_or("Claude Code Session")
                    .to_string()
            };

            // Always add session ID as a trailer (with blank line before it)
            let trailer = format!("Claude-Session-Id: {}", session_id);
            // Ensure blank line before trailer - trim prompts to avoid extra newlines
            let new_desc = format!(
                "{}\n\n{}\n\n{}",
                description,
                all_prompts.trim_end(),
                trailer
            );

            // Update the description using stdin to preserve formatting
            let mut child = Command::new("jj")
                .args(["describe", "-r", &session_change, "--stdin"])
                .stdin(std::process::Stdio::piped())
                .spawn()?;

            if let Some(stdin) = child.stdin.as_mut() {
                use std::io::Write;
                stdin.write_all(new_desc.as_bytes())?;
            }

            child.wait()?;

            eprintln!("Updated session description");
        }
        // Don't create a change yet - wait for the first tool use
    }
    Ok(())
}

fn handle_pre_tool_use(input: HookInput) -> Result<()> {
    let session_id = input.session_id;

    // Store the current working copy ID before we do anything
    let original_working_copy_id = get_current_change_id()?;
    let original_working_copy_file = get_temp_file_path(&session_id, "original-working-copy.txt");
    fs::write(&original_working_copy_file, &original_working_copy_id)?;

    // Check if we're already on a temporary workspace (continuing from previous tool)
    let current_desc = get_current_description()?;
    if current_desc.contains("[Claude Workspace]") && current_desc.contains(&session_id) {
        eprintln!("Already on temporary workspace, continuing");
        return Ok(());
    }

    // Create a new temporary workspace on top of current change
    eprintln!("Creating temporary workspace for Claude edits");
    run_jj_command(&["new"])?;

    // Add description to the temporary workspace
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

    // Get stored original working copy
    let original_working_copy_file = get_temp_file_path(&session_id, "original-working-copy.txt");
    if !original_working_copy_file.exists() {
        eprintln!("PostToolUse: No original working copy file found");
        return Ok(());
    }

    let original_working_copy_id = fs::read_to_string(&original_working_copy_file)?
        .trim()
        .to_string();

    // Get current workspace change ID
    let workspace_change_id = get_current_change_id()?;

    // Check if there are any changes to move using jj diff --stat
    // This outputs "0 files changed, 0 insertions(+), 0 deletions(-)" when empty
    let diff_stat = Command::new("jj").args(["diff", "--stat"]).output()?;
    let diff_output = String::from_utf8_lossy(&diff_stat.stdout);

    // Check if the diff shows no changes
    if diff_output.starts_with("0 files changed, 0 insertions") {
        eprintln!("PostToolUse: No changes made, abandoning workspace");
        run_jj_command(&["abandon", &workspace_change_id])?;
        // We're already back on the original after abandon
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
        let description =
            env::var("JJCC_DESC").unwrap_or_else(|_| format!("Claude Code Session {}", session_id));

        let trailer = format!("Claude-Session-Id: {}", session_id);

        let prompt_file = get_temp_file_path(&session_id, "prompts.txt");
        let message = if prompt_file.exists() {
            let prompts = fs::read_to_string(&prompt_file)?;
            format!("{}\n\n{}\n\n{}", description, prompts.trim_end(), trailer)
        } else {
            format!("{}\n\n{}", description, trailer)
        };

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

    // Navigate back to the original using jj edit
    eprintln!("PostToolUse: Switching back to original working copy");
    run_jj_command(&["edit", &original_working_copy_id])?;

    eprintln!("PostToolUse: Back on original working copy");
    Ok(())
}

fn handle_session_end(input: HookInput) -> Result<()> {
    let session_id = input.session_id;
    eprintln!("Session {} ended", session_id);

    // Clean up temporary files
    let prompts_file = get_temp_file_path(&session_id, "prompts.txt");
    let _ = fs::remove_file(prompts_file);

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
