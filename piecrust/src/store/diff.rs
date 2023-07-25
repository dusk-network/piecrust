// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::io;
use std::io::{Read, Write};

use qbsdiff::bsdiff::Bsdiff;
use qbsdiff::bspatch::Bspatch;

use crate::store::memory::MemoryMmap;

/// Compute the diff between an `old` and a `new` buffer, and write it to thee
/// given writer. The length of new buffer is written first, in the form of a
/// `u64`.
pub fn diff<T: Write>(
    old: &[u8],
    new: &[u8],
    writer: &mut T,
) -> io::Result<u64> {
    let new_len = new.len() as u64;
    let new_len_bytes = new_len.to_le_bytes();

    writer.write_all(&new_len_bytes)?;

    Bsdiff::new(old, new).compare(writer)
}

/// Patches the given `mmap` with the given `patch` on top of the `old`
/// buffer. The mmap will be grown if the length included in the diff is
/// larger than it's current length.
pub fn patch<T: Read>(
    old: &[u8],
    patch: &mut T,
    mmap: &mut MemoryMmap,
) -> io::Result<u64> {
    let mut new_len_bytes = [0u8; 8];

    patch.read_exact(&mut new_len_bytes)?;
    let new_len = u64::from_le_bytes(new_len_bytes) as usize;

    let delta = new_len - mmap.len();
    if delta > 0 {
        mmap.grow_by(delta)?;
    }

    // This reads the whole patch into memory. It might cause a problem in the
    // future if diffs are too large. We should consider *not* compressing diffs
    // if this is the case.
    let mut patch_bytes = Vec::new();
    patch.read_to_end(&mut patch_bytes)?;

    Bspatch::new(&patch_bytes)?.apply(old, mmap.as_bytes_mut())
}
