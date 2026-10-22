# Contributing to rm-progress

First off, thanks for taking the time to contribute! ❤️

All types of contributions are encouraged and valued. Please read the relevant section below before making your contribution. It will make it a lot easier for maintainers and smooth out the experience for all involved.

> And if you like the project, but just don't have time to contribute, that's fine. There are other easy ways to support the project and show your appreciation:
> - Star the project
> - Mention it in your project's readme
> - Tell your friends and colleagues about it

## Table of Contents

- [I Have a Question](#i-have-a-question)
- [Reporting Bugs](#reporting-bugs)
- [Suggesting Enhancements](#suggesting-enhancements)
- [Your First Code Contribution](#your-first-code-contribution)
  - [Performance](#performance)
- [Join The Project Team](#join-the-project-team)

## I Have a Question

Before you ask a question, please search the existing [Issues](https://github.com/CleanCut/rm-progress/issues) and [Discussions](https://github.com/CleanCut/rm-progress/discussions). If you still need help, open a [Discussion](https://github.com/CleanCut/rm-progress/discussions) with as much context as you can, including your operating system, filesystem, and `rmp --version`.

This project is maintained by volunteers in their spare time. Please be patient, and consider opening a pull request with the change you want to see.

## Reporting Bugs

> ### Legal Notice
> When contributing to this project, you must have authored the content, have the necessary rights to the content, and agree that the content you contribute may be provided under the project license.

A good bug report shouldn't leave others needing to chase you up for more information. Before submitting one:

- Make sure you are using the latest version of `rmp`.
- Search the [issues](https://github.com/CleanCut/rm-progress/issues) to see if someone has already reported it.
- Collect:
  - The exact command you ran, and the full output (warnings included)
  - Your operating system, CPU architecture, and filesystem (APFS, ext4, XFS, a network mount, ...)
  - Whether you can reproduce it reliably, and whether older versions behave the same

Then [open an issue](https://github.com/CleanCut/rm-progress/issues/new) describing the behavior you expected, the behavior you saw, and steps someone else can follow to reproduce it.

**Never attach or describe a reproduction that deletes anything outside a throwaway directory you created for it.**

## Suggesting Enhancements

Enhancement suggestions are tracked as [GitHub Discussions](https://github.com/CleanCut/rm-progress/discussions). Before opening one, check that it hasn't already been suggested, and think about whether it fits the scope of the project: `rmp` aims to stay a small, fast, dependable `rm` with good feedback, not a Swiss Army knife.

- Use a **clear and descriptive title**.
- Describe the **current behavior**, the **behavior you want**, and why it would be useful to most users.

## Your First Code Contribution

You'll need a stable [Rust toolchain](https://rustup.rs/). `rmp` supports Linux and macOS.

```shell
git clone https://github.com/CleanCut/rm-progress
cd rm-progress
cargo test
```

Before opening a pull request, please make sure all of these pass, since CI runs the same checks:

```shell
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Some guidelines:

- Keep `src/main.rs` a thin shell. All logic lives in the library under `src/`, where tests can reach it.
- Tests must only ever delete inside a temporary directory they create (see the existing tests, which use [`tempfile`](https://docs.rs/tempfile)).
- Prefer adding assertions to an existing test over adding a new test that builds the same fixture.
- Add a line to the `Unreleased` section of [CHANGELOG.md](CHANGELOG.md) describing your change from a user's point of view.

### Performance

`rmp` has to stay at least as fast as `rm -rf`. Before changing the thread cap (`MAX_THREADS` in `src/lib.rs`) or how workers report progress, time the change against `rm -rf` on both macOS and Linux, alternating the two, on a machine with nothing else running. Timings taken on a busy machine mislead: on APFS they pointed at a different thread count than quiet runs did.

What the measurements so far have shown:

- Deleting scales only to about 5 or 6 threads. Past that, each extra thread adds kernel time without deleting any faster, and on APFS 18 threads is slower than `rm -rf`.
- Sending one progress message per deleted file halves throughput on Linux at 6 threads. Workers send a tally at the end of each batch of up to 1,024 files, and every 50 ms within one.
- The counting pass costs about 1% of the run on APFS and about 10% on Linux, where deleting is much faster.
- Redrawing the progress bar costs nothing measurable. A hidden bar ran no faster than one redrawn at indicatif's default of 20 per second, or at anything from 1 to 40 per second. To time drawing at all, run `rmp` on a pseudo-terminal with `TERM` set, since indicatif hides the bar when stderr is not a terminal or `TERM` is unset.

## Join The Project Team

Would you like to become a member of the team? Make a few contributions first to make it clear that you are comfortable interacting with the project, then open an issue asking to join, explaining what you would like to do. Right now, @CleanCut is the [BDFL](https://en.wikipedia.org/wiki/Benevolent_dictator_for_life), but would be happy to share the load.
