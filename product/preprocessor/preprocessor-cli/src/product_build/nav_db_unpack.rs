// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;

const UNPACK_RECIPE: &str = "nav-db-package-xz-v1";

/// The immutable ZIP already caches the final XZ pages. Copy those exact bytes,
/// not the raw node output: no compressor, second cache, or new GC roots needed.
pub(super) fn sync_nav_db_unpacked_zip(
    zip_path: &Path,
    unpacked_root: &Path,
    published_filename: &str,
    known_sha256: Option<&str>,
) -> anyhow::Result<(bool, PathBuf)> {
    let started = Instant::now();
    let verified = verified_artifact_file(zip_path)?;
    verify_expected_artifact(
        &verified,
        zip_path,
        known_sha256.unwrap_or(&verified.sha256),
        verified.size_bytes,
        "nav-db package",
    )?;
    let publish_dir = unpacked_root.parent().context("missing publication root")?;
    let unpack_dir = unpacked_target_dir(unpacked_root, published_filename)?;
    // Serialize retries of this same publication, without blocking other products.
    let _lock = acquire_named_publication_lock(
        artifact_root_from_publish_dir(publish_dir)?,
        &format!(
            "nav-unpack-{}",
            hash_text(&unpack_dir.display().to_string())
        ),
        |message| eprintln!("{message}"),
    )?;
    let marker = unpacked_marker_path(unpacked_root, published_filename)?;
    let marker_value = unpacked_marker_value(&verified.sha256, Some(UNPACK_RECIPE));
    let mut archive = ZipArchive::new(File::open(zip_path)?)?;
    let members = package_members(&mut archive)?;
    let hit = fs::read_to_string(&marker).ok().as_deref().map(str::trim)
        == Some(marker_value.as_str())
        && complete_tree(&unpack_dir, &members)?;
    if !hit {
        fs::create_dir_all(unpacked_root)?;
        let scratch = tempfile::Builder::new()
            .prefix(".nav-db-unpack-")
            .tempdir_in(unpacked_root)?;
        for (name, _) in &members {
            let bytes = read_member(&mut archive, name)?; // Includes ZIP CRC validation.
            if name.starts_with("page_") && !nav_kv_package::is_xz(&bytes) {
                bail!("nav-db package member {name} is not XZ encoded");
            }
            fs::write(scratch.path().join(name), bytes)?;
        }
        // tempfile directories start as 0700; the web server must be able to
        // traverse the published tree even when a different user built it.
        fs::set_permissions(scratch.path(), fs::Permissions::from_mode(0o755))?;
        // Failure while copying leaves the previous tree untouched. The completion
        // marker is removed before replacement and written only after all files exist.
        match fs::remove_file(&marker) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        if unpack_dir.exists() {
            fs::remove_dir_all(&unpack_dir)?;
        }
        fs::rename(scratch.path(), &unpack_dir)?;
        fs::write(&marker, format!("{marker_value}\n"))?;
    }
    eprintln!(
        "nav-db-unpack source=package-xz cache_hit={hit} pages={} elapsed_ms={}",
        members.len() - 2,
        started.elapsed().as_millis(),
    );
    Ok((hit, unpack_dir))
}

fn read_member(archive: &mut ZipArchive<File>, name: &str) -> anyhow::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    archive
        .by_name(name)?
        .read_to_end(&mut bytes)
        .with_context(|| format!("failed to read nav-db ZIP member {name}"))?;
    Ok(bytes)
}

fn package_members(archive: &mut ZipArchive<File>) -> anyhow::Result<Vec<(String, u64)>> {
    let manifest: serde_json::Value =
        serde_json::from_slice(&read_member(archive, "manifest.json")?)?;
    let page_count = manifest["page_count"]
        .as_u64()
        .context("nav-db package manifest missing page_count")?;
    if page_count.checked_add(2) != Some(archive.len() as u64) {
        bail!("nav-db package member count does not match page_count");
    }
    // Construct canonical member names; never extract an archive-supplied path.
    ["manifest.json".to_string(), "root".to_string()]
        .into_iter()
        .chain((0..page_count).map(|page| format!("page_{page:04}")))
        .map(|name| {
            let member = archive.by_name(&name)?;
            if !member.is_file() || member.compression() != zip::CompressionMethod::Stored {
                bail!("nav-db package member {name} must be a Stored ZIP file");
            }
            Ok((name, member.size()))
        })
        .collect()
}

