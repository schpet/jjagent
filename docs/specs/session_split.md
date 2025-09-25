# Session Split Command

## Design

Add `jjcc session split <uuid>` command that:
1. Finds commit with matching Claude-Session-Id trailer (furthest descendant if multiple)
2. Creates empty commit on top with same session ID trailer
3. Copies commit description from original (first line only) with suffix
4. User remains on their working copy (@)

Command structure:
```
jjcc session split <uuid>
```

## Implementation Notes

- Reuse existing `find_session_commit()` logic (handles multiple matches)
- Edge case handling:
  - If @ is descendant of session: use `jj new <change_id> --insert-before @`
  - If @ IS the session: use `jj new <change_id>` then `jj edit @+`
  - If @ is not descendant: error with message like "Working copy must be a descendant of session commit"
- Use `jj describe -r-` with trailer formatting from existing code
- Format: `<first_line> (split <iso8601_timestamp>)\n\nClaude-Session-Id: <uuid>`
- Use `chrono::Utc::now().to_rfc3339()` for timestamp

## Acceptance Criteria

### Basic Functionality
- [ ] Creates empty commit on top of found session commit
- [ ] New commit has identical Claude-Session-Id trailer
- [ ] Copies first line of description from original with `(split <timestamp>)` suffix
- [ ] User remains on their working copy (@ unchanged)
- [ ] New commit is inserted between session commit and working copy
- [ ] Original session commit remains unchanged
- [ ] Prints success message with new commit ID and description

### Error Handling
- [ ] Errors if session ID not found
- [ ] Uses furthest descendant if multiple commits have same session ID
- [ ] Errors if not in jj repo
- [ ] Errors when @ is not a descendant of session commit
- [ ] Handles when @ IS the session commit (creates new commit on top, moves @ forward)

### Tests
- [ ] `test_session_split_basic` - splits a session successfully
- [ ] `test_session_split_not_found` - handles missing session
- [ ] `test_session_split_multiple_sessions` - uses furthest descendant
- [ ] `test_session_split_preserves_original` - verifies original unchanged
- [ ] `test_session_split_empty_commit` - verifies new commit has no changes
- [ ] `test_session_split_working_copy_unchanged` - @ remains on working copy
- [ ] `test_session_split_description` - copies only first line + suffix + trailer
- [ ] `test_session_split_working_copy_is_session` - handles @ = session commit
- [ ] `test_session_split_diverged_working_copy` - errors when @ not descendant of session