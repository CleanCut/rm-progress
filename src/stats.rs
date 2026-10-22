use std::ops::AddAssign;

/// Tallies of filesystem entries. Directories contribute nothing to `bytes`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Statistics {
    pub files: u64,
    pub dirs: u64,
    pub bytes: u64,
}

impl Statistics {
    /// One non-directory entry (file, symlink, socket, ...) of `bytes` length.
    pub fn file(bytes: u64) -> Self {
        Self { files: 1, bytes, ..Default::default() }
    }

    /// One directory.
    pub fn dir() -> Self {
        Self { dirs: 1, ..Default::default() }
    }

    /// Files plus directories.
    pub fn entries(&self) -> u64 {
        self.files + self.dirs
    }
}

impl AddAssign for Statistics {
    fn add_assign(&mut self, other: Self) {
        self.files += other.files;
        self.dirs += other.dirs;
        self.bytes += other.bytes;
    }
}
