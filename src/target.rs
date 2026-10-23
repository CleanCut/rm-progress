use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use rustix::fs::statvfs;

use crate::count::disk;

/// A path named on the command line, checked and ready to count and delete.
#[derive(Debug)]
pub struct Target {
    pub path: PathBuf,
    pub kind: Kind,
    /// The device of the filesystem the target is on.
    pub dev: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Dir,
    /// Anything that is not a directory, including a symlink to one.
    File {
        bytes: u64,
        /// Disk space the file takes up.
        disk: u64,
    },
}

#[derive(Debug)]
pub enum TargetError {
    Dot,
    Root,
    Io(io::Error),
}

impl fmt::Display for TargetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dot => f.write_str("refusing to remove '.' or '..'"),
            Self::Root => f.write_str("refusing to remove '/'"),
            Self::Io(error) => error.fmt(f),
        }
    }
}

impl Target {
    /// Looks up `operand` without following a final symlink, ignoring trailing slashes so that
    /// `link/` names the link rather than the directory it points to.
    pub fn new(operand: &Path) -> Result<Self, TargetError> {
        let bytes = operand.as_os_str().as_bytes();
        let end = bytes.iter().rposition(|&b| b != b'/').map_or(0, |i| i + 1);
        if end == 0 && !bytes.is_empty() {
            return Err(TargetError::Root);
        }
        let trimmed = &bytes[..end];
        let name = trimmed.rsplit(|&b| b == b'/').next().unwrap_or_default();
        if name == b"." || name == b".." {
            return Err(TargetError::Dot);
        }
        let path = PathBuf::from(OsStr::from_bytes(trimmed));
        let metadata = fs::symlink_metadata(&path).map_err(TargetError::Io)?;
        let kind = if metadata.is_dir() {
            Kind::Dir
        } else {
            Kind::File { bytes: metadata.len(), disk: disk(&metadata) }
        };
        Ok(Self { path, kind, dev: metadata.dev() })
    }

    /// Bytes free for unprivileged use on the filesystem the target is on.
    pub fn free_space(&self) -> io::Result<u64> {
        let on = match self.kind {
            Kind::Dir => &self.path,
            Kind::File { .. } => match self.path.parent() {
                Some(parent) if !parent.as_os_str().is_empty() => parent,
                _ => Path::new("."),
            },
        };
        let stats = statvfs(on)?;
        Ok(stats.f_bavail * stats.f_frsize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_dot_dotdot_and_root() {
        for operand in [".", "..", "./", "a/.", "a/..//", "/tmp/.."] {
            assert!(
                matches!(Target::new(Path::new(operand)), Err(TargetError::Dot)),
                "{operand} is refused as a dot directory"
            );
        }
        for operand in ["/", "//", "///"] {
            assert!(
                matches!(Target::new(Path::new(operand)), Err(TargetError::Root)),
                "{operand} is refused as the root"
            );
        }
        assert!(matches!(Target::new(Path::new("")), Err(TargetError::Io(_))));
    }
}
