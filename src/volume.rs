#[cfg(target_os = "macos")]
use std::ffi::OsStr;
use std::fs::File;
use std::io;
#[cfg(target_os = "macos")]
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

#[cfg(target_os = "macos")]
use objc2::rc::autoreleasepool;
#[cfg(target_os = "macos")]
use objc2_foundation::{NSArray, NSNumber, NSString, NSURL};
#[cfg(target_os = "macos")]
use rustix::fs::fstatfs;
#[cfg(not(target_os = "macos"))]
use rustix::fs::fstatvfs;

use crate::target::{Kind, Target};

/// A filesystem a run deletes from, with its free space before deleting and the free space
/// expected afterward. It stays open, so its free space can be read once its targets are gone.
pub struct Volume<'t> {
    /// One of the targets on this filesystem.
    pub target: &'t Target,
    pub before: u64,
    /// Free space expected once deleting finishes, `before` until the caller adds what it frees.
    pub expected: u64,
    dir: File,
}

impl<'t> Volume<'t> {
    /// Opens the filesystem `target` is on and reads its free space.
    pub fn open(target: &'t Target) -> io::Result<Self> {
        let dir = match target.kind {
            Kind::Dir => &target.path,
            Kind::File { .. } => match target.path.parent() {
                Some(parent) if !parent.as_os_str().is_empty() => parent,
                _ => Path::new("."),
            },
        };
        let dir = File::open(dir)?;
        let before = free_space(&dir)?;
        Ok(Self { target, before, expected: before, dir })
    }

    /// Bytes free on the filesystem now. On macOS this is what Finder reports as available,
    /// which includes purgeable space.
    pub fn free_space(&self) -> io::Result<u64> {
        free_space(&self.dir)
    }
}

#[cfg(not(target_os = "macos"))]
fn free_space(dir: &File) -> io::Result<u64> {
    let stats = fstatvfs(dir)?;
    Ok(stats.f_bavail * stats.f_frsize)
}

#[cfg(target_os = "macos")]
fn free_space(dir: &File) -> io::Result<u64> {
    let stats = fstatfs(dir)?;
    let mount: Vec<u8> =
        stats.f_mntonname.iter().map(|&c| c as u8).take_while(|&b| b != 0).collect();
    let mount = OsStr::from_bytes(&mount).to_str().ok_or(io::ErrorKind::InvalidData)?;
    autoreleasepool(|_| {
        let url = NSURL::fileURLWithPath(&NSString::from_str(mount));
        let key = NSString::from_str("NSURLVolumeAvailableCapacityForImportantUsageKey");
        let values = url
            .resourceValuesForKeys_error(&NSArray::from_slice(&[&*key]))
            .map_err(|error| io::Error::other(error.localizedDescription().to_string()))?;
        let number = values
            .objectForKey(&key)
            .and_then(|value| value.downcast::<NSNumber>().ok())
            .ok_or(io::ErrorKind::Unsupported)?;
        u64::try_from(number.longLongValue()).map_err(|_| io::ErrorKind::InvalidData.into())
    })
}
