# jjcc Basic Behavior Specification (Revised)

## Desired Workflow

When Claude uses file editing tools, the following sequence should occur:

### 1. PreToolUse Hook
Starting state: User is on revision `xyz` (their working copy)

#### First Tool Use
1. Create a new commit between the parent and user's working copy:
   ```
   jj new --insert-before xyz -m "Claude Code Session"
   ```
   This automatically:
   - Creates Claude's commit as a child of xyz's parent
   - Rebases xyz to depend on Claude's commit
   - Switches to Claude's new commit for editing
   - Claude can see all files as they exist up to the parent, but not the user's uncommitted changes

2. Claude performs file edits in this new commit.

#### Subsequent Tool Uses in Same Session
1. If `claude_commit` already exists from a previous tool use:
   - Create a new temporary child of the user's working copy:
     ```
     jj new xyz -m "Claude temp commit"
     ```
   - Claude performs edits in this temporary commit (can see all user changes)
   - After edits, squash the temporary commit into `claude_commit`:
     ```
     jj squash --into <claude_commit>
     ```

   This accumulates all of Claude's changes from the session into a single commit while allowing Claude to see the user's current state.

### 2. PostToolUse Hook
After Claude completes edits:

1. Switch back to the user's working copy:
   ```
   jj edit xyz
   ```

## Result Structure
```
@  xyz (user's working copy - now depends on Claude's changes)
│
○  claude_commit (Claude Code Session)
│
○  parent_commit
```

## Advantages of This Approach
- Uses `--insert-before` to automatically create the desired structure on first tool use
- No manual rebasing needed for initial setup
- Claude always sees the user's current state when making subsequent edits
- Keeps Claude's changes separate and visible in the log
- Makes the user's changes depend on Claude's edits
- Maintains a clear, linear history of what Claude modified
- Allows users to easily review Claude's changes as a distinct commit