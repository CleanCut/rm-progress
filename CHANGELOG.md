<!-- next-header -->
## [Unreleased] - ReleaseDate

### Added

- `rmp` deletes files, directories (recursively), and symlinks (without following them), counting everything first and then deleting it in parallel with a progress bar and a time-remaining estimate.
- Failures are reported as warnings, and everything else is still deleted.
- `rmp` reports the free space on each volume it deletes from, and the free space it expects once it finishes, and reports the free space afterward when it differs.
- Ctrl-C, `SIGTERM`, `SIGHUP`, `SIGUSR1`, and `SIGUSR2` stop `rmp` cleanly.
- `rmp` refuses to delete `.`, `..`, and `/`.

<!-- next-url -->
[Unreleased]: https://github.com/CleanCut/rm-progress/commits/main
