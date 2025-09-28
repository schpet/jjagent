# Issue #2: debug_assert in Production Code

## Severity
Critical

## Location
src/main.rs:312-315 (in `handle_post_tool_use`)

## Description
The code uses `debug_assert!` to check if `jj diff --stat` succeeded. This assertion only runs in debug builds and is compiled out in release builds. If `jj diff --stat` fails in production, the code will silently continue and attempt to parse garbage output, leading to undefined behavior.

```rust
let diff_stat = Command::new("jj").args(["diff", "--stat"]).output()?;
let diff_output = String::from_utf8_lossy(&diff_stat.stdout);

// Invariant: Change detection must be tool-agnostic
debug_assert!(
    diff_stat.status.success(),
    "jj diff --stat must succeed for proper change attribution"
);

// More robust check for no changes - look for the exact pattern
let has_no_changes = diff_output.trim() == "0 files changed, 0 insertions(+), 0 deletions(-)";
```

## Steps to Reproduce
1. Build jjcc in release mode: `cargo build --release`
2. Create a scenario where `jj diff --stat` would fail:
   - Corrupt jj repository
   - Permission issues
   - jj binary not in PATH
   - Invalid working copy state
3. Trigger `handle_post_tool_use` hook
4. Observe that:
   - In debug builds: panic with assertion message
   - In release builds: silently continues, `has_no_changes` may be incorrectly determined from garbage output

Example test case:
```rust
#[test]
fn test_diff_stat_failure_handling() {
    // Simulate jj diff --stat failure
    // In debug: should panic
    // In release: currently has undefined behavior, should return error
}
```

## Acceptance Criteria
- [ ] `jj diff --stat` failures are detected in both debug and release builds
- [ ] When `jj diff --stat` fails, an appropriate error is returned with context
- [ ] The error message helps users understand what went wrong
- [ ] Tests verify behavior in both success and failure cases
- [ ] Replace `debug_assert!` with proper error handling using `anyhow::Result`

## Recommended Fix
Replace the debug_assert with proper error handling:

```rust
let diff_stat = Command::new("jj").args(["diff", "--stat"]).output()?;

if !diff_stat.status.success() {
    let stderr = String::from_utf8_lossy(&diff_stat.stderr);
    anyhow::bail!(
        "Failed to check for changes using `jj diff --stat`: {}",
        stderr
    );
}

let diff_output = String::from_utf8_lossy(&diff_stat.stdout);
let has_no_changes = diff_output.trim() == "0 files changed, 0 insertions(+), 0 deletions(-)";
```