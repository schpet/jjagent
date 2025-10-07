# Parallel Sessions and Locking Strategy

## Problem Statement

When running multiple Claude Code sessions in parallel (e.g., `claude -p "task 1" & claude -p "task 2"`), concurrent jj operations cause failures:

1. **Divergent change IDs**: Both sessions try to modify the same change (@ or session change), creating multiple commits with the same change ID but different commit IDs
2. **Concurrent checkout errors**: `jj edit` or `jj squash` operations fail with "Concurrent checkout" when both sessions try to modify the working copy simultaneously
3. **Operation DAG conflicts**: jj's operation log branches when concurrent operations occur, requiring reconciliation

### Example Failure

```
⏺ jj edit failed: Error: Change ID `yuoxltyzoryo` is divergent
  Hint: Use commit ID to select single revision from: 59c2dc0ccc4e, aece2b880c83

⏺ jj squash failed: Internal error: Concurrent checkout
```

### Root Cause

The hooks (`PreToolUse`, `PostToolUse`) perform multiple jj operations that modify the working copy:
- `jj new` - Creates precommit
- `jj edit` - Switches working copy
- `jj squash` - Modifies commits and may trigger rebase

When two sessions run these operations simultaneously, they race to modify the same repository state, leading to divergence and checkout failures.

## Locking Strategy

### Approach: Working Copy Lock

Use a **single working copy lock** to serialize all hook operations that touch the working copy (@).

**Rationale:**
- The fundamental issue is concurrent modification of the working copy
- jj operations are fast (10-50ms typically), so serialization won't impact UX
- Simple to implement and reason about
- Prevents all observed failure modes

### Lock Semantics

**Lock file:** `.jj/jjagent-wc.lock`

**Lock type:** Exclusive advisory file lock (POSIX flock/fcntl)

**Lock scope:**
- **Acquire:** At the start of `PreToolUse`
- **Hold:** Throughout tool execution (Edit, Write, etc.)
- **Release:** At the end of `PostToolUse` or `Stop` hook

**Critical insight:** The lock must span from PreToolUse → tool execution → PostToolUse. If we release the lock after PreToolUse, another session can create its own precommit and race to PostToolUse, causing divergence.

**Lock lifetime:**
- PreToolUse writes lock PID/session to `.jj/jjagent-wc.lock`
- PostToolUse/Stop verifies lock ownership and releases
- If lock is stale (process dead), it can be forcibly acquired

**Timeout:** 5 minutes (300 seconds)
- Long timeout accommodates slow tool operations (large file edits, etc.)
- If lock cannot be acquired within timeout, fail with clear error
- Error message indicates which session holds the lock

**Retry strategy:** Exponential backoff
- Initial retry: 100ms
- Max retry: 5 seconds
- Total timeout: 5 minutes
- Progress indicator every 10 seconds

### Why Not Lock Per Session?

Per-session locks would require:
1. Tracking which change ID belongs to which session
2. Acquiring multiple locks (session lock + working copy lock)
3. Complex deadlock prevention when sessions interact

Since hook operations are fast, a global working copy lock is simpler and sufficient.

### Why Not Lock the Entire Repo?

Repository-level locking would prevent read operations (like `jj log`) from running concurrently, which is unnecessary. We only need to serialize operations that modify the working copy.

## Implementation

### Rust Implementation

Use the `fs2` crate for cross-platform file locking and store lock metadata:

