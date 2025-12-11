//! Core jj (Jujutsu) operations for managing session changes.
//!
//! This module provides functions to interact with the jj version control system
//! for managing Claude Code session changes. It handles:
//! - Finding and creating session changes
//! - Squashing precommit changes into session changes
//! - Detecting and counting conflicts
//! - Handling conflict resolution by creating numbered session parts

use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

use crate::session::SessionId;

/// Check if the current directory is a jj repository
/// Returns true if `jj root` succeeds, indicating we're in a jj repo
pub fn is_jj_repo() -> bool {
    Command::new("jj")
        .args(["root"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Check if the working copy (@) is at a head (has no descendants)
/// Returns true if @ has no descendants, false otherwise
/// If repo_path is provided, runs jj in that directory
pub fn is_at_head_in(repo_path: Option<&Path>) -> Result<bool> {
    let mut cmd = Command::new("jj");
    if let Some(path) = repo_path {
        cmd.current_dir(path);
    }

    let output = cmd
        .args([
            "log",
            "-r",
            "descendants(@) ~ @",
            "--limit",
            "1",
            "-T",
            "true",
            "--no-graph",
        ])
        .output()
        .context("Failed to execute jj log")?;

    if !output.status.success() {
        anyhow::bail!("jj log failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    // If there's no output, @ has no descendants (is at head)
    Ok(output.stdout.is_empty())
}

/// Check if the working copy (@) is at a head in the current directory
pub fn is_at_head() -> Result<bool> {
    is_at_head_in(None)
}

/// Check if there are any conflicts in the working copy (@)
/// Returns true if conflicts exist, false otherwise
/// If repo_path is provided, runs jj in that directory
pub fn has_conflicts_in(repo_path: Option<&Path>) -> Result<bool> {
    let mut cmd = Command::new("jj");
    if let Some(path) = repo_path {
        cmd.current_dir(path);
    }

    let output = cmd
        .args([
            "log",
            "-r",
            "conflicts() & @",
            "--no-graph",
            "-T",
            "change_id.short()",
        ])
        .output()
        .context("Failed to execute jj log for conflict detection")?;

    if !output.status.success() {
        anyhow::bail!(
            "jj log failed while checking for conflicts: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    // If there's any output, it means @ has conflicts
    Ok(!stdout.trim().is_empty())
}

/// Check if there are any conflicts in the working copy (@) in the current directory
pub fn has_conflicts() -> Result<bool> {
    has_conflicts_in(None)
}

/// Find the closest descendant commit with the given session ID
/// Returns the change ID if found, None otherwise
/// Excludes immutable commits from the search results
/// If repo_path is provided, runs jj in that directory
pub fn find_session_change_in(
    session_id: &str,
    repo_path: Option<&Path>,
) -> Result<Option<String>> {
    // Use revset to filter candidates and template to check exact match
    // Exclude immutable commits to prevent trying to squash into them
    let revset = format!(
        r#"(descendants(@) ~ @) & description("{}") & ~immutable()"#,
        session_id
    );
    let template = format!(
        r#"if(trailers.any(|t| t.key() == "Claude-session-id" && t.value() == "{}"), change_id.short() ++ "\n", "")"#,
        session_id
    );

    let mut cmd = Command::new("jj");
    if let Some(path) = repo_path {
        cmd.current_dir(path);
    }

    let output = cmd
        .args([
            "log",
            "-r",
            &revset,
            "-T",
            &template,
            "--no-graph",
            "--ignore-working-copy",
        ])
        .output()
        .context("Failed to execute jj log")?;

    if !output.status.success() {
        anyhow::bail!("jj log failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let change_ids = parse_change_ids(&stdout);

    // Return the first match (closest descendant)
    Ok(change_ids.into_iter().next())
}

/// Find the closest descendant commit with the given session ID in the current directory
/// Returns the change ID if found, None otherwise
pub fn find_session_change(session_id: &str) -> Result<Option<String>> {
    find_session_change_in(session_id, None)
}

/// Find any commit with the given session ID (not limited to descendants)
/// Returns the change ID if found, None otherwise
/// Excludes immutable commits from the search results
/// If repo_path is provided, runs jj in that directory
pub fn find_session_change_anywhere_in(
    session_id: &str,
    repo_path: Option<&Path>,
) -> Result<Option<String>> {
    // Use revset to filter candidates and template to check exact match
    // Exclude immutable commits to prevent trying to squash into them
    let revset = format!(r#"all() & description("{}") & ~immutable()"#, session_id);
    let template = format!(
        r#"if(trailers.any(|t| t.key() == "Claude-session-id" && t.value() == "{}"), change_id ++ "\n", "")"#,
        session_id
    );

    let mut cmd = Command::new("jj");
    if let Some(path) = repo_path {
        cmd.current_dir(path);
    }

    let output = cmd
        .args([
            "log",
            "-r",
            &revset,
            "-T",
            &template,
            "--no-graph",
            "--ignore-working-copy",
        ])
        .output()
        .context("Failed to execute jj log")?;

    if !output.status.success() {
        anyhow::bail!("jj log failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let change_ids = parse_change_ids(&stdout);

    // Return the first match
    Ok(change_ids.into_iter().next())
}

/// Find any commit with the given session ID in the current directory
/// Returns the change ID if found, None otherwise
pub fn find_session_change_anywhere(session_id: &str) -> Result<Option<String>> {
    find_session_change_anywhere_in(session_id, None)
}

/// Count how many commits exist with the given session ID
/// This is used to determine the part number for conflict handling
/// If repo_path is provided, runs jj in that directory
pub fn count_session_parts_in(session_id: &str, repo_path: Option<&Path>) -> Result<usize> {
    // Use revset to filter candidates and template to check exact match
    let revset = format!(r#"all() & description("{}")"#, session_id);
    let template = format!(
        r#"if(trailers.any(|t| t.key() == "Claude-session-id" && t.value() == "{}"), change_id.short() ++ "\n", "")"#,
        session_id
    );

    let mut cmd = Command::new("jj");
    if let Some(path) = repo_path {
        cmd.current_dir(path);
    }

    let output = cmd
        .args([
            "log",
            "-r",
            &revset,
            "-T",
            &template,
            "--no-graph",
            "--ignore-working-copy",
        ])
        .output()
        .context("Failed to execute jj log")?;

    if !output.status.success() {
        anyhow::bail!("jj log failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let change_ids = parse_change_ids(&stdout);

    Ok(change_ids.len())
}

/// Count how many commits exist with the given session ID in the current directory
pub fn count_session_parts(session_id: &str) -> Result<usize> {
    count_session_parts_in(session_id, None)
}

/// Create a new session change commit inserted before @-
/// This creates the commit structure: @ -> uwc -> session -> base
/// If repo_path is provided, runs jj in that directory
pub fn create_session_change_in(session_id: &SessionId, repo_path: Option<&Path>) -> Result<()> {
    let message = crate::session::format_session_message(session_id);

    let mut cmd = Command::new("jj");
    if let Some(path) = repo_path {
        cmd.current_dir(path);
    }

    let output = cmd
        .args(["new", "--insert-before", "@-", "--no-edit", "-m", &message])
        .output()
        .context("Failed to execute jj new")?;

    if !output.status.success() {
        anyhow::bail!("jj new failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    Ok(())
}

/// Create a new session change commit inserted before @- in the current directory
pub fn create_session_change(session_id: &SessionId) -> Result<()> {
    create_session_change_in(session_id, None)
}

/// Count conflicts on or after a specific change
/// Uses the revset: conflicts() & (change_id:: | change_id)
/// This counts conflicts in the specified change and all its descendants
/// If repo_path is provided, runs jj in that directory
pub fn count_conflicts_in(change_id: &str, repo_path: Option<&Path>) -> Result<usize> {
    let revset = format!("conflicts() & ({}:: | {})", change_id, change_id);

    let mut cmd = Command::new("jj");
    if let Some(path) = repo_path {
        cmd.current_dir(path);
    }

    let output = cmd
        .args([
            "log",
            "-r",
            &revset,
            "--no-graph",
            "-T",
            "change_id.short()",
        ])
        .output()
        .context("Failed to execute jj log for conflict counting")?;

    if !output.status.success() {
        anyhow::bail!(
            "jj log failed while counting conflicts: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let count = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();
    Ok(count)
}

/// Count conflicts on or after a specific change in the current directory
pub fn count_conflicts(change_id: &str) -> Result<usize> {
    count_conflicts_in(change_id, None)
}

/// Get the change ID of a specific revision
/// Get the description of a given revision
/// If repo_path is provided, runs jj in that directory
pub fn get_commit_description_in(revset: &str, repo_path: Option<&Path>) -> Result<String> {
    let mut cmd = Command::new("jj");
    if let Some(path) = repo_path {
        cmd.current_dir(path);
    }

    let output = cmd
        .args([
            "log",
            "-r",
            revset,
            "-T",
            "description",
            "--no-graph",
            "--ignore-working-copy",
        ])
        .output()
        .context("Failed to execute jj log")?;

    if !output.status.success() {
        anyhow::bail!(
            "jj log failed for revset '{}': {}",
            revset,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let description = String::from_utf8_lossy(&output.stdout);
    Ok(description.trim().to_string())
}

/// Get the description of a given revision in the current directory
pub fn get_commit_description(revset: &str) -> Result<String> {
    get_commit_description_in(revset, None)
}

/// Get the change ID of a given revision
/// If repo_path is provided, runs jj in that directory
pub fn get_change_id_in(revset: &str, repo_path: Option<&Path>) -> Result<String> {
    let mut cmd = Command::new("jj");
    if let Some(path) = repo_path {
        cmd.current_dir(path);
    }

    let output = cmd
        .args([
            "log",
            "-r",
            revset,
            "-T",
            "change_id.short()",
            "--no-graph",
            "--ignore-working-copy",
        ])
        .output()
        .context("Failed to execute jj log to get change ID")?;

    if !output.status.success() {
        anyhow::bail!(
            "jj log failed while getting change ID: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let change_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if change_id.is_empty() {
        anyhow::bail!("No change found for revset: {}", revset);
    }

    Ok(change_id)
}

/// Get the change ID of a specific revision in the current directory
pub fn get_change_id(revset: &str) -> Result<String> {
    get_change_id_in(revset, None)
}

/// Check if the current commit (@) is a precommit for the given session
/// Returns true if @ has a Claude-precommit-session-id trailer matching the session_id
/// If repo_path is provided, runs jj in that directory
pub fn is_current_commit_precommit_for_session_in(
    session_id: &str,
    repo_path: Option<&Path>,
) -> Result<bool> {
    let template =
        r#"trailers.map(|t| if(t.key() == "Claude-precommit-session-id", t.value(), "")).join("")"#;

    let mut cmd = Command::new("jj");
    if let Some(path) = repo_path {
        cmd.current_dir(path);
    }

    let output = cmd
        .args([
            "log",
            "-r",
            "@",
            "-T",
            template,
            "--no-graph",
            "--ignore-working-copy",
        ])
        .output()
        .context("Failed to execute jj log to check precommit")?;

    if !output.status.success() {
        anyhow::bail!(
            "jj log failed while checking precommit: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let precommit_session_id = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // If there's no trailer, this is not a precommit
    if precommit_session_id.is_empty() {
        return Ok(false);
    }

    // Check if the session ID matches
    Ok(precommit_session_id == session_id)
}

/// Check if the current commit (@) is a precommit for the given session in the current directory
pub fn is_current_commit_precommit_for_session(session_id: &str) -> Result<bool> {
    is_current_commit_precommit_for_session_in(session_id, None)
}

/// Check if the current commit (@) has a Claude-session-id trailer
/// Returns the session ID if present, None otherwise
/// If repo_path is provided, runs jj in that directory
pub fn get_current_commit_session_id_in(repo_path: Option<&Path>) -> Result<Option<String>> {
    let template =
        r#"trailers.map(|t| if(t.key() == "Claude-session-id", t.value(), "")).join("")"#;

    let mut cmd = Command::new("jj");
    if let Some(path) = repo_path {
        cmd.current_dir(path);
    }

    let output = cmd
        .args([
            "log",
            "-r",
            "@",
            "-T",
            template,
            "--no-graph",
            "--ignore-working-copy",
        ])
        .output()
        .context("Failed to execute jj log to check session ID")?;

    if !output.status.success() {
        anyhow::bail!(
            "jj log failed while checking session ID: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let session_id = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // If there's no trailer, return None
    if session_id.is_empty() {
        Ok(None)
    } else {
        Ok(Some(session_id))
    }
}

/// Check if the current commit (@) has a Claude-session-id trailer in the current directory
pub fn get_current_commit_session_id() -> Result<Option<String>> {
    get_current_commit_session_id_in(None)
}

/// Get the Claude-session-id trailer from a specific revision
/// If multiple Claude-session-id trailers exist, returns the last one
/// Returns None if no session ID trailer is found
/// If repo_path is provided, runs jj in that directory
pub fn get_session_id_in(revset: &str, repo_path: Option<&Path>) -> Result<Option<String>> {
    // Use jj template to extract only Claude-session-id trailer values
    // We get all of them and will pick the last one
    let template =
        r#"trailers.filter(|t| t.key() == "Claude-session-id").map(|t| t.value()).join("\n")"#;

    let mut cmd = Command::new("jj");
    if let Some(path) = repo_path {
        cmd.current_dir(path);
    }

    let output = cmd
        .args([
            "log",
            "-r",
            revset,
            "-T",
            template,
            "--no-graph",
            "--ignore-working-copy",
        ])
        .output()
        .context("Failed to execute jj log to get session ID")?;

    if !output.status.success() {
        anyhow::bail!(
            "jj log failed for revset '{}': {}",
            revset,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let session_ids_str = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if session_ids_str.is_empty() {
        Ok(None)
    } else {
        // Return the last session ID if multiple exist
        let last_session_id = session_ids_str
            .lines()
            .filter(|line| !line.trim().is_empty())
            .next_back()
            .map(|s| s.to_string());
        Ok(last_session_id)
    }
}

/// Get the Claude-session-id trailer from a specific revision in the current directory
pub fn get_session_id(revset: &str) -> Result<Option<String>> {
    get_session_id_in(revset, None)
}

/// Get all trailers from a specific commit
/// Returns a vector of formatted trailer lines (e.g., "Key: Value")
/// If repo_path is provided, runs jj in that directory
pub fn get_commit_trailers_in(revset: &str, repo_path: Option<&Path>) -> Result<Vec<String>> {
    let template = r#"trailers.map(|t| t.key() ++ ": " ++ t.value()).join("\n")"#;

    let mut cmd = Command::new("jj");
    if let Some(path) = repo_path {
        cmd.current_dir(path);
    }

    let output = cmd
        .args([
            "log",
            "-r",
            revset,
            "-T",
            template,
            "--no-graph",
            "--ignore-working-copy",
        ])
        .output()
        .context("Failed to execute jj log to get trailers")?;

    if !output.status.success() {
        anyhow::bail!(
            "jj log failed while getting trailers: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let trailers_str = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if trailers_str.is_empty() {
        Ok(Vec::new())
    } else {
        Ok(trailers_str.lines().map(|s| s.to_string()).collect())
    }
}

/// Get all trailers from a specific commit in the current directory
pub fn get_commit_trailers(revset: &str) -> Result<Vec<String>> {
    get_commit_trailers_in(revset, None)
}

/// Update a commit's description while preserving its trailers
/// The new_message should not include trailers - they will be automatically appended
/// If repo_path is provided, runs jj in that directory
pub fn update_description_preserving_trailers_in(
    revset: &str,
    new_message: &str,
    repo_path: Option<&Path>,
) -> Result<()> {
    // Get existing trailers
    let trailers = get_commit_trailers_in(revset, repo_path)?;

    // Build the complete message: new message + blank line + trailers
    let complete_message = if trailers.is_empty() {
        new_message.to_string()
    } else {
        format!("{}\n\n{}", new_message.trim(), trailers.join("\n"))
    };

    // Update the commit description
    let mut cmd = Command::new("jj");
    if let Some(path) = repo_path {
        cmd.current_dir(path);
    }

    let output = cmd
        .args(["describe", "-r", revset, "-m", &complete_message])
        .output()
        .context("Failed to execute jj describe")?;

    if !output.status.success() {
        anyhow::bail!(
            "jj describe failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(())
}

/// Update a commit's description while preserving its trailers in the current directory
pub fn update_description_preserving_trailers(revset: &str, new_message: &str) -> Result<()> {
    update_description_preserving_trailers_in(revset, new_message, None)
}

/// Attempt to squash precommit into session change (happy path)
/// Returns true if new conflicts were introduced, false otherwise
/// If repo_path is provided, runs jj in that directory
///
/// This function:
/// 1. Counts conflicts on the session change before squash
/// 2. Squashes the precommit into the session change (from current position, without edit)
/// 3. Restores uwc by squashing it into the new empty commit
/// 4. Counts conflicts after squash
/// 5. Returns whether new conflicts were introduced
pub fn squash_precommit_into_session_in(
    _precommit_id: &str,
    session_id: &str,
    uwc_id: &str,
    repo_path: Option<&Path>,
) -> Result<bool> {
    // Count conflicts before squash
    let conflicts_before = count_conflicts_in(session_id, repo_path)?;

    // Get uwc description before modifying anything
    let uwc_description = get_commit_description_in(uwc_id, repo_path)?;

    // Squash precommit into session (from current position @ = precommit)
    // This leaves us on a new empty commit above uwc
    let mut cmd = Command::new("jj");
    if let Some(path) = repo_path {
        cmd.current_dir(path);
    }
    let output = cmd
        .args(["squash", "--into", session_id, "--use-destination-message"])
        .output()
        .context("Failed to execute jj squash")?;

    if !output.status.success() {
        anyhow::bail!(
            "jj squash failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Now we're on a new empty commit above uwc
    // Restore uwc by squashing it into the current empty commit
    let mut cmd = Command::new("jj");
    if let Some(path) = repo_path {
        cmd.current_dir(path);
    }
    let output = cmd
        .args([
            "squash",
            "--from",
            "@-", // from uwc (which is now @-)
            "--into",
            "@", // into current empty commit
            "-m",
            &uwc_description, // preserve uwc's description
        ])
        .output()
        .context("Failed to restore uwc")?;

    if !output.status.success() {
        anyhow::bail!(
            "Failed to restore uwc: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Count conflicts after squash
    let conflicts_after = count_conflicts_in(session_id, repo_path)?;

    // Return true if new conflicts were introduced
    Ok(conflicts_after > conflicts_before)
}

/// Attempt to squash precommit into session change in the current directory
pub fn squash_precommit_into_session(
    precommit_id: &str,
    session_id: &str,
    uwc_id: &str,
) -> Result<bool> {
    squash_precommit_into_session_in(precommit_id, session_id, uwc_id, None)
}

/// Handle squash conflicts by undoing and renaming precommit to "pt. N"
/// If repo_path is provided, runs jj in that directory
///
/// This function:
/// 1. Runs `jj undo` twice to revert both squash operations (precommit->session, uwc->@)
/// 2. Renames precommit to "jjagent: session {short_id} pt. {part}"
/// 3. Creates a new working copy on top
/// 4. Attempts to move uwc to the tip by squashing it into the new working copy
pub fn handle_squash_conflicts_in(
    session_id: &SessionId,
    part: usize,
    repo_path: Option<&Path>,
) -> Result<()> {
    // Undo twice: once for uwc restoration squash, once for precommit->session squash
    for _ in 0..2 {
        let mut cmd = Command::new("jj");
        if let Some(path) = repo_path {
            cmd.current_dir(path);
        }
        let output = cmd
            .args(["undo"])
            .output()
            .context("Failed to execute jj undo")?;

        if !output.status.success() {
            anyhow::bail!(
                "jj undo failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    // Rename precommit to "pt. N" with trailer
    let message = crate::session::format_session_part_message(session_id, part);
    let mut cmd = Command::new("jj");
    if let Some(path) = repo_path {
        cmd.current_dir(path);
    }
    let output = cmd
        .args(["describe", "-m", &message])
        .output()
        .context("Failed to execute jj describe")?;

    if !output.status.success() {
        anyhow::bail!(
            "jj describe failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Create new working copy on top
    let mut cmd = Command::new("jj");
    if let Some(path) = repo_path {
        cmd.current_dir(path);
    }
    let output = cmd
        .args(["new"])
        .output()
        .context("Failed to execute jj new")?;

    if !output.status.success() {
        anyhow::bail!("jj new failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    // Try to move uwc to the tip
    // Find the uwc by looking for the first non-session change in ancestors
    // This should be the user's working copy that existed before the session changes
    let mut cmd = Command::new("jj");
    if let Some(path) = repo_path {
        cmd.current_dir(path);
    }

    // Use jj template to mark each commit as SESSION or OTHER based on trailer presence
    let template = r#"if(trailers.any(|t| t.key() == "Claude-session-id"), "SESSION:", "OTHER:") ++ change_id ++ "\n""#;
    let log_output = cmd
        .args([
            "log",
            "-r",
            "::@- & ~root()", // All ancestors of @- except root
            "--no-graph",
            "-T",
            template,
        ])
        .output()
        .context("Failed to get ancestor changes")?;

    // Find a non-session change that appears to be "trapped" between session changes
    let mut uwc_id = None;
    if log_output.status.success() {
        let output = String::from_utf8_lossy(&log_output.stdout);
        let mut found_session = false;

        for line in output.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if line.starts_with("SESSION:") {
                found_session = true;
            } else if let Some(change_id) = line.strip_prefix("OTHER:")
                && found_session
            {
                uwc_id = Some(change_id.to_string());
                break;
            }
        }
    }

    if let Some(uwc_id) = uwc_id {
        // First get the uwc's description to preserve it
        let mut cmd = Command::new("jj");
        if let Some(path) = repo_path {
            cmd.current_dir(path);
        }
        let desc_output = cmd
            .args(["log", "-r", &uwc_id, "--no-graph", "-T", "description"])
            .output()
            .context("Failed to get uwc description")?;

        if !desc_output.status.success() {
            anyhow::bail!(
                "Failed to get uwc description: {}",
                String::from_utf8_lossy(&desc_output.stderr)
            );
        }

        let uwc_description = String::from_utf8_lossy(&desc_output.stdout)
            .trim()
            .to_string();

        // Count conflicts in the entire stack before attempting squash
        // We need to check from root:: to catch all conflicts
        let conflicts_before = count_conflicts_in("root()", repo_path)?;

        // Try to squash uwc into the new working copy, preserving uwc's description
        let mut cmd = Command::new("jj");
        if let Some(path) = repo_path {
            cmd.current_dir(path);
        }
        let squash_output = cmd
            .args([
                "squash",
                "--from",
                &uwc_id,
                "--into",
                "@",
                "-m",
                &uwc_description,
            ])
            .output()
            .context("Failed to squash uwc to tip")?;

        if squash_output.status.success() {
            // Check if new conflicts were introduced anywhere in the stack
            let conflicts_after = count_conflicts_in("root()", repo_path)?;

            if conflicts_after > conflicts_before {
                // New conflicts introduced, undo the squash
                let mut cmd = Command::new("jj");
                if let Some(path) = repo_path {
                    cmd.current_dir(path);
                }
                let undo_output = cmd
                    .args(["undo"])
                    .output()
                    .context("Failed to undo uwc squash")?;

                if !undo_output.status.success() {
                    anyhow::bail!(
                        "Failed to undo uwc squash: {}",
                        String::from_utf8_lossy(&undo_output.stderr)
                    );
                }
            }
            // If no new conflicts, we successfully moved uwc to the tip
        }
    }

    Ok(())
}

/// Handle squash conflicts in the current directory
pub fn handle_squash_conflicts(session_id: &SessionId, part: usize) -> Result<()> {
    handle_squash_conflicts_in(session_id, part, None)
}

/// Split a change by inserting a new change before @ (working copy)
/// The reference can be either a Claude session ID or a jj reference (change ID, revset, etc.)
/// Session IDs are looked up first before treating as a jj ref
/// The reference must be an ancestor of @
/// If the reference has a session ID, creates a new session part
pub fn split_change(reference: &str, repo_path: Option<&Path>) -> Result<()> {
    // First, try to interpret reference as a Claude session ID
    let actual_reference = match find_session_change_anywhere_in(reference, repo_path)? {
        Some(change_id) => {
            // Found a session by ID, use the change_id
            change_id
        }
        None => {
            // Not a session ID, treat as a jj reference
            reference.to_string()
        }
    };

    // Check if reference is an ancestor of @
    let mut cmd = Command::new("jj");
    if let Some(path) = repo_path {
        cmd.current_dir(path);
    }
    let output = cmd
        .args([
            "log",
            "-r",
            &format!("{}..@", actual_reference),
            "--no-graph",
            "-T",
            "change_id.short()",
        ])
        .output()
        .context("Failed to check if reference is an ancestor")?;

    if !output.status.success() {
        anyhow::bail!(
            "Failed to check ancestry: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    // If the output is empty, then reference is not a proper ancestor
    if stdout.trim().is_empty() {
        anyhow::bail!("Reference '{}' is not an ancestor of @", reference);
    }

    // Get the session ID from the reference commit using trailers
    // We extract the first Claude-session-id trailer value
    let template =
        r#"trailers.map(|t| if(t.key() == "Claude-session-id", t.value(), "")).join("\n")"#;
    let mut cmd = Command::new("jj");
    if let Some(path) = repo_path {
        cmd.current_dir(path);
    }
    let output = cmd
        .args(["log", "-r", &actual_reference, "--no-graph", "-T", template])
        .output()
        .context("Failed to get reference commit info")?;

    if !output.status.success() {
        anyhow::bail!(
            "Failed to get reference commit: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let session_id_output = String::from_utf8_lossy(&output.stdout);
    let session_id = session_id_output
        .lines()
        .find(|line| !line.trim().is_empty())
        .context("Reference commit does not have a Claude-session-id trailer")?;

    let session_id = SessionId::from_full(session_id);

    // Count existing session parts
    let next_part = count_session_parts_in(session_id.full(), repo_path)? + 1;

    // Insert a new change before @, keeping @ as working copy
    let message = crate::session::format_session_part_message(&session_id, next_part);
    let mut cmd = Command::new("jj");
    if let Some(path) = repo_path {
        cmd.current_dir(path);
    }
    let output = cmd
        .args(["new", "--insert-before", "@", "--no-edit", "-m", &message])
        .output()
        .context("Failed to insert new change")?;

    if !output.status.success() {
        anyhow::bail!(
            "Failed to insert new change: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(())
}

/// Move session tracking to an existing jj revision
/// Verifies the reference is an ancestor of @ and updates its description with the session ID trailer
pub fn move_session_into(
    session_id: &str,
    reference: &str,
    repo_path: Option<&Path>,
) -> Result<()> {
    // Verify that reference is an ancestor of @ (working copy)
    // Use ref..@ to check if there are descendants between ref and @
    // If ref is @ itself, this will be empty, which means it's not a proper ancestor
    let mut cmd = Command::new("jj");
    if let Some(path) = repo_path {
        cmd.current_dir(path);
    }
    let output = cmd
        .args([
            "log",
            "-r",
            &format!("{}..@", reference),
            "--no-graph",
            "-T",
            "change_id.short()",
        ])
        .output()
        .context("Failed to verify ancestry")?;

    if !output.status.success() {
        anyhow::bail!(
            "Error: '{}' is not an ancestor of the working copy",
            reference
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    // If the output is empty, then reference is @ or is not an ancestor
    if stdout.trim().is_empty() {
        anyhow::bail!(
            "Error: '{}' is not an ancestor of the working copy",
            reference
        );
    }

    // Get the current description of the target revision
    let current_description = get_commit_description_in(reference, repo_path)?;

    // Parse the description to extract title and existing trailers
    let (title, existing_trailers) = parse_description_and_trailers(&current_description);

    // Remove any existing Claude-session-id trailers
    let filtered_trailers: Vec<String> = existing_trailers
        .into_iter()
        .filter(|t| !t.starts_with("Claude-session-id:"))
        .collect();

    // Add the new session ID trailer
    let mut new_trailers = filtered_trailers;
    new_trailers.push(format!("Claude-session-id: {}", session_id));

    // Build the complete message
    let complete_message = if new_trailers.is_empty() {
        title
    } else {
        format!("{}\n\n{}", title.trim(), new_trailers.join("\n"))
    };

    // Update the commit description
    let mut cmd = Command::new("jj");
    if let Some(path) = repo_path {
        cmd.current_dir(path);
    }

    let output = cmd
        .args(["describe", "-r", reference, "-m", &complete_message])
        .output()
        .context("Failed to execute jj describe")?;

    if !output.status.success() {
        anyhow::bail!(
            "jj describe failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(())
}

/// Parse a commit description into title and trailers
/// Returns (title, trailers) where trailers is a Vec of "Key: Value" strings
fn parse_description_and_trailers(description: &str) -> (String, Vec<String>) {
    let lines: Vec<&str> = description.lines().collect();

    // Find where trailers start (after the last blank line)
    let mut trailer_start = None;
    for (i, line) in lines.iter().enumerate().rev() {
        if line.trim().is_empty() {
            trailer_start = Some(i + 1);
            break;
        }
    }

    match trailer_start {
        Some(start) if start < lines.len() => {
            // Check if lines after the blank line are actually trailers
            let potential_trailers: Vec<&str> = lines[start..].to_vec();
            let are_trailers = potential_trailers
                .iter()
                .all(|line| line.contains(':') || line.trim().is_empty());

            if are_trailers {
                let title = lines[..start - 1].join("\n");
                let trailers: Vec<String> = potential_trailers
                    .iter()
                    .filter(|line| !line.trim().is_empty())
                    .map(|s| s.to_string())
                    .collect();
                (title, trailers)
            } else {
                // Not trailers, entire description is title
                (description.to_string(), Vec::new())
            }
        }
        _ => {
            // No blank line found, entire description is title
            (description.to_string(), Vec::new())
        }
    }
}

/// Parse change IDs from jj log output
/// Format: change_id\n per line
fn parse_change_ids(output: &str) -> Vec<String> {
    output
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .map(|s| s.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_change_ids_single() {
        let output = "abcd1234\n";
        let change_ids = parse_change_ids(output);
        assert_eq!(change_ids.len(), 1);
        assert_eq!(change_ids[0], "abcd1234");
    }

    #[test]
    fn test_parse_change_ids_multiple() {
        let output = "abcd1234\nefgh5678\nijkl9012\n";
        let change_ids = parse_change_ids(output);
        assert_eq!(change_ids.len(), 3);
        assert_eq!(change_ids[0], "abcd1234");
        assert_eq!(change_ids[1], "efgh5678");
        assert_eq!(change_ids[2], "ijkl9012");
    }

    #[test]
    fn test_parse_change_ids_empty() {
        let output = "";
        let change_ids = parse_change_ids(output);
        assert_eq!(change_ids.len(), 0);
    }

    #[test]
    fn test_parse_change_ids_with_whitespace() {
        let output = "  abcd1234  \n\n  efgh5678  \n";
        let change_ids = parse_change_ids(output);
        assert_eq!(change_ids.len(), 2);
        assert_eq!(change_ids[0], "abcd1234");
        assert_eq!(change_ids[1], "efgh5678");
    }
}
