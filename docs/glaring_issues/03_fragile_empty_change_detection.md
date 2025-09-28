# Issue #3: Fragile Empty Change Detection

## Severity
High

## Location
src/main.rs:318 (in `handle_post_tool_use`)

## Description
The code uses exact string matching to detect empty changes from `jj diff --stat` output. This approach is fragile and will break if jj changes its output format, adds localization, tweaks spacing, or modifies punctuation.

```rust
let diff_output = String::from_utf8_lossy(&diff_stat.stdout);

// More robust check for no changes - look for the exact pattern
let has_no_changes = diff_output.trim() == "0 files changed, 0 insertions(+), 0 deletions(-)";
```

The comment claims this is "more robust" but it's actually quite brittle.

## Steps to Reproduce
1. Mock or modify jj's output format in any of these ways:
   - Change spacing: `"0 files changed,  0 insertions(+),  0 deletions(-)"` (double space)
   - Change wording: `"0 files modified, 0 insertions(+), 0 deletions(-)"`
   - Add localization: `"0 fichiers modifiés, 0 insertions(+), 0 suppressions(-)"`
   - Change punctuation: `"0 files changed, 0 insertions (+), 0 deletions (-)"`
   - Add extra info: `"0 files changed, 0 insertions(+), 0 deletions(-) in 0.001s"`
2. Run the hook
3. Observe that `has_no_changes` is incorrectly determined as `false`
4. This causes the empty temp change to be kept instead of abandoned

Example test:
```rust
#[test]
fn test_empty_change_detection_various_formats() {
    // Test should handle variations:
    assert!(is_empty_diff("0 files changed, 0 insertions(+), 0 deletions(-)"));
    assert!(is_empty_diff("0 files changed,  0 insertions(+),  0 deletions(-)")); // extra space
    assert!(is_empty_diff("  0 files changed, 0 insertions(+), 0 deletions(-)  ")); // leading/trailing
    // Should fail with current implementation
}
```

## Acceptance Criteria
- [ ] Empty change detection works across jj version differences
- [ ] Detection is resilient to output format changes
- [ ] Detection is simple and doesn't require parsing complex output
- [ ] Tests cover both empty and non-empty diffs
- [ ] Implementation uses a stable output format that's guaranteed to be empty when there are no changes

## Recommended Fix (Best - Use --git format)
The `--git` format produces completely empty output when there are no changes, making detection trivial and robust:

```rust
// Check if there are any changes using git-format diff
let diff_output = Command::new("jj")
    .args(["diff", "--git"])
    .output()?;

if !diff_output.status.success() {
    anyhow::bail!("Failed to check for changes using `jj diff --git`");
}

// Git format produces empty output when there are no changes
let has_no_changes = diff_output.stdout.is_empty();
```

### Why --git format is better:
- **Simple**: Empty diff = empty output (0 bytes), no parsing needed
- **Stable**: Git diff format is standardized and unlikely to change
- **Reliable**: No locale issues, spacing variations, or version differences
- **Unambiguous**: Either there's output or there isn't

### Verification:
```bash
# Empty change produces empty output
$ jj new && jj diff --git
(no output)

# Change produces git-format diff
$ echo "test" > file.txt && jj diff --git
diff --git a/file.txt b/file.txt
new file mode 100644
index 0000000000..9daeafb986
--- /dev/null
+++ b/file.txt
@@ -0,0 +1,1 @@
+test
```

## Alternative Fix (Option B - name-only)
If for some reason --git format is not preferred:

```rust
// Check if there are any changed files
let diff_files = Command::new("jj")
    .args(["diff", "--name-only"])
    .output()?;

if !diff_files.status.success() {
    anyhow::bail!("Failed to check for changed files");
}

let has_no_changes = diff_files.stdout.is_empty();
```

This is also simple and reliable, but --git format is more standard.