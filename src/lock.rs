//! Working copy lock for preventing concurrent jj operations.
//!
//! This module implements a file-based locking mechanism to serialize hook operations
//! that modify the jj working copy. The lock is held from PreToolUse through tool execution
//! until PostToolUse/Stop, preventing race conditions between parallel Claude sessions.
//!
//! Uses file existence as the lock mechanism since each hook runs in a separate process.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const LOCK_FILENAME: &str = "jjagent-wc.lock";
const LOCK_TIMEOUT_SECS: u64 = 300; // 5 minutes
const INITIAL_RETRY_MS: u64 = 100;
const MAX_RETRY_MS: u64 = 5000; // 5 seconds
const PROGRESS_INTERVAL_SECS: u64 = 10;

#[derive(Serialize, Deserialize, Debug)]
struct LockMetadata {
    pid: u32,
    session_id: String,
    acquired_at: u64, // Unix timestamp
}

impl LockMetadata {
    fn new(session_id: String) -> Self {
        Self {
            pid: std::process::id(),
            session_id,
            acquired_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }

    fn age_seconds(&self) -> u64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        now.saturating_sub(self.acquired_at)
    }
}

fn get_lock_path() -> PathBuf {
    Path::new(".jj").join(LOCK_FILENAME)
}

fn read_lock_holder(lock_path: &Path) -> Option<LockMetadata> {
    let mut file = File::open(lock_path).ok()?;
    let mut contents = String::new();
    file.read_to_string(&mut contents).ok()?;
    serde_json::from_str(&contents).ok()
}

/// Acquire the working copy lock in PreToolUse hook
pub fn acquire_lock(session_id: &str) -> Result<()> {
    let lock_path = get_lock_path();

    std::fs::create_dir_all(".jj").context("Failed to create .jj directory")?;

    let timeout = Duration::from_secs(LOCK_TIMEOUT_SECS);
    let start = Instant::now();
    let mut retry_delay = Duration::from_millis(INITIAL_RETRY_MS);
    let mut last_progress = Instant::now();

    loop {
        // Try to atomically create the lock file
        match OpenOptions::new()
            .create_new(true) // Fails if file exists (atomic operation)
            .write(true)
            .open(&lock_path)
        {
            Ok(mut file) => {
                // Write lock metadata
                let metadata = LockMetadata::new(session_id.to_string());
                file.write_all(serde_json::to_string(&metadata)?.as_bytes())?;
                file.sync_all()?;

                eprintln!(
                    "jjagent: Acquired working copy lock (session {})",
                    &session_id[..8.min(session_id.len())]
                );
                return Ok(());
            }
            Err(_) if start.elapsed() < timeout => {
                // Check if lock is stale and can be stolen
                if let Some(metadata) = read_lock_holder(&lock_path) {
                    if metadata.age_seconds() > LOCK_TIMEOUT_SECS {
                        eprintln!(
                            "jjagent: Lock is stale ({:.0}s old), attempting to steal it",
                            metadata.age_seconds()
                        );
                        // Try to remove stale lock
                        if std::fs::remove_file(&lock_path).is_ok() {
                            continue; // Try to acquire again immediately
                        }
                    }
                }

                if last_progress.elapsed() >= Duration::from_secs(PROGRESS_INTERVAL_SECS) {
                    let holder = read_lock_holder(&lock_path);
                    eprintln!(
                        "jjagent: Waiting for working copy lock... ({:.0}s elapsed){}",
                        start.elapsed().as_secs_f64(),
                        holder
                            .as_ref()
                            .map(|m| format!(
                                " [held by session {} for {:.0}s]",
                                &m.session_id[..8.min(m.session_id.len())],
                                m.age_seconds()
                            ))
                            .unwrap_or_default()
                    );
                    last_progress = Instant::now();
                }

                std::thread::sleep(retry_delay);
                retry_delay = std::cmp::min(retry_delay * 2, Duration::from_millis(MAX_RETRY_MS));
            }
            Err(e) => {
                let holder = read_lock_holder(&lock_path);
                let holder_info = holder
                    .as_ref()
                    .map(|m| {
                        format!(
                            " (session {} for {:.0}s)",
                            &m.session_id[..8.min(m.session_id.len())],
                            m.age_seconds()
                        )
                    })
                    .unwrap_or_default();

                anyhow::bail!(
                    "Failed to acquire working copy lock after {:.0}s: {}.\n\
                     Another Claude session is running{}.\n\
                     Wait for it to finish or remove the lock file:\n  \
                     rm .jj/{}",
                    timeout.as_secs_f64(),
                    e,
                    holder_info,
                    LOCK_FILENAME
                );
            }
        }
    }
}

