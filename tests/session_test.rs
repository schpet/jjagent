use jjagent::session::{
    SessionId, format_precommit_message, format_session_message, format_session_part_message,
};

#[test]
fn test_session_id_from_full() {
    let full = "abcd1234-5678-90ab-cdef-1234567890ab";
    let session_id = SessionId::from_full(full);

    assert_eq!(session_id.full(), full);
    assert_eq!(session_id.short(), "abcd1234");
}

#[test]
fn test_session_id_short_extraction() {
    let full = "12345678-abcd-efgh-ijkl-mnopqrstuvwx";
    let session_id = SessionId::from_full(full);

    assert_eq!(session_id.short(), "12345678");
}

#[test]
fn test_session_id_short_id_less_than_8_chars() {
    let full = "abc";
    let session_id = SessionId::from_full(full);

    assert_eq!(session_id.short(), "abc");
    assert_eq!(session_id.full(), "abc");
}

#[test]
fn test_format_precommit_message() {
    let session_id = SessionId::from_full("abcd1234-5678-90ab-cdef-1234567890ab");
    let message = format_precommit_message(&session_id);

    assert_eq!(message, "jjagent: precommit abcd1234");
}

#[test]
fn test_format_session_message() {
    let session_id = SessionId::from_full("abcd1234-5678-90ab-cdef-1234567890ab");
    let message = format_session_message(&session_id);

    let expected =
        "jjagent: session abcd1234\n\nClaude-session-id: abcd1234-5678-90ab-cdef-1234567890ab";
    assert_eq!(message, expected);
}

#[test]
fn test_format_session_part_message() {
    let session_id = SessionId::from_full("abcd1234-5678-90ab-cdef-1234567890ab");
    let message = format_session_part_message(&session_id, 2);

    let expected = "jjagent: session abcd1234 pt. 2\n\nClaude-session-id: abcd1234-5678-90ab-cdef-1234567890ab";
    assert_eq!(message, expected);
}

#[test]
fn test_format_session_part_message_higher_parts() {
    let session_id = SessionId::from_full("test-session-id");

    assert_eq!(
        format_session_part_message(&session_id, 3),
        "jjagent: session test-ses pt. 3\n\nClaude-session-id: test-session-id"
    );

    assert_eq!(
        format_session_part_message(&session_id, 10),
        "jjagent: session test-ses pt. 10\n\nClaude-session-id: test-session-id"
    );
}

#[test]
fn test_commit_message_with_trailer_format() {
    // Ensure the trailer format follows RFC 2822-like convention
    let session_id = SessionId::from_full("abcd1234-5678-90ab-cdef-1234567890ab");
    let message = format_session_message(&session_id);

    // Should have blank line before trailer
    assert!(message.contains("\n\nClaude-session-id:"));

    // Trailer should be at the end
    assert!(message.ends_with("abcd1234-5678-90ab-cdef-1234567890ab"));
}
