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

/// Output structure for injecting additional context into Claude
#[derive(Debug, Serialize)]
pub struct HookSpecificOutput {
    #[serde(rename = "hookEventName")]
    pub hook_event_name: String,
    #[serde(rename = "additionalContext")]
    pub additional_context: String,
}

/// Response structure for Claude Code hooks to control execution
#[derive(Debug, Serialize)]
pub struct HookResponse {
    #[serde(rename = "continue")]
    pub continue_execution: bool,
    #[serde(rename = "stopReason", skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    #[serde(rename = "hookSpecificOutput", skip_serializing_if = "Option::is_none")]
    pub hook_specific_output: Option<HookSpecificOutput>,
}

impl HookResponse {
    /// Create a response that allows execution to continue
    pub fn continue_execution() -> Self {
        Self {
            continue_execution: true,
            stop_reason: None,
            hook_specific_output: None,
        }
    }

    /// Create a response with additional context for Claude
    pub fn with_context(hook_event_name: impl Into<String>, context: impl Into<String>) -> Self {
        Self {
            continue_execution: true,
            stop_reason: None,
            hook_specific_output: Some(HookSpecificOutput {
                hook_event_name: hook_event_name.into(),
                additional_context: context.into(),
            }),
        }
    }

    /// Create a response that stops execution with an error message
    pub fn stop(reason: impl Into<String>) -> Self {
        Self {
            continue_execution: false,
            stop_reason: Some(reason.into()),
            hook_specific_output: None,
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
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub hook_event_name: Option<String>,
    #[serde(default)]
    pub transcript_path: Option<String>,
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

/// Handle PreToolUse hook - acquires lock and creates a new precommit change
pub fn handle_pretool_hook(input: HookInput) -> Result<()> {
    // Check if we're in a jj repo - if not, this is a noop
    if !crate::jj::is_jj_repo() {
        eprintln!("jjagent: Not in a jj repository, skipping hook");
        return Ok(());
    }

    // Acquire lock first - this will be held until PostToolUse/Stop
    crate::lock::acquire_lock(&input.session_id).context("Failed to acquire working copy lock")?;

    // Update stale working copy to sync with any operations that happened while waiting for lock
    // This is critical with watchman auto-snapshot to avoid divergence
    let _output = Command::new("jj")
        .args(["workspace", "update-stale"])
        .output()
        .context("Failed to update stale working copy")?;

    // Note: update-stale succeeds with "Working copy already up to date" if not stale
    // so we don't need to check the output

    // Invariant check: ensure we're not on a session change (has Claude-session-id trailer)
    // This prevents Claude from working directly on a session change
    match crate::jj::get_current_commit_session_id() {
        Ok(Some(session_id)) => {
            // Release lock on error
            let _ = crate::lock::release_lock(&input.session_id);
            anyhow::bail!(
                "Working copy (@) is a session change with Claude-session-id: {}. \
                 Cannot work directly on a session change. Please move to a different change.",
                session_id
            );
        }
        Err(e) => {
            // Release lock on error
            let _ = crate::lock::release_lock(&input.session_id);
            anyhow::bail!(
                "Failed to check if current commit is a session change: {}",
                e
            );
        }
        Ok(None) => {
            // All good, we're not on a session change
        }
    }

    // Invariant check: ensure we're at a head (no descendants) before creating a new change
    // This prevents branching which jjagent aims to avoid
    match crate::jj::is_at_head() {
        Ok(false) => {
            // Release lock on error
            let _ = crate::lock::release_lock(&input.session_id);
            anyhow::bail!(
                "Working copy (@) is not at a head - it has descendants. \
                 jjagent requires a linear history. Please resolve this before continuing."
            );
        }
        Err(e) => {
            // Release lock on error
            let _ = crate::lock::release_lock(&input.session_id);
            anyhow::bail!("Failed to check if at head: {}", e);
        }
        Ok(true) => {
            // All good, we're at a head
        }
    }

    // Invariant check: ensure there are no conflicts in the working copy
    // This prevents Claude from working on a conflicted state
    match crate::jj::has_conflicts() {
        Ok(true) => {
            // Release lock on error
            let _ = crate::lock::release_lock(&input.session_id);
            anyhow::bail!(
                "Working copy (@) has conflicts. \
                 Please resolve all conflicts before continuing."
            );
        }
        Err(e) => {
            // Release lock on error
            let _ = crate::lock::release_lock(&input.session_id);
            anyhow::bail!("Failed to check for conflicts: {}", e);
        }
        Ok(false) => {
            // All good, no conflicts
        }
    }

    let session_id = SessionId::from_full(&input.session_id);
    let commit_message = format_precommit_message(&session_id);

    let output = Command::new("jj")
        .args(["new", "-m", &commit_message])
        .output()
        .context("Failed to execute jj new command")?;

    if !output.status.success() {
        // Release lock on error
        let _ = crate::lock::release_lock(&input.session_id);
        anyhow::bail!(
            "jj new command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Lock remains held until PostToolUse or Stop
    Ok(())
}

/// Finalize a precommit by squashing it into the session change
/// 1. Verifies @ is a precommit for this session (noop if not)
/// 2. Finds or creates session change
/// 3. Attempts to squash precommit into session
/// 4. If conflicts occur, handles them by creating a new session part
fn finalize_precommit(session_id: SessionId) -> Result<()> {
    // Update stale working copy before any jj operations
    // This prevents "stale working copy" errors during squash operations
    // especially when file watchers create automatic snapshots
    let _output = Command::new("jj")
        .args(["workspace", "update-stale"])
        .output()
        .context("Failed to update stale working copy")?;

    // Invariant check: ensure there are no conflicts in the working copy
    // This prevents finalizing changes with unresolved conflicts
    if crate::jj::has_conflicts()? {
        anyhow::bail!(
            "Working copy (@) has conflicts. \
             Cannot finalize changes until conflicts are resolved."
        );
    }

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

/// Handle PostToolUse hook - squashes changes and manages conflicts, then releases lock
pub fn handle_posttool_hook(input: HookInput) -> Result<()> {
    // Check if we're in a jj repo - if not, this is a noop
    if !crate::jj::is_jj_repo() {
        eprintln!("jjagent: Not in a jj repository, skipping hook");
        return Ok(());
    }

    let session_id = SessionId::from_full(&input.session_id);

    // Small delay to allow file watchers (watchman, fsmonitor) to complete their snapshots
    // This reduces the chance of concurrent operations creating divergent operation log branches
    // that can interfere with linearization and squashing
    // Configurable via JJAGENT_POSTTOOL_DELAY_MS (default: 100ms)
    let delay_ms = std::env::var("JJAGENT_POSTTOOL_DELAY_MS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(100);

    if delay_ms > 0 {
        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
    }

    // Do the actual work
    let result = finalize_precommit(session_id);

    // Always release lock, even on error
    match crate::lock::release_lock(&input.session_id) {
        Ok(()) => result,
        Err(e) => {
            eprintln!("jjagent: Warning - failed to release lock: {}", e);
            result
        }
    }
}

/// Handle Stop hook - finalizes any precommit and releases lock
/// This hook runs when Claude exits (normally or interrupted).
/// If @ is a precommit for this session, it finalizes the changes.
/// Otherwise, it's a noop (user is already on uwc or another session is active).
pub fn handle_stop_hook(input: HookInput) -> Result<()> {
    // Check if we're in a jj repo - if not, this is a noop
    if !crate::jj::is_jj_repo() {
        eprintln!("jjagent: Not in a jj repository, skipping hook");
        return Ok(());
    }

    let session_id = SessionId::from_full(&input.session_id);

    // Do the actual work
    let result = finalize_precommit(session_id);

    // Always release lock, even on error
    match crate::lock::release_lock(&input.session_id) {
        Ok(()) => result,
        Err(e) => {
            eprintln!("jjagent: Warning - failed to release lock: {}", e);
            result
        }
    }
}

/// Handle SessionStart hook - injects session ID into Claude's context
/// This runs once when a new Claude session begins
pub fn handle_session_start_hook(input: &HookInput) -> Result<HookResponse> {
    let context_message = format!(
        "System Note: The current session ID is {}. I must use this ID for session-specific tasks.",
        input.session_id
    );

    Ok(HookResponse::with_context("SessionStart", context_message))
}

/// Handle UserPromptSubmit hook - re-injects session ID if it's been lost from context
/// This runs before each user prompt, checking if the session ID is still in recent transcript
pub fn handle_user_prompt_submit_hook(input: &HookInput) -> Result<HookResponse> {
    // If no transcript path provided, just continue without injecting
    let Some(transcript_path) = &input.transcript_path else {
        return Ok(HookResponse::continue_execution());
    };

    // Read the last 20 lines of the transcript to check if session ID is present
    let transcript_content =
        std::fs::read_to_string(transcript_path).context("Failed to read transcript file")?;

    let lines: Vec<&str> = transcript_content.lines().collect();
    let recent_lines = if lines.len() > 20 {
        &lines[lines.len() - 20..]
    } else {
        &lines[..]
    };
    let recent_transcript = recent_lines.join("\n");

    // If session ID is not in recent transcript, re-inject it
    if !recent_transcript.contains(&input.session_id) {
        let context_message = format!(
            "System Note: The current session ID is {}. I must use this ID for session-specific tasks.",
            input.session_id
        );
        Ok(HookResponse::with_context(
            "UserPromptSubmit",
            context_message,
        ))
    } else {
        Ok(HookResponse::continue_execution())
    }
}
