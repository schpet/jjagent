use jjagent::hooks::{HookInput, HookResponse};
use std::io::Write;

#[test]
fn test_hook_response_continue() {
    let response = HookResponse::continue_execution();
    let json = serde_json::to_string(&response).unwrap();
    assert_eq!(json, r#"{"continue":true}"#);
}

#[test]
fn test_hook_response_stop() {
    let response = HookResponse::stop("test error message");
    let json = serde_json::to_string(&response).unwrap();
    assert_eq!(
        json,
        r#"{"continue":false,"stopReason":"test error message"}"#
    );
}

#[test]
fn test_hook_response_stop_does_not_include_null_reason() {
    let response = HookResponse::continue_execution();
    let json = serde_json::to_string(&response).unwrap();
    // Verify that stopReason is not included when None (due to skip_serializing_if)
    assert!(!json.contains("stopReason"));
}

#[test]
fn test_hook_response_with_context() {
    let response = HookResponse::with_context("UserPromptSubmit", "Test context message");
    let json = serde_json::to_string(&response).unwrap();
    assert_eq!(
        json,
        r#"{"continue":true,"hookSpecificOutput":{"hookEventName":"UserPromptSubmit","additionalContext":"Test context message"}}"#
    );
}

#[test]
fn test_user_prompt_submit_hook_without_transcript() {
    let input = HookInput {
        session_id: "test-session-456".to_string(),
        tool_name: None,
        hook_event_name: Some("UserPromptSubmit".to_string()),
        transcript_path: None,
    };

    let response = jjagent::hooks::handle_user_prompt_submit_hook(&input).unwrap();
    let json = serde_json::to_string(&response).unwrap();

    // Without transcript, should just continue without injecting
    assert_eq!(json, r#"{"continue":true}"#);
}

#[test]
fn test_user_prompt_submit_hook_first_session() {
    // Create a temporary transcript file with no previous session ID
    let temp_dir = tempfile::tempdir().unwrap();
    let transcript_path = temp_dir.path().join("transcript.txt");
    let mut file = std::fs::File::create(&transcript_path).unwrap();
    writeln!(file, "Some conversation").unwrap();
    writeln!(file, "without any session ID marker").unwrap();

    let input = HookInput {
        session_id: "test-session-first".to_string(),
        tool_name: None,
        hook_event_name: Some("UserPromptSubmit".to_string()),
        transcript_path: Some(transcript_path.to_string_lossy().to_string()),
    };

    let response = jjagent::hooks::handle_user_prompt_submit_hook(&input).unwrap();
    let json = serde_json::to_string(&response).unwrap();

    // No previous session found, should inject it
    assert!(json.contains("test-session-first"));
    assert!(json.contains("UserPromptSubmit"));
    assert!(json.contains("hookSpecificOutput"));
}

#[test]
fn test_user_prompt_submit_hook_same_session() {
    // Create a temporary transcript file with the same session ID
    let temp_dir = tempfile::tempdir().unwrap();
    let transcript_path = temp_dir.path().join("transcript.txt");
    let mut file = std::fs::File::create(&transcript_path).unwrap();
    writeln!(file, "Some conversation").unwrap();
    writeln!(file, "System Note: The current session ID is 12345-abcde. I must use this ID for session-specific tasks.").unwrap();
    writeln!(file, "More conversation").unwrap();

    let input = HookInput {
        session_id: "12345-abcde".to_string(),
        tool_name: None,
        hook_event_name: Some("UserPromptSubmit".to_string()),
        transcript_path: Some(transcript_path.to_string_lossy().to_string()),
    };

    let response = jjagent::hooks::handle_user_prompt_submit_hook(&input).unwrap();
    let json = serde_json::to_string(&response).unwrap();

    // Same session ID found, should just continue
    assert_eq!(json, r#"{"continue":true}"#);
}

#[test]
fn test_user_prompt_submit_hook_different_session() {
    // Create a temporary transcript file with a different session ID
    let temp_dir = tempfile::tempdir().unwrap();
    let transcript_path = temp_dir.path().join("transcript.txt");
    let mut file = std::fs::File::create(&transcript_path).unwrap();
    writeln!(file, "Some conversation").unwrap();
    writeln!(file, "System Note: The current session ID is old-session-id. I must use this ID for session-specific tasks.").unwrap();
    writeln!(file, "More conversation").unwrap();

    let input = HookInput {
        session_id: "new-session-id".to_string(),
        tool_name: None,
        hook_event_name: Some("UserPromptSubmit".to_string()),
        transcript_path: Some(transcript_path.to_string_lossy().to_string()),
    };

    let response = jjagent::hooks::handle_user_prompt_submit_hook(&input).unwrap();
    let json = serde_json::to_string(&response).unwrap();

    // Different session ID, should inject the new one
    assert!(json.contains("new-session-id"));
    assert!(json.contains("UserPromptSubmit"));
    assert!(json.contains("hookSpecificOutput"));
}
