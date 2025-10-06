use jjagent::hooks::HookResponse;

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
