// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct ArtifactVerificationStats {
    pub(super) hashed_files: u64,
    pub(super) hashed_bytes: u64,
    pub(super) reused_checks: u64,
}

impl ArtifactVerificationStats {
    pub(super) fn since(self, earlier: Self) -> Self {
        Self {
            hashed_files: self.hashed_files.saturating_sub(earlier.hashed_files),
            hashed_bytes: self.hashed_bytes.saturating_sub(earlier.hashed_bytes),
            reused_checks: self.reused_checks.saturating_sub(earlier.reused_checks),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct ArtifactFileIdentity {
    device: u64,
    inode: u64,
    size_bytes: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
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

#[derive(Default)]
struct ArtifactVerificationCache {
    state: Mutex<ArtifactVerificationState>,
}

static ARTIFACT_VERIFICATION_CACHE: OnceLock<ArtifactVerificationCache> = OnceLock::new();

fn process_artifact_verification_cache() -> &'static ArtifactVerificationCache {
    ARTIFACT_VERIFICATION_CACHE.get_or_init(ArtifactVerificationCache::default)
}

pub(super) fn artifact_verification_stats() -> anyhow::Result<ArtifactVerificationStats> {
    process_artifact_verification_cache().stats()
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
        let digest = entry.get_or_init(|| {
            hashed_here = true;
            hash_open_artifact_file(file, identity, path).map_err(|error| format!("{error:#}"))
        });
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("artifact verification cache lock poisoned"))?;
            if hashed_here {
                state.stats.hashed_files += 1;
                state.stats.hashed_bytes =
                    state.stats.hashed_bytes.saturating_add(identity.size_bytes);
            } else {
                state.stats.reused_checks += 1;
            }
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
    mut file: File,
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
}
