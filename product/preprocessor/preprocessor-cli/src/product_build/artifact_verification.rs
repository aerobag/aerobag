// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;
use std::cell::Cell;

// Attached to the inode: hardlink publication preserves the receipt, while GC
// removes it with the last artifact link. No separate receipt store can leak.
#[cfg(target_os = "linux")]
const RECEIPT_ATTRIBUTE: &std::ffi::CStr = c"user.aerobag.verified-sha256-v1";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct ArtifactVerificationStats {
    pub(super) hashed_files: u64,
    pub(super) hashed_bytes: u64,
    pub(super) reused_checks: u64,
    pub(super) persisted_checks: u64,
    pub(super) receipt_write_failures: u64,
}

impl ArtifactVerificationStats {
    pub(super) fn since(self, earlier: Self) -> Self {
        Self {
            hashed_files: self.hashed_files.saturating_sub(earlier.hashed_files),
            hashed_bytes: self.hashed_bytes.saturating_sub(earlier.hashed_bytes),
            reused_checks: self.reused_checks.saturating_sub(earlier.reused_checks),
            persisted_checks: self
                .persisted_checks
                .saturating_sub(earlier.persisted_checks),
            receipt_write_failures: self
                .receipt_write_failures
                .saturating_sub(earlier.receipt_write_failures),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
struct ArtifactFileIdentity {
    device: u64,
    inode: u64,
    size_bytes: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
}

#[derive(Serialize, Deserialize)]
struct VerificationReceipt {
    schema_version: u32,
    identity: ArtifactFileIdentity,
    sha256: String,
}

fn read_receipt(file: &File, identity: ArtifactFileIdentity) -> Option<String> {
    let bytes = read_receipt_attribute(file)?;
    let receipt: VerificationReceipt = serde_json::from_slice(&bytes).ok()?;
    (receipt.schema_version == 1
        && receipt.identity == identity
        && receipt.sha256.len() == 64
        && receipt.sha256.bytes().all(|b| b.is_ascii_hexdigit()))
    .then_some(receipt.sha256)
}

#[cfg(target_os = "linux")]
fn read_receipt_attribute(file: &File) -> Option<Vec<u8>> {
    use std::os::fd::AsRawFd;
    let mut bytes = vec![0_u8; 1024];
    // Fixed bound: oversized, unsupported, inaccessible and absent attributes
    // are all cache misses, never an excuse to skip content verification.
    let size = unsafe {
        libc::fgetxattr(
            file.as_raw_fd(),
            RECEIPT_ATTRIBUTE.as_ptr(),
            bytes.as_mut_ptr().cast(),
            bytes.len(),
        )
    };
    if size <= 0 {
        return None;
    }
    bytes.truncate(size as usize);
    Some(bytes)
}

#[cfg(not(target_os = "linux"))]
fn read_receipt_attribute(_: &File) -> Option<Vec<u8>> {
    None
}

#[cfg(target_os = "linux")]
fn write_receipt_attribute(file: &File, bytes: &[u8]) -> bool {
    use std::os::fd::AsRawFd;
    unsafe {
        libc::fsetxattr(
            file.as_raw_fd(),
            RECEIPT_ATTRIBUTE.as_ptr(),
            bytes.as_ptr().cast(),
            bytes.len(),
            0,
        ) == 0
    }
}

#[cfg(not(target_os = "linux"))]
fn write_receipt_attribute(_: &File, _: &[u8]) -> bool {
    false
}

impl ArtifactFileIdentity {
    fn from_metadata(metadata: &fs::Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            size_bytes: metadata.len(),
            modified_seconds: metadata.mtime(),
            modified_nanoseconds: metadata.mtime_nsec(),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct VerifiedArtifactFile {
    pub(super) sha256: String,
    pub(super) size_bytes: u64,
}

type CachedArtifactDigest = Result<String, String>;

#[derive(Default)]
struct ArtifactVerificationState {
    entries: BTreeMap<ArtifactFileIdentity, Arc<OnceLock<CachedArtifactDigest>>>,
    stats: ArtifactVerificationStats,
}

struct ArtifactVerificationCache {
    state: Mutex<ArtifactVerificationState>,
    reuse_receipts: bool,
}

impl Default for ArtifactVerificationCache {
    fn default() -> Self {
        Self {
            state: Mutex::default(),
            // An explicit audit ignores persistent evidence, but still shares
            // freshly hashed immutable identities within this process.
            reuse_receipts: env::var_os("AEROBAG_REHASH_ARTIFACTS").is_none_or(|v| v != "1"),
        }
    }
}

thread_local! {
    static THREAD_STATS: Cell<ArtifactVerificationStats> = Cell::new(ArtifactVerificationStats::default());
}

pub(super) fn thread_artifact_verification_stats() -> ArtifactVerificationStats {
    THREAD_STATS.get()
}

static ARTIFACT_VERIFICATION_CACHE: OnceLock<ArtifactVerificationCache> = OnceLock::new();

fn process_artifact_verification_cache() -> &'static ArtifactVerificationCache {
    ARTIFACT_VERIFICATION_CACHE.get_or_init(ArtifactVerificationCache::default)
}

pub(super) fn artifact_verification_stats() -> anyhow::Result<ArtifactVerificationStats> {
    process_artifact_verification_cache().stats()
}

#[cfg(test)]
pub(super) fn artifact_verification_is_cached(path: &Path) -> anyhow::Result<bool> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("failed to stat artifact {}", path.display()))?;
    let identity = ArtifactFileIdentity::from_metadata(&metadata);
    Ok(process_artifact_verification_cache()
        .state
        .lock()
        .map_err(|_| anyhow::anyhow!("artifact verification cache lock poisoned"))?
        .entries
        .contains_key(&identity))
}

impl ArtifactVerificationCache {
    fn stats(&self) -> anyhow::Result<ArtifactVerificationStats> {
        Ok(self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("artifact verification cache lock poisoned"))?
            .stats)
    }

    fn verify_file(&self, path: &Path) -> anyhow::Result<VerifiedArtifactFile> {
        let file = File::open(path)
            .with_context(|| format!("failed to open artifact {}", path.display()))?;
        let metadata = file
            .metadata()
            .with_context(|| format!("failed to stat artifact {}", path.display()))?;
        if !metadata.is_file() {
            bail!(
                "expected artifact file, found non-file at {}",
                path.display()
            );
        }
        let identity = ArtifactFileIdentity::from_metadata(&metadata);
        let entry = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("artifact verification cache lock poisoned"))?;
            state
                .entries
                .entry(identity)
                .or_insert_with(|| Arc::new(OnceLock::new()))
                .clone()
        };

