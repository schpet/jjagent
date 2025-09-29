# jjagent - track claude code sessions as jj changes

> [!IMPORTANT]
> WIP - not fully cooked

automatically squashes claude's changes into a claude session specific change that is added between `@` and `@-` – allowing you to run multiple claude sessions and track the changes on a separate change id.

## installation

homebrew:

```bash
brew install schpet/tap/jjagent
```

or grab a release:

https://github.com/schpet/jjagent/releases/latest

or, clone the repo locally

```bash
cargo install --path .
```

## usage

### kick off a session

```bash
# start a new claude session with jj tracking
jjagent claude start -m "working on authentication feature"

# or use the modular approach
claude --session-id "$(jjagent claude issue -m 'working on feature x')" --settings "$(jjagent settings)"

# resume an existing session
jjagent claude resume <session-id or jj-ref>
```

### global configuration

to use jjagent hooks globally with claude code, add the settings to `~/.claude/settings.json`:

```bash
jjagent settings | jq .
```

either review and merge these with your settings, or if you don't have existing settings:

```bash
jjagent settings | jq . > ~/.claude/settings.json
```

### commands

- `jjagent claude start -m <message>` - start a new claude session with jj tracking
- `jjagent claude issue -m <message>` - create a jj change and return session id (for manual claude invocation)
- `jjagent claude resume <session-id or ref>` - resume an existing session
- `jjagent claude session split <session-id>` - split a session to continue work in a new commit
- `jjagent settings` - output claude code hook configuration json
- `jjagent claude config` - same as `settings` (deprecated)

## limitations

doesn't track code generated from bash tool calls

## acknowledgements

completely inspired by using [gitbutler's](https://github.com/gitbutlerapp/gitbutler) claude hooks integration.

https://docs.gitbutler.com/features/ai-integration/claude-code-hooks#installing-gitbutler-as-a-hook
