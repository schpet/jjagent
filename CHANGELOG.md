# Changelog

## [Unreleased]

## [0.4.2] - 2025-10-28

### Added

- `jjagent claude statusline` command for displaying session change info in claude code status lines

## [0.4.1] - 2025-10-24

### Changed

- Improve arguments for the /jjagent:into command

### Added

- /jjagent:insert-after slash command to set a ref where this change should land after

## [0.4.0] - 2025-10-23

### Added

- /jjagent:into and jjagent into commands

## [0.3.0] - 2025-10-20

### Added

- setup a claude code plugin for easier setup
- add `jjagent split <SESSION_ID_OR_REF>` command to split sessions into new changes
- add `/jjagent:describe` claude code slash command to describe a session
- add `/jjagent:split` claude code slash command to split a session from claude code

### Fixed

- log errors to disk when logging is enabled

### Changed

- prevent claude from making changes when @ is on a session change as an invariant style check

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

[Unreleased]: https://github.com/schpet/jjagent/compare/v0.4.2...HEAD
[0.4.2]: https://github.com/schpet/jjagent/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/schpet/jjagent/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/schpet/jjagent/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/schpet/jjagent/compare/v0.2.6...v0.3.0
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
