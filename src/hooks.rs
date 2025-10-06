//! Claude Code hooks for integrating with jj.
//!
//! This module implements the PreToolUse and PostToolUse hooks that Claude Code
//! calls before and after each tool invocation (Write, Edit, etc.).
//!
//! # Workflow
//!
//! 1. **PreToolUse**: Creates a precommit change to stage Claude's upcoming changes
//! 2. **PostToolUse**: Squashes precommit into session change, handles conflicts
//!
//! The hooks maintain a linear history where user changes (uwc) always stay on top,
//! and Claude's changes are isolated in session-specific changes below.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::process::Command;

use crate::session::{SessionId, format_precommit_message};

/// Response structure for Claude Code hooks to control execution
#[derive(Debug, Serialize)]
pub struct HookResponse {
    #[serde(rename = "continue")]
    pub continue_execution: bool,
    #[serde(rename = "stopReason", skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
}

impl HookResponse {
    /// Create a response that stops execution with an error message
    pub fn stop(reason: impl Into<String>) -> Self {
        Self {
            continue_execution: false,
            stop_reason: Some(reason.into()),
        }
    }

    /// Output this response as JSON to stdout
    pub fn output(&self) {
        if let Ok(json) = serde_json::to_string(self) {
            println!("{}", json);
        }
    }
}

/// Input structure for Claude Code hooks
#[derive(Debug, Deserialize)]
pub struct HookInput {
    pub session_id: String,
    pub tool_name: String,
}

impl HookInput {
    /// Read hook input from stdin
    pub fn from_stdin() -> Result<Self> {
        let mut buffer = String::new();
        std::io::stdin()
            .read_to_string(&mut buffer)
            .context("Failed to read hook input from stdin")?;

        serde_json::from_str(&buffer).context("Failed to parse hook input JSON")
    }
}

/// Handle PreToolUse hook - creates a new precommit change
pub fn handle_pretool_hook(input: HookInput) -> Result<()> {
    let session_id = SessionId::from_full(&input.session_id);
    let commit_message = format_precommit_message(&session_id);

    let output = Command::new("jj")
        .args(["new", "-m", &commit_message])
        .output()
        .context("Failed to execute jj new command")?;

    if !output.status.success() {
        anyhow::bail!(
            "jj new command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(())
}

/// Finalize a precommit by squashing it into the session change
/// 1. Verifies @ is a precommit for this session (noop if not)
/// 2. Finds or creates session change
/// 3. Attempts to squash precommit into session
/// 4. If conflicts occur, handles them by creating a new session part
fn finalize_precommit(session_id: SessionId) -> Result<()> {
    // Verify @ is a precommit for this session
    // If not (different session or not a precommit), this is a noop
    if !crate::jj::is_current_commit_precommit_for_session(session_id.full())? {
        return Ok(());
    }

    // Check if session change exists anywhere (not just in descendants)
    let session_change = crate::jj::find_session_change_anywhere(session_id.full())?;
    if session_change.is_none() {
        crate::jj::create_session_change(&session_id)?;
    }

    // Find the session change (either existing or just created)
    let session_change = crate::jj::find_session_change_anywhere(session_id.full())?
        .context("Session change should exist")?;

    // Get change IDs
    // @ is currently at precommit (from pretool hook)
    let precommit_id = crate::jj::get_change_id("@")?;
    let uwc_id = crate::jj::get_change_id("@-")?;
    let session_id_str = session_change.change_id;

    // Attempt to squash precommit into session
    let new_conflicts =
        crate::jj::squash_precommit_into_session(&precommit_id, &session_id_str, &uwc_id)?;

    // If conflicts were introduced, handle them
    if new_conflicts {
        // Count existing session parts to determine the next part number
        let existing_parts = crate::jj::count_session_parts(session_id.full())?;
        let next_part = existing_parts + 1;

        crate::jj::handle_squash_conflicts(&session_id, next_part)?;
    }

    Ok(())
}

/// Handle PostToolUse hook - squashes changes and manages conflicts
pub fn handle_posttool_hook(input: HookInput) -> Result<()> {
    let session_id = SessionId::from_full(&input.session_id);
    finalize_precommit(session_id)
}

/// Handle Stop hook - finalizes any precommit and returns to uwc
/// This hook runs when Claude exits (normally or interrupted).
/// If @ is a precommit for this session, it finalizes the changes.
/// Otherwise, it's a noop (user is already on uwc or another session is active).
pub fn handle_stop_hook(input: HookInput) -> Result<()> {
    let session_id = SessionId::from_full(&input.session_id);
    finalize_precommit(session_id)
}
