# jjagent - track claude code sessions as jj changes

tracks claude code sessions as jujutsu [changes](https://jj-vcs.github.io/jj/latest/glossary/#change). allowing you and coding agents to work together at the same time while keeping an organized set of changes to review.

> You see, jj was designed around a single feature requirement. That requirement led to a very simple design addition to Git's DVCS model, that naturally enabled all of the features:
>
> jj was designed to support concurrency.

– [Jujutsu is great for the wrong reason](https://www.felesatra.moe/blog/2024/12/23/jj-is-great-for-the-wrong-reason)

## how it works

(todo...)

## constraints

- `@`, aka working copy commit, must be kept at [head](https://jj-vcs.github.io/jj/latest/glossary/#head) – if you move `@` elsewhere while claude is doing its thing you are in for a bad time: claude will branch or otherwise do things on wrong assumptions
- when claude is editing files, avoid running jj commands that might have side effects, ensure if you're running jj commands while claude sessions are updating files, that you use `--ignore-working-copy`. things like [running `jj log` within `watch`](https://jj-vcs.github.io/jj/latest/FAQ/#can-i-monitor-how-jj-log-evolves), shell prompts need to have `--ignore-working-copy`
- avoid running `jj describe` interactively: if claude code edits a file while you have your describe editor open you'll run into 'Error: The "@" expression resolved to more than one operation'


## installation

### homebrew

```bash
brew install schpet/tap/jjagent
```

### binaries

https://github.com/schpet/jjagent/releases/latest

### from source

```bash
# clone jj agent locally
cargo install --path .
```

## setup

1. update ~/.claude/settings.json with the json this command dumps out
    ```bash
    jjagent claude settings
    ```
2. use claude code normally in a jj repo - jjagent runs automatically via hooks

## development

Run tests:
```bash
cargo test
```
