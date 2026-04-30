// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process;
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
        remove_unreferenced_hash_best_effort(store_root, kind, &old_hash);
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
                remove_unreferenced_hash_best_effort(store_root, kind, &hash);
            }
            Ok(())
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

fn remove_unreferenced_hash_best_effort(
    store_root: &Path,
    kind: Kind,
    hash: &str,
) {
    if let Err(err) = remove_unreferenced_hash(store_root, kind, hash) {
        tracing::warn!(
            kind = kind.directory(),
            hash,
            "failed to remove unreferenced dedup canonical: {err}"
        );
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
            replace_with_copy(canonical, target, &tmp)
        }
    }
}

fn replace_with_copy(
    canonical: &Path,
    target: &Path,
    tmp: &Path,
) -> io::Result<()> {
    let result = fs::copy(canonical, tmp).and_then(|_| fs::rename(tmp, target));
    if result.is_err() {
        let _ = fs::remove_file(tmp);
    }
    result
}

fn tmp_path(path: &Path) -> PathBuf {
    let counter = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_else(|| "dedup".into());

    path.with_file_name(format!(
        ".{file_name}.dedup-tmp-{}-{counter}",
        process::id()
    ))
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn canonical_path(store_root: &Path, kind: Kind, bytes: &[u8]) -> PathBuf {
        store_root
            .join(DEDUP_DIR)
            .join(kind.directory())
            .join(hash_bytes(bytes))
    }

    fn set_mode(path: &Path, mode: u32) {
        let mut permissions =
            fs::metadata(path).expect("metadata").permissions();
        permissions.set_mode(mode);
        fs::set_permissions(path, permissions).expect("set permissions");
    }

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

    #[test]
    fn copy_fallback_creates_independent_target() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store_root = dir.path();
        let kind = Kind::Bytecode;
        let bytes = b"canonical bytecode";
        let canonical = canonical_path(store_root, kind, bytes);
        let target = store_root.join("target");
        let tmp = tmp_path(&target);

        fs::create_dir_all(canonical.parent().expect("canonical dir"))
            .expect("canonical dir");
        fs::write(&canonical, bytes).expect("canonical");

        replace_with_copy(&canonical, &target, &tmp)
            .expect("copy fallback should write target");

        assert_eq!(fs::read(&target).expect("read target"), bytes);
        assert_ne!(
            fs::metadata(&canonical).expect("canonical metadata").ino(),
            fs::metadata(&target).expect("target metadata").ino()
        );

        remove_unreferenced_hash(store_root, kind, &hash_bytes(bytes))
            .expect("evict unreferenced canonical");

        assert!(!canonical.exists());
        assert_eq!(fs::read(&target).expect("read copied target"), bytes);
    }

    #[test]
    fn write_ignores_old_canonical_eviction_failure() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store_root = dir.path();
        let target = store_root.join("target");
        let kind = Kind::Bytecode;
        let old_bytes = b"old bytecode";
        let new_bytes = b"new bytecode";

        write(store_root, kind, &target, old_bytes).expect("write old bytes");

        let canonical_dir = store_root.join(DEDUP_DIR).join(kind.directory());
        let old_canonical = canonical_path(store_root, kind, old_bytes);
        let new_canonical = canonical_path(store_root, kind, new_bytes);
        fs::write(&new_canonical, new_bytes).expect("write new canonical");
        set_mode(&canonical_dir, 0o500);

        write(store_root, kind, &target, new_bytes)
            .expect("write should ignore old canonical cleanup failure");

        set_mode(&canonical_dir, 0o700);
        assert_eq!(fs::read(&target).expect("read target"), new_bytes);
        assert!(old_canonical.exists());
    }

    #[test]
    fn remove_file_ignores_canonical_eviction_failure() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store_root = dir.path();
        let target = store_root.join("target");
        let kind = Kind::Bytecode;
        let bytes = b"bytecode";

        write(store_root, kind, &target, bytes).expect("write bytes");

        let canonical_dir = store_root.join(DEDUP_DIR).join(kind.directory());
        let canonical = canonical_path(store_root, kind, bytes);
        set_mode(&canonical_dir, 0o500);

        remove_file(store_root, kind, &target)
            .expect("remove should ignore canonical cleanup failure");

        set_mode(&canonical_dir, 0o700);
        assert!(!target.exists());
        assert!(canonical.exists());
    }
}
