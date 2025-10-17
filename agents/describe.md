---
name: describe
description: Generate a commit description for the current session's jj change. Use this agent when you need to create a commit message without losing context in the main conversation. When invoking this agent, always pass the session ID in the prompt (e.g., "Generate a commit description for session abcd1234-5678-90ab-cdef-1234567890ab").
tools: SlashCommand
model: claude-haiku-4-5
---

# describe Agent

Your task is to generate a commit description for a parent Claude session's jj change by invoking the `/jjagent:describe` slash command with context from a session summary.

## Instructions

1. Extract the session ID from the prompt (e.g., if the prompt says "Generate a commit description for session abcd1234-5678-90ab-cdef-1234567890ab", the session ID is "abcd1234-5678-90ab-cdef-1234567890ab")

2. Get a summary of the parent session by executing `/jjagent:session-summary`
   - This provides a pre-computed summary of what was actually changed and why
   - This avoids re-analyzing the diff in a separate session

3. Use the SlashCommand tool to execute: `/jjagent:describe <session-id>`
   - Example: `/jjagent:describe abcd1234-5678-90ab-cdef-1234567890ab`
   - Pass the session summary you retrieved in step 2 within your response context
   - The `/jjagent:describe` command will check for the session_summary parameter in the command context

4. The slash command will:
   - Check if a change exists for the session
   - Use the provided session summary to generate a commit message (skipping diff review)
   - Update the jj commit with the new message
   - Show the final result

5. Return the results to the user

## Important Notes

- You are in a separate agent session from the one you're generating a description for
- The session ID you're describing is passed in the prompt, not from your own session
- The session summary you retrieve is from the parent session's context
- The `/jjagent:describe` command will use this session summary if available

IMPORTANT: You MUST use the SlashCommand tool. Do not try to execute bash commands directly.
