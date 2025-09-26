# Concurrent Session Detection

## Problem

Multiple concurrent Claude Code sessions can corrupt a jj repository when operating simultaneously. The core issue: one session can `jj new` off another session's change, creating tangled, corrupted commit history.

### Race Condition Scenarios

#### Scenario 1: Concurrent PreToolUse
- **Time 0**: @ = user_work (abc123)
- **Time 1**: Session A PreToolUse stores abc123 as original, creates temp_workspace_A, moves to it
- **Time 2**: Session B PreToolUse (before A completes) finds @ = temp_workspace_A, stores it as "original"
- **Corruption**: Session B thinks Session A's temp workspace is the user's real work

#### Scenario 2: Landing on Another Session's Claude Change
- **Time 0**: @ = user_work
- **Time 1**: Session A creates claude_change_A with Claude-Session-Id: A
- **Time 2**: User runs `jj edit claude_change_A`
- **Time 3**: Session B PreToolUse stores claude_change_A as "original", creates temp workspace on top
- **Corruption**: Session B will squash changes into Session A's Claude change

#### Scenario 3: Crashed Session Recovery
- **Time 1**: Session A PreToolUse creates temp_workspace_A
- **Time 2**: Session A crashes
- **Time 3**: User does `jj edit temp_workspace_A` to investigate
- **Time 4**: Session B PreToolUse stores temp_workspace_A as "original"
- **Corruption**: Session B treats abandoned workspace as real work

## Solution: Fail Fast Detection

Detect unsafe states and fail immediately with clear errors before corrupting anything.

### Detection Strategy

#### Detection Points

**PreToolUse** (before any mutations):
- Check if @ is a Claude change from a different session
- Check if @ is a temporary workspace from any session
- Exception: Allow if @ is this session's own temp workspace (continuation case)
- Exception: Allow if @ is a normal user commit (no Claude markers)

**PostToolUse** (before squashing):
- Check if stored "original" is a Claude change from a different session
- Check if stored "original" is a temporary workspace
- Ensures original hasn't been compromised during tool execution

### How to Detect

**Claude changes from other sessions:**
- Extract `Claude-Session-Id: <uuid>` trailer from commit description
- Compare with current session ID
- Trailers are in the last paragraph after a blank line

**Temporary workspaces:**
- Search for `[Claude Workspace]` marker in commit description
- Any temp workspace is unsafe (even from same session if not currently on it)

**Verification timing:**
- PreToolUse: Verify current @ BEFORE storing original working copy file
- PostToolUse: Verify stored original BEFORE squashing workspace changes

## Acceptance Criteria

### Detection Requirements
- [ ] Detects @ is Claude change from different session (PreToolUse)
- [ ] Detects @ is temporary workspace from different session (PreToolUse)
- [ ] Detects stored original is Claude change from different session (PostToolUse)
- [ ] Detects stored original is temporary workspace (PostToolUse)
- [ ] Allows @ to be this session's temp workspace (continuation case)
- [ ] Allows @ to be normal user commit (no Claude-Session-Id)

### Error Handling
- [ ] Fails before storing original working copy file
- [ ] Fails before creating any commits
- [ ] Fails before squashing in PostToolUse
- [ ] Error clearly explains what's wrong
- [ ] Error shows conflicting session IDs (truncated to 8 chars)
- [ ] Error shows change IDs (truncated to 12 chars)
- [ ] Error provides step-by-step fix instructions

### Correct Operation
- [ ] Sequential sessions work (A completes, B starts)
- [ ] Same session multiple tool uses work
- [ ] Session continues on its own temp workspace
- [ ] Session works with its own Claude changes
- [ ] Doesn't interfere with normal jj operations

## Test Cases

### Should FAIL

#### 1. concurrent_session_on_temp_workspace
- Session A creates temp workspace and is on it
- Session B tries to run PreToolUse in same repo
- Result: Session B fails with "Temporary workspace detected" error

#### 2. concurrent_session_on_claude_change
- Session A creates Claude change with Claude-Session-Id: session-a
- User manually moves to that Claude change (`jj edit`)
- Session B tries to run PreToolUse
- Result: Session B fails with "Concurrent Claude session detected" showing both session IDs

