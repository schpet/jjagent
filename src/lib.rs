use anyhow::{Context, Result, bail};
use chrono::Utc;
use std::process::Command;

/// Find a commit with the given session ID
/// If multiple commits have the same session ID, returns the furthest descendant
pub fn find_session_commit(session_id: &str) -> Result<Option<String>> {
    let output = Command::new("jj")
        .args([
            "log",
            "-r",
            &format!(
                "description(glob:'*Jjagent-claude-session-id: {}*')",
                session_id
            ),
            "--no-graph",
            "-T",
            "change_id",
            "--limit",
            "1",
        ])
        .output()
        .context("Failed to search for session commit")?;

    if output.status.success() && !output.stdout.is_empty() {
        Ok(Some(
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
        ))
    } else {
        Ok(None)
    }
}

/// Check if the working copy (@) is a descendant of the given commit
fn is_descendant_of(commit_id: &str) -> Result<bool> {
    // Use jj log to check if @ is a descendant of the commit
    // The revset ::commit_id will include all ancestors of commit_id
    // If @ is in commit_id::, then @ is a descendant of commit_id
    let output = Command::new("jj")
        .args([
            "log",
            "-r",
            &format!("@ & {}::", commit_id),
            "--no-graph",
            "-T",
            "change_id",
            "--limit",
            "1",
        ])
        .output()
        .context("Failed to check descendant relationship")?;

    Ok(output.status.success() && !output.stdout.is_empty())
}

/// Get the change ID of the current working copy (@)
fn get_current_change_id() -> Result<String> {
    let output = Command::new("jj")
        .args(["log", "-r", "@", "--no-graph", "-T", "change_id"])
        .output()
        .context("Failed to get current change id")?;

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Get the first line of a commit's description
fn get_commit_first_line(commit_id: &str) -> Result<String> {
    let output = Command::new("jj")
        .args(["log", "-r", commit_id, "--no-graph", "-T", "description"])
        .output()
        .context("Failed to get commit description")?;

    let description = String::from_utf8_lossy(&output.stdout);
    Ok(description
        .lines()
        .next()
        .unwrap_or("Claude Code Session")
        .to_string())
}

/// Split a session to create a new commit with the same session ID
pub fn session_split(session_id: &str, custom_description: Option<&str>) -> Result<()> {
    // Find the session commit
    let session_commit = find_session_commit(session_id)?
        .ok_or_else(|| anyhow::anyhow!("Session {} not found", session_id))?;

    eprintln!(
        "Found session commit: {}",
        &session_commit[0..12.min(session_commit.len())]
    );

    // Get current working copy
    let current_change_id = get_current_change_id()?;

    // Check the relationship between @ and the session commit
    let is_same = current_change_id == session_commit;
    let is_descendant = if !is_same {
        is_descendant_of(&session_commit)?
    } else {
        false
    };

    // Build the new description
    let new_description = if let Some(desc) = custom_description {
        // Use custom description with just the trailer
        format!("{}\n\nJjagent-claude-session-id: {}", desc, session_id)
    } else {
        // Get the first line of the session commit's description
        let first_line = get_commit_first_line(&session_commit)?;
        let timestamp = Utc::now().to_rfc3339();
        format!(
            "{} (split {})\n\nJjagent-claude-session-id: {}",
            first_line, timestamp, session_id
        )
    };

    if is_same {
        // @ IS the session commit - create new commit on top and move @ to it
        eprintln!("Creating new commit on top of session commit");

        // Create new empty commit on top of the session
        let output = Command::new("jj")
            .args(["new", &session_commit])
            .output()
            .context("Failed to create new commit")?;

        if !output.status.success() {
            bail!(
                "Failed to create new commit: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        // Set the description with trailer
        let mut child = Command::new("jj")
            .args(["describe", "-m", &new_description])
            .spawn()
            .context("Failed to set description")?;

        child.wait()?;

        // Get the new commit ID for display
        let new_id = get_current_change_id()?;
        eprintln!("Created split commit: {}", &new_id[0..12.min(new_id.len())]);
        let display_desc = if let Some(desc) = custom_description {
            desc.to_string()
        } else {
            get_commit_first_line(&session_commit)
                .unwrap_or_else(|_| String::from("Claude Code Session"))
        };
        eprintln!("Description: {}", display_desc);
    } else if is_descendant {
        // @ is a descendant of session - insert new commit before @
        eprintln!("Inserting new commit between session and working copy");

        // Create new empty commit after session and before @ without moving
        let output = Command::new("jj")
            .args([
                "new",
                "--no-edit",
                "--insert-after",
                &session_commit,
                "--insert-before",
                "@",
            ])
            .output()
            .context("Failed to create new commit")?;

        if !output.status.success() {
            bail!(
                "Failed to create new commit: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        // New commit was created at @-, describe it
        let mut child = Command::new("jj")
            .args(["describe", "-r", "@-", "-m", &new_description])
            .spawn()
            .context("Failed to set description")?;

        child.wait()?;

        // Get the new commit ID for display
        let output = Command::new("jj")
            .args(["log", "-r", "@-", "--no-graph", "-T", "change_id"])
            .output()
            .context("Failed to get new commit id")?;

        let new_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        eprintln!("Created split commit: {}", &new_id[0..12.min(new_id.len())]);
        let display_desc = if let Some(desc) = custom_description {
            desc.to_string()
        } else {
            get_commit_first_line(&session_commit)
                .unwrap_or_else(|_| String::from("Claude Code Session"))
        };
        eprintln!("Description: {}", display_desc);
    } else {
        // @ is not a descendant of the session commit
        bail!(
            "Working copy must be a descendant of session commit {}",
            &session_commit[0..12.min(session_commit.len())]
        );
    }

    Ok(())
}