```rust
use fs2::FileExt;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::time::{Duration, Instant};
use serde::{Serialize, Deserialize};
use anyhow::{Context, Result};

#[derive(Serialize, Deserialize)]
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
        now - self.acquired_at
    }
}

/// Acquire lock in PreToolUse, persist to disk
pub fn acquire_lock(session_id: &str) -> Result<()> {
    let lock_path = std::path::Path::new(".jj").join("jjagent-wc.lock");

    std::fs::create_dir_all(".jj")
        .context("Failed to create .jj directory")?;

    let mut file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&lock_path)
        .context("Failed to open lock file")?;

    let timeout = Duration::from_secs(300); // 5 minutes
    let start = Instant::now();
    let mut retry_delay = Duration::from_millis(100);
    let mut last_progress = Instant::now();

    loop {
        match file.try_lock_exclusive() {
            Ok(()) => {
                // Write lock metadata
                let metadata = LockMetadata::new(session_id.to_string());
                file.set_len(0)?;
                file.write_all(serde_json::to_string(&metadata)?.as_bytes())?;
                file.sync_all()?;

                eprintln!("jjagent: Acquired working copy lock (session {})", &session_id[..8]);
                return Ok(());
            }
            Err(_) if start.elapsed() < timeout => {
                if last_progress.elapsed() >= Duration::from_secs(10) {
                    let holder = read_lock_holder(&lock_path);
                    eprintln!(
                        "jjagent: Waiting for working copy lock... ({:.0}s elapsed){}",
                        start.elapsed().as_secs_f64(),
                        holder.as_ref()
                            .map(|m| format!(" [held by session {} for {:.0}s]",
                                &m.session_id[..8], m.age_seconds()))
                            .unwrap_or_default()
                    );
                    last_progress = Instant::now();
                }

                std::thread::sleep(retry_delay);
                retry_delay = std::cmp::min(retry_delay * 2, Duration::from_secs(5));
            }
            Err(e) => {
                let holder = read_lock_holder(&lock_path);
                let holder_info = holder.as_ref()
                    .map(|m| format!(" (session {} for {:.0}s)", &m.session_id[..8], m.age_seconds()))
                    .unwrap_or_default();

                anyhow::bail!(
                    "Failed to acquire working copy lock after {:.0}s: {}.\n\
                     Another Claude session is running{}.\n\
                     Wait for it to finish or remove the lock file:\n  \
                     rm .jj/jjagent-wc.lock",
                    timeout.as_secs_f64(),
                    e,
                    holder_info
                );
            }
        }
    }
}

fn read_lock_holder(lock_path: &std::path::Path) -> Option<LockMetadata> {
    let mut file = File::open(lock_path).ok()?;
    let mut contents = String::new();
    file.read_to_string(&mut contents).ok()?;
    serde_json::from_str(&contents).ok()
}

/// Verify lock ownership and release in PostToolUse/Stop
pub fn release_lock(session_id: &str) -> Result<()> {
    let lock_path = std::path::Path::new(".jj").join("jjagent-wc.lock");

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
                &session_id[..8],
                &metadata.session_id[..8]
            );
        }

        if metadata.age_seconds() > 300 {
            eprintln!(
                "jjagent: Warning - lock is stale ({:.1}m old)",
                metadata.age_seconds() as f64 / 60.0
            );
        }
    }

    // Delete lock file to release
    std::fs::remove_file(&lock_path)
        .context("Failed to remove lock file")?;

    eprintln!("jjagent: Released working copy lock (session {})", &session_id[..8]);
    Ok(())
}
```

### Usage in Hooks

```rust
pub fn handle_pretool_hook(input: HookInput) -> Result<()> {
    // Acquire and persist lock to disk
    acquire_lock(&input.session_id)?;

    let session_id = SessionId::from_full(&input.session_id);
    let commit_message = format_precommit_message(&session_id);

    let output = Command::new("jj")
        .args(["new", "-m", &commit_message])
        .output()
        .context("Failed to execute jj new command")?;

    if !output.status.success() {
        // Release lock on error
        let _ = release_lock(&input.session_id);
        anyhow::bail!(
            "jj new command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Lock remains held until PostToolUse or Stop
    Ok(())
}

pub fn handle_posttool_hook(input: HookInput) -> Result<()> {
    let session_id = SessionId::from_full(&input.session_id);

    // Do the actual work
    let result = finalize_precommit(session_id);

    // Always release lock, even on error
    match release_lock(&input.session_id) {
        Ok(()) => result,
        Err(e) => {
            eprintln!("jjagent: Warning - failed to release lock: {}", e);
            result
        }
    }
}

pub fn handle_stop_hook(input: HookInput) -> Result<()> {
    let session_id = SessionId::from_full(&input.session_id);
    let result = finalize_precommit(session_id);

    match release_lock(&input.session_id) {
        Ok(()) => result,
        Err(e) => {
            eprintln!("jjagent: Warning - failed to release lock: {}", e);
            result
        }
    }
}
```

