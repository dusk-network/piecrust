// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::io;
use std::io::{Read, Write};

use qbsdiff::bsdiff::Bsdiff;
use qbsdiff::bspatch::Bspatch;

use crate::store::memory::Memory;

/// Compute the diff between the old and the new memory, and write it to the
/// given writer.
///
/// First, the difference in size between the old and the new memory will be
/// written, to handle growth of the memory when [`patch`] is called, and then
/// the old memory will be grown to match the size of the new memory.
///
/// # Important
/// The old memory should be discarded (dropped) after this operation.
pub fn diff<T: Write>(
    old: &Memory,
    new: &Memory,
    writer: &mut T,
) -> io::Result<()> {
    let mut old = old.write();
    let new = new.read();

    let old_len = old.len();
    let new_len = new.len();

    debug_assert!(
        new_len >= old_len,
        "Length of memories should strictly increase"
    );

    let delta_len = new_len - old_len;
    let delta_len_bytes = delta_len.to_le_bytes();

    writer.write_all(&delta_len_bytes)?;

    if delta_len > 0 {
        old.grow_by(delta_len)?;
    }

    Bsdiff::new(old.as_bytes(), new.as_bytes()).compare(writer)?;

    Ok(())
}

/// Applies the given patch on top of the new memory, using the old one as a
/// base for the patch.
///
/// First, the growth delta is read from the patch, and if it's larger than zero
/// both memories are grown. Then the patch is applied on top on the new memory,
/// stemming from the contents of the old.
///
/// # Important
/// The old memory should be discarded (dropped) after this operation.
pub fn patch<T: Read>(
    old: &Memory,
    new: &Memory,
    patch: &mut T,
) -> io::Result<()> {
    let mut old = old.write();
    let mut new = new.write();

    let mut delta_len_bytes = [0u8; 8];

    patch.read_exact(&mut delta_len_bytes)?;
    let delta_len = u64::from_le_bytes(delta_len_bytes) as usize;

    if delta_len > 0 {
        old.grow_by(delta_len)?;
        new.grow_by(delta_len)?;
    }

    // This reads the whole patch into memory. It might cause a problem in the
    // future if diffs are too large. We should consider *not* compressing diffs
    // if this is the case.
    let mut patch_bytes = Vec::new();
    patch.read_to_end(&mut patch_bytes)?;

    Bspatch::new(&patch_bytes)?.apply(old.as_bytes(), new.as_bytes_mut())?;

    Ok(())
}
