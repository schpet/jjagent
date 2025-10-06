# jjagent - track claude code sessions as jj changes

> [!IMPORTANT]
> WIP - not fully cooked, i published this to show a friend but it OFTEN puts my repo into a bad state and i'm working to improve that :-) 

tracks claude code sessions automatically as distinct [changes](https://jj-vcs.github.io/jj/latest/glossary/#change). allowing you and coding agents to work together at the same time while keeping an organized set of changes to review.

> You see, jj was designed around a single feature requirement. That requirement led to a very simple design addition to Git's DVCS model, that naturally enabled all of the features:
>
> jj was designed to support concurrency.

– [Jujutsu is great for the wrong reason](https://www.felesatra.moe/blog/2024/12/23/jj-is-great-for-the-wrong-reason)

## how it works

TODO

## installation

```bash
cargo install --path .
```

## setup

1. Add jjagent to your Claude Code hooks:
```bash
# update ~/.claude/settings.json with this json:
jjagent claude settings
```

2. Use Claude Code as normal in a jj repo - jjagent runs automatically via hooks

## development

Run tests:
```bash
cargo test
```

see [docs/workflow.md](docs/workflow.md) for the complete technical workflow specification.
