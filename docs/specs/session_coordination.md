# Session Coordination: Serialize File Editing Operations

## Problem

Multiple concurrent Claude sessions are valuable - users want to chat, search code, and work on different tasks simultaneously across terminal windows. This should work fine.

The conflict happens specifically during **file editing operations** when jjagent manipulates the jj working copy:

**The dangerous window:**
- **PreToolUse**: `jj new` creates temp workspace → @ moves to temp_workspace_A
- **[File editing happens]**: Session writes/edits files in temp workspace
- **PostToolUse**: `jj squash` merges changes back → @ returns to original work

**What goes wrong with concurrent edits:**
1. **Time 0**: @ = user_work
2. **Time 1**: Session A PreToolUse → creates temp_workspace_A → @ = temp_workspace_A
3. **Time 2**: Session B PreToolUse → sees @ = temp_workspace_A → thinks this is user's work → stores wrong original
4. **Time 3**: Session A PostToolUse → squashes, returns to user_work
5. **Time 4**: Session B PostToolUse → squashes to wrong place → CORRUPTION

**Current behavior:**
- Session B fails immediately: "Error: Temporary workspace detected"
- Forces user to manually coordinate sessions
- Prevents concurrent sessions from coexisting peacefully

**What users want:**
- Run multiple Claude sessions simultaneously (chat, read, search in parallel)
- Sessions automatically coordinate when they need to edit files
- No manual coordination or terminal-switching required

## Solution: Automatic Edit Serialization

When a session needs to edit files (PreToolUse), check if another session is currently editing. If yes, **wait automatically** until that session finishes its edit cycle.

### Key Insight

**Non-editing operations are safe to run concurrently:**
- Reading files
- Searching codebase
- Running read-only bash commands
- Chatting/answering questions

**Only editing operations need serialization:**
- PreToolUse → PostToolUse cycle (the temp workspace lifecycle)
- This is when @ moves and jj history is manipulated

### User Experience

#### Scenario 1: Concurrent Chat + Edit
1. Session A: User asks "what does this function do?" (reading files, no editing)
2. Session B: User asks "add error handling here" (needs to edit files)
3. Both sessions work simultaneously with no coordination needed
4. Result: Seamless parallelism for read-only operations

#### Scenario 2: Concurrent Edits (Automatic Queueing)
1. Session A: PreToolUse → editing files in temp_workspace_A
2. Session B: User asks to edit different file
3. Session B PreToolUse detects temp_workspace_A exists (different session)
4. Session B shows: `⏳ Waiting for another editing session (abc12345) to complete...`
5. Session A: PostToolUse completes → temp workspace cleaned up → @ back to user_work
6. Session B: Detects workspace is clear → proceeds with its own PreToolUse
7. Session B edits files normally
8. Result: Sessions automatically queued, no manual coordination

#### Scenario 3: Crashed Session During Edit
1. Session A: PreToolUse → creates temp_workspace_A → crashes mid-edit
2. Session B: User asks to edit a file (unaware Session A crashed)
3. Session B: PreToolUse detects temp_workspace_A from different session
4. Session B: Waits, checking for liveness signals
5. After 2 minutes: No liveness signals from Session A
6. Session B fails with recovery instructions:
   ```
   Error: Another editing session appears abandoned

   Session abc12345 created a temporary workspace but has not shown
   activity for 2 minutes. It may have crashed or been force-quit.

   To recover:
   1. Run `jj edit <your-work>` to return to your working copy
   2. Run `jj abandon <temp-workspace-id>` to clean up
   3. Retry this session

   Current change: temp_workspace_id_here
   This session:   def45678
   Other session:  abc12345

   Or use --force to proceed anyway (may lose uncommitted work from crashed session)
   ```

#### Scenario 4: Session Blocked on User Input
1. Session A: PreToolUse → Edit tool → waiting for user approval on destructive operation
2. User (forgetting about Session A) switches terminal, asks Session B to edit
3. Session B: `⏳ Waiting for editing session abc12345 (1m 30s)...`
4. User notices message, returns to Session A, approves operation
5. Session A: PostToolUse completes
6. Session B: Immediately proceeds with its edit
7. Result: Clear feedback prevents confusion

### What Success Looks Like

**Seamless concurrency:**
- Run 5 Claude sessions simultaneously for reading/chatting
- No coordination needed for non-editing operations
- Automatic serialization only when editing files

**Zero manual coordination:**
- Sessions automatically queue when editing
- Users never manually check for/cancel other sessions
- Clear feedback about what's happening and why

**Graceful degradation:**
- Crashed sessions don't permanently block others (2min timeout)
- Recovery instructions are clear and actionable
- `--force` flag available for advanced users

**Safety maintained:**
- No concurrent edits ever (serialized automatically)
- No corruption from misattributed changes
- No branches created from concurrent `jj new` operations

## Acceptance Criteria

### Detection and Waiting
- [ ] PreToolUse detects temp workspace from different session (extracts session ID from description)
- [ ] PreToolUse waits if different session's temp workspace exists
- [ ] Polls every 2-5 seconds for temp workspace cleanup
- [ ] Shows waiting message with other session ID (truncated to 8 chars)
- [ ] Shows elapsed wait time after 30 seconds
- [ ] Proceeds immediately when temp workspace disappears and @ is back on safe commit

