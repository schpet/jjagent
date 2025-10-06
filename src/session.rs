//! Session ID management and commit message formatting.
//!
//! This module provides utilities for working with Claude Code session IDs:
//! - SessionId type with full and short forms
//! - Commit message formatting for precommit and session changes
//! - Trailer formatting for storing session metadata

/// Represents a Claude Code session ID with both full and short forms
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionId {
    full: String,
    short: String,
}

impl SessionId {
    /// Create a SessionId from a full session ID string
    /// The short form is the first 8 characters of the full ID
    pub fn from_full(full_id: &str) -> Self {
        let short = full_id.chars().take(8).collect();
        Self {
            full: full_id.to_string(),
            short,
        }
    }

    /// Get the full session ID
    pub fn full(&self) -> &str {
        &self.full
    }

    /// Get the short session ID (first 8 characters)
    pub fn short(&self) -> &str {
        &self.short
    }
}

/// Format a precommit message for the given session
/// Example: "jjagent: precommit abcd1234"
pub fn format_precommit_message(session_id: &SessionId) -> String {
    format!("jjagent: precommit {}", session_id.short())
}

/// Format a session message with trailer for the given session
/// Example:
/// ```text
/// jjagent: session abcd1234
///
/// Claude-session-id: abcd1234-5678-90ab-cdef-1234567890ab
/// ```
pub fn format_session_message(session_id: &SessionId) -> String {
    format!(
        "jjagent: session {}\n\nClaude-session-id: {}",
        session_id.short(),
        session_id.full()
    )
}

/// Format a session part message (for conflict scenarios)
/// Example:
/// ```text
/// jjagent: session abcd1234 pt. 2
///
/// Claude-session-id: abcd1234-5678-90ab-cdef-1234567890ab
/// ```
pub fn format_session_part_message(session_id: &SessionId, part: usize) -> String {
    format!(
        "jjagent: session {} pt. {}\n\nClaude-session-id: {}",
        session_id.short(),
        part,
        session_id.full()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_id_basic() {
        let sid = SessionId::from_full("test-full-id");
        assert_eq!(sid.full(), "test-full-id");
        assert_eq!(sid.short(), "test-ful");
    }

    #[test]
    fn test_message_formats() {
        let sid = SessionId::from_full("abcd1234");
        assert_eq!(
            format_precommit_message(&sid),
            "jjagent: precommit abcd1234"
        );
        assert!(format_session_message(&sid).contains("Claude-session-id:"));
        assert!(format_session_part_message(&sid, 2).contains("pt. 2"));
    }
}
