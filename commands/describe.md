---
description: Generate a commit description for the current session's jj change
model: claude-haiku-4-5
allowed-tools: Bash(jjagent:*), Bash(jj:*)
---

# jj-describe

Generate a commit description for a Claude session's jj change

## instructions

You must follow these steps to create a proper commit message:

1. **Determine session ID:**
   - Use the session ID from the system reminder (current session)
   - Store the session ID for use in subsequent steps

2. **Check if a change exists for the session:**
   - Run: `jjagent change-id <session-id>` to check if a change exists
   - **IMPORTANT:** If the command fails or returns an error, immediately stop and inform the user:
     "No jj change exists for this session yet. There's nothing to describe."
   - Do NOT proceed to the next steps if no change ID is found

3. **Gather context:**
   - Run: `jj diff -r "$(jjagent change-id <session-id>)"` to see ONLY the diff
   - Review the diff to understand what was actually changed
   - Review the conversation/context to understand why a change was made
   - **Do NOT read the existing commit message** - it will be replaced entirely

4. **Generate a NEW commit message:**
   - **First line (subject):**
     - 50 characters or less
     - Capitalize the first letter
     - Use imperative mood (e.g., "Add feature" not "Added feature" or "Adds feature")
     - No period at the end
     - Concise summary of the change, not just "jjagent: session <id>"
   - **Second line:** Blank
   - **Body (if needed):**
     - Wrap at 72 characters
     - Provide more detailed explanatory text with technical details, but stay concise
     - Explain what and why, not how the code works
     - Use bullet points for multiple items if appropriate
     - Blank lines separate paragraphs

5. **Update the description:**
   - Run: `jjagent describe <session-id> -m "your commit message here"`
   - **Do NOT include any trailers** (Claude-session-id, etc.) - they are preserved automatically
   - Only include the subject line and body
   - Use a heredoc or proper quoting to preserve formatting
   - Note: This is the Rust CLI tool command, not the slash command

6. **Show the final change**
   - Run: `jj show "$(jjagent change-id <session-id>) -s` and show the user direct output formatted as a code block

## Example commit message (what you pass to `jjagent describe`):

```
Add SessionStart and UserPromptSubmit hooks

Implement hook handlers that inject the session ID into Claude's
context at the start of a session and re-inject it when it's been
lost from the recent transcript.

- Add HookSpecificOutput structure for passing context to Claude
- Implement handle_session_start_hook to inject session ID
- Implement handle_user_prompt_submit_hook to re-inject when needed
- Add comprehensive tests for both hook handlers
```

Note: Do NOT include the `Claude-session-id` trailer - it's preserved automatically.

## Important notes:

- Do NOT use the generic "jjagent: session <id>" format for the subject line
- DO write a concise summary in the subject, with technical details in the body
- DO analyze the actual changes and write a meaningful, specific subject
- DO use imperative mood ("Add", "Fix", "Refactor", not "Added", "Fixed", "Refactored")
- DO keep the subject line to 50 characters or less
- DO wrap body lines at 72 characters
- Do NOT include trailers - they are preserved automatically by `jjagent describe`
- Do NOT look at the existing commit message - generate a fresh one from the diff
