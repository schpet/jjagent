# jjagent - track claude code sessions as jj changes

> [!IMPORTANT]
> WIP - not fully cooked

tracks claude code sessions automatically as distinct [changes](https://jj-vcs.github.io/jj/latest/glossary/#change). allowing you and coding agents to work together at the same time while keeping an organized set of changes to review.

> You see, jj was designed around a single feature requirement. That requirement led to a very simple design addition to Git's DVCS model, that naturally enabled all of the features:
>
> jj was designed to support concurrency.

– [Jujutsu is great for the wrong reason](https://www.felesatra.moe/blog/2024/12/23/jj-is-great-for-the-wrong-reason)

## how it works

agent changes will be inserted between `@` and `@-`. your working copy is rebased automatically.

**NOT IMPLEMENTED YET** eventually, would like to also have some convenient support for workspaces, e.g. so one agent can run their tests and break things without affecting other agents or yourself.

> Workspaces let you add additional working copies attached to the same repo. A common use case is so you can run a slow build or test in one workspace while you're continuing to write code in another workspace.

– [jj workspace docs](https://jj-vcs.github.io/jj/latest/cli-reference/#jj-workspace)

## installation

<details>
<summary>Homebrew</summary>

```bash
brew install schpet/tap/jjagent
```
</details>

<details>
<summary>Download binary</summary>

Grab the latest release from:
https://github.com/schpet/jjagent/releases/latest
</details>

<details>
<summary>Build from source</summary>

Clone the repo and install locally:

```bash
cargo install --path .
```
</details>

## global configuration

```bash
# view the settings
jjagent settings | jq .

# apply them globally (you might want to manually merge if you have existing settings!)
jjagent settings | jq . > ~/.claude/settings.json
```

## usage

### kick off a session

```bash
# with jjagent globally configured in ~/claude/settings.json, it'll work with any claude session
claude

# you can also use it without global setup like this
claude --settings "$(jjagent settings)"

# to provide a description for your session, you can use claude's session id
claude --session-id $(jjagent claude issue -m "working on feature x")

# alternatively, kick off claude from jjagent
jjagent claude start -m "working on authentication feature" -- --permission-mode acceptEdits

# resume an existing session
jjagent claude resume <session-id or jj-ref>
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
