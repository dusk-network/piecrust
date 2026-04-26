// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const DEDUP_DIR: &str = ".dedup";
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) fn write<P, T, B>(
    store_root: P,
    kind: &str,
    target: T,
    bytes: B,
) -> io::Result<()>
where
    P: AsRef<Path>,
    T: AsRef<Path>,
    B: AsRef<[u8]>,
{
    let store_root = store_root.as_ref();
    let target = target.as_ref();
    let bytes = bytes.as_ref();
    let hash = blake3::hash(bytes).to_hex().to_string();
    let canonical_dir = store_root.join(DEDUP_DIR).join(kind);
    let canonical = canonical_dir.join(hash);

    fs::create_dir_all(&canonical_dir)?;
    write_canonical_if_missing(&canonical, bytes)?;
    replace_with_hard_link_or_copy(&canonical, target)
}

fn write_canonical_if_missing(path: &Path, bytes: &[u8]) -> io::Result<()> {
    match fs::read(path) {
        Ok(existing) if existing == bytes => Ok(()),
        Ok(_) => replace_file(path, bytes),
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            replace_file(path, bytes)
        }
        Err(err) => Err(err),
    }
}

fn replace_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = tmp_path(path);
    let result = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .and_then(|mut file| file.write_all(bytes))
        .and_then(|_| fs::rename(&tmp, path));

    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }

    result
}

fn replace_with_hard_link_or_copy(
    canonical: &Path,
    target: &Path,
) -> io::Result<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }

    let tmp = tmp_path(target);
    let _ = fs::remove_file(&tmp);

    match fs::hard_link(canonical, &tmp) {
        Ok(()) => fs::rename(&tmp, target),
        Err(_) => {
            let _ = fs::remove_file(&tmp);
            let result = fs::copy(canonical, &tmp)
                .and_then(|_| fs::rename(&tmp, target));
            if result.is_err() {
                let _ = fs::remove_file(&tmp);
            }
            result
        }
    }
}

fn tmp_path(path: &Path) -> PathBuf {
    let counter = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_else(|| "dedup".into());

    path.with_file_name(format!(
        ".{file_name}.dedup-tmp-{}-{counter}",
        std::process::id()
    ))
}
