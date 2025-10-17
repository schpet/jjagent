# Changelog

## [Unreleased]

### Added

- prevent claude from making changes when @ is on a session change (has Claude-session-id trailer)
- split command
- claude code plugin support for easier hook install
- log errors to disk when logging is enabled
- add `jjagent describe <claude-session-id> -m ...` command that preserves description trailers
- add /jja-describe claude code slash command to add llm generated descripitons
- add /jja-split claude code slash command to split a session into a new change part

## [0.2.6] - 2025-10-13

### Changed

- prevent claude from making changes when @ is not at the head
- prevent claude from making changes when repo is in a conflicted state

## [0.2.5] - 2025-10-13

### Changed

- introduced small delay to reduce branching

## [0.2.4] - 2025-10-09

### Fixed

- avoid creating .jj dirs in non-jj repos

## [0.2.3] - 2025-10-09

### Changed

- improved management of user's working copy during conflicts

## [0.2.2] - 2025-10-07

### Added

- prevent concurrent edits

## [0.2.1] - 2025-10-07

### Added

- prevent concurrent edits

## [0.2.0] - 2025-10-06

### Changed

- rewrite it all: use a better approach that doesn't require persisting state

## [0.1.3] - 2025-09-28

### Fixed

- fix CI

## [0.1.2] - 2025-09-28

### Fixed

- fix CI with fmt

## [0.1.1] - 2025-09-28

### Fixed

- support rust 1.90.0 for ci

## [0.1.0] - 2025-09-28

### Added

- initial release

[Unreleased]: https://github.com/schpet/jjagent/compare/v0.2.6...HEAD
[0.2.6]: https://github.com/schpet/jjagent/compare/v0.2.5...v0.2.6
[0.2.5]: https://github.com/schpet/jjagent/compare/v0.2.4...v0.2.5
[0.2.4]: https://github.com/schpet/jjagent/compare/v0.2.3...v0.2.4
[0.2.3]: https://github.com/schpet/jjagent/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/schpet/jjagent/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/schpet/jjagent/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/schpet/jjagent/compare/v0.1.3...v0.2.0
[0.1.3]: https://github.com/schpet/jjagent/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/schpet/jjagent/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/schpet/jjagent/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/schpet/jjagent/releases/tag/v0.1.0