#### 3. poisoned_original_working_copy
- Session A runs PreToolUse (stores original working copy)
- During tool execution, the original commit is corrupted (gets Claude-Session-Id from different session)
- Session A tries to run PostToolUse
- Result: Fails with "Concurrent Claude session detected" on the stored original

### Should SUCCEED

#### 4. sequential_sessions
- Session A runs PreToolUse → creates file → PostToolUse (completes fully, returns to original)
- Session B starts PreToolUse in same repo
- Result: Success, Session B works normally

#### 5. same_session_continuation
- Session A runs PreToolUse → creates file → PostToolUse (returns to original)
- Session A runs PreToolUse again (second tool in same session)
- Result: Success, reuses temp workspace or creates new one

#### 6. session_with_own_claude_change
- Session A runs PreToolUse → creates file → PostToolUse (creates claude_change_a)
- Session A runs PreToolUse → creates file → PostToolUse (squashes into same claude_change_a)
- Result: Success, session can work with its own Claude changes

## Edge Cases

### User manually adds Claude-Session-Id trailer
- **Behavior**: Detected as different session, fails fast
- **Rationale**: Prevents accidental corruption; users shouldn't manually add these trailers

### /tmp files deleted mid-session
- **Behavior**: PostToolUse returns early (existing behavior)
- **Rationale**: Graceful degradation - no crash, just can't complete the session properly

### User does `jj edit` mid-session
- **Behavior**: Next PreToolUse detects if new @ is unsafe
- **Rationale**: Protects against manual navigation to dangerous states

### Multiple commits with same session ID
- **Behavior**: `find_session_commit()` returns furthest descendant (existing behavior)
- **Rationale**: Already handled; session continues on most recent commit

### Temp workspace description format
- **Current format**: `[Claude Workspace] Temporary workspace for session <uuid>`
- **Detection**: Search for `"[Claude Workspace]"` substring (simple, robust)
- **Why not trailer**: Temp workspaces are ephemeral, don't need structured metadata

## Error Message Examples

### Concurrent session detected:
```
Error: Concurrent Claude session detected

The working copy is a Claude change from another session.
Another Claude Code session is likely active in this repo.

To fix:
1. Complete or cancel the other Claude session
2. Run `jj edit <your-work>` to return to your working copy
3. Retry this session

Current change: abc123456789
This session:   def45678
Other session:  abc12345
```

### Temp workspace detected:
```
Error: Temporary workspace detected

The working copy is a temporary Claude workspace.
This indicates an interrupted or concurrent session.

To fix:
1. Run `jj edit <your-work>` to return to your working copy
2. Optionally abandon the temp workspace: `jj abandon abc123456789`
3. Retry this session

Current change: abc123456789
This session:   def45678
```

## Implementation Notes

### Why check commit descriptions?
- **Claude changes**: Have `Claude-Session-Id: <uuid>` trailer (structured, parseable)
- **Temp workspaces**: Have `[Claude Workspace]` in description (simple marker)
- Different purposes: Claude changes are permanent history, temp workspaces are ephemeral

### Why check both PreToolUse and PostToolUse?
- **PreToolUse**: Prevent starting in a bad state
- **PostToolUse**: Catch corruption that happens during tool execution (race conditions, user actions)

### Performance considerations
- Each verification runs 1-2 `jj log` commands (~10ms each)
- Only happens at hook boundaries (not during tool execution)
- Acceptable overhead for correctness guarantee

### Implementation approach
- Add helper functions: `get_session_id_from_change()`, `is_temp_workspace()`, `verify_change_safe_for_session()`
- Modify `handle_pre_tool_use()`: verify @ before storing original or creating temp workspace
- Modify `handle_post_tool_use()`: verify stored original before squashing
- Fail fast: verification happens before any mutations

### Future improvements
- Add lock file in `.jj/` directory for global session tracking
- Detect concurrent sessions earlier (during UserPromptSubmit)
- Add `jjcc session list` to show active sessions