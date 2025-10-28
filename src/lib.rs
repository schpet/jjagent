//! jjagent - Track Claude Code sessions as jj changes
//!
//! This crate provides integration between Claude Code and Jujutsu (jj) version control,
//! automatically managing session changes to maintain a clean, linear history.
//!
//! # Features
//!
//! - Automatic session isolation: Each Claude session gets its own change
//! - Linear history: No branches, everything stacked in order
//! - Conflict handling: Automatic detection and resolution via numbered parts
//! - User changes preserved: Your working copy (uwc) stays on top, untouched
//!
//! # Modules
//!
//! - [`hooks`]: Claude Code hook handlers (PreToolUse, PostToolUse)
//! - [`jj`]: Core jj operations (session changes, squashing, conflict detection)
//! - [`session`]: Session ID management and message formatting
//! - [`lock`]: Working copy lock for preventing concurrent operations
//! - [`logger`]: Optional logging for debugging

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::json;
use std::io::{self, Read};
use std::path::Path;
use std::process::Command;

pub mod hooks;
pub mod jj;
pub mod lock;
pub mod logger;
pub mod session;

pub fn get_executable_path() -> Result<std::path::PathBuf> {
    std::env::current_exe().context("Failed to get current executable path")
}

pub fn format_claude_settings() -> Result<String> {
    let exe_path = get_executable_path()?;
    let exe_str = exe_path.to_string_lossy();

    let pre_tool_use_cmd = format!("{} claude hooks PreToolUse", exe_str);
    let post_tool_use_cmd = format!("{} claude hooks PostToolUse", exe_str);
    let stop_cmd = format!("{} claude hooks Stop", exe_str);

    let config = json!({
        "hooks": {
            "PreToolUse": [{
                "matcher": "Edit|MultiEdit|Write",
                "hooks": [{
                    "type": "command",
                    "command": pre_tool_use_cmd
                }]
            }],
            "PostToolUse": [{
                "matcher": "Edit|MultiEdit|Write",
                "hooks": [{
                    "type": "command",
                    "command": post_tool_use_cmd
                }]
            }],
            "Stop": [{
                "hooks": [{
                    "type": "command",
                    "command": stop_cmd
                }]
            }]
        }
    });

    Ok(serde_json::to_string_pretty(&config)?)
}

/// Split a change by inserting a new change before @ (working copy)
pub fn split_change(reference: &str) -> Result<()> {
    jj::split_change(reference, None)
}

/// Move session tracking to an existing jj revision
/// The reference must be an ancestor of @ (working copy)
pub fn move_session_into(session_id: &str, reference: &str) -> Result<()> {
    jj::move_session_into(session_id, reference, None)
}

/// Update a session change's description while preserving trailers
/// Looks up the change by session ID and updates its description with the new message
/// while automatically preserving all existing trailers
pub fn describe_session_change(session_id: &str, new_message: &str) -> Result<()> {
    // Find the change by session ID
    let change_id =
        jj::find_session_change_anywhere(session_id)?.context("No change found for session ID")?;

    // Update the description while preserving trailers
    jj::update_description_preserving_trailers(&change_id, new_message)?;

    Ok(())
}

/// Format a commit message for a session change
/// If no custom message is provided, uses the default session message format
/// If a custom message is provided, appends the Claude-session-id trailer
pub fn format_session_commit_message(
    session_id: &str,
    custom_message: Option<&str>,
) -> Result<String> {
    let sid = session::SessionId::from_full(session_id);

    let message = match custom_message {
        None => session::format_session_message(&sid),
        Some(msg) => format!("{}\n\nClaude-session-id: {}", msg, sid.full()),
    };

    Ok(message)
}

/// Input format for status line command
/// Note: Unknown fields are ignored by default, ensuring forward compatibility
/// if Claude Code adds new fields in the future
#[derive(Deserialize)]
struct StatuslineInput {
    session_id: String,
    workspace: WorkspaceInfo,
}

/// Workspace information from Claude Code
/// Note: Unknown fields are ignored by default
#[derive(Deserialize)]
struct WorkspaceInfo {
    current_dir: String,
}

/// Format jj session change info for status line
/// Reads JSON input from stdin with session_id and workspace.current_dir
/// Outputs the jj session change info part only (if in jj repo and session has a change)
/// Returns empty string if no session change found
pub fn format_jj_statusline_info() -> Result<String> {
    // Read JSON from stdin
    let mut stdin = io::stdin();
    let mut input = String::new();
    stdin.read_to_string(&mut input)?;

    // Parse JSON
    let data: StatuslineInput = serde_json::from_str(&input)?;

    // Check if we're in a jj repo
    let is_jj_repo = Command::new("jj")
        .arg("--ignore-working-copy")
        .arg("root")
        .current_dir(&data.workspace.current_dir)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !is_jj_repo {
        return Ok(String::new());
    }

    // Try to get the session change
    let repo_path = Path::new(&data.workspace.current_dir);
    let change_id = match jj::find_session_change_anywhere_in(&data.session_id, Some(repo_path))
        .ok()
        .flatten()
    {
        Some(id) => id,
        None => return Ok(String::new()),
    };

    // Get formatted commit info with jj log
    let jj_output = Command::new("jj")
        .arg("log")
        .arg("--ignore-working-copy")
        .arg("--color=always")
        .arg("--no-graph")
        .arg("-r")
        .arg(&change_id)
        .arg("-T")
        .arg("format_commit_summary_with_refs(self, bookmarks)")
        .current_dir(&data.workspace.current_dir)
        .output();

    if let Ok(jj_output) = jj_output
        && jj_output.status.success()
    {
        let change_info = String::from_utf8_lossy(&jj_output.stdout)
            .trim()
            .to_string();
        if !change_info.is_empty() {
            return Ok(change_info);
        }
    }

    Ok(String::new())
}