### Liveness Detection
- [ ] Distinguishes active editing sessions from crashed sessions
- [ ] Active session = has shown liveness signal in last 2 minutes
- [ ] Crashed session = no liveness signal for 2+ minutes
- [ ] Liveness mechanism survives: suspend/resume, network hiccups, slow operations
- [ ] Works across different terminals, tmux/screen sessions

### Safety Invariants
- [ ] Never allows two sessions in temp workspaces simultaneously
- [ ] Waiting session re-checks state before proceeding (double-check pattern)
- [ ] After waiting completes, verifies @ is on safe commit (not another session's temp workspace)
- [ ] PostToolUse still validates original wasn't corrupted during edit

### User Communication
- [ ] Initial wait message appears within 1 second of detection
- [ ] Message updates every 30 seconds with elapsed time
- [ ] Success message when edit window clears
- [ ] Non-editing operations show no messages (silent success)
- [ ] Messages distinguish between "waiting" and "timeout" states

### Failure Handling
- [ ] After 2 minutes of no liveness signals, fails with "abandoned session" error
- [ ] Error shows: current change ID, this session ID, other session ID
- [ ] Error provides step-by-step recovery instructions
- [ ] `--force` flag bypasses waiting and liveness checks
- [ ] `--force` shows clear warning about potential corruption

### Edge Cases
- [ ] Session A crashes before creating liveness signal → Session B times out and provides recovery
- [ ] Session A very slow tool (5 min compile) → Session B sees liveness, continues waiting
- [ ] Session A blocked on user input → Session B waits (sees liveness from hooks)
- [ ] Multiple queued sessions (A editing, B waiting, C waiting) → B proceeds after A, C after B
- [ ] Session A PostToolUse fails partway → temp workspace remains → Session B times out
- [ ] User manually does `jj squash` to clean up → Session B sees cleanup, proceeds

## Liveness Mechanism Design

### Requirements for Liveness Signal

**Must prove:**
- Session is still running (not crashed/killed)
- Session is actively handling the edit cycle
- Signal is recent (within last 2 minutes)

**Must be:**
- Detectable by other sessions (readable from jj repo state)
- Updated periodically without user action
- Cleaned up when session ends normally
- Safe if left behind when session crashes

### Liveness Options

#### Option A: Timestamp Trailer in Temp Workspace Description
**How it works:**
- Temp workspace description includes: `Last-Active: 2025-09-26T15:30:45Z`
- PreToolUse hook updates this trailer every 30 seconds via `jj describe`
- PostToolUse hook updates it every 30 seconds
- Waiting sessions read timestamp, check if < 2 minutes old

**Pros:**
- All state in jj repo (no external files)
- Survives network filesystems
- Easy to inspect manually (`jj log`)

**Cons:**
- Creates many jj operations (updates every 30s)
- Changes commit hash of temp workspace repeatedly
- May interfere with content-addressed operations

#### Option B: Heartbeat File in .jj/jjagent/
**How it works:**
- Session A creates `.jj/jjagent/session-<session-id>.heartbeat`
- File contains timestamp, updated every 30 seconds (via hook or background thread)
- Session B reads this file to check liveness
- File deleted on normal session end

**Pros:**
- No jj operations (just file writes)
- Doesn't change commit hashes
- Simple to implement and debug

**Cons:**
- External to jj repo state (can get out of sync)
- May not work reliably on network filesystems (NFS caching)
- Requires manual cleanup if session crashes

#### Option C: Hook-Based Heartbeat
**How it works:**
- Every PreToolUse/PostToolUse hook execution touches heartbeat file
- No background thread needed (hooks provide the liveness signal)
- Waiting session checks heartbeat file age

**Pros:**
- Simpler than background thread
- Natural cadence (updates when session is actually doing work)
- No extra overhead when session idle

**Cons:**
- Only updates during tool use (may miss idle periods)
- Session blocked on user input looks "dead" after 2 min
- May timeout legitimate long-running operations if user not actively prompting

### Recommended Approach: Simple Polling with Timeout

**Implementation:**
- Poll temp workspace existence every 2 seconds
- 60-second timeout from when waiting starts
- If temp workspace disappears → proceed
- If 60 seconds elapse → fail with recovery instructions

**Why this works:**
- Much simpler (no heartbeat files needed)
- Uses jj's own state (temp workspace commit)
- Timeout protects against crashed sessions
- Consistent with existing patterns (no extra files)

## Acceptance Criteria (Timeout-Based)

### Waiting Behavior
- [x] Polls temp workspace existence every 2 seconds
- [x] Shows waiting message with other session ID (truncated to 8 chars)
- [x] Shows elapsed time after 30 seconds
- [x] Proceeds immediately when temp workspace disappears
- [x] Re-checks state after waiting completes (double-check pattern)

### Timeout Handling
- [x] Times out after 60 seconds if temp workspace still exists
- [x] Error message mentions "60 seconds" timeout
- [x] Error provides recovery instructions
- [x] Shows current change ID, this session ID, other session ID
- [x] Fails gracefully (doesn't corrupt repo)

## Non-Goals

### What This Does NOT Solve

**True parallel file editing:**
- Sessions still serialize when editing files
- Cannot have two sessions writing different files simultaneously
- Why: JJ working copy is single-threaded by design
- Alternative: Users can use `jj workspace add` for true parallelism

**Cross-machine coordination:**
- Heartbeat mechanism only works on local machine
- Network filesystem latency may cause false timeouts
- Why: Distributed consensus is complex, out of scope
- Workaround: Teams use separate clones or workspaces

**Read-modify-write races at file level:**
- Two sessions could read same file, both edit it, last write wins
- This is inherent to any version control workflow
- Why: This is a user coordination problem, not a tool problem
- Workaround: Users coordinate at task level, not file level

**Session priorities:**
- All sessions have equal priority (FIFO queue)
- Cannot "interrupt" a long-running session
- Why: Interruption is complex and potentially corrupting
- Workaround: User manually cancels session if needed

## Risks and Mitigations

### Risk: Timeout Too Short
- **Impact**: Session B times out while Session A is legitimately working on slow operation
- **Mitigation**: 60-second timeout is conservative (most operations << 60s)
- **Mitigation**: User can manually clean up and retry
- **Severity**: Medium (annoying but not corrupting)

### Risk: Timeout Too Long
- **Impact**: Session B waits too long for crashed Session A
- **Mitigation**: 60 seconds balances responsiveness vs safety
- **Mitigation**: User can Ctrl-C and manually recover
- **Severity**: Low (just user annoyance)


### Risk: Thundering Herd (many sessions waiting)
- **Impact**: 5 sessions queued, user confused about what's happening
- **Mitigation**: Clear message shows which session is blocking
- **Mitigation**: User can cancel waiting sessions (Ctrl-C)
- **Future**: Add `jjagent session list` to show queue
- **Severity**: Low (rare, user-caused)

### Risk: Deadlock (session waiting for itself)
- **Impact**: Session gets confused, waits for its own temp workspace
- **Mitigation**: Check if temp workspace session ID matches current session
- **Mitigation**: Session can always proceed if temp workspace is its own
- **Severity**: Low (easy to detect and handle)

## Open Questions

### Timeout Duration
- **Option A**: 60 seconds (covers most tools, may timeout slow compiles)
- **Option B**: 2 minutes (very conservative, slow to detect crashed sessions)
- **Option C**: Configurable per-repo via CLAUDE.md
- **Recommendation**: 60 seconds (chosen for balance of responsiveness and safety)

### Progress Updates Frequency
- **Option A**: Silent waiting (single message, no updates)
- **Option B**: Update every 30 seconds with elapsed time
- **Option C**: Update every 60 seconds with elapsed time
- **Recommendation**: Option B (users need reassurance it's still working)

### Force Flag Behavior
- **Option A**: `--force` skips waiting entirely (proceeds immediately)
- **Option B**: `--force` reduces timeout to 10 seconds
- **Option C**: `--force-session <session-id>` targets specific session
- **Recommendation**: Option A with scary warning about corruption risk

### Heartbeat Update Frequency
- **Option A**: Every 30 seconds (fine-grained, more writes)
- **Option B**: Every 60 seconds (coarser, fewer writes)
- **Option C**: On every hook execution (variable cadence)
- **Recommendation**: Option C (natural, no extra overhead)

### Multiple Waiting Sessions (Queue Order)
- **Scenario**: Session A editing, Session B waiting, Session C starts
- **Option A**: C waits for A directly (parallel waiters, race to proceed)
- **Option B**: C waits for B (serial queue, FIFO order)
- **Recommendation**: Option A (simpler, both proceed when A finishes)

## Success Metrics

**Reduced friction:**
- 90% reduction in manual session coordination
- Users report seamless multi-session workflow
- No need to track which terminal has active editing session

**Safety maintained:**
- Zero concurrent edit operations
- Zero corruption incidents from concurrent edits
- Timeout mechanism prevents permanent blocking

**Performance:**
- Non-editing operations have zero overhead
- Editing operations: < 100ms detection overhead
- Typical wait time: < 30 seconds (most edits are fast)

**User satisfaction:**
- Clear understanding of what's happening during waits
- Recovery from crashes is straightforward
- `--force` flag available when needed (but rarely used)

## Future Enhancements

### Phase 2: Advanced Queue Visibility
- `jjagent session list` - show active editing sessions and queue
- `jjagent session kill <id>` - forcefully terminate another session's edit lock
- Show estimated wait time based on historical session duration

### Phase 3: Read-Write Locks
- Allow multiple readers (non-editing sessions) always
- Only block when writer (editing session) is active
- More permissive, better parallelism

### Phase 4: Cross-Machine Coordination
- Use jj operation log for distributed coordination
- Detect sessions on other machines
- Handle network filesystems gracefully

### Phase 5: Operation-Level Locking
- Lock individual files/directories instead of whole working copy
- Allow concurrent edits to different files
- Requires deeper jj integration