# https://just.systems

default:
    @just -l -u

install:
    cargo install --path .

release:
    cargo fmt --check

    svbump write "$(changelog version latest)" package.version Cargo.toml
    cargo check

    jj split Cargo.toml Cargo.lock CHANGELOG.md -m "chore: Release jjagent version $(svbump read package.version Cargo.toml)"

    git tag "v$(svbump read package.version Cargo.toml)" "$(jj log -r @- -T commit_id --no-graph)"
    git push origin "v$(svbump read package.version Cargo.toml)"
