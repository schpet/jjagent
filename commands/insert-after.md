---
description: Insert a new session change after a specific revision
argument-hint: <ref> [message]
model: claude-haiku-4-5
allowed-tools: Bash(jjagent:*), Bash(jj:*)
---

# jjagent:insert-after

Insert a new session change after a specific revision using jj's --insert-after flag

## instructions

You must follow these steps:

1. **Parse arguments:**
   - `$1` (required): The jj reference to insert after
   - `$2 $3 $4...` (optional): Custom commit message for the session (all remaining arguments)
   - If `$1` is empty, inform the user that a reference is required

2. **Get the current session ID:**
   - Extract it from the system reminder at the start of this conversation
   - The format is: "The current session ID is <uuid>"

3. **Check if a session change already exists:**
   - Run: `jjagent change-id <session-id>`
   - If this succeeds and returns a change ID, the session already has a change
   - If it fails, the session doesn't have a change yet

4. **Validate that ref is an ancestor of @ (working copy):**
   - Run: `jj log -r "$1..@" --no-graph -T "change_id.short()"`
   - If this command fails or returns empty output, `$1` is not an ancestor of `@`
   - If not an ancestor, inform the user: "Error: '$1' is not an ancestor of the working copy. Please choose a revision that comes before @ in the commit history."
   - Stop execution if validation fails

5. **Handle existing session change:**
   - If a change already exists for this session, **STOP and ask the user for confirmation**:
     "This session already has a change at <change-id>. Would you like to rebase it to insert after '$1'?"
   - **IMPORTANT: You MUST wait for explicit user confirmation before proceeding**
   - Do NOT rebase without the user's explicit approval
   - If the user confirms YES, then run: `jj rebase -r <change-id> --insert-after "$1"`
   - If the user says NO or declines, stop and explain that no changes were made
   - After rebasing (if confirmed), show the result with: `jj log -r <change-id> --no-graph`

6. **Create new session change (if no existing change):**
   - Build the commit message using the session-message command:
     - If custom message arguments were provided (if `$2` is not empty):
       - Combine all arguments from `$2` onwards into the message
       - Run: `jjagent session-message <session-id> "$2 $3 $4..."` (all remaining args)
     - If no custom message (if `$2` is empty):
       - Run: `jjagent session-message <session-id>`
   - Create the change: `jj new --insert-after "$1" --no-edit -m "$(jjagent session-message <session-id> [message if provided])"`
   - Show the result with: `jj log -r @ --no-graph`

7. **Inform the user:**
   - Confirm that the session change has been created/rebased
   - Explain that future edits will be tracked in this change

## Example usage

```bash
# Insert session after the parent of working copy
/jjagent:insert-after @-

# Insert with custom message
/jjagent:insert-after @-- "Implement user authentication"

# Insert after a specific change ID
/jjagent:insert-after qwerty123
```

## Important notes

- The `$1` (ref) must be an ancestor of `@` (working copy)
- If a session change already exists, offer to rebase it
- Always validate the reference before creating/rebasing changes
- Use proper quoting when passing `$1` to shell commands
- When passing custom messages to `jjagent session-message`, combine all arguments from `$2` onwards
