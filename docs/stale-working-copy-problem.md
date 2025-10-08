# Stale Working Copy Problem in Parallel Sessions

## Problem

When two Claude Code sessions run concurrently, the second session encounters a "stale working copy" error even though file-based locking prevents concurrent hook execution.

### What Happens

1. **Session 1** acquires lock → runs `jj new` → creates operation A → releases lock
2. **Session 2** waits for lock → acquires lock → tries to run `jj new` → **fails with stale working copy error**

### Error Messages Observed

```
Error: The working copy is stale (not updated since operation cc30f36ac8ac).
Hint: Run `jj workspace update-stale` to update it.
```

### Root Cause

The file lock (implemented in `src/lock.rs`) successfully prevents concurrent modification, but it doesn't solve the **working copy staleness** problem that is internal to jj.

When Session 2 starts (before Session 1 finishes), it loads the jj repository state at that point in time. By the time Session 2 acquires the lock, Session 1 has created new operations, making Session 2's view of the working copy stale.

From `jj help new`:

> By default, Jujutsu snapshots the working copy at the beginning of every command.

This snapshot happens when the jj process starts, which for Session 2 is *before* Session 1's changes are committed.

## Attempted Solutions

### ❌ Attempt 1: Add `--ignore-working-copy` to all jj commands

**Rationale:** Prevent jj from snapshotting/updating the working copy, avoiding staleness.

**Problem:** The `--ignore-working-copy` flag has two effects:
1. Don't snapshot at the beginning ✓
2. Don't update at the end ✗

Effect #2 breaks the intended behavior - we *want* jj to update `@` after our operations. Using this flag caused all file changes to end up in `uwc` instead of being properly tracked in session changes.

**Evidence:** Test snapshots showed files appearing in the wrong commits:
```diff
 @  uwc

+Added regular file session1_file.txt:  # Should be in session change below!
+        1: session 1 work
 ○  jjagent: session session1
```

## Correct Solution

### ✅ Run `jj workspace update-stale` after acquiring lock

Add a working copy refresh step in `src/hooks.rs::handle_pretool_hook()`:

```rust
pub fn handle_pretool_hook(input: HookInput) -> Result<()> {
    // 1. Acquire lock first
    crate::lock::acquire_lock(&input.session_id)?;

    // 2. Update working copy if stale (from operations that happened while waiting)
    let update_output = Command::new("jj")
        .args(["workspace", "update-stale"])
        .output()
        .context("Failed to update stale working copy")?;

    // Note: This succeeds with "Working copy already up to date" if not stale
    // so we don't need to check the output

    // 3. Now run jj new (and other commands) with fresh state
    let session_id = SessionId::from_full(&input.session_id);
    let commit_message = format_precommit_message(&session_id);

    let output = Command::new("jj")
        .args(["new", "-m", &commit_message])
        .output()?;

    // ... rest of implementation
}
```

### Why This Works

1. **Lock prevents concurrent modification** - Only one session can run jj commands at a time
2. **Update-stale refreshes the view** - Session 2 gets the latest state after Session 1 finishes
3. **Normal jj behavior works** - Commands snapshot and update the working copy as intended

### Implementation Notes

- `jj workspace update-stale` is idempotent - it succeeds with "Working copy already up to date" if the working copy is fresh
- No need to check if the working copy is actually stale - just always run it after acquiring the lock
- The command automatically handles any concurrent modifications detected in the operation log
- Keep all existing jj commands unchanged (no `--ignore-working-copy` flags needed)

## Testing

The fix can be verified with:
1. Run `cargo test test_parallel_sessions_with_locking` - should pass without stale errors
2. Manual test in a real repo with two concurrent `claude` sessions
3. Check that changes end up in the correct commits (not all in `uwc`)

## References

- jj working copy documentation: https://jj-vcs.github.io/jj/latest/working-copy/#stale-working-copy
- Lock implementation: `src/lock.rs`
- Hook implementation: `src/hooks.rs`
- Parallel session test: `tests/parallel_sessions_test.rs`
