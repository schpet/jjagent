---
description: Summarize the context of the current Claude Code session by analyzing user prompts
model: claude-haiku-4-5
---

Summarize the context of the current Claude Code session by analyzing the user's prompts and the conversation flow.

## instructions

You must follow these steps to create a context summary:

1. **Review the conversation history:**
   - Analyze all user prompts in this session
   - Understand the overall intent and goal of the session
   - Note the progression and flow of requests

2. **Generate a summary:**
   - Create a concise summary that captures:
     - **Main Goal:** What the user is trying to accomplish
     - **Key Requests:** The main asks or tasks from the user
     - **Context:** Any important background or constraints mentioned
   - Keep it brief (3-5 bullet points typically)

3. **Present the summary:**
   - Format it clearly for easy reference
   - Make it actionable and easy to understand

## Example summary output:

```
Session Summary:
- User wants to simplify the describe-context command
- Should focus on summarizing the chat session, not jj operations
- Remove session ID handling and jj-specific logic
- Make it a simple conversation analyzer
```

## Important notes:

- DO focus on what the user is trying to accomplish
- DO capture the key requests and context
- DO keep it simple and straightforward
- DO NOT include technical implementation details