        let mut hashed_here = false;
        let mut persisted_here = false;
        let mut receipt_write_failed = false;
        let digest = entry.get_or_init(|| {
            if self.reuse_receipts {
                if let Some(sha256) = read_receipt(&file, identity) {
                    persisted_here = true;
                    return Ok(sha256);
                }
            }
            hashed_here = true;
            let result = hash_open_artifact_file(&file, identity, path);
            if let Ok(sha256) = &result {
                let receipt = VerificationReceipt {
                    schema_version: 1,
                    identity,
                    sha256: sha256.clone(),
                };
                receipt_write_failed = !write_receipt_attribute(
                    &file,
                    &serde_json::to_vec(&receipt).expect("receipt JSON"),
                );
            }
            result.map_err(|error| format!("{error:#}"))
        });
        // Recheck even on a hit: never use evidence for a different open-file
        // identity if a writer changed it while the receipt was being read.
        if ArtifactFileIdentity::from_metadata(&file.metadata()?) != identity {
            bail!("artifact changed while verifying {}", path.display());
        }
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("artifact verification cache lock poisoned"))?;
            let update = |stats: &mut ArtifactVerificationStats| {
                stats.hashed_files += u64::from(hashed_here);
                stats.hashed_bytes += if hashed_here { identity.size_bytes } else { 0 };
                stats.persisted_checks += u64::from(persisted_here);
                stats.reused_checks += u64::from(!hashed_here && !persisted_here);
                stats.receipt_write_failures += u64::from(receipt_write_failed);
            };
            update(&mut state.stats);
            THREAD_STATS.with(|stats| {
                let mut value = stats.get();
                update(&mut value);
                stats.set(value);
            });
        }
        let sha256 = digest
            .as_ref()
            .map_err(|error| anyhow::anyhow!(error.clone()))?
            .clone();
        Ok(VerifiedArtifactFile {
            sha256,
            size_bytes: identity.size_bytes,
        })
    }
}

