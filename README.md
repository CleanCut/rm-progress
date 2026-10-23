# rm-progress

[![Crates.io](https://img.shields.io/crates/v/rm-progress.svg)](https://crates.io/crates/rm-progress)
[![CI](https://github.com/CleanCut/rm-progress/actions/workflows/ci.yml/badge.svg)](https://github.com/CleanCut/rm-progress/actions/workflows/ci.yml)

`rmp` is a parallel `rm -rf` that shows you what it's doing. It counts everything you asked it to delete, then deletes it in parallel with a progress bar and a time-remaining estimate.

[Questions], [bug reports], and contributions are most welcome!

```text
$ rmp target node_modules
Counted 50,100 files, 151 directories, 1.36 GiB (Free space 235.40 GiB -> ~236.76 GiB)
██████████████████████████░░░░░░░░░░░░░ 897.50 MiB/1.36 GiB (1s left) 33,299/50,251 entries
```

...and when it finishes, the bar is replaced by the time it took:

```text
Counted 50,100 files, 151 directories, 1.36 GiB (Free space 235.40 GiB -> ~236.76 GiB)
Deleted in 3.142 seconds
```

On macOS, sizes are in the decimal units Finder uses (GB), and elsewhere in binary units (GiB). On macOS, free space is what Finder reports as available, which includes purgeable space macOS can reclaim on demand. On Linux, it is the space available to users other than root. The free space after deleting is an estimate of what deleting frees. A file with another hard link outside what you delete frees nothing, and on macOS neither does the part of a file that is shared with an APFS clone or kept by a snapshot, so `rmp` leaves those out. It cannot see blocks shared through reflinks on Linux filesystems such as Btrfs or XFS, so the estimate there can be too high. When the free space `rmp` finds afterward differs from the estimate, the `Deleted` line reports it.

If any files or directories were left behind, a warning reports how many.

## Features

- **Fast** - Counting and deleting both run on up to 6 threads via [Rayon], which deletes faster than `rm -rf` (see [Performance](#performance)).
- **Informative** - A live counter of files, directories, and bytes while counting, then a progress bar measured in bytes (KiB, MiB, GiB, ...) with a countdown estimate while deleting. Before deleting, it shows the free space now and the free space you'll have afterward.
- **Persistent** - A failure is reported as a warning, and `rmp` keeps going and deletes everything else it can.
- **Careful** - Symlinks are deleted, never followed. `rmp` refuses to delete `.`, `..`, or `/`.
- **Interruptible** - Ctrl-C, `SIGTERM`, `SIGHUP`, `SIGUSR1`, and `SIGUSR2` all stop `rmp` quickly and cleanly, reporting how many files and directories were left.
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
| `-r`, `-f`, `-rf` | Ignored with a warning, so `rm -rf` habits work: `rmp` always deletes directories and never asks first |
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
