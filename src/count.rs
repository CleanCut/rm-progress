use std::collections::HashMap;
use std::fs::{self, Metadata};
use std::mem;
use std::ops::AddAssign;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

use rayon::Scope;

#[cfg(target_os = "macos")]
use crate::clone::clones;
use crate::interrupt::Interrupt;
use crate::stats::Statistics;
use crate::target::{Kind, Target};

/// Most non-directory entries one rayon job handles, so a huge directory spreads across threads.
pub(crate) const BATCH: usize = if cfg!(test) { 4 } else { 1024 };

/// What counting finds: the entries, the disk space deleting them frees on their own, and the
/// disk space they share with other files.
#[derive(Debug, Default)]
pub struct Counted {
    pub stats: Statistics,
    pub disk: u64,
    pub shared: Vec<Shared>,
}

impl Counted {
    /// One non-directory entry at `path`, described by `metadata`.
    fn file(path: &Path, metadata: &Metadata) -> Self {
        let stats = Statistics::file(metadata.len());
        let (alone, clones) = split(path, metadata);
        if metadata.nlink() > 1 {
            let links = Group::Links { dev: metadata.dev(), ino: metadata.ino() };
            let shared = Shared { group: links, members: metadata.nlink(), disk: alone };
            return Self { stats, disk: 0, shared: vec![shared] };
        }
        Self { stats, disk: alone, shared: clones.into_iter().collect() }
    }
}

impl AddAssign for Counted {
    fn add_assign(&mut self, other: Self) {
        self.stats += other.stats;
        self.disk += other.disk;
        self.shared.extend(other.shared);
    }
}

/// Disk space a file shares with other files, which deleting frees only along with all of them.
#[derive(Debug)]
pub struct Shared {
    pub group: Group,
    /// How many files are in the group, counted or not.
    pub members: u64,
    pub disk: u64,
}

/// Files that share disk space.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum Group {
    /// Hard links to one inode.
    Links { dev: u64, ino: u64 },
    /// Full APFS clones of one data stream.
    Clones { dev: u64, id: u64 },
}

/// Disk space deleting everything counted on one filesystem frees, counting the space a group of
/// files shares once every file in the group has been counted.
#[derive(Default)]
pub struct Freed {
    alone: u64,
    groups: HashMap<Group, Members>,
}

struct Members {
    counted: u64,
    total: u64,
    disk: u64,
}

impl Freed {
    /// Adds `disk` bytes freed on their own and one member of each group in `shared`.
    pub fn add(&mut self, disk: u64, shared: Vec<Shared>) {
        self.alone += disk;
        for Shared { group, members, disk } in shared {
            self.groups
                .entry(group)
                .or_insert(Members { counted: 0, total: members, disk })
                .counted += 1;
        }
    }

    /// The bytes freed so far, counting only the groups whose every member was added.
    pub fn bytes(&self) -> u64 {
        let groups = self.groups.values().filter(|members| members.counted >= members.total);
        self.alone + groups.map(|members| members.disk).sum::<u64>()
    }
}

/// Disk space the entry described by `metadata` takes up, from its allocated blocks.
fn allocated(metadata: &Metadata) -> u64 {
    metadata.blocks() * 512
}

/// Splits the disk space of the file at `path` into what deleting it frees on its own and, when
/// it is a full APFS clone, the space its clones share.
#[cfg(target_os = "macos")]
fn split(path: &Path, metadata: &Metadata) -> (u64, Option<Shared>) {
    let allocated = allocated(metadata);
    if allocated == 0 {
        return (0, None);
    }
    let Ok(Some(found)) = clones(path) else {
        return (allocated, None);
    };
    let shared = allocated.saturating_sub(found.private);
    let family = (found.count > 1 && shared > 0).then(|| Shared {
        group: Group::Clones { dev: metadata.dev(), id: found.id },
        members: u64::from(found.count),
        disk: shared,
    });
    (found.private, family)
}

/// Splits the disk space of the file at `path` into what deleting it frees on its own and the
/// space it shares with clones, which is none off macOS.
#[cfg(not(target_os = "macos"))]
fn split(_path: &Path, metadata: &Metadata) -> (u64, Option<Shared>) {
    (allocated(metadata), None)
}

/// Sends tallies of everything under `targets` to `tx` on the rayon pool, returning when the
/// walk finishes or `interrupt` is raised. Entries that cannot be read are skipped silently.
pub fn count(targets: &[Target], tx: &Sender<Counted>, interrupt: &Interrupt) {
    let counter = Counter { tx, interrupt };
    rayon::scope(|scope| {
        for target in targets {
            match target.kind {
                Kind::Dir => {
                    let path = target.path.clone();
                    scope.spawn(move |scope| counter.dir(scope, path));
                }
                Kind::File { .. } => counter.send(counter.files(vec![target.path.clone()])),
            }
        }
    });
}

#[derive(Clone, Copy)]
struct Counter<'a> {
    tx: &'a Sender<Counted>,
    interrupt: &'a Interrupt,
}

impl<'a> Counter<'a> {
    fn dir<'s>(self, scope: &Scope<'s>, path: PathBuf)
    where
        'a: 's,
    {
        let mut counted = Counted {
            stats: Statistics::dir(),
            disk: fs::symlink_metadata(&path).map_or(0, |metadata| allocated(&metadata)),
            shared: Vec::new(),
        };
        if let Ok(entries) = fs::read_dir(&path) {
            let mut batch = Vec::new();
            for entry in entries.flatten() {
                if self.interrupt.is_raised() {
                    return;
                }
                if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                    let path = entry.path();
                    scope.spawn(move |scope| self.dir(scope, path));
                } else {
                    batch.push(entry.path());
                    if batch.len() == BATCH {
                        let batch = mem::take(&mut batch);
                        scope.spawn(move |_| self.send(self.files(batch)));
                    }
                }
            }
            counted += self.files(batch);
        }
        self.send(counted);
    }

    fn files(self, paths: Vec<PathBuf>) -> Counted {
        let mut counted = Counted::default();
        for path in paths {
            if self.interrupt.is_raised() {
                break;
            }
            if let Ok(metadata) = fs::symlink_metadata(&path) {
                counted += Counted::file(&path, &metadata);
            }
        }
        counted
    }

    fn send(self, counted: Counted) {
        // The receiver outlives every worker.
        let _ = self.tx.send(counted);
    }
}
