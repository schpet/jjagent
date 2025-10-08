# https://just.systems

default:
    @just -l -u

logs-roll:
    #!/usr/bin/env bash
    if [ -f ~/.cache/jjagent/jjagent.jsonl ]; then
        mv ~/.cache/jjagent/jjagent.jsonl ~/.cache/jjagent/jjagent.jsonl.$(date +%Y%m%d_%H%M%S)
    fi

install: logs-roll
    cargo install --path .

release:
    cargo fmt --check
    cargo clippy --all-targets --all-features -- -D warnings

    svbump write "$(changelog version latest)" package.version Cargo.toml
    cargo check

    jj split Cargo.toml Cargo.lock CHANGELOG.md -m "chore: Release jjagent version $(svbump read package.version Cargo.toml)"

    jj bookmark move main --to @-
    jj git push

    git tag "v$(svbump read package.version Cargo.toml)" "$(jj log -r @- -T commit_id --no-graph)"
    git push origin "v$(svbump read package.version Cargo.toml)"
