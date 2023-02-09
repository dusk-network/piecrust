use std::io;
use std::io::{Read, Write};

use bsdiff::diff::diff as bsdiff_diff;
use bsdiff::patch::patch as bsdiff_patch;

use crate::mmap::MmapMut;

/// Compute the diff between an `old` and a `new` buffer, and write it to thee
/// given writer. The length of new buffer is written first, in the form of a
/// `u64`.
pub fn diff<T: Write>(
    old: &[u8],
    new: &[u8],
    writer: &mut T,
) -> io::Result<()> {
    let new_len = new.len() as u64;
    let new_len_bytes = new_len.to_le_bytes();

    writer.write_all(&new_len_bytes)?;

    bsdiff_diff(old, new, writer)
}

/// Patches the given `mmap` with the given `patch` on top of the `old`
/// buffer. The mmap will be grown if the length included in the diff is
/// larger than it's current length.
pub fn patch<T: Read>(
    old: &[u8],
    patch: &mut T,
    mmap: &mut MmapMut,
) -> io::Result<()> {
    let mut new_len_bytes = [0u8; 8];

    patch.read_exact(&mut new_len_bytes)?;
    let new_len = u64::from_le_bytes(new_len_bytes) as usize;

    let delta = new_len - mmap.len();

    if delta > 0 {
        mmap.grow_by(delta)?;
    }

    bsdiff_patch(old, patch, mmap)
}
