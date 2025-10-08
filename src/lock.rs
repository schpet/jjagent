//! Working copy lock for preventing concurrent jj operations.
//!
//! This module implements a file-based locking mechanism to serialize hook operations
//! that modify the jj working copy. The lock is held from PreToolUse through tool execution
//! until PostToolUse/Stop, preventing race conditions between parallel Claude sessions.

use anyhow::{Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const LOCK_FILENAME: &str = "jjagent-wc.lock";
const LOCK_TIMEOUT_SECS: u64 = 300; // 5 minutes
const INITIAL_RETRY_MS: u64 = 100;
const MAX_RETRY_MS: u64 = 5000; // 5 seconds
const PROGRESS_INTERVAL_SECS: u64 = 10;

// Global storage for lock file handles to keep locks alive between hooks
lazy_static::lazy_static! {
    static ref LOCK_HANDLES: Mutex<HashMap<String, File>> = Mutex::new(HashMap::new());
}

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

    let mut file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&lock_path)
        .context("Failed to open lock file")?;

    let timeout = Duration::from_secs(LOCK_TIMEOUT_SECS);
    let start = Instant::now();
    let mut retry_delay = Duration::from_millis(INITIAL_RETRY_MS);
    let mut last_progress = Instant::now();

    loop {
        match file.try_lock_exclusive() {
            Ok(()) => {
                // Write lock metadata
                let metadata = LockMetadata::new(session_id.to_string());
                file.set_len(0)?;
                file.write_all(serde_json::to_string(&metadata)?.as_bytes())?;
                file.sync_all()?;

                // Store the file handle globally to keep the lock alive
                {
                    let mut handles = LOCK_HANDLES.lock().unwrap();
                    handles.insert(session_id.to_string(), file);
                }

                eprintln!(
                    "jjagent: Acquired working copy lock (session {})",
                    &session_id[..8.min(session_id.len())]
                );
                return Ok(());
            }
            Err(_) if start.elapsed() < timeout => {
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

    // First, remove and drop the file handle to release the lock
    {
        let mut handles = LOCK_HANDLES.lock().unwrap();
        if handles.remove(session_id).is_none() {
            eprintln!(
                "jjagent: Warning - no lock handle found for session {}",
                &session_id[..8.min(session_id.len())]
            );
        }
    }
    // File handle is dropped here, releasing the OS-level lock

    if !lock_path.exists() {
        anyhow::bail!(
            "Lock file doesn't exist. PreToolUse may not have run or lock was manually deleted."
        );
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

        // Verify we have the file handle stored
        {
            let handles = LOCK_HANDLES.lock().unwrap();
            assert!(
                handles.contains_key(session_id),
                "Lock handle should be stored"
            );
        }

        // Try to acquire the same lock from a different "session" - should fail
        let result = std::thread::spawn(move || {
            // This should fail immediately since we're using a 0 timeout for testing
            let file = OpenOptions::new()
                .create(false)
                .read(true)
                .write(true)
                .open(get_lock_path())
                .ok()?;

            // This should fail since the lock is held by the other session
            file.try_lock_exclusive().ok()
        })
        .join()
        .unwrap();

        assert!(
            result.is_none(),
            "Should not be able to acquire lock while it's held"
        );

        // Release the lock
        release_lock(session_id).unwrap();

        // Verify the lock file is removed
        assert!(
            !lock_path.exists(),
            "Lock file should be deleted after release"
        );

        // Verify the handle is removed
        {
            let handles = LOCK_HANDLES.lock().unwrap();
            assert!(
                !handles.contains_key(session_id),
                "Lock handle should be removed"
            );
        }

        // Restore original directory
        std::env::set_current_dir(original_dir).unwrap();
    }
}
