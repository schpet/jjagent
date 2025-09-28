# jjagent - JJ Claude Code Integration

> !IMPORTANT
> WIP - not full cooked

Integrate Claude Code sessions with jj version control, automatically creating and managing commits for Claude's changes.

## Features

- Automatically creates a "Claude Code Session" commit for each Claude session
- Inserts Claude's changes cleanly between parent and working copy commits
- Accumulates all changes from a claude session into a single commit

## Installation

```bash
./install.sh
```

Or manually:

```bash
cargo install --path .
# Then update your ~/.claude/settings.json (see below)
```

## Claude Configuration

Add the following to your `~/.claude/settings.json` to enable jjagent hooks:

```json
{
  "hooks": {
    "UserPromptSubmit": [
      {
        "matcher": "",
        "hooks": [
          {
            "type": "command",
            "command": "jjagent claude hooks UserPromptSubmit"
          }
        ]
      }
    ],
    "PreToolUse": [
      {
        "matcher": "Edit|MultiEdit|Write",
        "hooks": [
          {
            "type": "command",
            "command": "jjagent claude hooks PreToolUse"
          }
        ]
      }
    ],
    "PostToolUse": [
      {
        "matcher": "Edit|MultiEdit|Write",
        "hooks": [
          {
            "type": "command",
            "command": "jjagent claude hooks PostToolUse"
          }
        ]
      }
    ],
    "Stop": [
      {
        "matcher": "",
        "hooks": [
          {
            "type": "command",
            "command": "jjagent claude hooks Stop"
          }
        ]
      }
    ],
    "SessionEnd": [
      {
        "matcher": "",
        "hooks": [
          {
            "type": "command",
            "command": "jjagent claude hooks SessionEnd"
          }
        ]
      }
    ]
  }
}
```

### Important Configuration Notes

- **All hooks require the nested structure**: Each hook type must have a `matcher` field (can be empty string) and a `hooks` array containing the command objects
- **PreToolUse/PostToolUse matcher**: Set to `"Edit|MultiEdit|Write"` to trigger on file editing tools only
  - **Bash tool excluded**: Changes made via Bash commands are NOT tracked, as Bash is often used for non-code tasks (running builds, tests, package managers, etc.)
- **Command path**: Ensure `jjagent` is in your PATH (typically installed to `~/.cargo/bin/jjagent`)

## Usage

### Starting a Claude session with a custom description

Instead of running `claude` directly, use `jjagent claude start` to set an initial description for the jj change:

```bash
jjagent claude start -m "Feature XYZ"
```

This will:
1. Create a new jj change with description "Feature XYZ\n\nClaude-Session-Id: {session_id}"
2. Launch Claude
3. All edits will be squashed into this change, preserving the "Feature XYZ" description

You can also include existing trailers in your message:

```bash
jjagent claude start -m "Feature XYZ

Implement the new feature

Linear-issue-id: ABC-123"
```

The `Claude-Session-Id` trailer will be appended to your existing trailers with proper formatting.

Any additional arguments after the message will be forwarded to the `claude` command:

```bash
# Start with permissions bypassed (for trusted environments)
jjagent claude start -m "Quick fix" --dangerously-skip-permissions

# Combine multiple claude flags
jjagent claude start -m "Review changes" -- --permission-mode plan
```

### Resuming a Claude session

Resume an existing session by passing either a jj ref or a Claude session ID:

```bash
# Resume by jj ref (change ID)
jjagent claude resume abc123

# Resume by session ID
jjagent claude resume 550e8400-e29b-41d4-a716-446655440000

# Resume and update the commit description
jjagent claude resume abc123 -m "Updated description"
```

If no message is provided, the existing description is kept.

### Splitting a session

If you want to continue working in the same Claude session but create a new commit:

```bash
jjagent claude session split <session-id>
```

Or with a custom description:

```bash
jjagent claude session split <session-id> -m "Part 2: Implementation"
```

## How It Works

### Tool Support

jjagent automatically attributes changes to Claude sessions for file editing tools only:
- **File editing tools**: Edit, MultiEdit, Write
- **Bash commands**: **NOT tracked** - Bash is often used for non-code tasks like running builds, tests, installing packages, etc. If Claude modifies files via Bash, those changes will not be associated with the Claude session commit.

### When Claude edits files:

1. **First edit in a session**:
   - Creates a temporary workspace on top of your current working copy
   - Makes the edits in this temporary workspace
   - Rebases the workspace to insert it before your working copy
   - Converts it to the "Claude Code Session" commit with session ID trailer
   - Returns you to your original working copy

2. **Subsequent edits in the same session**:
   - Creates a new temporary workspace on top of your current working copy
   - Makes the edits in this temporary workspace
   - Squashes the changes into the existing Claude session commit
   - Returns you to your original working copy
   - If multiple commits have the same session ID, uses the furthest descendant

3. **Final structure**:
   ```
   @  your-working-copy (unchanged)
   │
   ○  Claude Code Session <session-id>
   │  Claude-Session-Id: <uuid>
   │
   ○  parent-commit
   ```

### Key behaviors:

- **Temporary workspaces**: All edits happen in temporary workspaces marked with `[Claude Workspace]`
- **Session tracking**: Uses `Claude-Session-Id` git trailer for finding and updating sessions
- **Smart abandonment**: Only abandons empty workspaces when no changes were made
- **Always returns to original**: After each operation, you're back on your original working copy
- **Accumulates prompts**: All prompts from a session are stored in the commit description

## Testing Your Configuration

After setting up the configuration, test it with:

```bash
cd your-jj-repo
claude -p "create a test file" --permission-mode bypassPermissions
jj log --limit 3
```

You should see a new "Claude Code Session" commit in your jj log.

## Troubleshooting

### Hooks not running

1. **Check configuration format**: Ensure all hooks have the nested `matcher` and `hooks` structure
2. **Verify jjagent is accessible**: Run `which jjagent` to ensure it's in PATH
3. **Debug hooks**: Run Claude with `--debug hooks` flag to see hook execution details
4. **Not in jj repo**: jjagent silently skips when not in a jj repository

### Environment Variables

- `JJAGENT_DISABLE=1`: Disable jjagent hooks entirely

## Development

See [plan/spec-basic-behavior.md](plan/spec-basic-behavior.md) for the detailed specification.

## License

[Add your license here]
