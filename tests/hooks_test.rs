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
    let response = HookResponse::with_context("SessionStart", "Test context message");
    let json = serde_json::to_string(&response).unwrap();
    assert_eq!(
        json,
        r#"{"continue":true,"hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext":"Test context message"}}"#
    );
}

#[test]
fn test_session_start_hook() {
    let input = HookInput {
        session_id: "test-session-123".to_string(),
        tool_name: None,
        hook_event_name: Some("SessionStart".to_string()),
        transcript_path: None,
    };

    let response = jjagent::hooks::handle_session_start_hook(&input).unwrap();
    let json = serde_json::to_string(&response).unwrap();

    // Verify the response contains the session ID
    assert!(json.contains("test-session-123"));
    assert!(json.contains("SessionStart"));
    assert!(json.contains("hookSpecificOutput"));
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
fn test_user_prompt_submit_hook_with_transcript_missing_session() {
    // Create a temporary transcript file
    let temp_dir = tempfile::tempdir().unwrap();
    let transcript_path = temp_dir.path().join("transcript.txt");
    let mut file = std::fs::File::create(&transcript_path).unwrap();
    writeln!(file, "Some conversation").unwrap();
    writeln!(file, "without the session ID").unwrap();
    writeln!(file, "in recent lines").unwrap();

    let input = HookInput {
        session_id: "test-session-789".to_string(),
        tool_name: None,
        hook_event_name: Some("UserPromptSubmit".to_string()),
        transcript_path: Some(transcript_path.to_string_lossy().to_string()),
    };

    let response = jjagent::hooks::handle_user_prompt_submit_hook(&input).unwrap();
    let json = serde_json::to_string(&response).unwrap();

    // Session ID not in transcript, should inject it
    assert!(json.contains("test-session-789"));
    assert!(json.contains("UserPromptSubmit"));
    assert!(json.contains("hookSpecificOutput"));
}

#[test]
fn test_user_prompt_submit_hook_with_transcript_has_session() {
    // Create a temporary transcript file
    let temp_dir = tempfile::tempdir().unwrap();
    let transcript_path = temp_dir.path().join("transcript.txt");
    let mut file = std::fs::File::create(&transcript_path).unwrap();
    writeln!(file, "Some conversation").unwrap();
    writeln!(file, "that includes test-session-999").unwrap();
    writeln!(file, "in recent lines").unwrap();

    let input = HookInput {
        session_id: "test-session-999".to_string(),
        tool_name: None,
        hook_event_name: Some("UserPromptSubmit".to_string()),
        transcript_path: Some(transcript_path.to_string_lossy().to_string()),
    };

    let response = jjagent::hooks::handle_user_prompt_submit_hook(&input).unwrap();
    let json = serde_json::to_string(&response).unwrap();

    // Session ID is in transcript, should just continue
    assert_eq!(json, r#"{"continue":true}"#);
}
