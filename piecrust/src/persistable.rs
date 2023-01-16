// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use bytecheck::CheckBytes;
use rkyv::validation::validators::DefaultValidator;
use rkyv::{
    ser::serializers::{BufferScratch, CompositeSerializer, WriteSerializer},
    ser::Serializer,
    Archive, Deserialize,
};

use std::fs::OpenOptions;
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::ptr;

use crate::error::Error::{self, PersistenceError};

const PERSISTABLE_SCRATCH_SIZE: usize = 64;

pub trait Persistable {
    fn restore<A, P: AsRef<Path>>(path: P) -> Result<A, Error>
    where
        A: Archive,
        <A as Archive>::Archived: for<'a> CheckBytes<DefaultValidator<'a>>
            + Deserialize<A, rkyv::Infallible>,
    {
        let file =
            std::fs::File::open(path.as_ref()).map_err(PersistenceError)?;
        let metadata =
            std::fs::metadata(path.as_ref()).map_err(PersistenceError)?;
        let ptr = unsafe {
            libc::mmap(
                ptr::null_mut(),
                u32::MAX as usize,
                libc::PROT_READ,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                0,
            ) as *mut u8
        };
        let slice = unsafe {
            core::slice::from_raw_parts(ptr, metadata.len() as usize)
        };
        let archived: &<A as Archive>::Archived =
            rkyv::check_archived_root::<A>(slice).unwrap();
        Ok(archived.deserialize(&mut rkyv::Infallible).unwrap())
    }

    fn persist<P: AsRef<Path>>(&self, path: P) -> Result<(), Error>
    where
        Self: Archive
            + for<'a> rkyv::Serialize<
                rkyv::ser::serializers::CompositeSerializer<
                    rkyv::ser::serializers::WriteSerializer<std::fs::File>,
                    rkyv::ser::serializers::BufferScratch<
                        &'a mut [u8; PERSISTABLE_SCRATCH_SIZE],
                    >,
                >,
            >,
        <Self as Archive>::Archived: for<'a> CheckBytes<DefaultValidator<'a>>,
        Self: Sized,
    {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .map_err(PersistenceError)?;

        let mut scratch_buf = [0u8; PERSISTABLE_SCRATCH_SIZE];
        let scratch = BufferScratch::new(&mut scratch_buf);
        let serializer = WriteSerializer::new(file);
        let mut composite =
            CompositeSerializer::new(serializer, scratch, rkyv::Infallible);

        composite.serialize_value(self).unwrap();
        Ok(())
    }
}
