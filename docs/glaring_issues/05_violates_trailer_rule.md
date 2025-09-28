# Issue #5: Violates Own Trailer Parsing Rule

## Severity
Medium

## Location
src/main.rs:331-342 (in `handle_post_tool_use`)

## Description
The project's CLAUDE.md explicitly states: "if we need to read structured information from a commit or change description/message (e.g. a claude code session id) always parse it from the trailer and not the title (which also may include it)."

However, the code searches for Claude session IDs using a glob pattern that matches anywhere in the description, including the title:

```rust
let search_output = Command::new("jj")
    .args([
        "log",
        "-r",
        &format!("description(glob:'*Claude-Session-Id: {}*')", session_id),
        "--no-graph",
        "-T",
        "change_id",
        "--limit",
        "1",
    ])
    .output()?;
```

This can cause false positives if someone writes "Claude-Session-Id: xxx" in commit body text, commit title, or comments.

## Steps to Reproduce
1. Create a commit with the following description:
   ```
   Testing the Claude-Session-Id: abc-123-fake feature

   This commit tests what happens when we mention
   Claude-Session-Id: xyz-789-fake in the body text.
   ```
   (Note: no actual trailer with Claude-Session-Id)

2. Run a Claude session with session_id = "abc-123-fake"
3. The search in `handle_post_tool_use` will incorrectly match this unrelated commit
4. Changes may be squashed into the wrong commit

Example test:
```rust
#[test]
fn test_session_id_search_only_matches_trailers() {
    // Create commit with session ID in title but not trailer
    // Create commit with session ID in body but not trailer
    // Create commit with session ID in trailer
    // Only the third should match
}
```

## Acceptance Criteria
- [ ] Session ID search only matches commits with session IDs in trailers
- [ ] Session ID mentions in title or body text are ignored
- [ ] Tests verify that non-trailer mentions don't cause false matches
- [ ] Consistent with existing helper functions `get_session_id_from_change` and `extract_session_id_from_temp_change` which parse trailers correctly
- [ ] Either:
  - Option A: Use the existing `get_session_id_from_change` helper in a loop/filter
  - Option B: Create a more precise jj revset that checks trailers specifically
  - Option C: Parse results and verify trailer format

## Recommended Fix (Option A - Use existing helper)
```rust
// Instead of using glob pattern, search commits and filter by parsing trailers
// This is more reliable but potentially slower for large repos

// First get all commits (we could scope this better with other criteria)
let all_changes = Command::new("jj")
    .args([
        "log",
        "-r", "all()",  // Could be scoped better
        "--no-graph",
        "-T", "change_id",
    ])
    .output()?;

// Filter to find the one with matching session ID in trailer
let mut existing_id: Option<String> = None;
for change_id in String::from_utf8_lossy(&all_changes.stdout).lines() {
    if let Some(found_session) = get_session_id_from_change(change_id.trim())? {
        if found_session == session_id {
            existing_id = Some(change_id.trim().to_string());
            break;
        }
    }
}

if let Some(existing_id) = existing_id {
    eprintln!(
        "PostToolUse: Found existing Claude change {}",
        &existing_id[0..12.min(existing_id.len())]
    );
    // ... squash logic
}
```

## Recommended Fix (Option B - Better scoping)
The above is inefficient. A better approach would be to:
1. Scope the search to recent commits or commits in the current branch
2. Or maintain a mapping file of session_id -> change_id
3. Or use jj's trailer: revset predicate if it exists

```rust
// Check if jj supports trailer matching in revsets
// If so: description(exact:'Claude-Session-Id: <session-id>')
// or look for a trailer: revset function
```