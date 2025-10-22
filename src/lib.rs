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
use serde_json::json;

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
