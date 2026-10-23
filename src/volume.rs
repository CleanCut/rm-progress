use std::fs::File;
use std::io;
use std::path::Path;

use rustix::fs::fstatvfs;

use crate::target::{Kind, Target};

/// A filesystem a run deletes from, with its free space before deleting and the free space
/// expected afterward. It stays open, so its free space can be read once its targets are gone.
pub struct Volume<'t> {
    /// One of the targets on this filesystem.
    pub target: &'t Target,
    pub before: u64,
    pub expected: u64,
    dir: File,
}

impl<'t> Volume<'t> {
    /// Opens the filesystem `target` is on, expecting deleting to free `disk` bytes of it.
    pub fn open(target: &'t Target, disk: u64) -> io::Result<Self> {
        let dir = match target.kind {
            Kind::Dir => &target.path,
            Kind::File { .. } => match target.path.parent() {
                Some(parent) if !parent.as_os_str().is_empty() => parent,
                _ => Path::new("."),
            },
        };
        let dir = File::open(dir)?;
        let before = free_space(&dir)?;
        Ok(Self { target, before, expected: before + disk, dir })
    }

    /// Bytes free for unprivileged use on the filesystem now.
    pub fn free_space(&self) -> io::Result<u64> {
        free_space(&self.dir)
    }
}

fn free_space(dir: &File) -> io::Result<u64> {
    let stats = fstatvfs(dir)?;
    Ok(stats.f_bavail * stats.f_frsize)
}
