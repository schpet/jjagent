---
description: Create a new change after a reference and track the session in it
model: claude-haiku-4-5
allowed-tools: Bash(jjagent after:*)
---

# jja-after

Create a new change after a reference and track the session in it

## instructions

You must follow these steps:

1. **Get the current session ID:**
   - Extract it from the system reminder at the start of this conversation
   - The format is: "The current session ID is <uuid>"

2. **Get the target reference:**
   - The reference is passed as the first argument: $1
   - This should be a jj reference (change ID, revset, etc.)

3. **Run the after command:**
   - Execute: `jjagent after <session-id> <ref>`
   - This will create a new change after the specified reference and move session tracking to it

4. **Inform the user:**
   - Tell them that a new change has been created after the specified reference
   - Explain that the session is now tracking this new change

## Example

If the session ID is `abcd1234-5678-90ab-cdef-1234567890ab` and the target ref is `@-`, run:

```bash
jjagent after abcd1234-5678-90ab-cdef-1234567890ab @-
```

Then inform the user that a new change has been created and the session is now tracking it.
