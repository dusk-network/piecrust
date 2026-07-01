// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use piecrust::{ContractId, contract_merkle_position};

fn main() -> ExitCode {
    match run() {
        Ok(collisions) if collisions == 0 => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<usize, String> {
    let args = env::args_os().skip(1).collect::<Vec<_>>();

    if args.is_empty() || args.len() % 2 != 0 {
        return Err(format!(
            "usage: cargo run -p piecrust --example check_contract_positions -- <label> <state_dir> [<label> <state_dir> ...]"
        ));
    }

    let mut total_collisions = 0;
    for pair in args.chunks_exact(2) {
        let label = pair[0].to_string_lossy();
        let state_dir = PathBuf::from(&pair[1]);
        total_collisions += check_state(&label, &state_dir)?;
    }

    Ok(total_collisions)
}

fn check_state(label: &str, state_dir: &Path) -> Result<usize, String> {
    let leaf_dir = state_dir.join("main").join("leaf");
    if !leaf_dir.is_dir() {
        return Err(format!(
            "{label}: missing latest-state leaf directory: {}",
            leaf_dir.display()
        ));
    }

    let mut checked = 0usize;
    let mut skipped = 0usize;
    let mut positions = BTreeMap::<u64, ContractId>::new();
    let mut collisions = Vec::<(u64, ContractId, ContractId)>::new();

    for entry in std::fs::read_dir(&leaf_dir).map_err(|err| {
        format!("{label}: failed to read {}: {err}", leaf_dir.display())
    })? {
        let entry = entry.map_err(|err| {
            format!("{label}: failed to read leaf directory entry: {err}")
        })?;
        let path = entry.path();
        if !path.is_dir() {
            skipped += 1;
            continue;
        }

        let Some(contract_id) = contract_id_from_dir_name(entry.file_name())?
        else {
            skipped += 1;
            continue;
        };

        if !path.join("element").is_file() {
            skipped += 1;
            continue;
        }

        checked += 1;
        let pos = contract_merkle_position(&contract_id);
        match positions.entry(pos) {
            Entry::Vacant(entry) => {
                entry.insert(contract_id);
            }
            Entry::Occupied(entry) => {
                let existing = *entry.get();
                if existing != contract_id {
                    collisions.push((pos, existing, contract_id));
                }
            }
        }
    }

    println!(
        "{label}: contracts={checked} unique_positions={} collisions={} skipped={skipped}",
        positions.len(),
        collisions.len(),
    );

    for (pos, first, second) in collisions.iter() {
        println!("{label}: COLLISION pos={pos}");
        println!("  first:  {first}");
        println!("  second: {second}");
    }

    Ok(collisions.len())
}

fn contract_id_from_dir_name(
    dir_name: impl AsRef<OsStr>,
) -> Result<Option<ContractId>, String> {
    let Some(dir_name) = dir_name.as_ref().to_str() else {
        return Ok(None);
    };

    if dir_name.len() != 64 || !dir_name.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Ok(None);
    }

    let bytes = hex::decode(dir_name)
        .map_err(|err| format!("invalid contract id {dir_name}: {err}"))?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| format!("invalid contract id length: {dir_name}"))?;

    Ok(Some(ContractId::from_bytes(bytes)))
}