/// Release the working copy lock in PostToolUse/Stop hook
pub fn release_lock(session_id: &str) -> Result<()> {
    let lock_path = get_lock_path();

    if !lock_path.exists() {
        // Lock already released or never acquired - not an error
        eprintln!(
            "jjagent: Lock already released or not held (session {})",
            &session_id[..8.min(session_id.len())]
        );
        return Ok(());
    }

    // Read and verify ownership
    if let Some(metadata) = read_lock_holder(&lock_path) {
        if metadata.session_id != session_id {
            anyhow::bail!(
                "Lock ownership mismatch!\n\
                 Expected session: {}\n\
                 Found session:    {}\n\
                 Another session may have stolen the lock after timeout.",
                &session_id[..8.min(session_id.len())],
                &metadata.session_id[..8.min(metadata.session_id.len())]
            );
        }

        let age = metadata.age_seconds();
        if age > LOCK_TIMEOUT_SECS {
            eprintln!(
                "jjagent: Warning - lock is stale ({:.1}m old)",
                age as f64 / 60.0
            );
        }
    }

    // Delete lock file to release
    std::fs::remove_file(&lock_path).context("Failed to remove lock file")?;

    eprintln!(
        "jjagent: Released working copy lock (session {})",
        &session_id[..8.min(session_id.len())]
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // Note: Integration test in tests/parallel_sessions_test.rs validates
    // actual lock acquire/release behavior with real jj operations.
    // Directory-dependent unit tests are omitted to avoid test interference.

    #[test]
    fn test_lock_metadata() {
        let session_id = "test-session-12345678";
        let metadata = LockMetadata::new(session_id.to_string());

        assert_eq!(metadata.session_id, session_id);
        assert_eq!(metadata.pid, std::process::id());

        // Age should be approximately 0
        let age = metadata.age_seconds();
        assert!(age < 2, "Age should be less than 2 seconds, got {}", age);
    }

    #[test]
    fn test_lock_path() {
        let path = get_lock_path();
        assert!(path.to_str().unwrap().ends_with("jjagent-wc.lock"));
        assert!(path.to_str().unwrap().contains(".jj"));
    }

    #[test]
    fn test_lock_persistence_between_acquire_and_release() {
        // Create a temporary directory for testing
        let temp_dir = TempDir::new().unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        let session_id = "test-session-persistence";

        // Acquire the lock
        acquire_lock(session_id).unwrap();

        // Verify the lock file exists
        let lock_path = get_lock_path();
        assert!(lock_path.exists(), "Lock file should exist after acquire");

        // Verify lock metadata is correct
        let metadata = read_lock_holder(&lock_path).unwrap();
        assert_eq!(metadata.session_id, session_id);

        // Try to acquire the same lock from a different "session" - should fail
        let other_session_id = "other-session";
        let result = std::thread::spawn(move || {
            // Try to create the lock file (should fail since it already exists)
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(get_lock_path())
                .is_ok()
        })
        .join()
        .unwrap();

        assert!(
            !result,
            "Should not be able to acquire lock while it's held"
        );

        // Release the lock
        release_lock(session_id).unwrap();

        // Verify the lock file is removed
        assert!(
            !lock_path.exists(),
            "Lock file should be deleted after release"
        );

        // Now another session should be able to acquire the lock
        acquire_lock(other_session_id).unwrap();
        assert!(lock_path.exists(), "Lock file should exist for new session");
        release_lock(other_session_id).unwrap();

        // Restore original directory
        std::env::set_current_dir(original_dir).unwrap();
    }
}
