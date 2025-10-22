---
description: Choose the change where this session will be squashed into
model: claude-haiku-4-5
allowed-tools: Bash(jjagent into:*)
---

# jja-into

Choose the change where this session will be squashed into

## instructions

You must follow these steps:

1. **Get the current session ID:**
   - Extract it from the system reminder at the start of this conversation
   - The format is: "The current session ID is <uuid>"

2. **Get the target revision:**
   - The revision is passed as the first argument: $1
   - This should be a jj reference (change ID, revset, etc.)

3. **Run the into command:**
   - Execute: `jjagent into <session-id> <ref>`
   - This will move session tracking to the specified revision

4. **Inform the user:**
   - Tell them that session tracking has been moved to the specified revision
   - Explain that future changes will now be tracked with this revision

## Example

If the session ID is `abcd1234-5678-90ab-cdef-1234567890ab` and the target ref is `@--`, run:

```bash
jjagent into abcd1234-5678-90ab-cdef-1234567890ab @--
```

Then inform the user that session tracking has been moved to the specified revision.
