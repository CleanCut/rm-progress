//! `rmp` deletes files and directories like `rm -rf`, in parallel, with a progress bar.
//!
//! A run counts every target on the rayon pool, then deletes them on the same pool. Workers send
//! their tallies over a channel to the calling thread, which sums them and draws the display.

pub mod args;
#[cfg(target_os = "macos")]
mod clone;
pub mod count;
pub mod delete;
pub mod interrupt;
pub mod stats;
pub mod target;
mod ui;
pub mod volume;

use std::ffi::OsString;
use std::iter;
use std::num::NonZero;
use std::process::ExitCode;
use std::sync::mpsc::{self, Sender};
use std::thread;

use rayon::ThreadPoolBuilder;

use args::{ArgsError, Command};
use count::{Freed, count};
use delete::{Event, delete};
use interrupt::Interrupt;
use stats::Statistics;
use target::Target;
use ui::{CountLine, DeleteBar};
use volume::Volume;

/// Most worker threads a run uses. Deleting gets slower, not faster, with many more than this.
const MAX_THREADS: usize = 6;

/// Runs `rmp` with the arguments that follow the program name.
pub fn run(args: impl IntoIterator<Item = OsString>) -> ExitCode {
    let operands = match args::parse(args) {
        Ok(Command::Help) => {
            print!("{}", ui::usage());
            return ExitCode::SUCCESS;
        }
        Ok(Command::Version) => {
            println!("rmp {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Ok(Command::Remove(operands)) => operands,
        Err(ArgsError::NoOperands) => {
            eprint!("{}", ui::usage());
            return ExitCode::from(2);
        }
        Err(ArgsError::UnknownOption(option)) => {
            ui::error(format!(
                "unknown option '{}'. To delete a path that starts with '-', run \
                 'rmp -- {0}' or 'rmp ./{0}'.",
                option.display()
            ));
            return ExitCode::from(2);
        }
    };
    let interrupt = match Interrupt::register() {
        Ok(interrupt) => interrupt,
        Err(error) => {
            ui::error(format!("cannot handle signals: {error}"));
            return ExitCode::FAILURE;
        }
    };
    let threads = thread::available_parallelism().map_or(1, NonZero::get).min(MAX_THREADS);
    if let Err(error) = ThreadPoolBuilder::new().num_threads(threads).build_global() {
        ui::error(format!("cannot start worker threads: {error}"));
        return ExitCode::FAILURE;
    }

    let mut failed = false;
    let mut targets: Vec<Target> = operands
        .iter()
        .filter_map(|operand| {
            Target::new(operand)
                .inspect_err(|error| {
                    ui::warn(format!("cannot remove '{}': {error}", operand.display()));
                    failed = true;
                })
                .ok()
        })
        .collect();
    if targets.is_empty() {
        return ExitCode::FAILURE;
    }
    targets.sort_by_key(|target| target.dev);

    let (total, volumes) = count_all(&targets, &interrupt);
    if let Some(signal) = interrupt.signal() {
        ui::stopped(signal, " before anything was deleted");
        return signal_exit(signal);
    }
    failed |= delete_all(&targets, total, &volumes, &interrupt) > 0;
    if let Some(signal) = interrupt.signal() {
        ui::stopped(signal, "");
        return signal_exit(signal);
    }
    if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}

/// Counts `targets`, sorted by filesystem, behind a live counter line, reading each filesystem's
/// free space on another thread meanwhile. Returns the totals and, unless `interrupt` is raised,
/// each filesystem whose free space can be read.
fn count_all<'t>(targets: &'t [Target], interrupt: &Interrupt) -> (Statistics, Vec<Volume<'t>>) {
    let groups: Vec<&[Target]> = targets.chunk_by(|a, b| a.dev == b.dev).collect();
    let line = CountLine::new();
    let mut total = Statistics::default();
    let mut volumes = thread::scope(|scope| {
        let opening = scope.spawn(|| groups.iter().map(|group| Volume::open(&group[0])).collect());
        let disks: Vec<u64> = groups
            .iter()
            .map(|group| {
                let mut freed = Freed::default();
                drive(
                    |tx| count(group, &tx, interrupt),
                    |burst| {
                        for counted in burst {
                            total += counted.stats;
                            freed.add(counted.disk, counted.shared);
                        }
                        line.update(&total);
                    },
                );
                freed.bytes()
            })
            .collect();
        let opened: Vec<_> = opening.join().unwrap_or_default();
        opened
            .into_iter()
            .zip(disks)
            .filter_map(|(volume, disk)| {
                let mut volume = volume.ok()?;
                volume.expected += disk;
                Some(volume)
            })
            .collect::<Vec<_>>()
    });
    if interrupt.is_raised() {
        volumes.clear();
    }
    line.finish(&total, &volumes);
    (total, volumes)
}

/// Deletes `targets` behind a progress bar toward `total`, returning how many failures occurred.
fn delete_all(
    targets: &[Target],
    total: Statistics,
    volumes: &[Volume],
    interrupt: &Interrupt,
) -> u64 {
    let bar = DeleteBar::new(total);
    let mut done = Statistics::default();
    let mut failures = 0;
    drive(
        |tx| delete(targets, &tx, interrupt),
        |burst| {
            for event in burst {
                match event {
                    Event::Removed(stats) => done += stats,
                    Event::Failed(failure) => {
                        failures += 1;
                        bar.warn(failure);
                    }
                }
            }
            bar.update(&done);
        },
    );
    let after: Vec<_> =
        volumes.iter().filter_map(|volume| Some((volume, volume.free_space().ok()?))).collect();
    bar.finish(&done, &after, failures);
    failures
}

/// Runs `work` on another thread until it drops its sender, handing each burst of the messages
/// it sends to `on_burst` on this thread.
fn drive<T: Send>(
    work: impl FnOnce(Sender<T>) + Send,
    mut on_burst: impl FnMut(&mut dyn Iterator<Item = T>),
) {
    let (tx, rx) = mpsc::channel();
    thread::scope(|scope| {
        scope.spawn(move || work(tx));
        while let Ok(first) = rx.recv() {
            on_burst(&mut iter::once(first).chain(rx.try_iter()));
        }
    });
}

/// The shell convention for a process stopped by `signal`.
fn signal_exit(signal: i32) -> ExitCode {
    u8::try_from(128 + signal).map_or(ExitCode::FAILURE, ExitCode::from)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
    use std::path::Path;

    use signal_hook::consts::SIGINT;
    use tempfile::TempDir;

    use super::*;
    use crate::delete::Action;
    use crate::target::Kind;

    fn collect<T: Send>(work: impl FnOnce(Sender<T>) + Send) -> Vec<T> {
        let mut messages = Vec::new();
        drive(work, |burst| messages.extend(burst));
        messages
    }

    fn link_bytes(path: &Path) -> u64 {
        fs::symlink_metadata(path).unwrap().len()
    }

    fn disk(path: &Path) -> u64 {
        fs::symlink_metadata(path).unwrap().blocks() * 512
    }

    /// The totals of `targets` and the disk space deleting them frees.
    fn count_freed(targets: &[Target], interrupt: &Interrupt) -> (Statistics, u64) {
        let mut total = Statistics::default();
        let mut freed = Freed::default();
        for counted in collect(|tx| count(targets, &tx, interrupt)) {
            total += counted.stats;
            freed.add(counted.disk, counted.shared);
        }
        (total, freed.bytes())
    }

    #[test]
    fn counts_then_deletes_without_following_symlinks() {
        let tmp = TempDir::new().unwrap();
        let keep = tmp.path().join("keep");
        fs::create_dir(&keep).unwrap();
        fs::write(keep.join("canary"), "tweet").unwrap();
        let doomed = tmp.path().join("doomed");
        fs::create_dir_all(doomed.join("sub/deep")).unwrap();
        fs::create_dir(doomed.join("empty")).unwrap();
        fs::write(doomed.join("a.txt"), "hello").unwrap();
        for i in 0..6 {
            fs::write(doomed.join(format!("f{i}")), "x").unwrap();
        }
        fs::write(doomed.join("sub/deep/b.bin"), vec![0; 1000]).unwrap();
        fs::write(doomed.join("sub/c"), "").unwrap();
        symlink(&keep, doomed.join("link")).unwrap();
        let locked = doomed.join("holder/locked");
        fs::create_dir_all(&locked).unwrap();
        fs::write(locked.join("stuck"), "abc").unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();
        let top_link = tmp.path().join("top_link");
        symlink(&keep, &top_link).unwrap();

        let targets = vec![
            Target::new(&doomed).unwrap(),
            Target::new(&tmp.path().join("top_link/")).unwrap(),
        ];
        assert_eq!(targets[1].path, top_link, "a trailing slash is dropped");
        assert!(matches!(targets[1].kind, Kind::File { .. }), "a symlink to a directory is a file");

        let interrupt = Interrupt::default();
        let (total, freed) = count_freed(&targets, &interrupt);
        let links = link_bytes(&doomed.join("link")) + link_bytes(&top_link);
        assert_eq!(total, Statistics { files: 12, dirs: 6, bytes: 5 + 6 + 1000 + 3 + links });
        assert!(freed > total.bytes, "small files take up whole blocks of disk");

        fs::hard_link(doomed.join("a.txt"), doomed.join("twin")).unwrap();
        fs::hard_link(keep.join("canary"), doomed.join("canary")).unwrap();
        fs::write(keep.join("big"), vec![7; 64 * 1024]).unwrap();
        fs::copy(keep.join("big"), doomed.join("copy")).unwrap();
        fs::copy(doomed.join("sub/deep/b.bin"), doomed.join("sub/b2")).unwrap();
        let (total, more) = count_freed(&targets, &interrupt);
        let copies = if cfg!(target_os = "macos") {
            0
        } else {
            disk(&doomed.join("copy")) + disk(&doomed.join("sub/b2"))
        };
        assert_eq!(
            more,
            freed + copies,
            "an inode counts once, and only when all its links are counted, and on APFS a clone \
             counts only when all its clones are"
        );

        let stopped = Interrupt::default();
        stopped.raise(SIGINT);
        assert!(collect(|tx| delete(&targets, &tx, &stopped)).is_empty());
        assert!(doomed.join("a.txt").exists(), "an interrupted run deletes nothing");

        let mut done = Statistics::default();
        let mut failures = Vec::new();
        for event in collect(|tx| delete(&targets, &tx, &interrupt)) {
            match event {
                Event::Removed(stats) => done += stats,
                Event::Failed(failure) => failures.push(failure),
            }
        }
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

        assert_eq!(failures.len(), 1, "only the stuck file is reported: {failures:?}");
        assert_eq!(failures[0].path, locked.join("stuck"));
        assert_eq!(failures[0].action, Action::Remove);
        let left = Statistics { files: 1, dirs: 3, bytes: 3 };
        assert_eq!(
            done,
            Statistics {
                files: total.files - left.files,
                dirs: total.dirs - left.dirs,
                bytes: total.bytes - left.bytes,
            },
            "everything counted is deleted except the stuck file and the directories above it"
        );
        let mut remaining: Vec<_> =
            fs::read_dir(&doomed).unwrap().map(|e| e.unwrap().path()).collect();
        remaining.sort();
        assert_eq!(remaining, [doomed.join("holder")]);
        assert!(top_link.symlink_metadata().is_err(), "the top-level link is deleted");
        assert_eq!(fs::read_to_string(keep.join("canary")).unwrap(), "tweet");
    }
}
