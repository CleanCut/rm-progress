use std::fs;
use std::mem;
use std::path::PathBuf;
use std::sync::mpsc::Sender;

use rayon::Scope;

use crate::interrupt::Interrupt;
use crate::stats::Statistics;
use crate::target::{Kind, Target};

/// Most non-directory entries one rayon job handles, so a huge directory spreads across threads.
pub(crate) const BATCH: usize = if cfg!(test) { 4 } else { 1024 };

/// Sends tallies of everything under `targets` to `tx` on the rayon pool, returning when the
/// walk finishes or `interrupt` is raised. Entries that cannot be read are skipped silently.
pub fn count(targets: &[Target], tx: &Sender<Statistics>, interrupt: &Interrupt) {
    let counter = Counter { tx, interrupt };
    rayon::scope(|scope| {
        for target in targets {
            match target.kind {
                Kind::Dir => {
                    let path = target.path.clone();
                    scope.spawn(move |scope| counter.dir(scope, path));
                }
                Kind::File { bytes } => counter.send(Statistics::file(bytes)),
            }
        }
    });
}

#[derive(Clone, Copy)]
struct Counter<'a> {
    tx: &'a Sender<Statistics>,
    interrupt: &'a Interrupt,
}

impl<'a> Counter<'a> {
    fn dir<'s>(self, scope: &Scope<'s>, path: PathBuf)
    where
        'a: 's,
    {
        let mut stats = Statistics::dir();
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
            stats += self.files(batch);
        }
        self.send(stats);
    }

    fn files(self, paths: Vec<PathBuf>) -> Statistics {
        let mut stats = Statistics::default();
        for path in paths {
            if self.interrupt.is_raised() {
                break;
            }
            if let Ok(metadata) = fs::symlink_metadata(&path) {
                stats += Statistics::file(metadata.len());
            }
        }
        stats
    }

    fn send(self, stats: Statistics) {
        // The receiver outlives every worker.
        let _ = self.tx.send(stats);
    }
}
