# jjagent

tracks claude code sessions as jj [changes](https://jj-vcs.github.io/jj/latest/glossary/#change). allowing you and coding agents to work together at the same time while keeping an organized set of changes to review.

## how it works

when you start, `@` is at the head. lets call `@` 'users working copy' given its where you, the user works. you can change things while claude works away in the background and your changes will be here.

when a claude session is started and `PreToolUse` fires, jjagent will make a new change – a descendant of the users working copy. this is a fresh change for claude's changes to live in. after claude is done changing files, the `PostToolUse` fires and jjagent will squash those changes into a new direct ancestor of the users working copy. jj automatically rebases the descendants during the squash, and `@` is back to the users working copy. subsequent claude edit tool calls will find the session's change based on a Claude-session-id trailer in the change description.

multiple claude sessions can be going at one, a lock file is used to have them wait their turn before editing files.

it's attribution is not perfect: you might write a file while we're on a claude change, and claude might use bash to change stuff. room for improvement here! but it works well for me.

## assumptions, constraints, limitations

- you need to keep `@` as a descendent of claude's changes, the assumed workflow is that you will be working at the [head](https://jj-vcs.github.io/jj/latest/glossary/#head) or tip of descendants– if you move `@` backwards while claude is doing its thing you are in for a bad time: claude will branch or otherwise do things on wrong assumptions
- when claude is editing files, avoid running jj commands that might have side effects, ensure if you're running jj commands while claude sessions are updating files, that you use `--ignore-working-copy`. things like [running `jj log` within `watch`](https://jj-vcs.github.io/jj/latest/FAQ/#can-i-monitor-how-jj-log-evolves), shell prompts need to have `--ignore-working-copy`
- assumes you're running claude with 'accept edits on'
- avoid running `jj describe` interactively: if claude code edits a file while you have your describe editor open you'll run into 'Error: The "@" expression resolved to more than one operation'
- jjagent is currently only able to properly attribute changes from the `Edit|MultiEdit|Write` claude code tools, claude often changes files with bash and jjagent doesn't try to track that
- right now, jjagent is coupled very tightly to claude code. hopefully other agents (codex cli, gemini cli, et al) support hooks similar to claude code in the future and can be supported.

## installation

<details>
<summary>homebrew</summary>

```bash
brew install schpet/tap/jjagent
```

</details>

<details>
<summary>binaries</summary>

https://github.com/schpet/jjagent/releases/latest

</details>

<details>
<summary>from source</summary>

```bash
# clone jj agent locally
cargo install --path .
```

</details>

## setup

1. update ~/.claude/settings.json with the json this command dumps out:
   ```bash
   jjagent claude settings
   ```
2. use claude code normally in a jj repo - jjagent runs automatically via hooks

### experimental: via plugin

1. add the marketplace and install the plugin:
   ```bash
   /plugin marketplace add schpet/jjagent
   /plugin install jjagent@jjagent
   ```

2. use claude code normally in a jj repo - jjagent runs automatically via hooks

## development

Run tests:

```bash
cargo test
```

## mood board


> You see, jj was designed around a single feature requirement. That requirement led to a very simple design addition to Git's DVCS model, that naturally enabled all of the features:
>
> jj was designed to support concurrency.

– [Jujutsu is great for the wrong reason](https://www.felesatra.moe/blog/2024/12/23/jj-is-great-for-the-wrong-reason)


## acknowledgements

inspired directly by gitbutler's [claude code hooks](https://docs.gitbutler.com/features/ai-integration/claude-code-hooks)