fn hash_open_artifact_file(
    mut file: &File,
    identity: ArtifactFileIdentity,
    path: &Path,
) -> anyhow::Result<String> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .with_context(|| format!("failed to read artifact {}", path.display()))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    let after = file
        .metadata()
        .with_context(|| format!("failed to restat artifact {}", path.display()))?;
    if ArtifactFileIdentity::from_metadata(&after) != identity {
        bail!("artifact changed while hashing {}", path.display());
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub(super) fn verified_artifact_file(path: &Path) -> anyhow::Result<VerifiedArtifactFile> {
    process_artifact_verification_cache().verify_file(path)
}

pub(super) fn verify_expected_artifact(
    verified: &VerifiedArtifactFile,
    path: &Path,
    expected_sha256: &str,
    expected_size_bytes: u64,
    label: &str,
) -> anyhow::Result<()> {
    if verified.size_bytes != expected_size_bytes {
        bail!(
            "{label} size mismatch for {}: declared {} != actual {}",
            path.display(),
            expected_size_bytes,
            verified.size_bytes
        );
    }
    if verified.sha256 != expected_sha256 {
        bail!(
            "{label} checksum mismatch for {}: declared {} != actual {}",
            path.display(),
            expected_sha256,
            verified.sha256
        );
    }
    Ok(())
}

pub(super) fn verify_artifact_file(
    path: &Path,
    expected_sha256: &str,
    expected_size_bytes: u64,
    label: &str,
) -> anyhow::Result<()> {
    let verified = verified_artifact_file(path)?;
    verify_expected_artifact(&verified, path, expected_sha256, expected_size_bytes, label)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Barrier;
    use tempfile::tempdir;

    fn digest(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }

    #[test]
    fn hard_links_share_one_digest_verification() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("source.zip");
        let linked = temp.path().join("linked.zip");
        let bytes = b"same immutable artifact";
        fs::write(&source, bytes).unwrap();
        let cache = ArtifactVerificationCache::default();

        let source_verified = cache.verify_file(&source).unwrap();
        fs::hard_link(&source, &linked).unwrap();
        let linked_verified = cache.verify_file(&linked).unwrap();

        assert_eq!(source_verified.sha256, digest(bytes));
        assert_eq!(linked_verified.sha256, source_verified.sha256);
        assert_eq!(
            cache.stats().unwrap(),
            ArtifactVerificationStats {
                hashed_files: 1,
                hashed_bytes: bytes.len() as u64,
                reused_checks: 1,
                ..Default::default()
            }
        );
    }

    #[test]
    fn concurrent_checks_hash_one_identity_once() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("artifact.zip");
        let bytes = vec![0x5a; 2 * 1024 * 1024];
        fs::write(&path, &bytes).unwrap();
        let cache = Arc::new(ArtifactVerificationCache::default());
        let barrier = Arc::new(Barrier::new(8));
        let workers = (0..8)
            .map(|_| {
                let cache = Arc::clone(&cache);
                let barrier = Arc::clone(&barrier);
                let path = path.clone();
                thread::spawn(move || {
                    barrier.wait();
                    cache.verify_file(&path).unwrap().sha256
                })
            })
            .collect::<Vec<_>>();

        for worker in workers {
            assert_eq!(worker.join().unwrap(), digest(&bytes));
        }
        let stats = cache.stats().unwrap();
        assert_eq!(stats.hashed_files, 1);
        assert_eq!(stats.hashed_bytes, bytes.len() as u64);
        assert_eq!(stats.reused_checks, 7);
    }

    #[test]
    fn changed_file_identity_is_rehashed() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("artifact.zip");
        let original = b"original bytes";
        let changed = b"modified bytes";
        assert_eq!(original.len(), changed.len());
        fs::write(&path, original).unwrap();
        let cache = ArtifactVerificationCache::default();
        let first = cache.verify_file(&path).unwrap();

        fs::write(&path, changed).unwrap();
        let second = cache.verify_file(&path).unwrap();

        assert_eq!(first.sha256, digest(original));
        assert_eq!(second.sha256, digest(changed));
        assert_ne!(first.sha256, second.sha256);
        assert_eq!(cache.stats().unwrap().hashed_files, 2);
    }

    #[test]
    fn cached_digest_is_compared_with_every_declaration() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("artifact.zip");
        let bytes = b"declared artifact";
        fs::write(&path, bytes).unwrap();
        let cache = ArtifactVerificationCache::default();

        let verified = cache.verify_file(&path).unwrap();
        verify_expected_artifact(
            &verified,
            &path,
            &digest(bytes),
            bytes.len() as u64,
            "test artifact",
        )
        .unwrap();
        let reused = cache.verify_file(&path).unwrap();
        let error = verify_expected_artifact(
            &reused,
            &path,
            &"0".repeat(64),
            bytes.len() as u64,
            "test artifact",
        )
        .unwrap_err();

        assert!(error.to_string().contains("checksum mismatch"));
        assert_eq!(cache.stats().unwrap().hashed_files, 1);
        assert_eq!(cache.stats().unwrap().reused_checks, 1);
    }

    fn fresh_cache() -> ArtifactVerificationCache {
        ArtifactVerificationCache {
            state: Mutex::default(),
            reuse_receipts: true,
        }
    }

    // Invoked in a fresh OS process below: a process-local memoization cannot
    // accidentally satisfy the cross-build regression test.
    #[test]
    fn receipt_child_process() {
        let Some(path) = env::var_os("AEROBAG_RECEIPT_TEST_FILE") else {
            return;
        };
        let cache = ArtifactVerificationCache::default();
        let verified = cache.verify_file(Path::new(&path)).unwrap();
        assert_eq!(verified.sha256, digest(b"immutable artifact"));
        let expected: u64 = env::var("AEROBAG_RECEIPT_TEST_HASHES")
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(cache.stats().unwrap().hashed_files, expected);
        assert_eq!(cache.stats().unwrap().persisted_checks, 1 - expected);
        assert_eq!(cache.stats().unwrap().receipt_write_failures, 0);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn receipt_survives_process_restart_hardlinks_and_explicit_audit() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("source.zip");
        let linked = temp.path().join("published.zip");
        fs::write(&source, b"immutable artifact").unwrap();
        let child = |path: &Path, hashes: u64, audit: bool| {
            let result = Command::new(env::current_exe().unwrap())
                .args([
                    "--exact",
                    "product_build::artifact_verification::tests::receipt_child_process",
                    "--nocapture",
                ])
                .env("AEROBAG_RECEIPT_TEST_FILE", path)
                .env("AEROBAG_RECEIPT_TEST_HASHES", hashes.to_string())
                .env("AEROBAG_REHASH_ARTIFACTS", if audit { "1" } else { "0" })
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
            assert!(String::from_utf8_lossy(&result.stdout).contains("1 passed"));
        };
        child(&source, 1, false);
        fs::hard_link(&source, &linked).unwrap();
        fs::remove_file(&source).unwrap();
        child(&linked, 0, false);
        child(&linked, 1, true);
        child(&linked, 0, false);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn persisted_evidence_rejects_changed_bytes_replacement_and_wrong_declaration() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("artifact.zip");
        fs::write(&path, b"original").unwrap();
        let original = fresh_cache().verify_file(&path).unwrap();
        let warm = fresh_cache();
        let verified = warm.verify_file(&path).unwrap();
        assert_eq!(warm.stats().unwrap().persisted_checks, 1);
        assert!(verify_expected_artifact(&verified, &path, &"0".repeat(64), 8, "test").is_err());

        let file = File::open(&path).unwrap();
        let copied_receipt = read_receipt_attribute(&file).unwrap();
        fs::write(&path, b"modified").unwrap();
        // Deterministically alter mtime without a sleep or timestamp granularity assumption.
        file.set_times(
            fs::FileTimes::new().set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(123)),
        )
        .unwrap();
        let changed = fresh_cache();
        let verified = changed.verify_file(&path).unwrap();
        assert_eq!(changed.stats().unwrap().hashed_files, 1);
        assert!(verify_expected_artifact(&verified, &path, &original.sha256, 8, "test").is_err());

        let replacement = temp.path().join("replacement.zip");
        fs::write(&replacement, b"original").unwrap();
        // Even if a copy operation preserves xattrs, the new inode isn't the
        // one that was verified. It must get its own first content check.
        assert!(write_receipt_attribute(
            &File::open(&replacement).unwrap(),
            &copied_receipt
        ));
        fs::rename(&replacement, &path).unwrap();
        let replaced = fresh_cache();
        assert_eq!(replaced.verify_file(&path).unwrap().sha256, original.sha256);
        assert_eq!(replaced.stats().unwrap().hashed_files, 1);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn invalid_receipts_fall_back_to_hashing_and_audit_bypasses_valid_receipts() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("artifact.zip");
        fs::write(&path, b"payload").unwrap();
        let file = File::open(&path).unwrap();
        for bytes in [b"incomplete JSON".to_vec(), vec![b'x'; 2048]] {
            assert!(write_receipt_attribute(&file, &bytes));
            let cache = fresh_cache();
            assert_eq!(cache.verify_file(&path).unwrap().sha256, digest(b"payload"));
            assert_eq!(cache.stats().unwrap().hashed_files, 1);
        }
        let forged = VerificationReceipt {
            schema_version: 1,
            identity: ArtifactFileIdentity::from_metadata(&file.metadata().unwrap()),
            sha256: "0".repeat(64),
        };
        assert!(write_receipt_attribute(
            &file,
            &serde_json::to_vec(&forged).unwrap()
        ));
        let audit = ArtifactVerificationCache {
            reuse_receipts: false,
            ..fresh_cache()
        };
        assert_eq!(audit.verify_file(&path).unwrap().sha256, digest(b"payload"));
        assert_eq!(audit.stats().unwrap().hashed_files, 1);
        let warm = fresh_cache();
        assert_eq!(warm.verify_file(&path).unwrap().sha256, digest(b"payload"));
        assert_eq!(warm.stats().unwrap().persisted_checks, 1);
    }
}
