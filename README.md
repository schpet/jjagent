# jjcc - JJ Claude Code Integration

```
   /\_/\
  ( o.o )
   > ^ <
  /|   |\
 (_|   |_)
```

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

Add the following to your `~/.claude/settings.json` to enable jjcc hooks:

```json
{
  "hooks": {
    "UserPromptSubmit": [
      {
        "matcher": "",
        "hooks": [
          {
            "type": "command",
            "command": "jjcc claude hooks UserPromptSubmit"
          }
        ]
      }
    ],
    "PreToolUse": [
      {
        "matcher": "Edit|MultiEdit|Write|Bash",
        "hooks": [
          {
            "type": "command",
            "command": "jjcc claude hooks PreToolUse"
          }
        ]
      }
    ],
    "PostToolUse": [
      {
        "matcher": "Edit|MultiEdit|Write|Bash",
        "hooks": [
          {
            "type": "command",
            "command": "jjcc claude hooks PostToolUse"
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
            "command": "jjcc claude hooks Stop"
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
            "command": "jjcc claude hooks SessionEnd"
          }
        ]
      }
    ]
  }
}
```

### Important Configuration Notes

- **All hooks require the nested structure**: Each hook type must have a `matcher` field (can be empty string) and a `hooks` array containing the command objects
- **PreToolUse/PostToolUse matcher**: Set to `"Edit|MultiEdit|Write|Bash"` to trigger on file modifications and bash commands
- **Command path**: Ensure `jjcc` is in your PATH (typically installed to `~/.cargo/bin/jjcc`)

## How It Works

### Tool Support

jjcc automatically attributes changes to Claude sessions for all file modification tools:
- **File editing tools**: Edit, MultiEdit, Write
- **Bash commands**: Any bash command that modifies files (e.g., `cargo update`, `npm install`, code generation scripts)

The implementation leverages jj's universal change detection (`jj diff --stat`) which works regardless of how files are modified.

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
2. **Verify jjcc is accessible**: Run `which jjcc` to ensure it's in PATH
3. **Debug hooks**: Run Claude with `--debug hooks` flag to see hook execution details
4. **Not in jj repo**: jjcc silently skips when not in a jj repository

### Environment Variables

- `JJCC_DISABLE=1`: Disable jjcc hooks entirely

## Development

See [plan/spec-basic-behavior.md](plan/spec-basic-behavior.md) for the detailed specification.

## License

[Add your license here]
