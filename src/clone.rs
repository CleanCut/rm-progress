//! How an APFS file shares its blocks with clones, read with `getattrlist`.

use std::ffi::{CStr, CString, c_void};
use std::io;
use std::mem;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use libc::{
    ATTR_BIT_MAP_COUNT, ATTR_CMN_RETURNED_ATTRS, ATTR_CMNEXT_CLONEID, ATTR_CMNEXT_EXT_FLAGS,
    ATTR_CMNEXT_PRIVATESIZE, FSOPT_ATTR_CMN_EXTENDED, FSOPT_NOFOLLOW, FSOPT_PACK_INVAL_ATTRS,
    attribute_set_t, attrlist, getattrlist,
};

/// `ATTR_CMNEXT_CLONE_REFCNT` from `<sys/attr.h>`, which the `libc` crate does not define.
const ATTR_CMNEXT_CLONE_REFCNT: u32 = 0x1000;

/// `EF_MAY_SHARE_BLOCKS` from `<sys/stat.h>`, which the `libc` crate does not define.
const EF_MAY_SHARE_BLOCKS: u64 = 0x1;

const SHARING: u32 = ATTR_CMNEXT_CLONEID | ATTR_CMNEXT_EXT_FLAGS | ATTR_CMNEXT_CLONE_REFCNT;

/// A file's blocks, split into the ones deleting it frees and the ones its full clones share.
pub struct Clones {
    /// Bytes deleting the file frees on its own.
    pub private: u64,
    /// The data stream's identity, the same for every full clone of it.
    pub id: u64,
    /// How many full clones share the data stream, this file included.
    pub count: u32,
}

/// The buffer `getattrlist` fills for [`SHARING`], attributes in bit order.
#[repr(C, packed(4))]
#[derive(Default)]
struct SharingReply {
    length: u32,
    returned: Returned,
    id: u64,
    flags: u64,
    count: u32,
}

/// The buffer `getattrlist` fills for `ATTR_CMNEXT_PRIVATESIZE`.
#[repr(C, packed(4))]
#[derive(Default)]
struct PrivateReply {
    length: u32,
    returned: Returned,
    private: i64,
}

/// The attributes `getattrlist` returned, laid out as `attribute_set_t`.
#[repr(C)]
#[derive(Default)]
struct Returned {
    common: u32,
    volume: u32,
    dir: u32,
    file: u32,
    fork: u32,
}

const _: () = assert!(mem::size_of::<Returned>() == mem::size_of::<attribute_set_t>());

/// Reads how the file at `path`, not following a final symlink, shares its blocks with clones.
/// Returns `None` when it may share none, or its filesystem does not track clones (HFS+ reports
/// a clone count of 0).
pub fn clones(path: &Path) -> io::Result<Option<Clones>> {
    let path = CString::new(path.as_os_str().as_bytes())?;
    let mut sharing = SharingReply::default();
    query(&path, SHARING, &mut sharing)?;
    let SharingReply { returned, id, flags, count, .. } = sharing;
    if returned.fork & SHARING != SHARING || count == 0 || flags & EF_MAY_SHARE_BLOCKS == 0 {
        return Ok(None);
    }
    let mut private = PrivateReply::default();
    query(&path, ATTR_CMNEXT_PRIVATESIZE, &mut private)?;
    let PrivateReply { returned, private, .. } = private;
    if returned.fork & ATTR_CMNEXT_PRIVATESIZE == 0 {
        return Ok(None);
    }
    Ok(Some(Clones { private: u64::try_from(private).unwrap_or(0), id, count }))
}

/// Fills `reply`, a `#[repr(C, packed(4))]` buffer that starts with a length and the returned
/// attributes and then holds the extended common attributes in `fork`, in bit order.
#[allow(unsafe_code)]
fn query<R>(path: &CStr, fork: u32, reply: &mut R) -> io::Result<()> {
    let mut request = attrlist {
        bitmapcount: ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: ATTR_CMN_RETURNED_ATTRS,
        volattr: 0,
        dirattr: 0,
        fileattr: 0,
        forkattr: fork,
    };
    let options = FSOPT_NOFOLLOW | FSOPT_ATTR_CMN_EXTENDED | FSOPT_PACK_INVAL_ATTRS;
    // SAFETY: `path` is NUL-terminated, `request` is a valid `attrlist`, and `reply` is a
    // writable buffer of the size passed. FSOPT_PACK_INVAL_ATTRS keeps every requested attribute
    // at its offset whether or not the filesystem supports it.
    let status = unsafe {
        getattrlist(
            path.as_ptr(),
            (&raw mut request).cast::<c_void>(),
            (reply as *mut R).cast::<c_void>(),
            mem::size_of::<R>(),
            options,
        )
    };
    if status == 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
}
