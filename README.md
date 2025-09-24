# jjcc - JJ Claude Code Integration

Integrate Claude Code sessions with jj version control, automatically creating and managing commits for Claude's changes.

## Features

- Automatically creates a "Claude Code Session" commit for each Claude session
- Inserts Claude's changes cleanly between parent and working copy commits
- Accumulates all changes from a session into a single commit
- Preserves linear history with clear attribution

## Installation

```bash
cargo install --path .
```

Or using just:
```bash
just install
```

## Claude Configuration

Add the following to your `~/.claude/settings.json` to enable jjcc hooks:

```json
{
  "permissions": {
    "allow": [
      "WebFetch(domain:raw.githubusercontent.com)"
    ]
  },
  "hooks": {
    "UserPromptSubmit": [
      {
        "matcher": "",
        "hooks": [
          {
            "type": "command",
            "command": "jjcc hooks UserPromptSubmit"
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
            "command": "jjcc hooks PreToolUse"
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
            "command": "jjcc hooks PostToolUse"
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
            "command": "jjcc hooks Stop"
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
            "command": "jjcc hooks SessionEnd"
          }
        ]
      }
    ]
  }
}
```

### Important Configuration Notes

- **All hooks require the nested structure**: Each hook type must have a `matcher` field (can be empty string) and a `hooks` array containing the command objects
- **PreToolUse/PostToolUse matcher**: Set to `"Edit|MultiEdit|Write"` to only trigger on file modifications
- **Command path**: Ensure `jjcc` is in your PATH (typically installed to `~/.cargo/bin/jjcc`)

## How It Works

### When Claude edits files:

1. **First edit in a session**:
   - Creates a new commit using `jj new --insert-before` your working copy
   - This becomes the "Claude Code Session" commit

2. **Subsequent edits in the same session**:
   - Creates temporary child commits
   - Squashes them back into the Claude session commit

3. **Final structure**:
   ```
   @  your-working-copy
   │
   ○  Claude Code Session <uuid>
   │
   ○  parent-commit
   ```

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

- `JJCC_DESC`: Override the default "Claude Code Session" description

## Development

See [plan/spec-basic-behavior.md](plan/spec-basic-behavior.md) for the detailed specification.

## License

[Add your license here]