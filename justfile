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
    #!/usr/bin/env bash
    set -e

    cargo fmt --check
    cargo clippy --all-targets --all-features -- -D warnings

    VERSION="$(changelog version latest)"
    svbump write "$VERSION" package.version Cargo.toml
    svbump write "$VERSION" version .claude-plugin/plugin.json
    svbump write "$VERSION" version .claude-plugin/marketplace.json
    # Update nested plugin version using jq
    jq ".plugins[0].version = \"$VERSION\"" .claude-plugin/marketplace.json > .claude-plugin/marketplace.json.tmp
    mv .claude-plugin/marketplace.json.tmp .claude-plugin/marketplace.json

    cargo check

    jj split Cargo.toml Cargo.lock CHANGELOG.md .claude-plugin/plugin.json .claude-plugin/marketplace.json -m "chore: Release jjagent version $VERSION"

    jj bookmark move main --to @-
    jj git push

    git tag "v$VERSION" "$(jj log -r @- -T commit_id --no-graph)"
    git push origin "v$VERSION"

claude-remove-local:
  -claude plugin remove jjagent@jjagent
  -claude plugin marketplace remove jjagent

claude-install-local:
  claude plugin marketplace add ./
  claude plugin install jjagent@jjagent

claude-install-github:
  claude plugin marketplace add schpet/jjagent
  claude plugin install jjagent@jjagent
