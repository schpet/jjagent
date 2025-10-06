use anyhow::Result;
use chrono::Utc;
use serde::Serialize;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;

/// Log entry structure for JSONL output
#[derive(Debug, Serialize)]
pub struct LogEntry {
    timestamp: String,
    event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    jj_change_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    commit_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    prompt_preview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<serde_json::Value>,
}

/// Logger instance that writes to a JSONL file
pub struct Logger {
    file_path: Option<PathBuf>,
    mutex: Mutex<()>,
}

impl Default for Logger {
    fn default() -> Self {
        Self::new()
    }
}

impl Logger {
    /// Create a new logger based on environment variables
    pub fn new() -> Self {
        let file_path = if let Ok(custom_path) = env::var("JJAGENT_LOG_FILE") {
            Some(PathBuf::from(custom_path))
        } else if env::var("JJAGENT_LOG").unwrap_or_default() == "1" {
            Some(Self::default_log_path())
        } else {
            None
        };

        Logger {
            file_path,
            mutex: Mutex::new(()),
        }
    }

    /// Get the default log file path: ~/Library/Caches/jjagent/jjagent.jsonl on macOS, ~/.cache/jjagent/jjagent.jsonl elsewhere
    fn default_log_path() -> PathBuf {
        let cache_dir = env::var("XDG_CACHE_HOME")
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                dirs::home_dir().map(|h| {
                    if cfg!(target_os = "macos") {
                        h.join("Library").join("Caches")
                    } else {
                        h.join(".cache")
                    }
                })
            })
            .unwrap_or_else(|| PathBuf::from("/tmp"));

        cache_dir.join("jjagent").join("jjagent.jsonl")
    }

    /// Check if logging is enabled
    pub fn is_enabled(&self) -> bool {
        self.file_path.is_some()
    }

    /// Log an event
    pub fn log(&self, mut entry: LogEntry) -> Result<()> {
        let Some(ref path) = self.file_path else {
            return Ok(());
        };

        // Ensure the directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Add current working directory if not set
        if entry.cwd.is_none() {
            entry.cwd = env::current_dir()
                .ok()
                .and_then(|p| p.to_str().map(String::from));
        }

        // Add jj change_id and commit_id if not set
        if entry.jj_change_id.is_none() || entry.commit_id.is_none() {
            if let Ok(change_id) = get_jj_change_id() {
                entry.jj_change_id = Some(change_id);
            }
            if let Ok(commit_id) = get_commit_id() {
                entry.commit_id = Some(commit_id);
            }
        }

        // Serialize to JSON and append to file
        let json = serde_json::to_string(&entry)?;

        // Lock to ensure thread-safe writes
        let _guard = self.mutex.lock().unwrap();

        let mut file = OpenOptions::new().create(true).append(true).open(path)?;

        writeln!(file, "{}", json)?;

        Ok(())
    }

    /// Log a hook invocation
    pub fn log_hook(
        &self,
        hook_name: &str,
        session_id: Option<&str>,
        tool_name: Option<&str>,
        prompt: Option<&str>,
    ) {
        if !self.is_enabled() {
            return;
        }

        let prompt_preview = prompt.map(|p| {
            let preview = p.chars().take(100).collect::<String>();
            if p.len() > 100 {
                format!("{}...", preview)
            } else {
                preview
            }
        });

        let entry = LogEntry {
            timestamp: Utc::now().to_rfc3339(),
            event: format!("hook:{}", hook_name),
            session_id: session_id.map(String::from),
            cwd: None,
            jj_change_id: None,
            commit_id: None,
            tool_name: tool_name.map(String::from),
            prompt_preview,
            result: Some("started".to_string()),
            error_message: None,
            details: None,
        };

        let _ = self.log(entry);
    }

    /// Log a hook result
    pub fn log_hook_result(
        &self,
        hook_name: &str,
        session_id: Option<&str>,
        result: Result<(), &str>,
    ) {
        if !self.is_enabled() {
            return;
        }

        let (result_str, error_msg) = match result {
            Ok(_) => ("success".to_string(), None),
            Err(e) => ("error".to_string(), Some(e.to_string())),
        };

        let entry = LogEntry {
            timestamp: Utc::now().to_rfc3339(),
            event: format!("hook:{}:result", hook_name),
            session_id: session_id.map(String::from),
            cwd: None,
            jj_change_id: None,
            commit_id: None,
            tool_name: None,
            prompt_preview: None,
            result: Some(result_str),
            error_message: error_msg,
            details: None,
        };

        let _ = self.log(entry);
    }

    /// Log a session command
    pub fn log_session_command(
        &self,
        command: &str,
        session_id: Option<&str>,
        details: Option<serde_json::Value>,
    ) {
        if !self.is_enabled() {
            return;
        }

        let entry = LogEntry {
            timestamp: Utc::now().to_rfc3339(),
            event: format!("session:{}", command),
            session_id: session_id.map(String::from),
            cwd: None,
            jj_change_id: None,
            commit_id: None,
            tool_name: None,
            prompt_preview: None,
            result: Some("started".to_string()),
            error_message: None,
            details,
        };

        let _ = self.log(entry);
    }

    /// Log a session command result
    pub fn log_session_result(
        &self,
        command: &str,
        session_id: Option<&str>,
        result: Result<(), &str>,
    ) {
        if !self.is_enabled() {
            return;
        }

        let (result_str, error_msg) = match result {
            Ok(_) => ("success".to_string(), None),
            Err(e) => ("error".to_string(), Some(e.to_string())),
        };

        let entry = LogEntry {
            timestamp: Utc::now().to_rfc3339(),
            event: format!("session:{}:result", command),
            session_id: session_id.map(String::from),
            cwd: None,
            jj_change_id: None,
            commit_id: None,
            tool_name: None,
            prompt_preview: None,
            result: Some(result_str),
            error_message: error_msg,
            details: None,
        };

        let _ = self.log(entry);
    }
}

