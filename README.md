# rm-progress

[![Crates.io](https://img.shields.io/crates/v/rm-progress.svg)](https://crates.io/crates/rm-progress)
[![CI](https://github.com/CleanCut/rm-progress/actions/workflows/ci.yml/badge.svg)](https://github.com/CleanCut/rm-progress/actions/workflows/ci.yml)

`rmp` is a parallel `rm -rf` that shows you what it's doing. It counts everything you asked it to delete, then deletes it in parallel with a progress bar and a time-remaining estimate.

[Questions], [bug reports], and contributions are most welcome!

```text
$ rmp target node_modules
Counted 50,100 files, 151 directories, 1,463,091,200 bytes
██████████████████████████░░░░░░░░░░░░░ 897.50 MiB/1.36 GiB (1s left) 33,299/50,251 entries
```

...and when it finishes, the bar is replaced by the totals:

```text
Counted 50,100 files, 151 directories, 1,463,091,200 bytes
Deleted 1.36 GiB in 3 seconds
```

If anything was left behind, or the files changed while `rmp` was deleting them, a warning reports each count that differs from what was counted.

## Features

- **Fast** - Counting and deleting both run on up to 6 threads via [Rayon], which deletes faster than `rm -rf` (see [Performance](#performance)).
- **Informative** - A live counter of files, directories, and bytes while counting, then a progress bar measured in bytes (KiB, MiB, GiB, ...) with a countdown estimate while deleting.
- **Persistent** - A failure is reported as a warning, and `rmp` keeps going and deletes everything else it can.
- **Careful** - Symlinks are deleted, never followed. `rmp` refuses to delete `.`, `..`, or `/`.
- **Interruptible** - Ctrl-C, `SIGTERM`, `SIGHUP`, `SIGUSR1`, and `SIGUSR2` all stop `rmp` quickly and cleanly, reporting what was deleted so far.
- **Colorful** - Colors when writing to a terminal, plain text when not. `NO_COLOR` and `CLICOLOR_FORCE` are honored.
- **Portable** - Linux and macOS.

## Install

```shell
cargo install rm-progress
```

The crate is `rm-progress`, and the command it installs is `rmp`.

## Usage

```text
rmp [OPTIONS] PATH...
```

Every argument is a path to delete. Files are deleted, directories are deleted recursively, and symlinks are deleted without touching what they point to (including when the path is given with a trailing slash, like `link/`).

| Option | Meaning |
| --- | --- |
| `-h`, `--help` | Print help |
| `-V`, `--version` | Print the version |
| `--` | Treat every argument after this one as a path |

To delete a path that starts with `-`, use `rmp -- -weird-name` or `rmp ./-weird-name`.

### Exit status

| Code | Meaning |
| --- | --- |
| `0` | Everything was deleted |
| `1` | At least one path could not be deleted (each one was reported as a warning) |
| `2` | The command line could not be understood |
| `128 + N` | Stopped by signal `N` (for example, `130` for Ctrl-C) |

`SIGKILL` cannot be caught by any program, so `kill -9` stops `rmp` immediately. Each file is removed with a single system call, so whatever is left behind is intact, just not yet deleted.

## Performance

Deleting 50,000 4 KiB files and 100 12 MiB files, spread over 101 directories, on an Apple M5 Pro laptop (18 cores):

| Filesystem | `rm -rf` | `rmp` |
| --- | --- | --- |
| APFS (macOS) | 3.25-3.51 s | 2.13-2.30 s |
| overlayfs on ext4 (Linux container on the same laptop) | 0.31-0.34 s | 0.11-0.13 s |

More threads are not faster. On APFS, deleting with more than about 6 threads takes longer, and at 18 threads `rmp` would be slower than `rm -rf`.

## Contribution

See [CONTRIBUTING.md](CONTRIBUTING.md). All contributions are assumed to be licensed under the MIT license.

## License

Distributed under the terms of the MIT license. See [LICENSE](LICENSE).

## Sponsor

Star this repository and [sponsor](https://github.com/sponsors/CleanCut) me to support projects like this. 💖

[Questions]: https://github.com/CleanCut/rm-progress/discussions
[bug reports]: https://github.com/CleanCut/rm-progress/issues/new
[Rayon]: https://github.com/rayon-rs/rayon