## Alternative: Add UserPromptSubmit Hook (Future Enhancement)

Claude Code supports a `UserPromptSubmit` hook that runs when the user sends a prompt. We could use this to:

1. Set up the session's initial working copy state
2. Ensure each session starts with its own change ID
3. Validate no conflicts before starting work

This would complement the locking strategy by reducing the window where locks are needed.

## Testing Strategy

### Unit Tests

Mock file locking to test:
- Lock acquisition succeeds
- Lock timeout behavior
- Lock release on success
- Lock release on error (RAII)

### Integration Tests

Create real test repos and run parallel sessions:

```bash
# Terminal 1
cd test-repo
claude -p "edit file1.txt" --session-id session-1

# Terminal 2
cd test-repo
claude -p "edit file2.txt" --session-id session-2
```

Verify:
1. Both sessions complete successfully
2. No divergent changes are created
3. No "concurrent checkout" errors
4. Locks are properly released

### Stress Test

Run many parallel sessions:

```bash
for i in {1..10}; do
  claude -p "echo test $i > file$i.txt" &
done
wait
```

Verify all sessions complete without errors.

## Error Messages

When lock acquisition fails, provide actionable error:

```
Failed to acquire working copy lock after 30s: Resource busy.

Another Claude session may be running in this repository.
Please wait for the other session to complete, or if you're sure
no other session is running, remove the lock file:

  rm .jj/jjagent-wc.lock

If this problem persists, please report it at:
https://github.com/yourusername/jjagent/issues
```

## Performance Considerations

### Lock Contention

With N parallel sessions:
- Average wait time: (N-1) × avg_hook_duration / 2
- Typical hook duration: 10-50ms
- For 2 sessions: ~5-25ms additional latency
- For 10 sessions: ~45-225ms additional latency

This is acceptable because:
1. Claude Code operations are async (user doesn't block)
2. File editing is inherently sequential (user types one thing at a time)
3. The alternative (failures) is worse than slight serialization

### Lock Granularity Trade-offs

We could reduce contention with:
- **Per-file locks**: Lock only files being edited
- **Per-change locks**: Lock only the change being modified

However, these add complexity:
- Need to track which files/changes will be modified before acquiring lock
- Risk of deadlock if operations need multiple locks
- jj's internal operations may touch unexpected changes

The working copy lock is simpler and sufficient for typical usage patterns.

## Migration and Rollout

### Compatibility

The locking mechanism is backward compatible:
- Old sessions without locking will still work (advisory lock)
- New sessions will properly serialize
- Gradually reduces failures as users upgrade

### Gradual Rollout

1. **Phase 1**: Add locking to `PostToolUse` only (highest risk of conflicts)
2. **Phase 2**: Add locking to `PreToolUse`
3. **Phase 3**: Add locking to `Stop` hook

This allows testing and validation at each step.

### Escape Hatch

Users can disable locking:

```bash
export JJAGENT_NO_LOCK=1
```

This allows bypassing if locking causes issues, until they're resolved.

## Future Enhancements

### 1. Lock Monitoring

Add telemetry to track:
- Lock acquisition time
- Lock wait time
- Lock timeout frequency

This helps identify performance issues and tune timeout values.

### 2. Lock Health Check

Add a `jjagent status` command:
```bash
jjagent status
# Lock file: .jj/jjagent-wc.lock
# Status: Locked
# Held by: PID 12345 (session abc12345)
# Acquired: 2.3s ago
```

### 3. Per-Session State Isolation

Instead of locking, isolate each session to its own change from the start:
- `UserPromptSubmit` hook creates session-specific working copy
- Each session works in its own change subtree
- Merge sessions explicitly when needed

This is more complex but could eliminate need for locking entirely.

## References

- [jj operation log documentation](https://martinvonz.github.io/jj/latest/operation-log/)
- [POSIX file locking](https://man7.org/linux/man-pages/man2/flock.2.html)
- [fs2 crate documentation](https://docs.rs/fs2/latest/fs2/)
- Claude Code hooks: PreToolUse, PostToolUse, Stop, UserPromptSubmit
