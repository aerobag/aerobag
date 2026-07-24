// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::{bail, Context};
use preprocessor_core::nav_kv::{NavKvPrefixStats, NavKvRoot};
use std::{
    collections::BTreeSet,
    env, fs,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};
use zip::ZipArchive;

fn main() -> anyhow::Result<()> {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|arg| arg == "--prefix-size") {
        args.remove(0);
        if args.len() != 2 {
            bail!("usage: had-query --prefix-size <had-dir-or-zip> <prefix>");
        }
        let source = PathBuf::from(args.remove(0));
        let prefix = args.remove(0);
        let (root, stats) = if source.is_dir() {
            prefix_size_had_dir(&source, &prefix)?
        } else {
            prefix_size_had_zip(&source, &prefix)?
        };
        print_prefix_stats(&root, &prefix, &stats)?;
        return Ok(());
    }
    if args.len() != 2 {
        bail!("usage: had-query <had-dir-or-zip> <key>\n       had-query --prefix-size <had-dir-or-zip> <prefix>");
    }
    let source = PathBuf::from(args.remove(0));
    let key = args.remove(0);
    let value = if source.is_dir() {
        query_had_dir(&source, &key)?
    } else {
        query_had_zip(&source, &key)?
    };
    let value = value.with_context(|| format!("missing HAD key {key}"))?;
    if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&value) {
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else if let Ok(text) = std::str::from_utf8(&value) {
        println!("{text}");
    } else {
        bail!("value for {key} is not UTF-8 or JSON");
    }
    Ok(())
}

fn print_prefix_stats(
    root: &NavKvRoot,
    prefix: &str,
    stats: &NavKvPrefixStats,
) -> anyhow::Result<()> {
    let storage_pages = stats
        .matching_leaf_pages
        .iter()
        .chain(stats.external_value_pages.iter())
        .copied()
        .collect::<BTreeSet<_>>();
    let storage_bytes = storage_pages.len() as u64 * u64::from(root.page_size());
    let value_pages = stats.external_value_pages.len();
    let payload_bytes = stats.key_bytes + stats.value_bytes;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "prefix": prefix,
            "key_count": stats.key_count,
            "key_bytes": stats.key_bytes,
            "value_bytes": stats.value_bytes,
            "key_value_bytes": payload_bytes,
            "key_value_kib": payload_bytes as f64 / 1024.0,
            "key_value_mib": payload_bytes as f64 / 1024.0 / 1024.0,
            "inline_value_count": stats.inline_value_count,
            "external_value_count": stats.external_value_count,
            "matching_leaf_pages": stats.matching_leaf_pages.len(),
            "external_value_pages": value_pages,
            "storage_pages_page_granularity": storage_pages.len(),
            "storage_bytes_page_granularity": storage_bytes,
            "storage_kib_page_granularity": storage_bytes as f64 / 1024.0,
            "storage_mib_page_granularity": storage_bytes as f64 / 1024.0 / 1024.0,
            "nav_db_page_size": root.page_size(),
            "nav_db_page_count": root.page_count(),
        }))?
    );
    Ok(())
}

fn prefix_size_had_dir(dir: &Path, prefix: &str) -> anyhow::Result<(NavKvRoot, NavKvPrefixStats)> {
    let root_bytes = fs::read(dir.join("root"))
        .with_context(|| format!("failed to read {}", dir.join("root").display()))?;
    let root = NavKvRoot::parse(&root_bytes).map_err(anyhow::Error::msg)?;
    let stats = root
        .prefix_stats(prefix, |page_index| read_dir_page(dir, page_index).ok())
        .with_context(|| format!("failed to scan HAD prefix {prefix}"))?;
    Ok((root, stats))
}

fn prefix_size_had_zip(path: &Path, prefix: &str) -> anyhow::Result<(NavKvRoot, NavKvPrefixStats)> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut archive =
        ZipArchive::new(file).with_context(|| format!("failed to read zip {}", path.display()))?;
    let root_bytes = read_zip_member(&mut archive, "root")?;
    let root = NavKvRoot::parse(&root_bytes).map_err(anyhow::Error::msg)?;
    let stats = root
        .prefix_stats(prefix, |page_index| {
            read_zip_member(&mut archive, &format!("page_{page_index:04}")).ok()
        })
        .with_context(|| format!("failed to scan HAD prefix {prefix}"))?;
    Ok((root, stats))
}

fn query_had_dir(dir: &Path, key: &str) -> anyhow::Result<Option<Vec<u8>>> {
    let root_bytes = fs::read(dir.join("root"))
        .with_context(|| format!("failed to read {}", dir.join("root").display()))?;
    let root = NavKvRoot::parse(&root_bytes).map_err(anyhow::Error::msg)?;
    Ok(root.extract_value(key, |page_index| read_dir_page(dir, page_index).ok()))
}

fn read_dir_page(dir: &Path, page_index: u32) -> anyhow::Result<Vec<u8>> {
    let path = dir.join(format!("page_{page_index:04}"));
    let bytes = fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
    decode_xz_if_needed(&bytes).with_context(|| format!("failed to decode {}", path.display()))
}

fn decode_xz_if_needed(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    nav_kv_package::decode_xz_if_needed(bytes)
        .map(|bytes| bytes.into_owned())
        .map_err(anyhow::Error::msg)
}

fn query_had_zip(path: &Path, key: &str) -> anyhow::Result<Option<Vec<u8>>> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut archive =
        ZipArchive::new(file).with_context(|| format!("failed to read zip {}", path.display()))?;
    let root_bytes = read_zip_member(&mut archive, "root")?;
    let root = NavKvRoot::parse(&root_bytes).map_err(anyhow::Error::msg)?;
    Ok(root.extract_value(key, |page_index| {
        read_zip_member(&mut archive, &format!("page_{page_index:04}")).ok()
    }))
}

fn read_zip_member(archive: &mut ZipArchive<File>, name: &str) -> anyhow::Result<Vec<u8>> {
    let mut file = archive
        .by_name(name)
        .with_context(|| format!("missing zip member {name}"))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .with_context(|| format!("failed to read zip member {name}"))?;
    if name.starts_with("page_") {
        decode_xz_if_needed(&bytes).with_context(|| format!("failed to decode zip member {name}"))
    } else {
        Ok(bytes)
    }
}
