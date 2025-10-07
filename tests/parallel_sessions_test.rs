//! Integration tests for parallel Claude sessions with locking
//!
//! These tests verify that when multiple Claude sessions run concurrently,
//! the working copy lock prevents divergence and race conditions.

use std::process::Command;
use std::sync::{Arc, Barrier};
use std::thread;
use tempfile::TempDir;

fn create_test_repo() -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path();

    // Initialize a jj repo
    Command::new("jj")
        .args(["git", "init", "--colocate"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to initialize jj repo");

    // Create initial commit
    std::fs::write(repo_path.join("README.md"), "# Test Repo\n").unwrap();
    Command::new("jj")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to create initial commit");

    // Create uwc
    Command::new("jj")
        .args(["new", "-m", "uwc"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to create uwc");

    temp_dir
}

fn run_pretool_hook(repo_path: &std::path::Path, session_id: &str) -> Result<(), String> {
    let exe_path = std::env::current_exe().unwrap();
    let jjagent_path = exe_path.parent().unwrap().parent().unwrap().join("jjagent");

    let input = format!(r#"{{"session_id":"{}"}}"#, session_id);

    let output = Command::new(&jjagent_path)
        .args(["claude", "hooks", "PreToolUse"])
        .current_dir(repo_path)
        .env("PATH", std::env::var("PATH").unwrap())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| format!("Failed to run PreToolUse: {}", e))?;

    // Write input
    use std::io::Write;
    let mut child = Command::new(&jjagent_path)
        .args(["claude", "hooks", "PreToolUse"])
        .current_dir(repo_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn PreToolUse: {}", e))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(input.as_bytes())
            .map_err(|e| format!("Failed to write stdin: {}", e))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("Failed to wait for PreToolUse: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "PreToolUse failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}

fn run_posttool_hook(repo_path: &std::path::Path, session_id: &str) -> Result<(), String> {
    let exe_path = std::env::current_exe().unwrap();
    let jjagent_path = exe_path.parent().unwrap().parent().unwrap().join("jjagent");

    let input = format!(r#"{{"session_id":"{}"}}"#, session_id);

    let mut child = Command::new(&jjagent_path)
        .args(["claude", "hooks", "PostToolUse"])
        .current_dir(repo_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn PostToolUse: {}", e))?;

    use std::io::Write;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(input.as_bytes())
            .map_err(|e| format!("Failed to write stdin: {}", e))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("Failed to wait for PostToolUse: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "PostToolUse failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}

#[test]
fn test_parallel_sessions_with_locking() {
    let temp_dir = create_test_repo();
    let repo_path = temp_dir.path();

    let session_id_1 = "session-1-12345678-1234-1234-1234-123456789012";
    let session_id_2 = "session-2-87654321-4321-4321-4321-210987654321";

    // Use a barrier to ensure both threads start at the same time
    let barrier = Arc::new(Barrier::new(2));
    let repo_path_1 = repo_path.to_path_buf();
    let repo_path_2 = repo_path.to_path_buf();
    let barrier_1 = Arc::clone(&barrier);
    let barrier_2 = Arc::clone(&barrier);

    let session_1_thread = thread::spawn(move || {
        barrier_1.wait(); // Wait for both threads to be ready
        eprintln!("Session 1: Running PreToolUse");
        run_pretool_hook(&repo_path_1, session_id_1)?;

        // Simulate tool execution
        thread::sleep(std::time::Duration::from_millis(100));

        eprintln!("Session 1: Running PostToolUse");
        run_posttool_hook(&repo_path_1, session_id_1)?;

        Ok::<(), String>(())
    });

    let session_2_thread = thread::spawn(move || {
        barrier_2.wait(); // Wait for both threads to be ready
        eprintln!("Session 2: Running PreToolUse");
        run_pretool_hook(&repo_path_2, session_id_2)?;

        // Simulate tool execution
        thread::sleep(std::time::Duration::from_millis(100));

        eprintln!("Session 2: Running PostToolUse");
        run_posttool_hook(&repo_path_2, session_id_2)?;

        Ok::<(), String>(())
    });

    let result_1 = session_1_thread.join().expect("Session 1 thread panicked");
    let result_2 = session_2_thread.join().expect("Session 2 thread panicked");

    // One or both should succeed - the important thing is no divergence
    eprintln!("Session 1 result: {:?}", result_1);
    eprintln!("Session 2 result: {:?}", result_2);

    // Check that there are no divergent changes
    let log_output = Command::new("jj")
        .args(["log", "-T", "change_id.short()"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to run jj log");

    let log_str = String::from_utf8_lossy(&log_output.stdout);
    eprintln!("Final jj log:\n{}", log_str);

    // Count occurrences of each change ID - no ID should appear more than once
    let mut change_ids = std::collections::HashMap::new();
    for line in log_str.lines() {
        let change_id = line.trim();
        if !change_id.is_empty() && !change_id.starts_with('@') {
            *change_ids.entry(change_id.to_string()).or_insert(0) += 1;
        }
    }

    for (change_id, count) in &change_ids {
        assert_eq!(
            *count, 1,
            "Change ID {} appears {} times - divergence detected!",
            change_id, count
        );
    }

    // At least one session should have succeeded
    assert!(
        result_1.is_ok() || result_2.is_ok(),
        "Both sessions failed: {:?}, {:?}",
        result_1,
        result_2
    );
}
