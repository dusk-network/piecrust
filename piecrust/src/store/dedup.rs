// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

const DEDUP_DIR: &str = ".dedup";
const HASH_HEX_LEN: usize = blake3::OUT_LEN * 2;
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);
static DEDUP_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy)]
pub(crate) enum Kind {
    Bytecode,
    Objectcode,
    ObjectcodeMeta,
}

impl Kind {
    fn directory(self) -> &'static str {
        match self {
            Self::Bytecode => "bytecode",
            Self::Objectcode => "objectcode",
            Self::ObjectcodeMeta => "objectcode-meta",
        }
    }
}

pub(crate) fn write<P, T, B>(
    store_root: P,
    kind: Kind,
    target: T,
    bytes: B,
) -> io::Result<()>
where
    P: AsRef<Path>,
    T: AsRef<Path>,
    B: AsRef<[u8]>,
{
    let _guard = DEDUP_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let store_root = store_root.as_ref();
    let target = target.as_ref();
    let bytes = bytes.as_ref();
    let hash = hash_bytes(bytes);
    let old_hash = file_hash(target)?;
    let canonical_dir = store_root.join(DEDUP_DIR).join(kind.directory());
    let canonical = canonical_dir.join(&hash);

    fs::create_dir_all(&canonical_dir)?;
    write_canonical_if_missing(&canonical, bytes)?;
    replace_with_hard_link_or_copy(&canonical, target)?;

    if let Some(old_hash) = old_hash.filter(|old_hash| old_hash != &hash) {
        remove_unreferenced_hash(store_root, kind, &old_hash)?;
    }

    Ok(())
}

pub(crate) fn remove_file<P, T>(
    store_root: P,
    kind: Kind,
    target: T,
) -> io::Result<()>
where
    P: AsRef<Path>,
    T: AsRef<Path>,
{
    let _guard = DEDUP_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let store_root = store_root.as_ref();
    let target = target.as_ref();
    let hash = file_hash(target)?;

    match fs::remove_file(target) {
        Ok(()) => {
            if let Some(hash) = hash {
                remove_unreferenced_hash(store_root, kind, &hash)?;
            }
            Ok(())
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

fn remove_unreferenced_hash(
    store_root: &Path,
    kind: Kind,
    hash: &str,
) -> io::Result<()> {
    if !is_hash(hash) {
        return Ok(());
    }

    let canonical =
        store_root.join(DEDUP_DIR).join(kind.directory()).join(hash);

    if !canonical.is_file() {
        return Ok(());
    }

    if has_live_hard_link(&canonical)? {
        return Ok(());
    }

    fs::remove_file(canonical)
}

fn has_live_hard_link(path: &Path) -> io::Result<bool> {
    Ok(fs::metadata(path)?.nlink() > 1)
}

fn file_hash(path: &Path) -> io::Result<Option<String>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(hash_bytes(&bytes))),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

fn hash_bytes(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

fn is_hash(hash: &str) -> bool {
    hash.len() == HASH_HEX_LEN && hash.bytes().all(|b| b.is_ascii_hexdigit())
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
        Ok(()) => {
            let result = fs::rename(&tmp, target);
            if result.is_err() {
                let _ = fs::remove_file(&tmp);
            }
            result
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hard_link_rename_failure_removes_tmp_link() {
        let dir = tempfile::tempdir().expect("tempdir");
        let canonical = dir.path().join("canonical");
        let target = dir.path().join("target");

        fs::write(&canonical, b"canonical").expect("canonical");
        fs::create_dir(&target).expect("target dir");

        replace_with_hard_link_or_copy(&canonical, &target)
            .expect_err("rename over directory should fail");

        let leaked_tmp = fs::read_dir(dir.path())
            .expect("read tempdir")
            .map(|entry| entry.expect("dir entry").file_name())
            .any(|name| name.to_string_lossy().contains("dedup-tmp"));
        assert!(!leaked_tmp);
        assert_eq!(fs::metadata(&canonical).expect("metadata").nlink(), 1);
    }
}
