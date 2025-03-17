// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use crate::store::hasher::Hash;
use bytecheck::CheckBytes;
use rkyv::{Archive, Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io;
use std::io::{ErrorKind, Read, Write};

#[derive(Debug, Clone, Default, Archive, Deserialize, Serialize)]
#[archive_attr(derive(CheckBytes))]
pub struct TreePos {
    tree_pos: BTreeMap<u32, (Hash, u64)>,
}

impl TreePos {
    pub fn insert(&mut self, k: u32, v: (Hash, u64)) {
        self.tree_pos.insert(k, v);
    }

    pub fn marshall<W: Write>(&self, w: &mut W) -> io::Result<()> {
        const CHUNK_SIZE: usize = 8192;
        const ELEM_SIZE: usize = 4 + 32 + 4;
        let mut b = [0u8; ELEM_SIZE * CHUNK_SIZE];
        let mut chk = 0;
        for (k, (h, p)) in self.tree_pos.iter() {
            let offset = chk * ELEM_SIZE;
            b[offset..(offset + 4)].copy_from_slice(&(*k).to_le_bytes());
            b[(offset + 4)..(offset + 36)].copy_from_slice(h.as_bytes());
            b[(offset + 36)..(offset + 40)]
                .copy_from_slice(&(*p as u32).to_le_bytes());
            chk = (chk + 1) % CHUNK_SIZE;
            if chk == 0 {
                w.write_all(b.as_slice())?;
            }
        }
        if chk != 0 {
            w.write_all(&b[..(chk * ELEM_SIZE)])?;
        }
        Ok(())
    }

    fn read_bytes<R: Read, const N: usize>(r: &mut R) -> io::Result<[u8; N]> {
        let mut buffer = [0u8; N];
        r.read_exact(&mut buffer)?;
        Ok(buffer)
    }

    fn is_eof<T>(r: &io::Result<T>) -> bool {
        if let Err(ref e) = r {
            if e.kind() == ErrorKind::UnexpectedEof {
                return true;
            }
        }
        false
    }

    pub fn unmarshall<R: Read>(r: &mut R) -> io::Result<Self> {
        let mut slf = Self::default();
        loop {
            let res = Self::read_bytes(r);
            if Self::is_eof(&res) {
                break;
            }
            let k = u32::from_le_bytes(res?);

            let res = Self::read_bytes(r);
            if Self::is_eof(&res) {
                break;
            }
            let hash = Hash::from(res?);

            let res = Self::read_bytes(r);
            if Self::is_eof(&res) {
                break;
            }
            let p = u32::from_le_bytes(res?);
            slf.tree_pos.insert(k, (hash, p as u64));
        }
        Ok(slf)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&u32, &(Hash, u64))> {
        self.tree_pos.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;
    use std::io::{BufReader, BufWriter};

    #[test]
    fn merkle_position_serialization() -> Result<(), io::Error> {
        const TEST_SIZE: u32 = 262144;
        const ELEM_SIZE: usize = 4 + 32 + 4;
        let mut marshalled = TreePos::default();
        let h = Hash::from([1u8; 32]);
        for i in 0..TEST_SIZE {
            marshalled.insert(i, (h, i as u64));
        }
        let v: Vec<u8> = Vec::new();
        let mut w = BufWriter::with_capacity(TEST_SIZE as usize * ELEM_SIZE, v);
        marshalled.marshall(&mut w)?;
        let mut r = BufReader::new(w.buffer());
        let unmarshalled = TreePos::unmarshall(&mut r)?;
        for i in 0..TEST_SIZE {
            assert_eq!(unmarshalled.tree_pos.get(&i), Some(&(h, i as u64)));
        }
        Ok(())
    }
}
