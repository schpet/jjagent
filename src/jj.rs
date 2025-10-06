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

/// Represents a jj commit with its change ID, description, and optional session ID
#[derive(Debug, Clone, PartialEq)]
pub struct Commit {
    pub change_id: String,
    pub description: String,
    pub session_id: Option<String>,
}

/// Get descendants of the current working copy (@)
/// Returns commits ordered from closest to farthest
/// If repo_path is provided, runs jj in that directory
pub fn get_descendants_in(repo_path: Option<&Path>) -> Result<Vec<Commit>> {
    let template = r#"change_id.short() ++ "\n" ++ description ++ "\n" ++ trailers.map(|t| if(t.key() == "Claude-session-id", t.value(), "")).join("") ++ "\n---\n""#;

    let mut cmd = Command::new("jj");
    if let Some(path) = repo_path {
        cmd.current_dir(path);
    }

    let output = cmd
        .args([
            "log",
            "-r",
            "descendants(@) ~ @",
            "-T",
            template,
            "--no-graph",
            "--ignore-working-copy",
        ])
        .output()
        .context("Failed to execute jj log")?;

    if !output.status.success() {
        anyhow::bail!("jj log failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_commits(&stdout)
}

/// Get descendants of the current working copy (@) in the current directory
/// Returns commits ordered from closest to farthest
pub fn get_descendants() -> Result<Vec<Commit>> {
    get_descendants_in(None)
}

/// Find the closest descendant commit with the given session ID
/// Returns None if no matching commit is found
/// If repo_path is provided, runs jj in that directory
pub fn find_session_change_in(
    session_id: &str,
    repo_path: Option<&Path>,
) -> Result<Option<Commit>> {
    let descendants = get_descendants_in(repo_path)?;

    // Descendants are ordered from closest to farthest
    for commit in descendants {
        if let Some(ref commit_session_id) = commit.session_id {
            if commit_session_id == session_id {
                return Ok(Some(commit));
            }
        }
    }

    Ok(None)
}

/// Find the closest descendant commit with the given session ID in the current directory
/// Returns None if no matching commit is found
pub fn find_session_change(session_id: &str) -> Result<Option<Commit>> {
    find_session_change_in(session_id, None)
}

/// Find any commit with the given session ID (not limited to descendants)
/// Returns None if no matching commit is found
/// If repo_path is provided, runs jj in that directory
pub fn find_session_change_anywhere_in(
    session_id: &str,
    repo_path: Option<&Path>,
) -> Result<Option<Commit>> {
    let template = r#"change_id.short() ++ "\n" ++ description ++ "\n" ++ trailers.map(|t| if(t.key() == "Claude-session-id", t.value(), "")).join("") ++ "\n---\n""#;

    let mut cmd = Command::new("jj");
    if let Some(path) = repo_path {
        cmd.current_dir(path);
    }

    let output = cmd
        .args(["log", "-r", "all()", "-T", template, "--no-graph"])
        .output()
        .context("Failed to execute jj log")?;

    if !output.status.success() {
        anyhow::bail!("jj log failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let commits = parse_commits(&stdout)?;

    // Find first commit with matching session ID
    for commit in commits {
        if let Some(ref commit_session_id) = commit.session_id {
            if commit_session_id == session_id {
                return Ok(Some(commit));
            }
        }
    }

    Ok(None)
}

/// Find any commit with the given session ID in the current directory
pub fn find_session_change_anywhere(session_id: &str) -> Result<Option<Commit>> {
    find_session_change_anywhere_in(session_id, None)
}

/// Count how many commits exist with the given session ID
/// This is used to determine the part number for conflict handling
/// If repo_path is provided, runs jj in that directory
pub fn count_session_parts_in(session_id: &str, repo_path: Option<&Path>) -> Result<usize> {
    let template = r#"change_id.short() ++ "\n" ++ description ++ "\n" ++ trailers.map(|t| if(t.key() == "Claude-session-id", t.value(), "")).join("") ++ "\n---\n""#;

    let mut cmd = Command::new("jj");
    if let Some(path) = repo_path {
        cmd.current_dir(path);
    }

    let output = cmd
        .args([
            "log",
            "-r",
            "all()",
            "-T",
            template,
            "--no-graph",
            "--ignore-working-copy",
        ])
        .output()
        .context("Failed to execute jj log")?;

    if !output.status.success() {
        anyhow::bail!("jj log failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let commits = parse_commits(&stdout)?;

    // Count commits with matching session ID
    let count = commits
        .iter()
        .filter(|c| {
            if let Some(ref commit_session_id) = c.session_id {
                commit_session_id == session_id
            } else {
                false
            }
        })
        .count();

    Ok(count)
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
        .args([
            "new",
            "--insert-before",
            "@-",
            "--no-edit",
            "-m",
            &message,
            "--ignore-working-copy",
        ])
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
            "--ignore-working-copy",
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

/// Attempt to squash precommit into session change (happy path)
/// Returns true if new conflicts were introduced, false otherwise
/// If repo_path is provided, runs jj in that directory
///
/// This function:
/// 1. Counts conflicts on the session change before squash
/// 2. Edits to the uwc commit
/// 3. Squashes the precommit into the session change
/// 4. Counts conflicts after squash
/// 5. Returns whether new conflicts were introduced
pub fn squash_precommit_into_session_in(
    precommit_id: &str,
    session_id: &str,
    uwc_id: &str,
    repo_path: Option<&Path>,
) -> Result<bool> {
    // Count conflicts before squash
    let conflicts_before = count_conflicts_in(session_id, repo_path)?;

    // Edit to uwc
    let mut cmd = Command::new("jj");
    if let Some(path) = repo_path {
        cmd.current_dir(path);
    }
    let output = cmd
        .args(["edit", uwc_id, "--ignore-working-copy"])
        .output()
        .context("Failed to execute jj edit")?;

    if !output.status.success() {
        anyhow::bail!(
            "jj edit failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Squash precommit into session
    let mut cmd = Command::new("jj");
    if let Some(path) = repo_path {
        cmd.current_dir(path);
    }
    let output = cmd
        .args([
            "squash",
            "--from",
            precommit_id,
            "--into",
            session_id,
            "--use-destination-message",
            "--ignore-working-copy",
        ])
        .output()
        .context("Failed to execute jj squash")?;

    if !output.status.success() {
        anyhow::bail!(
            "jj squash failed: {}",
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
/// 1. Runs `jj undo` twice to revert squash + edit
/// 2. Renames precommit to "jjagent: session {short_id} pt. {part}"
/// 3. Creates a new working copy on top
pub fn handle_squash_conflicts_in(
    session_id: &SessionId,
    part: usize,
    repo_path: Option<&Path>,
) -> Result<()> {
    // Undo twice: once for squash, once for edit
    for _ in 0..2 {
        let mut cmd = Command::new("jj");
        if let Some(path) = repo_path {
            cmd.current_dir(path);
        }
        let output = cmd
            .args(["undo", "--ignore-working-copy"])
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
        .args(["describe", "-m", &message, "--ignore-working-copy"])
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
        .args(["new", "--ignore-working-copy"])
        .output()
        .context("Failed to execute jj new")?;

    if !output.status.success() {
        anyhow::bail!("jj new failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    Ok(())
}

/// Handle squash conflicts in the current directory
pub fn handle_squash_conflicts(session_id: &SessionId, part: usize) -> Result<()> {
    handle_squash_conflicts_in(session_id, part, None)
}

/// Parse commits from jj log output
/// Format: change_id\ndescription\nsession_id\n---\n
fn parse_commits(output: &str) -> Result<Vec<Commit>> {
    let mut commits = Vec::new();
    let entries: Vec<&str> = output.split("---\n").collect();

    for entry in entries {
        // Don't trim here - we need to preserve the structure
        if entry.is_empty() {
            continue;
        }

        let lines: Vec<&str> = entry.lines().collect();
        if lines.is_empty() {
            continue;
        }

        let change_id = lines[0].trim().to_string();

        // Last line is always the session_id (may be empty)
        let session_id_line = lines.last().context("Expected at least one line")?.trim();
        let session_id = if session_id_line.is_empty() {
            None
        } else {
            Some(session_id_line.to_string())
        };

        // Everything between first and last line is the description
        let description = if lines.len() > 2 {
            lines[1..lines.len() - 1].join("\n")
        } else if lines.len() == 2 {
            // Only change_id and session_id, no description
            String::new()
        } else {
            // Only change_id, this shouldn't happen with our template
            String::new()
        };

        commits.push(Commit {
            change_id,
            description,
            session_id,
        });
    }

    Ok(commits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_commits_single() {
        let output = r#"abcd1234
commit message

---
"#;
        let commits = parse_commits(output).unwrap();
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].change_id, "abcd1234");
        assert_eq!(commits[0].description, "commit message");
        assert_eq!(commits[0].session_id, None);
    }

    #[test]
    fn test_parse_commits_with_trailer() {
        let output = r#"abcd1234
commit message

Claude-session-id: test-session-123
test-session-123
---
"#;
        let commits = parse_commits(output).unwrap();
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].change_id, "abcd1234");
        assert_eq!(
            commits[0].description,
            "commit message\n\nClaude-session-id: test-session-123"
        );
        assert_eq!(commits[0].session_id, Some("test-session-123".to_string()));
    }

    #[test]
    fn test_parse_commits_multiple() {
        let output = r#"abcd1234
first commit

---
efgh5678
second commit

Claude-session-id: session-2
session-2
---
"#;
        let commits = parse_commits(output).unwrap();
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].change_id, "abcd1234");
        assert_eq!(commits[0].description, "first commit");
        assert_eq!(commits[0].session_id, None);
        assert_eq!(commits[1].change_id, "efgh5678");
        assert_eq!(
            commits[1].description,
            "second commit\n\nClaude-session-id: session-2"
        );
        assert_eq!(commits[1].session_id, Some("session-2".to_string()));
    }

    #[test]
    fn test_parse_commits_empty() {
        let output = "";
        let commits = parse_commits(output).unwrap();
        assert_eq!(commits.len(), 0);
    }

    #[test]
    fn test_session_id_field() {
        let commit = Commit {
            change_id: "test123".to_string(),
            description: "jjagent: session abcd1234\n\nClaude-session-id: test-session-id"
                .to_string(),
            session_id: Some("test-session-id".to_string()),
        };

        assert_eq!(commit.session_id, Some("test-session-id".to_string()));
    }

    #[test]
    fn test_session_id_none() {
        let commit = Commit {
            change_id: "test123".to_string(),
            description: "Regular commit".to_string(),
            session_id: None,
        };

        assert_eq!(commit.session_id, None);
    }
}
