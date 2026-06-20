use std::borrow::Cow;
use std::io::{Cursor, Read, Write};

use zip::{write::SimpleFileOptions, CompressionMethod, DateTime as ZipDateTime};

const XZ_MAGIC: &[u8] = b"\xfd7zXZ\0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavKvPackageMembers {
    pub manifest: Vec<u8>,
    pub root: Vec<u8>,
    pub pages: Vec<Vec<u8>>,
}

pub fn is_xz(bytes: &[u8]) -> bool {
    bytes.starts_with(XZ_MAGIC)
}

// lzma-rs writes valid xz framing with uncompressed LZMA2 chunks. Use this for
// client-side rebuilt cache artifacts where avoiding a WASM encoder matters more
// than storage size. Producers should pass a real compressor through
// write_stored_xz_package_bytes_with_encoder.
pub fn xz_frame_uncompressed_bytes(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut input = Cursor::new(bytes);
    let mut output = Vec::new();
    lzma_rs::xz_compress(&mut input, &mut output)
        .map_err(|err| format!("failed to xz-frame nav-kv bytes: {err}"))?;
    Ok(output)
}

pub fn decode_xz_if_needed(bytes: &[u8]) -> Result<Cow<'_, [u8]>, String> {
    if !is_xz(bytes) {
        return Ok(Cow::Borrowed(bytes));
    }
    let mut input = Cursor::new(bytes);
    let mut output = Vec::new();
    lzma_rs::xz_decompress(&mut input, &mut output)
        .map_err(|err| format!("failed to xz-decode nav-kv page: {err}"))?;
    Ok(Cow::Owned(output))
}

pub fn write_stored_xz_framed_package_bytes(
    manifest: &[u8],
    root: &[u8],
    pages: &[Vec<u8>],
) -> Result<Vec<u8>, String> {
    write_stored_xz_package_bytes_with_encoder(manifest, root, pages, xz_frame_uncompressed_bytes)
}

// Writes the shared Stored-zip/nav-kv package shape. Producers should pass a
// real xz encoder; client-side cache rebuilds may pass xz_frame_uncompressed_bytes.
pub fn write_stored_xz_package_bytes_with_encoder(
    manifest: &[u8],
    root: &[u8],
    pages: &[Vec<u8>],
    encode_page: impl Fn(&[u8]) -> Result<Vec<u8>, String>,
) -> Result<Vec<u8>, String> {
    let cursor = Cursor::new(Vec::new());
    let mut writer = zip::ZipWriter::new(cursor);
    write_stored_member(&mut writer, "manifest.json", manifest)?;
    write_stored_member(&mut writer, "root", root)?;
    for (index, page) in pages.iter().enumerate() {
        let encoded = encode_page(page)?;
        write_stored_member(&mut writer, &format!("page_{index:04}"), &encoded)?;
    }
    writer
        .finish()
        .map(|cursor| cursor.into_inner())
        .map_err(|err| format!("failed to finish nav-kv package zip: {err}"))
}

pub fn read_package_bytes(product: &str, bytes: &[u8]) -> Result<NavKvPackageMembers, String> {
    let cursor = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|err| format!("failed to read {product} nav-kv package zip: {err}"))?;
    let manifest = read_zip_member(&mut archive, "manifest.json")?;
    let root = read_zip_member(&mut archive, "root")?;
    let manifest_value: serde_json::Value = serde_json::from_slice(&manifest)
        .map_err(|err| format!("failed to decode {product} nav-kv package manifest JSON: {err}"))?;
    let page_count = manifest_value
        .get("page_count")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| format!("{product} nav-kv package manifest missing page_count"))?;
    let mut pages = Vec::new();
    for page in 0..page_count {
        let encoded = read_zip_member(&mut archive, &format!("page_{page:04}"))?;
        let decoded = decode_xz_if_needed(&encoded)?;
        pages.push(decoded.into_owned());
    }
    Ok(NavKvPackageMembers {
        manifest,
        root,
        pages,
    })
}

