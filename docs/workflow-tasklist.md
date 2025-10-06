# Workflow Implementation Task List

## Phase 1: Foundation & Utilities
- [x] Add session ID types (short: 8 chars, full: complete session ID)
- [x] Implement session ID extraction/shortening utilities
- [x] Add commit message formatting helpers (with trailers)

## Phase 2: Pretool Hook
- [x] Implement pretool hook: `jj new -m "jjagent: precommit {short_session_id}"`
- [x] Add tests for pretool hook execution
- [x] Verify pretool creates correct commit structure

## Phase 3: Session Change Detection
- [x] Implement function to check descendants for `Claude-session-id:` trailer
- [x] Parse trailers from commit messages (not titles)
- [x] Return closest descendant with matching session ID if found
- [x] Add tests for session change detection

## Phase 4: Session Change Creation
- [x] Implement creating session change if none exists
- [x] Use `jj new --insert-before @- --no-edit -m "jjagent: session {short_session_id}\n\nClaude-session-id: {full_session_id}"`
- [x] Add tests for session change creation
- [x] Verify correct position in commit graph

## Phase 5: Conflict Counting
- [x] Implement function to count conflicts on/after a specific change
- [x] Use appropriate jj command to detect conflicts
- [x] Add tests for conflict counting in various scenarios

## Phase 6: Squash Attempt (Happy Path)
- [x] Implement editing uwc commit: `jj edit {uwc_id}`
- [x] Implement squashing precommit into session: `jj squash --from {precommit} --into {session} --use-destination-message`
- [x] Compare conflict counts before/after squash
- [x] Add tests for successful squash (no new conflicts)
- [x] Verify final state matches expected structure

## Phase 7: Conflict Handling Path
- [x] Implement `jj undo` twice to revert squash + edit
- [x] Verify @ is back on precommit change
- [x] Implement renaming precommit to "pt. 2" with trailer
- [x] Use `jj describe` to update message: `jjagent: session {short_id} pt. 2\n\nClaude-session-id: {full_session_id}`
- [x] Implement `jj new` to create fresh working copy
- [x] Add tests for conflict path
- [x] Verify final state with multiple session parts

## Phase 8: Integration & Posttool Hook
- [x] Integrate all posttool steps into single hook
- [x] Add comprehensive integration tests
- [x] Test complete workflow: pretool → changes → posttool
- [x] Test multiple iterations (pt. 2, pt. 3, etc.)

## Phase 9: Error Handling & Edge Cases
- [x] Handle errors (panic or handle, never silent drop)
- [x] Test with empty changes
- [x] Test with conflicting changes
- [x] Test with multiple concurrent sessions (if applicable)
- [x] Verify linear history is maintained

## Phase 10: Documentation & Polish
- [x] Update README with workflow explanation
- [x] Add inline code documentation
- [x] Create manual verification guide
- [ ] Test in real Claude Code sessions