/// Get the current jj change ID
fn get_jj_change_id() -> Result<String> {
    let output = Command::new("jj")
        .args(["log", "-r", "@", "--no-graph", "-T", "change_id"])
        .output()?;

    if !output.status.success() {
        anyhow::bail!("Failed to get jj change_id");
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Get the current commit ID (git SHA equivalent)
fn get_commit_id() -> Result<String> {
    let output = Command::new("jj")
        .args(["log", "-r", "@", "--no-graph", "-T", "commit_id"])
        .output()?;

    if !output.status.success() {
        anyhow::bail!("Failed to get commit_id");
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

// Helper module for getting home directory
mod dirs {
    use std::path::PathBuf;

    pub fn home_dir() -> Option<PathBuf> {
        std::env::var("HOME").ok().map(PathBuf::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_logger_enabled_with_env_var() {
        unsafe {
            env::set_var("JJAGENT_LOG", "1");
        }
        let logger = Logger::new();
        assert!(logger.is_enabled());
        unsafe {
            env::remove_var("JJAGENT_LOG");
        }
    }

    #[test]
    fn test_logger_custom_path() {
        let temp_dir = TempDir::new().unwrap();
        let log_path = temp_dir.path().join("custom.jsonl");
        unsafe {
            env::set_var("JJAGENT_LOG_FILE", log_path.to_str().unwrap());
        }

        let logger = Logger::new();
        assert!(logger.is_enabled());

        let entry = LogEntry {
            timestamp: Utc::now().to_rfc3339(),
            event: "test".to_string(),
            session_id: Some("test-session".to_string()),
            cwd: Some("/test/cwd".to_string()),
            jj_change_id: Some("abc123".to_string()),
            commit_id: Some("def456".to_string()),
            tool_name: None,
            prompt_preview: None,
            result: Some("success".to_string()),
            error_message: None,
            details: None,
        };

        logger.log(entry).unwrap();

        let content = fs::read_to_string(&log_path).unwrap();
        assert!(content.contains("test-session"));
        assert!(content.contains("abc123"));

        unsafe {
            env::remove_var("JJAGENT_LOG_FILE");
        }
    }

    #[test]
    fn test_log_hook() {
        let temp_dir = TempDir::new().unwrap();
        let log_path = temp_dir.path().join("hooks.jsonl");
        unsafe {
            env::set_var("JJAGENT_LOG_FILE", log_path.to_str().unwrap());
        }

        let logger = Logger::new();
        logger.log_hook(
            "PreToolUse",
            Some("session-123"),
            Some("Edit"),
            Some("This is a test prompt"),
        );

        let content = fs::read_to_string(&log_path).unwrap();
        assert!(content.contains("hook:PreToolUse"));
        assert!(content.contains("session-123"));
        assert!(content.contains("Edit"));

        unsafe {
            env::remove_var("JJAGENT_LOG_FILE");
        }
    }
}
