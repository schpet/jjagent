use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use tempfile::TempDir;

/// Strip ANSI color codes from a string for easier snapshot comparison
fn strip_ansi_codes(s: &str) -> String {
    let re = regex::Regex::new(r"\x1b\[[0-9;]*m").unwrap();
    re.replace_all(s, "").to_string()
}

/// Redact random IDs from output for stable snapshots
fn redact_ids(s: &str) -> String {
    let re = regex::Regex::new(r"[a-z]{8} [a-f0-9]{8}").unwrap();
    re.replace(s, "[CHANGE_ID] [COMMIT_ID]").to_string()
}

#[test]
fn test_statusline_ignores_extra_fields() {
    let repo = create_test_jj_repo();
    let repo_path = repo.path();
    let session_id = "test-session-extra-fields";

    // Create a session change
    create_session_change(repo_path, session_id, "Test extra fields");

    // Run statusline with extra fields that don't exist yet
    let input = format!(
        r#"{{
            "session_id": "{}",
            "workspace": {{
                "current_dir": "{}",
                "extra_workspace_field": "should be ignored"
            }},
            "model": {{
                "display_name": "Test Model"
            }},
            "future_field": "should also be ignored",
            "another_object": {{
                "nested": "data"
            }}
        }}"#,
        session_id,
        repo_path.display()
    );

    let output = run_statusline(&input);
    let stripped = strip_ansi_codes(&output);

    // Should still work and contain the feature info
    assert!(stripped.contains("Test extra fields"));

    // Verify output is not empty
    assert!(!stripped.is_empty());
}

/// Create a test jj repo
fn create_test_jj_repo() -> TempDir {
    let dir = TempDir::new().unwrap();
    let path = dir.path();

    // Initialize jj repo
    Command::new("jj")
        .args(["git", "init", "--colocate"])
        .current_dir(path)
        .output()
        .unwrap();

    // Create an initial commit
    fs::write(path.join("initial.txt"), "initial content").unwrap();
    Command::new("jj")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(path)
        .output()
        .unwrap();

    dir
}

/// Create a test git repo
fn create_test_git_repo() -> TempDir {
    let dir = TempDir::new().unwrap();
    let path = dir.path();

    // Initialize git repo
    Command::new("git")
        .args(["init"])
        .current_dir(path)
        .output()
        .unwrap();

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(path)
        .output()
        .unwrap();

    Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(path)
        .output()
        .unwrap();

    // Create an initial commit
    fs::write(path.join("initial.txt"), "initial content").unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(path)
        .output()
        .unwrap();

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(path)
        .output()
        .unwrap();

    Command::new("git")
        .args(["checkout", "-b", "feature-branch"])
        .current_dir(path)
        .output()
        .unwrap();

    dir
}

/// Run the statusline command with given input
fn run_statusline(input: &str) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_jjagent"))
        .args(["claude", "statusline"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    use std::io::Write;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input.as_bytes()).unwrap();
        stdin.flush().unwrap();
    }

    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "statusline command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8(output.stdout).unwrap()
}

/// Create a session change in a jj repo
fn create_session_change(repo_path: &Path, session_id: &str, message: &str) {
    let jjagent_binary = env!("CARGO_BIN_EXE_jjagent");

    // Get the session message
    let session_message_output = Command::new(jjagent_binary)
        .args(["session-message", session_id, message])
        .output()
        .unwrap();

    let session_message = String::from_utf8(session_message_output.stdout).unwrap();

    // Create a change with the session message
    Command::new("jj")
        .args(["new", "-m", &session_message])
        .current_dir(repo_path)
        .output()
        .unwrap();
}

#[test]
fn test_statusline_with_jj_session() {
    let repo = create_test_jj_repo();
    let repo_path = repo.path();
    let session_id = "test-session-123";

    // Create a session change
    create_session_change(repo_path, session_id, "Test feature");

    // Run statusline
    let input = format!(
        r#"{{
            "session_id": "{}",
            "workspace": {{
                "current_dir": "{}"
            }},
            "model": {{
                "display_name": "Test Model"
            }}
        }}"#,
        session_id,
        repo_path.display()
    );

    let output = run_statusline(&input);
    let stripped = strip_ansi_codes(&output);

    // The output should ONLY contain session info (jj part)
    assert!(stripped.contains("Test feature"));
    assert!(!stripped.contains(repo_path.to_str().unwrap()));
    assert!(!stripped.contains("Test Model"));

    // Redact the change ID and commit ID which are random
    let redacted = redact_ids(&stripped);

    insta::assert_snapshot!(redacted);
}

#[test]
fn test_statusline_without_jj_session() {
    let repo = create_test_jj_repo();
    let repo_path = repo.path();

    // Don't create a session change, use a non-existent session ID
    let session_id = "non-existent-session";

    let input = format!(
        r#"{{
            "session_id": "{}",
            "workspace": {{
                "current_dir": "{}"
            }},
            "model": {{
                "display_name": "Test Model"
            }}
        }}"#,
        session_id,
        repo_path.display()
    );

    let output = run_statusline(&input);
    let stripped = strip_ansi_codes(&output);

    // The output should be empty (no session change)
    assert_eq!(stripped, "");

    insta::assert_snapshot!(stripped);
}

#[test]
fn test_statusline_with_git_repo() {
    let repo = create_test_git_repo();
    let repo_path = repo.path();

    let input = format!(
        r#"{{
            "session_id": "test-session",
            "workspace": {{
                "current_dir": "{}"
            }},
            "model": {{
                "display_name": "Test Model"
            }}
        }}"#,
        repo_path.display()
    );

    let output = run_statusline(&input);
    let stripped = strip_ansi_codes(&output);

    // The output should be empty (not a jj repo)
    assert_eq!(stripped, "");

    insta::assert_snapshot!(stripped);
}
