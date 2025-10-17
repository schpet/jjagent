---
description: Split the current session into a new jj change
model: claude-haiku-4-5
allowed-tools: Bash(jjagent split:*)
---

# jja-split

Split the current Claude session into a new jj change

## instructions

You must follow these steps:

1. **Get the current session ID:**
   - Extract it from the system reminder at the start of this conversation
   - The format is: "The current session ID is <uuid>"

2. **Run the split command:**
   - Execute: `jjagent split <session-id>`
   - This will create a new change part for the session

3. **Inform the user:**
   - Tell them a new change part was created
   - Explain that future changes will now go into this new part

## Example

If the session ID is `abcd1234-5678-90ab-cdef-1234567890ab`, run:

```bash
jjagent split abcd1234-5678-90ab-cdef-1234567890ab
```

Then inform the user that a new change part has been created for the session.
