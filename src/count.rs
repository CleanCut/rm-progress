use std::fs::{self, Metadata};
use std::mem;
use std::ops::AddAssign;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::sync::mpsc::Sender;

use rayon::Scope;

use crate::interrupt::Interrupt;
use crate::stats::Statistics;
use crate::target::{Kind, Target};

/// Most non-directory entries one rayon job handles, so a huge directory spreads across threads.
pub(crate) const BATCH: usize = if cfg!(test) { 4 } else { 1024 };

/// What counting finds: the entries, and the disk space they take up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counted {
    pub stats: Statistics,
    pub disk: u64,
}

impl AddAssign for Counted {
    fn add_assign(&mut self, other: Self) {
        self.stats += other.stats;
        self.disk += other.disk;
    }
}

/// Disk space the entry described by `metadata` takes up, from its allocated blocks.
pub(crate) fn disk(metadata: &Metadata) -> u64 {
    metadata.blocks() * 512
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
                Kind::File { bytes, disk } => {
                    counter.send(Counted { stats: Statistics::file(bytes), disk })
                }
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
            disk: fs::symlink_metadata(&path).map_or(0, |metadata| disk(&metadata)),
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
                counted +=
                    Counted { stats: Statistics::file(metadata.len()), disk: disk(&metadata) };
            }
        }
        counted
    }

    fn send(self, counted: Counted) {
        // The receiver outlives every worker.
        let _ = self.tx.send(counted);
    }
}
