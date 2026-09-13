// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;
use preprocessor_live_feeds::engine::{
    sha256_hex, write_json_pretty_file, BuiltLiveFeedState, Clock, LiveFeedStatePayload,
};
use preprocessor_live_feeds::notam_store::is_incompatible_notam_catalog;
use product_contracts::{
    live_feed_compatibility_descriptor as live_feed_wire_inventory,
    LiveFeedCompatibilityDescriptor as LiveFeedWireInventory, NotamCatalogIdentity,
};
use serde::Deserialize;

const COMPATIBILITY_SCHEMA_VERSION: u32 = 1;
const PUBLICATION_PROVENANCE_SCHEMA_VERSION: u32 = 1;

pub(super) fn describe_compatibility(args: &[String]) -> anyhow::Result<Option<String>> {
    if !args
        .get(1)
        .is_some_and(|arg| arg == "--describe-compatibility")
    {
        return Ok(None);
    }
    if args.len() != 3 {
        bail!("--describe-compatibility requires exactly one product_artifacts.json path");
    }
    let publication = LoadedPublication::load(Path::new(&args[2]))?;
    Ok(Some(serde_json::to_string_pretty(
        &CompatibilityRequirements::from_loaded(&publication)?,
    )?))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StartupPublication {
    pub path: PathBuf,
    pub sha256: String,
}

pub(super) struct LoadedPublication {
    pub identity: StartupPublication,
    pub catalog: Arc<NotamAirportCatalog>,
}

impl LoadedPublication {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let path = fs::canonicalize(path)
            .with_context(|| format!("failed to resolve startup publication {}", path.display()))?;
        let bytes = fs::read(&path)
            .with_context(|| format!("failed to read startup publication {}", path.display()))?;
        let catalog = Arc::new(load_notam_airport_catalog_bytes(&path, &bytes)?);
        Ok(Self {
            identity: StartupPublication {
                path,
                sha256: sha256_hex(&bytes),
            },
            catalog,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CompatibilityRequirements {
    pub schema_version: u32,
    pub wire_contracts: LiveFeedWireInventory,
    pub notam_catalog: NotamCatalogIdentity,
    pub startup_publication: StartupPublication,
}

impl CompatibilityRequirements {
    pub fn from_loaded(publication: &LoadedPublication) -> anyhow::Result<Self> {
        Ok(Self {
            schema_version: COMPATIBILITY_SCHEMA_VERSION,
            wire_contracts: live_feed_wire_inventory(),
            notam_catalog: publication.catalog.identity().map_err(anyhow::Error::msg)?,
            startup_publication: publication.identity.clone(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DaemonCompatibilityDescriptor {
    pub schema_version: u32,
    pub executable_sha256: String,
    pub release_tag: Option<String>,
    pub launch_instance_id: Option<String>,
    pub process_instance_id: String,
    pub wire_contracts: LiveFeedWireInventory,
    pub configured_products: BTreeSet<String>,
    pub notam_catalog: Option<NotamCatalogIdentity>,
    pub startup_publication: Option<StartupPublication>,
    pub projection_ready: bool,
    pub published_state_id: Option<String>,
    pub ready: bool,
}

#[derive(Clone)]
pub(super) struct DaemonCompatibility(Arc<Mutex<DaemonCompatibilityDescriptor>>);

impl DaemonCompatibility {
    pub fn new(publication: Option<&LoadedPublication>) -> anyhow::Result<Self> {
        Self::with_identity(
            publication,
            sha256_hex(&fs::read("/proc/self/exe").context("failed to read running executable")?),
            env::var("AEROBAG_RELEASE_TAG")
                .ok()
                .filter(|v| !v.is_empty()),
            env::var("AEROBAG_RELEASE_LIVE_INSTANCE_ID")
                .ok()
                .filter(|v| !v.is_empty()),
            fs::read_to_string("/proc/sys/kernel/random/uuid")
                .context("failed to generate process-instance identity")?
                .trim()
                .to_string(),
        )
    }

    fn with_identity(
        publication: Option<&LoadedPublication>,
        executable_sha256: String,
        release_tag: Option<String>,
        launch_instance_id: Option<String>,
        process_instance_id: String,
    ) -> anyhow::Result<Self> {
        Ok(Self(Arc::new(Mutex::new(DaemonCompatibilityDescriptor {
            schema_version: COMPATIBILITY_SCHEMA_VERSION,
            executable_sha256,
            release_tag,
            launch_instance_id,
            process_instance_id,
            wire_contracts: live_feed_wire_inventory(),
            configured_products: BTreeSet::new(),
            notam_catalog: publication
                .map(|p| p.catalog.identity())
                .transpose()
                .map_err(anyhow::Error::msg)?,
            startup_publication: publication.map(|p| p.identity.clone()),
            projection_ready: false,
            published_state_id: None,
            ready: false,
        }))))
    }

    pub fn snapshot(&self) -> anyhow::Result<DaemonCompatibilityDescriptor> {
        Ok(self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("compatibility lock poisoned"))?
            .clone())
    }

    pub fn configure_products<'a>(
        &self,
        products: impl Iterator<Item = &'a str>,
    ) -> anyhow::Result<()> {
        self.0
            .lock()
            .map_err(|_| anyhow::anyhow!("compatibility lock poisoned"))?
            .configured_products = products.map(str::to_string).collect();
        Ok(())
    }

    fn published(&self, state_id: &str) -> anyhow::Result<()> {
        let mut descriptor = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("compatibility lock poisoned"))?;
        descriptor.projection_ready = true;
        descriptor.published_state_id = Some(state_id.to_string());
        descriptor.ready = descriptor.notam_catalog.is_some()
            && descriptor.startup_publication.is_some()
            && descriptor.configured_products.contains("notams")
            && descriptor.release_tag.is_some()
            && descriptor.launch_instance_id.is_some();
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PublishedNotamProvenance {
    schema_version: u32,
    wire_contracts: LiveFeedWireInventory,
    notam_catalog: NotamCatalogIdentity,
    state_id: String,
}

fn provenance_path(store: &NotamPersistentStore) -> PathBuf {
    store.root().join("published-provenance-v1.json")
}

fn published_provenance_matches(
    store: &NotamPersistentStore,
    live_root: &Path,
    catalog: &NotamCatalogIdentity,
) -> anyhow::Result<bool> {
    let bytes = match fs::read(provenance_path(store)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error).context("failed to read published NOTAM provenance"),
    };
    let Ok(provenance) = serde_json::from_slice::<PublishedNotamProvenance>(&bytes) else {
        return Ok(false);
    };
    Ok(
        provenance.schema_version == PUBLICATION_PROVENANCE_SCHEMA_VERSION
            && provenance.wire_contracts == live_feed_wire_inventory()
            && &provenance.notam_catalog == catalog
            && read_live_feed_catalog(live_root)?.is_some_and(|current| {
                current.products.get("notams").is_some_and(|entry| {
                    entry.current == provenance.state_id
                        && entry.state_sha256 == provenance.state_id
                })
            }),
    )
}

fn reset_notam_publication(live_root: &Path, store: &NotamPersistentStore) -> anyhow::Result<()> {
    remove_path_if_exists(&provenance_path(store))?;
    if let Some(mut current) = read_live_feed_catalog(live_root)? {
        current.products.remove("notams");
        write_live_feeds_current_manifest(live_root, &current)?;
    }
    // Old histories must not be rediscovered by the publisher under the new catalog.
    for directory in SIMULATION_PUBLICATION_DIRS {
        remove_path_if_exists(&live_root.join(directory).join("notams"))?;
    }
    Ok(())
}

pub(super) fn prepare_notam_startup(
    canonical_store: &NmsApiCollectorStore,
    publication_store: &NotamPersistentStore,
    live_root: &Path,
    catalog: &NotamCatalogIdentity,
    sender: &Sender<UpstreamEvent>,
) -> anyhow::Result<()> {
    let projection_matches = match publication_store.initialize() {
        Ok(()) => true,
        Err(error)
            if is_incompatible_notam_store_schema(&error)
                || is_incompatible_notam_catalog(&error) =>
        {
            false
        }
        Err(error) => return Err(error),
    };
    if !projection_matches || !published_provenance_matches(publication_store, live_root, catalog)?
    {
        reset_notam_publication(live_root, publication_store)?;
        canonical_store.initialize()?;
        if canonical_store.is_baseline_installed()? {
            let snapshot = canonical_store.canonical_source_snapshot()?;
            let observed_at_utc = canonical_store.poll_cursor()?.to_rfc3339();
            publication_store.rebuild_derived_projection(&snapshot.records, &observed_at_utc)?;
            publication_store.synchronize_canonical_source_snapshot(
                &snapshot.records,
                &observed_at_utc,
                &snapshot.cursor,
            )?;
        }
    }
    canonical_store.initialize()?;
    if canonical_store.is_baseline_installed()? {
        queue_nms_notam_state_event(
            canonical_store,
            publication_store,
            sender,
            &canonical_store.poll_cursor()?.to_rfc3339(),
        )?;
    }
    Ok(())
}

pub(super) struct ProvenancePublisher<C> {
    pub publisher: FileLiveFeedPublisher<C>,
    pub notam_store: Option<NotamPersistentStore>,
    pub compatibility: DaemonCompatibility,
}

impl<C: Clock> LiveFeedPublisher for ProvenancePublisher<C> {
    fn publish(&self, built: BuiltLiveFeedState) -> anyhow::Result<PublishedLiveFeedUpdate> {
        if built.product == "notams" {
            let store = self
                .notam_store
                .as_ref()
                .context("NOTAM publication has no loaded catalog")?;
            store.initialize()?;
            match &built.payload {
                LiveFeedStatePayload::NotamIncremental { state_root, .. }
                    if state_root == store.root() => {}
                _ => bail!("NOTAM publication is not from the catalog-bound projection"),
            }
        }
        self.publisher.publish(built)
    }

    fn acknowledge(&self, update: &PublishedLiveFeedUpdate) -> anyhow::Result<()> {
        self.publisher.acknowledge(update)?;
        if update.product == "notams" {
            let store = self
                .notam_store
                .as_ref()
                .context("NOTAM publication has no loaded catalog")?;
            store.initialize()?;
            let descriptor = self.compatibility.snapshot()?;
            let provenance = PublishedNotamProvenance {
                schema_version: PUBLICATION_PROVENANCE_SCHEMA_VERSION,
                wire_contracts: descriptor.wire_contracts,
                notam_catalog: descriptor
                    .notam_catalog
                    .context("missing loaded NOTAM catalog")?,
                state_id: update.version.clone(),
            };
            write_json_pretty_file(&provenance_path(store), &provenance)?;
            self.compatibility.published(&update.version)?;
        }
        Ok(())
    }

    fn maintain_after_acknowledgement(
        &self,
        update: &PublishedLiveFeedUpdate,
    ) -> anyhow::Result<()> {
        self.publisher.maintain_after_acknowledgement(update)
    }
}

pub(super) fn serve_compatibility_json(
    stream: &mut TcpStream,
    method: &str,
    compatibility: &DaemonCompatibility,
) -> anyhow::Result<()> {
    write_response(
        stream,
        method,
        "application/json",
        "no-cache, no-store",
        &serde_json::to_vec(&compatibility.snapshot()?)?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{request_once_with_compatibility, test_nms_record, write_test_publication};
    use preprocessor_live_feeds::engine::{read_json_value, LiveFeedTickResult};
    use tempfile::tempdir;

    struct TestInstance {
        compatibility: DaemonCompatibility,
        publisher: ProvenancePublisher<FixedClock>,
        task: Box<dyn DaemonLiveFeedTask + Send>,
        scratch: PathBuf,
    }

    impl TestInstance {
        fn start(
            root: &Path,
            publication: &LoadedPublication,
            instance: &str,
        ) -> anyhow::Result<Self> {
            let canonical = NmsApiCollectorStore::new(root.join("source"));
            let store = NotamPersistentStore::with_airport_catalog(
                root.join("projection"),
                Arc::clone(&publication.catalog),
            );
            let compatibility = DaemonCompatibility::with_identity(
                Some(publication),
                "a".repeat(64),
                Some("test-release".into()),
                Some(format!("launch-{instance}")),
                instance.into(),
            )?;
            compatibility.configure_products(["notams"].into_iter())?;
            let source = QueuedLiveFeedSource::new("notams");
            prepare_notam_startup(
                &canonical,
                &store,
                &root.join("live/v3"),
                &publication.catalog.identity().map_err(anyhow::Error::msg)?,
                &source.sender(),
            )?;
            let task = Box::new(ImmediateQueuedDaemonLiveFeedTask::new(
                LiveFeedSourceAndBuilder::new(source, NotamLiveFeedBuilder::new(store.root())),
                Duration::from_secs(60),
            ));
            let publisher = ProvenancePublisher {
                publisher: FileLiveFeedPublisher::new(
                    root.join("live/v3"),
                    FixedClock::new(parse_utc_timestamp(
                        "2026-07-24T00:00:00Z",
                        "test publication clock",
                    )?),
                ),
                notam_store: Some(store),
                compatibility: compatibility.clone(),
            };
            Ok(Self {
                compatibility,
                publisher,
                task,
                scratch: root.join("scratch"),
            })
        }

        fn tick(&mut self) -> LiveFeedTickResult {
            run_production_task_tick(
                parse_utc_timestamp("2026-07-24T00:00:00Z", "test tick").unwrap(),
                &mut self.task,
                &self.scratch,
                &self.publisher,
                &BroadcastSseBroker::default(),
                &DaemonStatus::default(),
            )
        }

        fn publish(&mut self) -> anyhow::Result<PublishedLiveFeedUpdate> {
            let result = self.tick();
            assert!(result.failures.is_empty(), "{:?}", result.failures);
            assert_eq!(
                result.published.len(),
                1,
                "startup must enqueue canonical state without upstream events"
            );
            assert!(self.compatibility.snapshot()?.ready);
            Ok(result.published.into_iter().next().unwrap())
        }
    }

    fn seed(root: &Path) -> anyhow::Result<NmsApiCollectorStore> {
        let store = NmsApiCollectorStore::new(root.join("source"));
        store.initialize()?;
        let first = test_nms_record("1000000000000001", "1", "AAA TEST")?;
        let mut second = test_nms_record("1000000000000002", "2", "BBB TEST")?;
        second.airport_id = Some("BBB".into());
        second.location = Some("BBB".into());
        store.install_baseline(
            "fixture",
            None,
            parse_utc_timestamp("2026-07-24T00:00:00Z", "test baseline")?,
            Path::new("/fixture/initial-load"),
            &[first, second],
        )?;
        Ok(store)
    }

    fn describe_http(
        root: &Path,
        compatibility: &DaemonCompatibility,
    ) -> anyhow::Result<DaemonCompatibilityDescriptor> {
        let response = request_once_with_compatibility(
            root,
            "GET /live-feeds/compatibility.json?fresh=1 HTTP/1.1\r\nHost: localhost\r\n\r\n",
            compatibility.clone(),
        )?;
        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"), "{response}");
        assert!(response.contains("Cache-Control: no-cache, no-store\r\n"));
        let (_, body) = response
            .split_once("\r\n\r\n")
            .context("missing HTTP body")?;
        Ok(serde_json::from_str(body)?)
    }

    #[test]
    fn loaded_catalog_a_remains_actual_after_manifest_and_navdb_become_b() -> anyhow::Result<()> {
        let temp = tempdir()?;
        seed(temp.path())?;
        let path = write_test_publication(temp.path(), &["AAA"])?;
        let a = LoadedPublication::load(&path)?;
        let mut instance = TestInstance::start(temp.path(), &a, "process-a")?;
        instance.publish()?;
        write_test_publication(temp.path(), &["BBB"])?;
        let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
        value["as_of_utc"] = "2026-07-25T00:00:00Z".into();
        fs::write(&path, serde_json::to_vec(&value)?)?;
        let b = LoadedPublication::load(&path)?;
        assert_ne!(a.identity.sha256, b.identity.sha256);
        assert_ne!(a.catalog.identity(), b.catalog.identity());
        let actual = describe_http(&temp.path().join("live"), &instance.compatibility)?;
        assert_eq!(actual.startup_publication, Some(a.identity.clone()));
        assert_eq!(
            actual.notam_catalog,
            Some(a.catalog.identity().map_err(anyhow::Error::msg)?)
        );
        assert!(actual.ready);
        assert!(product_contracts::require_live_feed_compatibility(
            &live_feed_wire_inventory(),
            &b.catalog.identity().map_err(anyhow::Error::msg)?,
            &actual.wire_contracts,
            actual.notam_catalog.as_ref().unwrap(),
        )
        .is_err());
        fs::remove_file(&path)?;
        assert_eq!(
            describe_http(&temp.path().join("live"), &instance.compatibility)?.startup_publication,
            Some(a.identity)
        );
        Ok(())
    }

    #[test]
    fn restart_b_rebuilds_and_publishes_b_from_canonical_without_upstream_events(
    ) -> anyhow::Result<()> {
        let temp = tempdir()?;
        let canonical = seed(temp.path())?;
        let fingerprint = canonical.current_fingerprint()?;
        let cursor = canonical.canonical_source_cursor()?;
        let path = write_test_publication(temp.path(), &["AAA"])?;
        let a = LoadedPublication::load(&path)?;
        let mut first = TestInstance::start(temp.path(), &a, "process-a")?;
        let old = first.publish()?;
        drop(first);
        write_test_publication(temp.path(), &["BBB"])?;
        let b = LoadedPublication::load(&path)?;
        let mut second = TestInstance::start(temp.path(), &b, "process-b")?;
        let before = describe_http(&temp.path().join("live"), &second.compatibility)?;
        assert_eq!(before.process_instance_id, "process-b");
        assert_eq!(
            before.notam_catalog,
            Some(b.catalog.identity().map_err(anyhow::Error::msg)?)
        );
        assert!(!before.ready);
        assert!(!before.projection_ready);
        assert!(before.published_state_id.is_none());
        assert!(!old.version_manifest_path.exists());
        assert!(!read_live_feed_catalog(&temp.path().join("live/v3"))?
            .unwrap()
            .products
            .contains_key("notams"));
        let new = second.publish()?;
        assert_ne!(old.version, new.version);
        let checkpoint = read_json_value(&new.state_path)?;
        assert_eq!(checkpoint["records"].as_array().unwrap().len(), 1);
        assert_eq!(checkpoint["records"][0]["airport_id"], "BBB");
        assert!(
            new.delta_path.is_none(),
            "catalog changes must begin with a full checkpoint"
        );
        assert_eq!(canonical.current_fingerprint()?, fingerprint);
        assert_eq!(canonical.canonical_source_cursor()?, cursor);
        Ok(())
    }

    #[test]
    fn matching_cached_provenance_becomes_ready_on_unchanged_publication() -> anyhow::Result<()> {
        let temp = tempdir()?;
        seed(temp.path())?;
        let path = write_test_publication(temp.path(), &["AAA"])?;
        let loaded = LoadedPublication::load(&path)?;
        let first = TestInstance::start(temp.path(), &loaded, "first")?.publish()?;
        let mut restarted = TestInstance::start(temp.path(), &loaded, "restarted")?;
        assert!(!restarted.compatibility.snapshot()?.ready);
        let next = restarted.publish()?;
        assert!(
            next.unchanged,
            "matching cached bytes should remain unchanged"
        );
        assert!(
            next.publication_ack.is_none(),
            "readiness cannot rely on a new journal acknowledgement"
        );
        assert_eq!(first.version, next.version);
        Ok(())
    }

    #[test]
    fn unknown_published_provenance_forces_full_republication() -> anyhow::Result<()> {
        let temp = tempdir()?;
        seed(temp.path())?;
        let path = write_test_publication(temp.path(), &["AAA"])?;
        let loaded = LoadedPublication::load(&path)?;
        let mut first = TestInstance::start(temp.path(), &loaded, "first")?;
        let old = first.publish()?;
        fs::remove_file(provenance_path(
            first.publisher.notam_store.as_ref().unwrap(),
        ))?;
        let mut restarted = TestInstance::start(temp.path(), &loaded, "restarted")?;
        assert!(!old.state_path.exists());
        assert!(!restarted.compatibility.snapshot()?.ready);
        let next = restarted.publish()?;
        assert!(!next.unchanged);
        assert!(next.delta_path.is_none());
        assert_eq!(
            first.compatibility.snapshot()?.published_state_id,
            Some(next.version)
        );
        Ok(())
    }

    #[test]
    fn missing_canonical_baseline_is_unready_without_upstream() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let path = write_test_publication(temp.path(), &["AAA"])?;
        let loaded = LoadedPublication::load(&path)?;
        let mut instance = TestInstance::start(temp.path(), &loaded, "empty")?;
        assert!(instance.tick().published.is_empty());
        let descriptor = describe_http(&temp.path().join("live"), &instance.compatibility)?;
        assert!(!descriptor.ready);
        assert!(!descriptor.projection_ready);
        assert!(descriptor.published_state_id.is_none());
        Ok(())
    }

    #[test]
    fn publication_failure_cannot_write_provenance_or_readiness() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let canonical = seed(temp.path())?;
        let before = canonical.current_fingerprint()?;
        let path = write_test_publication(temp.path(), &["AAA"])?;
        let loaded = LoadedPublication::load(&path)?;
        let mut instance = TestInstance::start(temp.path(), &loaded, "failure")?;
        fs::create_dir_all(temp.path().join("live/v3"))?;
        fs::write(
            temp.path().join("live/v3/states"),
            "blocked publication directory",
        )?;
        let result = instance.tick();
        assert!(!result.failures.is_empty());
        assert!(result.published.is_empty());
        assert!(!instance.compatibility.snapshot()?.ready);
        assert!(!provenance_path(instance.publisher.notam_store.as_ref().unwrap()).exists());
        assert_eq!(canonical.current_fingerprint()?, before);
        Ok(())
    }

    #[test]
    fn publication_provenance_failure_cannot_advertise_ready() -> anyhow::Result<()> {
        let temp = tempdir()?;
        seed(temp.path())?;
        let path = write_test_publication(temp.path(), &["AAA"])?;
        let loaded = LoadedPublication::load(&path)?;
        let mut instance = TestInstance::start(temp.path(), &loaded, "failure")?;
        fs::create_dir(provenance_path(
            instance.publisher.notam_store.as_ref().unwrap(),
        ))?;
        let result = instance.tick();
        assert!(!result.failures.is_empty());
        assert!(!instance.compatibility.snapshot()?.ready);
        assert!(!instance.compatibility.snapshot()?.projection_ready);
        Ok(())
    }

    #[test]
    fn runtime_and_offline_descriptors_export_identical_compiled_contracts() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let path = write_test_publication(temp.path(), &["AAA"])?;
        let loaded = LoadedPublication::load(&path)?;
        let requirements = CompatibilityRequirements::from_loaded(&loaded)?;
        let runtime = DaemonCompatibility::new(Some(&loaded))?.snapshot()?;
        assert_eq!(runtime.wire_contracts, requirements.wire_contracts);
        assert_eq!(runtime.wire_contracts, live_feed_wire_inventory());
        assert_eq!(
            runtime.notam_catalog.as_ref(),
            Some(&requirements.notam_catalog)
        );
        assert_eq!(
            runtime.startup_publication.as_ref(),
            Some(&requirements.startup_publication)
        );
        assert_eq!(runtime.executable_sha256.len(), 64);
        assert!(!runtime.process_instance_id.is_empty());
        assert_ne!(
            runtime.process_instance_id,
            DaemonCompatibility::new(None)?
                .snapshot()?
                .process_instance_id
        );
        let mut invalid = serde_json::to_value(requirements)?;
        invalid["unexpected"] = true.into();
        assert!(serde_json::from_value::<CompatibilityRequirements>(invalid).is_err());
        Ok(())
    }

    #[test]
    fn compatibility_head_is_uncached_and_has_no_body() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let runtime = DaemonCompatibility::new(None)?;
        let response = request_once_with_compatibility(
            temp.path(),
            "HEAD /live-feeds/compatibility.json HTTP/1.1\r\nHost: localhost\r\n\r\n",
            runtime,
        )?;
        assert!(response.contains("Cache-Control: no-cache, no-store\r\n"));
        assert_eq!(response.split_once("\r\n\r\n").unwrap().1, "");
        Ok(())
    }

    #[test]
    fn offline_cli_needs_only_a_publication_and_rejects_extra_arguments() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let path = write_test_publication(temp.path(), &["AAA"])?;
        let mut args = vec![
            "aerobag-live-feedsd".to_string(),
            "--describe-compatibility".to_string(),
            path.display().to_string(),
        ];
        let output = describe_compatibility(&args)?.context("missing offline descriptor")?;
        let descriptor: CompatibilityRequirements = serde_json::from_str(&output)?;
        assert_eq!(descriptor.notam_catalog.airport_count, 1);
        assert_eq!(descriptor.wire_contracts, live_feed_wire_inventory());
        assert!(!temp.path().join("source").exists());
        assert!(!temp.path().join("live").exists());
        args.push("--listen".into());
        assert!(describe_compatibility(&args).is_err());
        assert!(describe_compatibility(&args[..2]).is_err());
        Ok(())
    }

    #[test]
    fn missing_projection_provenance_overrides_matching_published_provenance() -> anyhow::Result<()>
    {
        let temp = tempdir()?;
        seed(temp.path())?;
        let path = write_test_publication(temp.path(), &["AAA"])?;
        let loaded = LoadedPublication::load(&path)?;
        let mut first = TestInstance::start(temp.path(), &loaded, "first")?;
        let old = first.publish()?;
        let connection = rusqlite::Connection::open(
            first.publisher.notam_store.as_ref().unwrap().sqlite_path(),
        )?;
        connection.execute(
            "DELETE FROM metadata WHERE key = 'notam_catalog_identity_v1'",
            [],
        )?;
        drop(connection);
        let mut restarted = TestInstance::start(temp.path(), &loaded, "restarted")?;
        assert!(!old.state_path.exists());
        assert!(!restarted.compatibility.snapshot()?.ready);
        assert!(!restarted.publish()?.unchanged);
        Ok(())
    }

    #[test]
    fn corrupt_cached_bytes_with_matching_provenance_cannot_advertise_ready() -> anyhow::Result<()>
    {
        let temp = tempdir()?;
        seed(temp.path())?;
        let path = write_test_publication(temp.path(), &["AAA"])?;
        let loaded = LoadedPublication::load(&path)?;
        let old = TestInstance::start(temp.path(), &loaded, "first")?.publish()?;
        fs::write(&old.state_path, "corrupt cached checkpoint")?;
        let mut restarted = TestInstance::start(temp.path(), &loaded, "restarted")?;
        let result = restarted.tick();
        assert!(!result.failures.is_empty());
        assert!(result.published.is_empty());
        assert!(!restarted.compatibility.snapshot()?.ready);
        assert!(!restarted.compatibility.snapshot()?.projection_ready);
        Ok(())
    }

    #[test]
    fn loaded_catalog_unions_every_bundle_and_rejects_unknown_manifest_schemas(
    ) -> anyhow::Result<()> {
        let temp = tempdir()?;
        let path_a = write_test_publication(&temp.path().join("a"), &["AAA", "BBB"])?;
        let path_b = write_test_publication(&temp.path().join("b"), &["BBB", "CCC"])?;
        let mut a: serde_json::Value = serde_json::from_slice(&fs::read(&path_a)?)?;
        let b: serde_json::Value = serde_json::from_slice(&fs::read(&path_b)?)?;
        let extra_root = path_a.parent().unwrap().join("unpacked");
        let b_root = path_b.parent().unwrap().join("unpacked");
        fs::rename(
            b_root.join("nav_db_fixture"),
            extra_root.join("nav_db_second"),
        )?;
        let mut bundle_b: serde_json::Value =
            serde_json::from_slice(&fs::read(b_root.join("bundle.json"))?)?;
        bundle_b["packages"][0]["relative_path"] = "nav_db_second.zip".into();
        fs::write(
            extra_root.join("bundle_second.json"),
            serde_json::to_vec(&bundle_b)?,
        )?;
        let mut entry = b["bundles"][0].clone();
        entry["relative_path"] = "bundle_second.json".into();
        a["bundles"].as_array_mut().unwrap().push(entry);
        fs::write(&path_a, serde_json::to_vec(&a)?)?;
        let loaded = LoadedPublication::load(&path_a)?;
        assert_eq!(
            loaded.catalog.airport_ids,
            BTreeSet::from(["AAA".into(), "BBB".into(), "CCC".into()])
        );
        a["schema_version"] = 99.into();
        fs::write(&path_a, serde_json::to_vec(&a)?)?;
        assert!(LoadedPublication::load(&path_a).is_err());
        a["schema_version"] = product_contracts::publication::current::v1::SCHEMA_VERSION.into();
        fs::write(&path_a, serde_json::to_vec(&a)?)?;
        bundle_b["schema_version"] = 99.into();
        fs::write(
            extra_root.join("bundle_second.json"),
            serde_json::to_vec(&bundle_b)?,
        )?;
        assert!(LoadedPublication::load(&path_a).is_err());
        Ok(())
    }
}