fn complete_tree(root: &Path, members: &[(String, u64)]) -> anyhow::Result<bool> {
    if !root.is_dir() {
        return Ok(false);
    }
    if fs::read_dir(root)?
        .collect::<std::io::Result<Vec<_>>>()?
        .len()
        != members.len()
    {
        return Ok(false);
    }
    for (name, size) in members {
        match fs::symlink_metadata(root.join(name)) {
            Ok(metadata) if metadata.is_file() && metadata.len() == *size => {}
            Ok(_) => return Ok(false),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn publication(root: &Path, timestamp: &str) -> PathBuf {
        root.join("published/main").join(timestamp).join("unpacked")
    }

    fn package(path: &Path, page: &[u8]) {
        // A real package shape, deliberately using a different valid XZ encoder
        // from production. Exact byte equality proves we did not recompress it.
        fs::write(
            path,
            nav_kv_package::write_stored_xz_framed_package_bytes(
                br#"{"page_count":1}"#,
                b"raw-root",
                &[page.to_vec()],
            )
            .unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn nav_db_unpack_reuses_exact_xz_bytes_across_publications_and_repairs_missing_files() {
        let temp = tempfile::tempdir().unwrap();
        let zip = temp.path().join("nav.zip");
        package(&zip, b"page contents");
        let mut archive = ZipArchive::new(File::open(&zip).unwrap()).unwrap();
        let encoded = read_member(&mut archive, "page_0000").unwrap();
        for timestamp in ["first", "second"] {
            let root = publication(temp.path(), timestamp);
            let (hit, output) = sync_nav_db_unpacked_zip(&zip, &root, "nav.zip", None).unwrap();
            assert!(!hit);
            assert_eq!(fs::metadata(&output).unwrap().mode() & 0o777, 0o755);
            assert_eq!(fs::read(output.join("page_0000")).unwrap(), encoded);
            let reader = nav_kv_package::NavKvDirectoryReader::new(&output, "test");
            assert_eq!(reader.read_root().unwrap(), b"raw-root");
            assert_eq!(reader.read_page(0).unwrap(), b"page contents");
            let before = fs::metadata(output.join("page_0000")).unwrap().ino();
            assert!(
                sync_nav_db_unpacked_zip(&zip, &root, "nav.zip", None)
                    .unwrap()
                    .0
            );
            assert_eq!(
                fs::metadata(output.join("page_0000")).unwrap().ino(),
                before
            );
            fs::remove_file(output.join("page_0000")).unwrap();
            assert!(
                !sync_nav_db_unpacked_zip(&zip, &root, "nav.zip", None)
                    .unwrap()
                    .0
            );
            assert_eq!(reader.read_page(0).unwrap(), b"page contents");
        }
        // Each publication owns its files; losing the ZIP does not break serving.
        fs::remove_file(zip).unwrap();
        let output = publication(temp.path(), "second").join("nav");
        assert_eq!(fs::read(output.join("page_0000")).unwrap(), encoded);
    }

    #[test]
    fn nav_db_unpack_retries_interruption_and_invalidates_changed_package() {
        let temp = tempfile::tempdir().unwrap();
        let zip = temp.path().join("nav.zip");
        let root = publication(temp.path(), "first");
        package(&zip, b"old page");
        let output = root.join("nav");
        fs::create_dir_all(&output).unwrap();
        fs::write(output.join("root"), b"interrupted partial output").unwrap();
        let (hit, _) = sync_nav_db_unpacked_zip(&zip, &root, "nav.zip", None).unwrap();
        assert!(!hit);
        let changed = temp.path().join("changed.zip");
        package(&changed, b"new page");
        assert!(
            !sync_nav_db_unpacked_zip(&changed, &root, "nav.zip", None)
                .unwrap()
                .0
        );
        let reader = nav_kv_package::NavKvDirectoryReader::new(output, "test");
        assert_eq!(reader.read_page(0).unwrap(), b"new page");
    }

    #[test]
    fn nav_db_unpack_rejects_checksum_and_crc_errors_without_replacing_good_output() {
        let temp = tempfile::tempdir().unwrap();
        let zip = temp.path().join("nav.zip");
        let root = publication(temp.path(), "first");
        package(&zip, b"good page");
        let (_, output) = sync_nav_db_unpacked_zip(&zip, &root, "nav.zip", None).unwrap();
        let marker = unpacked_marker_path(&root, "nav.zip").unwrap();
        let marker_before = fs::read(&marker).unwrap();
        let mut archive = ZipArchive::new(File::open(&zip).unwrap()).unwrap();
        let offset = archive.by_name("page_0000").unwrap().data_start() as usize;
        let mut bytes = fs::read(&zip).unwrap();
        bytes[offset + 8] ^= 1;
        let damaged = temp.path().join("damaged.zip");
        fs::write(&damaged, bytes).unwrap();
        for expected in [Some("0".repeat(64)), None] {
            assert!(
                sync_nav_db_unpacked_zip(&damaged, &root, "nav.zip", expected.as_deref()).is_err()
            );
            assert_eq!(fs::read(&marker).unwrap(), marker_before);
            let reader = nav_kv_package::NavKvDirectoryReader::new(&output, "test");
            assert_eq!(reader.read_page(0).unwrap(), b"good page");
            assert_eq!(fs::read_dir(&root).unwrap().count(), 1); // scratch cleaned up
        }
    }

    #[test]
    fn nav_db_unpack_rejects_raw_pages_and_unexpected_member_paths() {
        for name in ["page_0000", "../escape", "wrong-page"] {
            let temp = tempfile::tempdir().unwrap();
            let zip = temp.path().join("nav.zip");
            let mut writer = zip::ZipWriter::new(File::create(&zip).unwrap());
            for (member, bytes) in [
                ("manifest.json", br#"{"page_count":1}"#.as_slice()),
                ("root", b"root".as_slice()),
                (name, b"raw page".as_slice()),
            ] {
                writer
                    .start_file(
                        member,
                        zip::write::SimpleFileOptions::default()
                            .compression_method(zip::CompressionMethod::Stored),
                    )
                    .unwrap();
                writer.write_all(bytes).unwrap();
            }
            writer.finish().unwrap();
            let root = publication(temp.path(), "first");
            assert!(sync_nav_db_unpacked_zip(&zip, &root, "nav.zip", None).is_err());
            assert!(!root.join("nav").exists());
            assert!(!root.join("escape").exists());
            assert!(!unpacked_marker_path(&root, "nav.zip").unwrap().exists());
        }
    }

    #[test]
    fn nav_db_unpack_serializes_same_publication_writers() {
        let temp = tempfile::tempdir().unwrap();
        let zip = temp.path().join("nav.zip");
        let root = publication(temp.path(), "first");
        package(&zip, b"page contents");
        let barrier = Arc::new(std::sync::Barrier::new(4));
        let workers = (0..4)
            .map(|_| {
                let (zip, root, barrier) = (zip.clone(), root.clone(), barrier.clone());
                thread::spawn(move || {
                    barrier.wait();
                    sync_nav_db_unpacked_zip(&zip, &root, "nav.zip", None)
                        .unwrap()
                        .0
                })
            })
            .collect::<Vec<_>>();
        let hits = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(hits.iter().filter(|hit| !**hit).count(), 1);
        assert_eq!(hits.iter().filter(|hit| **hit).count(), 3);
    }

    #[test]
    #[ignore = "manual timing with AEROBAG_NAV_DB_UNPACK_BENCHMARK_ZIP; only writes a temporary publication"]
    fn nav_db_unpack_benchmark_existing_package() {
        let source = env::var_os("AEROBAG_NAV_DB_UNPACK_BENCHMARK_ZIP").expect("set benchmark ZIP");
        let temp = tempfile::tempdir().unwrap();
        let zip = temp.path().join("nav.zip");
        fs::copy(source, &zip).unwrap();
        verified_artifact_file(&zip).unwrap(); // As in production, verified before finalization.
        for timestamp in ["first", "second"] {
            let root = publication(temp.path(), timestamp);
            let started = Instant::now();
            let (_, output) = sync_nav_db_unpacked_zip(&zip, &root, "nav.zip", None).unwrap();
            eprintln!(
                "fresh_publication={timestamp} elapsed_ms={} files={}",
                started.elapsed().as_millis(),
                fs::read_dir(output).unwrap().count()
            );
        }
    }
}
