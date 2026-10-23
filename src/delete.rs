use std::fmt;
use std::fs;
use std::io;
use std::mem;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use rayon::Scope;

use crate::count::BATCH;
use crate::interrupt::Interrupt;
use crate::stats::Statistics;
use crate::target::{Kind, Target};

/// Longest a worker holds a tally of removed files before sending it.
const SEND_INTERVAL: Duration = Duration::from_millis(50);

/// What a deletion worker reports.
#[derive(Debug)]
pub enum Event {
    Removed(Statistics),
    Failed(Failure),
}

#[derive(Debug)]
pub struct Failure {
    pub path: PathBuf,
    pub action: Action,
    pub error: io::Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Read,
    Remove,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let verb = match self.action {
            Action::Read => "read",
            Action::Remove => "remove",
        };
        write!(f, "cannot {verb} '{}': {}", self.path.display(), self.error)
    }
}

/// Deletes `targets` on the rayon pool, sending `tx` running tallies of what it removes and an
/// [`Event`] for each failure, and returning when done or soon after `interrupt` is raised.
/// Symlinks are removed, never followed. A directory is left in place when anything inside it
/// remains.
pub fn delete(targets: &[Target], tx: &Sender<Event>, interrupt: &Interrupt) {
    let deleter = Deleter { tx, interrupt };
    rayon::scope(|scope| {
        for target in targets {
            match target.kind {
                Kind::Dir => {
                    let dir = Dir::new(target.path.clone(), None);
                    scope.spawn(move |scope| deleter.dir(scope, dir));
                }
                Kind::File { bytes, .. } => {
                    scope.spawn(move |_| {
                        if !interrupt.is_raised() {
                            deleter.remove_file(&target.path, bytes);
                        }
                    });
                }
            }
        }
    });
}

/// A directory being emptied, removed once its last pending job finishes.
struct Dir {
    path: PathBuf,
    /// Jobs still at work inside this directory, counting its own listing as one.
    pending: AtomicUsize,
    /// Set once anything inside this directory is known to remain.
    incomplete: AtomicBool,
    parent: Option<Arc<Dir>>,
}

impl Dir {
    fn new(path: PathBuf, parent: Option<Arc<Dir>>) -> Arc<Self> {
        Arc::new(Self {
            path,
            pending: AtomicUsize::new(1),
            incomplete: AtomicBool::new(false),
            parent,
        })
    }

    fn mark_incomplete(&self) {
        self.incomplete.store(true, Ordering::Relaxed);
    }
}

#[derive(Clone, Copy)]
struct Deleter<'a> {
    tx: &'a Sender<Event>,
    interrupt: &'a Interrupt,
}

impl<'a> Deleter<'a> {
    fn dir<'s>(self, scope: &Scope<'s>, dir: Arc<Dir>)
    where
        'a: 's,
    {
        match fs::read_dir(&dir.path) {
            Err(error) => {
                dir.mark_incomplete();
                self.fail(dir.path.clone(), Action::Read, error);
            }
            Ok(entries) => {
                let mut batch = Vec::new();
                for entry in entries {
                    if self.interrupt.is_raised() {
                        break;
                    }
                    let entry = match entry {
                        Ok(entry) => entry,
                        Err(error) => {
                            dir.mark_incomplete();
                            self.fail(dir.path.clone(), Action::Read, error);
                            break;
                        }
                    };
                    if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                        let child = Dir::new(entry.path(), Some(Arc::clone(&dir)));
                        dir.pending.fetch_add(1, Ordering::Relaxed);
                        scope.spawn(move |scope| self.dir(scope, child));
                    } else {
                        batch.push(entry.path());
                        if batch.len() == BATCH {
                            let batch = mem::take(&mut batch);
                            let dir = Arc::clone(&dir);
                            dir.pending.fetch_add(1, Ordering::Relaxed);
                            scope.spawn(move |_| {
                                self.remove_files(&dir, batch);
                                self.finish(dir);
                            });
                        }
                    }
                }
                self.remove_files(&dir, batch);
            }
        }
        self.finish(dir);
    }

    fn remove_files(self, dir: &Dir, paths: Vec<PathBuf>) {
        let mut removed = Statistics::default();
        let mut last_sent = Instant::now();
        for path in paths {
            if self.interrupt.is_raised() {
                break;
            }
            let bytes = fs::symlink_metadata(&path).map_or(0, |metadata| metadata.len());
            match fs::remove_file(&path) {
                Ok(()) => removed += Statistics::file(bytes),
                Err(error) => {
                    dir.mark_incomplete();
                    self.fail(path, Action::Remove, error);
                }
            }
            if last_sent.elapsed() >= SEND_INTERVAL {
                self.send_removed(mem::take(&mut removed));
                last_sent = Instant::now();
            }
        }
        self.send_removed(removed);
    }

    /// Ends one pending job in `dir`, removing each directory up the chain that it empties.
    fn finish(self, mut dir: Arc<Dir>) {
        loop {
            if dir.pending.fetch_sub(1, Ordering::AcqRel) != 1 {
                return;
            }
            let removed = !dir.incomplete.load(Ordering::Relaxed)
                && !self.interrupt.is_raised()
                && self.remove_dir(&dir.path);
            let Some(parent) = dir.parent.clone() else {
                return;
            };
            if !removed {
                parent.mark_incomplete();
            }
            dir = parent;
        }
    }

    fn remove_file(self, path: &Path, bytes: u64) -> bool {
        match fs::remove_file(path) {
            Ok(()) => {
                self.send(Event::Removed(Statistics::file(bytes)));
                true
            }
            Err(error) => {
                self.fail(path.to_owned(), Action::Remove, error);
                false
            }
        }
    }

    fn remove_dir(self, path: &Path) -> bool {
        match fs::remove_dir(path) {
            Ok(()) => {
                self.send(Event::Removed(Statistics::dir()));
                true
            }
            Err(error) => {
                self.fail(path.to_owned(), Action::Remove, error);
                false
            }
        }
    }

    fn fail(self, path: PathBuf, action: Action, error: io::Error) {
        self.send(Event::Failed(Failure { path, action, error }));
    }

    fn send_removed(self, removed: Statistics) {
        if removed.entries() > 0 {
            self.send(Event::Removed(removed));
        }
    }

    fn send(self, event: Event) {
        // The receiver outlives every worker.
        let _ = self.tx.send(event);
    }
}