fn write_stored_member<W: Write + std::io::Seek>(
    writer: &mut zip::ZipWriter<W>,
    name: &str,
    bytes: &[u8],
) -> Result<(), String> {
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .last_modified_time(ZipDateTime::default());
    writer
        .start_file(name, options)
        .map_err(|err| format!("failed to add {name} to nav-kv package zip: {err}"))?;
    writer
        .write_all(bytes)
        .map_err(|err| format!("failed to write {name} to nav-kv package zip: {err}"))
}

fn read_zip_member<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
) -> Result<Vec<u8>, String> {
    let mut member = archive
        .by_name(name)
        .map_err(|err| format!("nav-kv package zip missing {name}: {err}"))?;
    let mut bytes = Vec::new();
    member
        .read_to_end(&mut bytes)
        .map_err(|err| format!("failed to read nav-kv package zip member {name}: {err}"))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn package_round_trips_client_framed_xz_pages_in_stored_zip() {
        let manifest = br#"{"page_count":2}"#;
        let root = b"root";
        let pages = vec![b"page-one".to_vec(), b"page-two".to_vec()];
        let package = write_stored_xz_framed_package_bytes(manifest, root, &pages).unwrap();

        let mut archive = zip::ZipArchive::new(Cursor::new(&package)).unwrap();
        assert_eq!(
            archive.by_name("page_0000").unwrap().compression(),
            CompressionMethod::Stored
        );
        let mut encoded_page = Vec::new();
        archive
            .by_name("page_0000")
            .unwrap()
            .read_to_end(&mut encoded_page)
            .unwrap();
        assert!(is_xz(&encoded_page));

        let decoded = read_package_bytes("test", &package).unwrap();
        assert_eq!(decoded.manifest, manifest);
        assert_eq!(decoded.root, root);
        assert_eq!(decoded.pages, pages);
    }

    #[test]
    fn package_reader_accepts_system_xz_and_client_framed_xz_pages() {
        let manifest = br#"{"page_count":2}"#;
        let root = b"root";
        let pages = vec![b"ABCD".repeat(16 * 1024), b"EFGH".repeat(16 * 1024)];
        let system_package =
            write_stored_xz_package_bytes_with_encoder(manifest, root, &pages, system_xz).unwrap();
        let framed_package = write_stored_xz_framed_package_bytes(manifest, root, &pages).unwrap();

        let system_decoded = read_package_bytes("system", &system_package).unwrap();
        let framed_decoded = read_package_bytes("framed", &framed_package).unwrap();
        assert_eq!(system_decoded.pages, pages);
        assert_eq!(framed_decoded.pages, pages);

        let system_page = zip_member(&system_package, "page_0000");
        let framed_page = zip_member(&framed_package, "page_0000");
        assert!(is_xz(&system_page));
        assert!(is_xz(&framed_page));
        assert_ne!(
            system_page, framed_page,
            "test should exercise distinct xz encoders"
        );
        assert!(
            system_page.len() < pages[0].len() / 4,
            "system xz did not materially compress the fixture page: raw={} encoded={}",
            pages[0].len(),
            system_page.len()
        );
    }

    fn zip_member(package: &[u8], name: &str) -> Vec<u8> {
        let mut archive = zip::ZipArchive::new(Cursor::new(package)).unwrap();
        read_zip_member(&mut archive, name).unwrap()
    }

    fn system_xz(bytes: &[u8]) -> Result<Vec<u8>, String> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|err| err.to_string())?
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "aerobag-nav-kv-package-test-{}-{nonce}",
            std::process::id()
        ));
        std::fs::write(&path, bytes).map_err(|err| format!("write {}: {err}", path.display()))?;
        let output = Command::new("xz")
            .arg("--format=xz")
            .arg("--check=crc64")
            .arg("-6")
            .arg("--stdout")
            .arg("--threads=1")
            .arg(&path)
            .output()
            .map_err(|err| format!("run xz: {err}"));
        let _ = std::fs::remove_file(&path);
        let output = output?;
        if !output.status.success() {
            return Err(format!(
                "xz failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(output.stdout)
    }
}
