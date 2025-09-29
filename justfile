# https://just.systems

default:
    @just -l -u

install:
    cargo install --path .

release:
    cargo test
    cargo fmt --check

    svbump write "$(cargo run -- version latest)" package.version Cargo.toml
    cargo check

    git commit Cargo.toml Cargo.lock CHANGELOG.md -m "chore: Release jjagent version $(svbump read package.version Cargo.toml)"

    git tag "v$(svbump read package.version Cargo.toml)" "$(jj log -r @- -T commit_id --no-graph)"
