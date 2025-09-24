# jjcc Basic Behavior Specification

## Desired Workflow

When Claude uses file editing tools, the following sequence should occur:

### 1. PreToolUse Hook
Starting state: User is on revision `xyz` (their working copy)

#### First Tool Use
1. Create a new empty commit inserted before `xyz`:
   ```
   jj new --insert-before xyz
   ```
   This automatically:
   - Creates a new empty commit between xyz's parent and xyz
   - Makes xyz depend on this new commit
   - Switches to the new commit as the working copy

2. Claude performs file edits in this new commit (which becomes `claude_commit`).

#### Subsequent Tool Uses in Same Session
1. If `claude_commit` already exists from a previous tool use:
   - Create a new temporary child of `claude_commit`:
     ```
     jj new <claude_commit>
     ```
   - Claude performs edits in this temporary commit

### 2. PostToolUse Hook
After Claude completes edits:

#### First Tool Use
1. The current commit is already `claude_commit` inserted in the right place
2. Simply switch back to the user's working copy `xyz`

#### Subsequent Tool Uses
1. Squash the temporary commit back into `claude_commit`:
   ```
   jj squash --from <temp_commit> --into <claude_commit> --use-destination-message
   ```
2. Switch back to the user's working copy `xyz`

## Result Structure
```
@  xyz (user's working copy - now depends on Claude's changes)
│
○  claude_commit (Claude Code Session)
│
○  parent_commit
```

This approach:
- Keeps Claude's changes separate and visible in the log
- Inserts Claude's changes into the history before the user's working copy
- Makes the user's changes depend on Claude's edits
- Maintains a clear, linear history of what Claude modified
- Allows users to easily review Claude's changes as a distinct commit