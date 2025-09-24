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
    /// Handle Stop hook
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

    let input: HookInput = serde_json::from_str(&buffer).context("Failed to parse JSON input")?;

    match cli.command {
        Commands::Hooks(hook_cmd) => match hook_cmd {
            HookCommands::UserPromptSubmit => handle_user_prompt_submit(input)?,
            HookCommands::PreToolUse => handle_pre_tool_use(input)?,
            HookCommands::PostToolUse => handle_post_tool_use(input)?,
            HookCommands::Stop => handle_stop(input)?,
            HookCommands::SessionEnd => handle_session_end(input)?,
        },
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

            // Always add session ID as a trailer
            let trailer = format!("\nClaude-Session-Id: {}", session_id);
            let new_desc = format!("{}\n\n{}{}", description, all_prompts, trailer);

            // Update the description
            Command::new("jj")
                .args(["describe", "-r", &session_change, "-m", &new_desc])
                .output()?;

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

    // Check if a Claude change already exists for this session
    // Search for a change with the session ID in the trailer
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
        // Claude change exists for this session
        let claude_change_id = String::from_utf8_lossy(&search_output.stdout)
            .trim()
            .to_string();
        eprintln!(
            "Found existing Claude change {} for session",
            &claude_change_id[0..12.min(claude_change_id.len())]
        );

        // Store the Claude change ID for PostToolUse
        let claude_change_file = get_temp_file_path(&session_id, "claude-change.txt");
        fs::write(&claude_change_file, &claude_change_id)?;

        // Check if we're already on a temporary child of the Claude change
        let current_desc = get_current_description()?;
        if current_desc.contains("[Claude PreToolUse]") && current_desc.contains(&session_id) {
            // We're already on a temporary workspace for this session, just use it
            eprintln!("Already on temporary workspace, continuing");
            return Ok(());
        }

        // Create a new temporary child of the Claude change
        eprintln!("Creating temporary child of Claude change for editing");
        run_jj_command(&["new", &claude_change_id])?;

        // Add description to the temporary change
        run_jj_command(&[
            "describe",
            "-m",
            &format!(
                "[Claude PreToolUse] Temporary workspace for session {}",
                session_id
            ),
        ])?;

        return Ok(());
    }

    // If we reach here, we need to create a new Claude change
    {
        eprintln!("Creating Claude change for session {}", session_id);

        // Build the message from env var or default, plus stored prompts
        let description =
            env::var("JJCC_DESC").unwrap_or_else(|_| format!("Claude Code Session {}", session_id));

        // Always add session ID as a trailer
        let trailer = format!("\nClaude-Session-Id: {}", session_id);

        let prompt_file = get_temp_file_path(&session_id, "prompts.txt");
        let message = if prompt_file.exists() {
            let prompts = fs::read_to_string(&prompt_file)?;
            format!("{}\n\n{}{}", description, prompts, trailer)
        } else {
            format!("{}{}", description, trailer)
        };

        // Create new change inserted before the current working copy
        eprintln!("Creating new change inserted before working copy");
        run_jj_command(&["new", "--insert-before", &original_working_copy_id])?;

        // Get the new change ID (current @ after 'jj new --insert-before')
        let claude_change_id = get_current_change_id()?;

        // Store Claude's ID for PostToolUse
        let claude_change_file = get_temp_file_path(&session_id, "claude-change.txt");
        fs::write(&claude_change_file, &claude_change_id)?;
        eprintln!(
            "Created Claude change: {}",
            &claude_change_id[0..12.min(claude_change_id.len())]
        );

        // Add the description to Claude's change
        run_jj_command(&["describe", "-m", &message])?;
    }

    Ok(())
}

fn handle_post_tool_use(input: HookInput) -> Result<()> {
    let session_id = input.session_id;
    eprintln!("PostToolUse: Starting for session {}", session_id);

    // Get stored file paths
    let claude_change_file = get_temp_file_path(&session_id, "claude-change.txt");
    let original_working_copy_file = get_temp_file_path(&session_id, "original-working-copy.txt");

    if !claude_change_file.exists() {
        eprintln!(
            "PostToolUse: No Claude change file found for session {}",
            session_id
        );
        return Ok(());
    }

    if !original_working_copy_file.exists() {
        eprintln!(
            "PostToolUse: No original working copy file found for session {}",
            session_id
        );
        return Ok(());
    }

    let claude_change_id = fs::read_to_string(&claude_change_file)?.trim().to_string();
    let original_working_copy_id = fs::read_to_string(&original_working_copy_file)?
        .trim()
        .to_string();

    // Get current change ID (could be Claude's change on first use, or temp child on subsequent)
    let current_change_id = get_current_change_id()?;
    eprintln!(
        "PostToolUse: Current change: {}, Claude change: {}, Original: {}",
        &current_change_id[0..12.min(current_change_id.len())],
        &claude_change_id[0..12.min(claude_change_id.len())],
        &original_working_copy_id[0..12.min(original_working_copy_id.len())]
    );

    // If current change is not the Claude change, it's a temp child that needs squashing
    if current_change_id != claude_change_id {
        // Check if there are any changes to squash
        let status = Command::new("jj").args(["status", "--no-pager"]).output()?;
        let status_str = String::from_utf8_lossy(&status.stdout);

        if status_str.contains("(empty)") || status_str.contains("nothing changed") {
            eprintln!("PostToolUse: No changes to squash, abandoning temporary change");
            run_jj_command(&["abandon", &current_change_id])?;
        } else {
            // Squash the temporary child's changes back into Claude's change
            eprintln!(
                "PostToolUse: Squashing changes into Claude change {}",
                &claude_change_id[0..12.min(claude_change_id.len())]
            );

            // Use --use-destination-message to avoid interactive editor
            run_jj_command(&[
                "squash",
                "--from",
                &current_change_id,
                "--into",
                &claude_change_id,
                "--use-destination-message",
            ])?;
        }
    }

    // Switch back to the original working copy
    eprintln!(
        "PostToolUse: Switching back to original working copy {}",
        &original_working_copy_id[0..12.min(original_working_copy_id.len())]
    );
    run_jj_command(&["edit", &original_working_copy_id])?;

    Ok(())
}

fn handle_stop(input: HookInput) -> Result<()> {
    let session_id = input.session_id;
    eprintln!("Session {} stopped", session_id);
    // No bookmarking - users will manage bookmarks manually
    Ok(())
}

fn handle_session_end(input: HookInput) -> Result<()> {
    let session_id = input.session_id;
    eprintln!("Session {} ended", session_id);

    // Clean up temporary files
    let claude_change_file = get_temp_file_path(&session_id, "claude-change.txt");
    let _ = fs::remove_file(claude_change_file);

    let prompts_file = get_temp_file_path(&session_id, "prompts.txt");
    let _ = fs::remove_file(prompts_file);

    let original_working_copy_file = get_temp_file_path(&session_id, "original-working-copy.txt");
    let _ = fs::remove_file(original_working_copy_file);

    // Clean up legacy files if they exist
    for suffix in [
        "stashed.txt",
        "prompt.txt",
        "base-change.txt",
        "sibling-change.txt",
        "merge-change.txt",
        "structure-created.txt",
        "parent-change.txt",
        "user-change.txt",
        "claude-editing.txt",
    ] {
        let file = get_temp_file_path(&session_id, suffix);
        let _ = fs::remove_file(file);
    }

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
