use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde::Deserialize;
use serde_json::Value;

use crate::{
    engine::{
        read_json_value, write_json_pretty_file, BuiltLiveFeedState, CompiledFixtureEvent,
        CompiledFixtureTimeline, CycleDataProvider, DeltaPolicy, ProductBuilder, UpstreamEvent,
        UpstreamSource,
    },
    products::{directory_live_feed_state, json_live_feed_state, live_nexrad_tile_count},
};

#[derive(Debug, Deserialize)]
struct FixtureVersionManifest {
    product: String,
    version: String,
    state: FixturePayloadRef,
}

#[derive(Debug, Deserialize)]
struct FixturePayloadRef {
    url: String,
    state_sha256: String,
}

#[derive(Debug, Clone)]
pub struct SimulationClockMap {
    source_start_utc: DateTime<Utc>,
    simulation_start_utc: DateTime<Utc>,
    speedup: u32,
}

impl SimulationClockMap {
    pub fn new(
        source_start_utc: DateTime<Utc>,
        simulation_start_utc: DateTime<Utc>,
        speedup: u32,
    ) -> anyhow::Result<Self> {
        if speedup == 0 {
            bail!("simulation speedup must be greater than zero");
        }
        Ok(Self {
            source_start_utc,
            simulation_start_utc,
            speedup,
        })
    }

    pub fn source_to_simulation_time(
        &self,
        source_time: DateTime<Utc>,
    ) -> anyhow::Result<DateTime<Utc>> {
        let elapsed = self.source_elapsed(source_time);
        let scaled = elapsed.num_milliseconds() / i64::from(self.speedup);
        self.simulation_start_utc
            .checked_add_signed(Duration::milliseconds(scaled))
            .context("simulation time overflow")
    }

    pub fn source_to_virtual_time(
        &self,
        source_time: DateTime<Utc>,
    ) -> anyhow::Result<DateTime<Utc>> {
        self.simulation_start_utc
            .checked_add_signed(self.source_elapsed(source_time))
            .context("virtual simulation time overflow")
    }

    fn source_elapsed(&self, source_time: DateTime<Utc>) -> Duration {
        source_time.signed_duration_since(self.source_start_utc)
    }
}

#[derive(Debug, Clone)]
struct ScheduledFixtureEvent {
    simulation_time_utc: DateTime<Utc>,
    virtual_time_utc: DateTime<Utc>,
    source_id: String,
    version_manifest_url: String,
    event: CompiledFixtureEvent,
}

#[derive(Debug, Clone)]
pub struct SimulatedLiveFeedSource {
    product_id: String,
    events: Vec<ScheduledFixtureEvent>,
    cursor: usize,
}

impl SimulatedLiveFeedSource {
    pub fn from_timeline(
        product_id: impl Into<String>,
        timeline: CompiledFixtureTimeline,
        simulation_start_utc: DateTime<Utc>,
        speedup: u32,
    ) -> anyhow::Result<Self> {
        let product_id = product_id.into();
        let source_start_utc = fixture_product_source_start(&timeline, &product_id)?;
        let clock_map = SimulationClockMap::new(source_start_utc, simulation_start_utc, speedup)?;
        let mut events = timeline
            .events
            .into_iter()
            .filter(|event| event.product == product_id)
            .map(|event| {
                let simulation_time_utc =
                    clock_map.source_to_simulation_time(event.observed_at_utc)?;
                let virtual_time_utc = clock_map.source_to_virtual_time(event.observed_at_utc)?;
                let source_id = simulated_fixture_version(&event.version, virtual_time_utc);
                Ok(ScheduledFixtureEvent {
                    simulation_time_utc,
                    virtual_time_utc,
                    source_id,
                    version_manifest_url: event.version_manifest_url.clone(),
                    event,
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        events.sort_by_key(|event| (event.simulation_time_utc, event.event.version.clone()));
        Ok(Self {
            product_id,
            events,
            cursor: 0,
        })
    }

    pub fn remaining(&self) -> usize {
        self.events.len().saturating_sub(self.cursor)
    }
}

impl UpstreamSource for SimulatedLiveFeedSource {
    fn product_id(&self) -> &str {
        &self.product_id
    }

    fn poll_due(&mut self, now: DateTime<Utc>) -> anyhow::Result<Vec<UpstreamEvent>> {
        let start = self.cursor;
        while self
            .events
            .get(self.cursor)
            .is_some_and(|event| event.simulation_time_utc <= now)
        {
            self.cursor += 1;
        }
        Ok(self.events[start..self.cursor]
            .iter()
            .map(|scheduled| UpstreamEvent {
                product: scheduled.event.product.clone(),
                source_id: scheduled.source_id.clone(),
                observed_at_utc: scheduled.virtual_time_utc,
                payload_path: Some(PathBuf::from(&scheduled.version_manifest_url)),
            })
            .collect())
    }
}

pub fn fixture_source_start(timeline: &CompiledFixtureTimeline) -> anyhow::Result<DateTime<Utc>> {
    timeline
        .events
        .iter()
        .map(|event| event.observed_at_utc)
        .min()
        .context("fixture timeline has no events")
}

pub fn fixture_product_source_start(
    timeline: &CompiledFixtureTimeline,
    product_id: &str,
) -> anyhow::Result<DateTime<Utc>> {
    timeline
        .events
        .iter()
        .filter(|event| event.product == product_id)
        .map(|event| event.observed_at_utc)
        .min()
        .with_context(|| format!("fixture timeline has no {product_id} events"))
}

pub fn fixture_loop_duration(
    timeline: &CompiledFixtureTimeline,
    speedup: u32,
) -> anyhow::Result<Option<Duration>> {
    if speedup == 0 {
        bail!("simulation speedup must be greater than zero");
    }
    let mut bounds = BTreeMap::<String, (DateTime<Utc>, DateTime<Utc>)>::new();
    for event in &timeline.events {
        bounds
            .entry(event.product.clone())
            .and_modify(|(start, end)| {
                *start = (*start).min(event.observed_at_utc);
                *end = (*end).max(event.observed_at_utc);
            })
            .or_insert((event.observed_at_utc, event.observed_at_utc));
    }
    let shortest_source_ms = bounds
        .values()
        .filter_map(|(start, end)| {
            let millis = end.signed_duration_since(*start).num_milliseconds();
            (millis > 0).then_some(millis)
        })
        .min();
    let Some(source_ms) = shortest_source_ms else {
        return Ok(None);
    };
    let speedup = i64::from(speedup);
    let scaled_ms = ((source_ms + speedup - 1) / speedup).max(1);
    Ok(Some(Duration::milliseconds(scaled_ms)))
}

#[derive(Debug, Clone)]
pub struct CompiledFixtureStateBuilder {
    fixture_root: PathBuf,
    product_id: String,
}

impl CompiledFixtureStateBuilder {
    pub fn new(fixture_root: impl Into<PathBuf>, product_id: impl Into<String>) -> Self {
        Self {
            fixture_root: fixture_root.into(),
            product_id: product_id.into(),
        }
    }
}

impl ProductBuilder for CompiledFixtureStateBuilder {
    fn product_id(&self) -> &str {
        &self.product_id
    }

    fn build_state(
        &self,
        event: &UpstreamEvent,
        scratch_dir: &Path,
        _cycle_data: &dyn CycleDataProvider,
    ) -> anyhow::Result<BuiltLiveFeedState> {
        if event.product != self.product_id {
            bail!(
                "fixture builder for {} received {} event",
                self.product_id,
                event.product
            );
        }
        let version_manifest_path = event
            .payload_path
            .as_ref()
            .map(|path| {
                if path.is_absolute() {
                    path.clone()
                } else {
                    self.fixture_root.join(path)
                }
            })
            .unwrap_or_else(|| {
                self.fixture_root
                    .join("versions")
                    .join(&event.product)
                    .join(format!("{}.json", event.source_id))
            });
        let manifest: FixtureVersionManifest = serde_json::from_slice(
            &fs::read(&version_manifest_path)
                .with_context(|| format!("failed to read {}", version_manifest_path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", version_manifest_path.display()))?;
        if manifest.product != event.product {
            bail!(
                "fixture manifest {} is for product {}, expected {}",
                version_manifest_path.display(),
                manifest.product,
                event.product
            );
        }
        let state_path = self.fixture_root.join(&manifest.state.url);
        let mut state_value = read_json_value(&state_path)?;
        rewrite_simulated_state_metadata(&mut state_value, &event.source_id, event.observed_at_utc);
        if state_path.file_name().and_then(|name| name.to_str()) == Some("manifest.json")
            && state_path.parent().is_some_and(Path::is_dir)
        {
            let source_root = state_path
                .parent()
                .context("directory state manifest has no parent")?
                .to_path_buf();
            let state_root = write_simulated_directory_state(
                &source_root,
                &state_path,
                &state_value,
                scratch_dir,
                &event.product,
                &event.source_id,
            )?;
            let state_path = state_root.join("manifest.json");
            let count = if event.product == "nexrad" {
                live_nexrad_tile_count(&state_value)?
            } else {
                1
            };
            return Ok(directory_live_feed_state(
                &event.product,
                event.source_id.clone(),
                state_root,
                state_path,
                state_value,
                count,
            ));
        }
        let state_path = write_simulated_json_state(
            &state_value,
            scratch_dir,
            &event.product,
            &event.source_id,
        )?;
        let (delta_policy, count) = match event.product.as_str() {
            "metars" => (
                DeltaPolicy::KeyedRecords {
                    records_key: "metars_by_station".to_string(),
                    count_key: Some("metar_count".to_string()),
                },
                json_count(&state_value, "metar_count", "metars_by_station"),
            ),
            "obstacles" => (
                DeltaPolicy::KeyedRecords {
                    records_key: "obstacles_by_id".to_string(),
                    count_key: Some("obstacle_count".to_string()),
                },
                json_count(&state_value, "obstacle_count", "obstacles_by_id"),
            ),
            "tfrs" => (
                DeltaPolicy::None,
                state_value
                    .get("areas")
                    .and_then(serde_json::Value::as_array)
                    .map_or(0, Vec::len),
            ),
            "winds-aloft" => (
                DeltaPolicy::None,
                state_value
                    .get("files")
                    .and_then(serde_json::Value::as_array)
                    .map_or(0, Vec::len),
            ),
            _ => (DeltaPolicy::None, 1),
        };
        Ok(json_live_feed_state(
            &event.product,
            event.source_id.clone(),
            state_path,
            state_value,
            delta_policy,
            count,
        ))
    }
}

fn simulated_fixture_version(source_version: &str, observed_at_utc: DateTime<Utc>) -> String {
    let safe_source = source_version
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .take(96)
        .collect::<String>();
    format!(
        "{}_{:03}Z_{}",
        observed_at_utc.format("%Y%m%dT%H%M%S"),
        observed_at_utc.timestamp_subsec_millis(),
        safe_source
    )
}

fn rewrite_simulated_state_metadata(
    value: &mut Value,
    version: &str,
    observed_at_utc: DateTime<Utc>,
) {
    let timestamp = observed_at_utc.to_rfc3339_opts(SecondsFormat::Secs, true);
    let Some(object) = value.as_object_mut() else {
        return;
    };
    for key in ["version_label", "state_id"] {
        if object.contains_key(key) {
            object.insert(key.to_string(), Value::String(version.to_string()));
        }
    }
    for key in ["generated_at_utc", "observed_at_utc"] {
        if object.contains_key(key) {
            object.insert(key.to_string(), Value::String(timestamp.clone()));
        }
    }
}

fn write_simulated_json_state(
    value: &Value,
    scratch_dir: &Path,
    product: &str,
    version: &str,
) -> anyhow::Result<PathBuf> {
    let state_dir = scratch_dir.join("simulated-states").join(product);
    fs::create_dir_all(&state_dir)
        .with_context(|| format!("failed to create {}", state_dir.display()))?;
    let path = state_dir.join(format!("{version}.json"));
    write_json_pretty_file(&path, value)?;
    Ok(path)
}

fn write_simulated_directory_state(
    source_root: &Path,
    manifest_path: &Path,
    manifest_value: &Value,
    scratch_dir: &Path,
    product: &str,
    version: &str,
) -> anyhow::Result<PathBuf> {
    let state_root = scratch_dir
        .join("simulated-states")
        .join(product)
        .join(version);
    if state_root.exists() {
        fs::remove_dir_all(&state_root)
            .with_context(|| format!("failed to remove {}", state_root.display()))?;
    }
    copy_fixture_directory_state(source_root, &state_root, manifest_path, manifest_value)?;
    Ok(state_root)
}

fn copy_fixture_directory_state(
    source_root: &Path,
    dest_root: &Path,
    manifest_path: &Path,
    manifest_value: &Value,
) -> anyhow::Result<()> {
    fs::create_dir_all(dest_root)
        .with_context(|| format!("failed to create {}", dest_root.display()))?;
    for entry in fs::read_dir(source_root)
        .with_context(|| format!("failed to read {}", source_root.display()))?
    {
        let entry = entry.with_context(|| format!("failed to read {}", source_root.display()))?;
        let source_path = entry.path();
        let dest_path = dest_root.join(entry.file_name());
        if entry
            .file_type()
            .with_context(|| format!("failed to stat {}", source_path.display()))?
            .is_dir()
        {
            copy_fixture_directory_state(&source_path, &dest_path, manifest_path, manifest_value)?;
        } else if source_path == manifest_path {
            write_json_pretty_file(&dest_path, manifest_value)?;
        } else {
            hardlink_or_copy(&source_path, &dest_path)?;
        }
    }
    Ok(())
}

fn hardlink_or_copy(source: &Path, dest: &Path) -> anyhow::Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    match fs::hard_link(source, dest) {
        Ok(()) => Ok(()),
        Err(_) => fs::copy(source, dest)
            .map(|_| ())
            .with_context(|| format!("failed to copy {} to {}", source.display(), dest.display())),
    }
}

pub fn timeline_from_live_feed_root(
    fixture_root: &Path,
    fixture_id: impl Into<String>,
) -> anyhow::Result<CompiledFixtureTimeline> {
    if fixture_root.join("timeline.json").is_file() {
        let path = fixture_root.join("timeline.json");
        return serde_json::from_slice(
            &fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", path.display()));
    }
    let versions_root = fixture_root.join("versions");
    if !versions_root.is_dir() {
        bail!(
            "fixture root {} has neither timeline.json nor versions/",
            fixture_root.display()
        );
    }
    let mut events = Vec::new();
    let mut ordinal = 0_i64;
    for product_entry in sorted_read_dir(&versions_root)? {
        if !product_entry.file_type()?.is_dir() {
            continue;
        }
        let product = product_entry.file_name().to_string_lossy().into_owned();
        for version_entry in sorted_read_dir(&product_entry.path())? {
            let file_name = version_entry.file_name().to_string_lossy().into_owned();
            if !file_name.ends_with(".json") || !version_entry.file_type()?.is_file() {
                continue;
            }
            let version_manifest_path = version_entry.path();
            let manifest: FixtureVersionManifest =
                serde_json::from_slice(&fs::read(&version_manifest_path).with_context(|| {
                    format!("failed to read {}", version_manifest_path.display())
                })?)
                .with_context(|| format!("failed to parse {}", version_manifest_path.display()))?;
            if manifest.product != product {
                bail!(
                    "fixture manifest {} is for product {}, expected {}",
                    version_manifest_path.display(),
                    manifest.product,
                    product
                );
            }
            let state_path = fixture_root.join(&manifest.state.url);
            let state_value = read_json_value(&state_path)?;
            let observed_at_utc = live_state_observed_at(&state_value)
                .unwrap_or_else(|| fallback_fixture_time(ordinal));
            ordinal += 1;
            events.push(CompiledFixtureEvent {
                product: product.clone(),
                version: manifest.version.clone(),
                observed_at_utc,
                version_manifest_url: format!("versions/{product}/{file_name}"),
                state_url: manifest.state.url,
                state_sha256: manifest.state.state_sha256,
            });
        }
    }
    events.sort_by(|left, right| {
        (left.observed_at_utc, &left.product, &left.version).cmp(&(
            right.observed_at_utc,
            &right.product,
            &right.version,
        ))
    });
    Ok(CompiledFixtureTimeline {
        schema_version: 1,
        fixture_id: fixture_id.into(),
        events,
    })
}

fn json_count(value: &serde_json::Value, count_key: &str, records_key: &str) -> usize {
    value
        .get(count_key)
        .and_then(serde_json::Value::as_u64)
        .map(|count| count as usize)
        .or_else(|| {
            value
                .get(records_key)
                .and_then(serde_json::Value::as_object)
                .map(serde_json::Map::len)
        })
        .unwrap_or(0)
}

fn live_state_observed_at(value: &serde_json::Value) -> Option<DateTime<Utc>> {
    for key in ["observed_at_utc", "generated_at_utc"] {
        if let Some(text) = value.get(key).and_then(serde_json::Value::as_str) {
            if let Ok(time) = DateTime::parse_from_rfc3339(text) {
                return Some(time.with_timezone(&Utc));
            }
        }
    }
    None
}

fn fallback_fixture_time(ordinal: i64) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .expect("static timestamp is valid")
        .with_timezone(&Utc)
        + Duration::minutes(ordinal)
}

fn sorted_read_dir(path: &Path) -> anyhow::Result<Vec<fs::DirEntry>> {
    let mut entries = fs::read_dir(path)
        .with_context(|| format!("failed to read {}", path.display()))?
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("failed to read {}", path.display()))?;
    entries.sort_by_key(|entry| entry.file_name());
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::LiveFeedStatePayload;
    use chrono::TimeZone;
    use serde_json::json;
    use std::collections::BTreeSet;
    use tempfile::tempdir;

    struct EmptyCycleDataProvider;

    impl CycleDataProvider for EmptyCycleDataProvider {
        fn current_towered_metar_station_ids(&self) -> anyhow::Result<BTreeSet<String>> {
            Ok(BTreeSet::new())
        }
    }

    fn event(product: &str, version: &str, minute: u32) -> CompiledFixtureEvent {
        event_at(
            product,
            version,
            Utc.with_ymd_and_hms(2026, 5, 18, 12, minute, 0).unwrap(),
        )
    }

    fn event_at(
        product: &str,
        version: &str,
        observed_at_utc: DateTime<Utc>,
    ) -> CompiledFixtureEvent {
        CompiledFixtureEvent {
            product: product.to_string(),
            version: version.to_string(),
            observed_at_utc,
            version_manifest_url: format!("versions/{product}/{version}.json"),
            state_url: format!("states/{product}/{version}.json"),
            state_sha256: "0".repeat(64),
        }
    }

    #[test]
    fn simulation_source_replays_due_events_at_accelerated_time() -> anyhow::Result<()> {
        let timeline = CompiledFixtureTimeline {
            schema_version: 1,
            fixture_id: "mixed".to_string(),
            events: vec![
                event("metars", "m0", 0),
                event("nexrad", "n0", 0),
                event("metars", "m1", 10),
                event("metars", "m2", 20),
            ],
        };
        let simulation_start = Utc.with_ymd_and_hms(2026, 5, 18, 20, 0, 0).unwrap();
        let mut source =
            SimulatedLiveFeedSource::from_timeline("metars", timeline, simulation_start, 60)?;

        assert_eq!(source.remaining(), 3);
        assert_eq!(source.poll_due(simulation_start)?.len(), 1);
        assert_eq!(
            source
                .poll_due(Utc.with_ymd_and_hms(2026, 5, 18, 20, 0, 9).unwrap())?
                .len(),
            0
        );
        let due = source.poll_due(Utc.with_ymd_and_hms(2026, 5, 18, 20, 0, 10).unwrap())?;
        assert_eq!(
            due.iter()
                .map(|event| event.source_id.as_str())
                .collect::<Vec<_>>(),
            vec!["20260518T201000_000Z_m1"]
        );
        assert_eq!(
            due.iter()
                .map(|event| event.observed_at_utc)
                .collect::<Vec<_>>(),
            vec![Utc.with_ymd_and_hms(2026, 5, 18, 20, 10, 0).unwrap()]
        );
        assert_eq!(source.remaining(), 1);
        Ok(())
    }

    #[test]
    fn simulation_aligns_each_product_start_to_the_same_zero() -> anyhow::Result<()> {
        let timeline = CompiledFixtureTimeline {
            schema_version: 1,
            fixture_id: "mixed".to_string(),
            events: vec![
                event_at(
                    "metars",
                    "m0",
                    Utc.with_ymd_and_hms(2026, 5, 18, 12, 0, 0).unwrap(),
                ),
                event_at(
                    "metars",
                    "m1",
                    Utc.with_ymd_and_hms(2026, 5, 18, 12, 10, 0).unwrap(),
                ),
                event_at(
                    "nexrad",
                    "n0",
                    Utc.with_ymd_and_hms(2026, 5, 20, 3, 30, 0).unwrap(),
                ),
                event_at(
                    "nexrad",
                    "n1",
                    Utc.with_ymd_and_hms(2026, 5, 20, 3, 32, 0).unwrap(),
                ),
            ],
        };
        let simulation_start = Utc.with_ymd_and_hms(2026, 5, 18, 20, 12, 0).unwrap();
        let mut metars = SimulatedLiveFeedSource::from_timeline(
            "metars",
            timeline.clone(),
            simulation_start,
            60,
        )?;
        let mut nexrad =
            SimulatedLiveFeedSource::from_timeline("nexrad", timeline, simulation_start, 60)?;

        assert_eq!(metars.poll_due(simulation_start)?.len(), 1);
        assert_eq!(nexrad.poll_due(simulation_start)?.len(), 1);
        assert!(metars
            .poll_due(simulation_start + Duration::seconds(1))?
            .is_empty());
        assert_eq!(
            nexrad
                .poll_due(simulation_start + Duration::seconds(2))?
                .into_iter()
                .map(|event| event.source_id)
                .collect::<Vec<_>>(),
            vec!["20260518T201400_000Z_n1"]
        );
        Ok(())
    }

    #[test]
    fn simulation_clock_compresses_delivery_not_virtual_time() -> anyhow::Result<()> {
        let source_start = Utc.with_ymd_and_hms(2026, 5, 18, 12, 0, 0).unwrap();
        let simulation_start = Utc.with_ymd_and_hms(2026, 5, 18, 20, 12, 0).unwrap();
        let clock_map = SimulationClockMap::new(source_start, simulation_start, 720)?;
        let source_time = Utc.with_ymd_and_hms(2026, 5, 18, 13, 0, 0).unwrap();

        assert_eq!(
            clock_map.source_to_simulation_time(source_time)?,
            simulation_start + Duration::seconds(5)
        );
        assert_eq!(
            clock_map.source_to_virtual_time(source_time)?,
            simulation_start + Duration::hours(1)
        );
        Ok(())
    }

    #[test]
    fn fixture_loop_duration_uses_shortest_positive_product_span() -> anyhow::Result<()> {
        let timeline = CompiledFixtureTimeline {
            schema_version: 1,
            fixture_id: "mixed".to_string(),
            events: vec![
                event("metars", "m0", 0),
                event("metars", "m1", 30),
                event("nexrad", "n0", 0),
                event("nexrad", "n1", 6),
                event("tfrs", "t0", 0),
            ],
        };

        assert_eq!(
            fixture_loop_duration(&timeline, 60)?,
            Some(Duration::seconds(6))
        );
        Ok(())
    }

    #[test]
    fn compiled_fixture_builder_rewrites_json_state_to_simulated_clock() -> anyhow::Result<()> {
        let fixture = tempdir()?;
        let scratch = tempdir()?;
        let state_path = fixture.path().join("states").join("metars").join("m0.json");
        fs::create_dir_all(state_path.parent().unwrap())?;
        write_json_pretty_file(
            &state_path,
            &json!({
                "schema_version": 1,
                "product": "metars",
                "version_label": "m0",
                "generated_at_utc": "2026-01-01T00:00:00Z",
                "metar_count": 0,
                "metars_by_station": {}
            }),
        )?;
        let version_path = fixture
            .path()
            .join("versions")
            .join("metars")
            .join("m0.json");
        fs::create_dir_all(version_path.parent().unwrap())?;
        write_json_pretty_file(
            &version_path,
            &json!({
                "schema_version": 1,
                "product": "metars",
                "version": "m0",
                "state": {
                    "url": "states/metars/m0.json",
                    "bytes": 0,
                    "blob_sha256": "0".repeat(64),
                    "state_sha256": "0".repeat(64)
                }
            }),
        )?;
        let builder = CompiledFixtureStateBuilder::new(fixture.path(), "metars");
        let event_time = Utc.with_ymd_and_hms(2026, 5, 18, 20, 12, 0).unwrap();
        let event = UpstreamEvent {
            product: "metars".to_string(),
            source_id: "20260518T201200_000Z_m0".to_string(),
            observed_at_utc: event_time,
            payload_path: Some(PathBuf::from("versions/metars/m0.json")),
        };

        let built = builder.build_state(&event, scratch.path(), &EmptyCycleDataProvider)?;
        assert_eq!(built.version, "20260518T201200_000Z_m0");
        let LiveFeedStatePayload::JsonFile { path, value } = built.payload else {
            panic!("expected json payload");
        };
        assert_eq!(value["version_label"], "20260518T201200_000Z_m0");
        assert_eq!(value["generated_at_utc"], "2026-05-18T20:12:00Z");
        let written = read_json_value(&path)?;
        assert_eq!(written, value);
        Ok(())
    }

    #[test]
    fn compiled_fixture_builder_rewrites_directory_state_manifest() -> anyhow::Result<()> {
        let fixture = tempdir()?;
        let scratch = tempdir()?;
        let state_root = fixture.path().join("states").join("nexrad").join("n0");
        fs::create_dir_all(state_root.join("tiles"))?;
        fs::write(state_root.join("tiles").join("tile.png"), b"png")?;
        write_json_pretty_file(
            &state_root.join("manifest.json"),
            &json!({
                "schema_version": 1,
                "product": "nexrad",
                "state_id": "n0",
                "observed_at_utc": "2026-01-01T00:00:00Z",
                "levels": []
            }),
        )?;
        let version_path = fixture
            .path()
            .join("versions")
            .join("nexrad")
            .join("n0.json");
        fs::create_dir_all(version_path.parent().unwrap())?;
        write_json_pretty_file(
            &version_path,
            &json!({
                "schema_version": 1,
                "product": "nexrad",
                "version": "n0",
                "state": {
                    "url": "states/nexrad/n0/manifest.json",
                    "bytes": 0,
                    "blob_sha256": "0".repeat(64),
                    "state_sha256": "0".repeat(64)
                }
            }),
        )?;
        let builder = CompiledFixtureStateBuilder::new(fixture.path(), "nexrad");
        let event = UpstreamEvent {
            product: "nexrad".to_string(),
            source_id: "20260518T201200_000Z_n0".to_string(),
            observed_at_utc: Utc.with_ymd_and_hms(2026, 5, 18, 20, 12, 0).unwrap(),
            payload_path: Some(PathBuf::from("versions/nexrad/n0.json")),
        };

        let built = builder.build_state(&event, scratch.path(), &EmptyCycleDataProvider)?;
        let LiveFeedStatePayload::Directory {
            root,
            manifest_path,
            manifest_value,
        } = built.payload
        else {
            panic!("expected directory payload");
        };
        assert_eq!(manifest_value["state_id"], "20260518T201200_000Z_n0");
        assert_eq!(manifest_value["observed_at_utc"], "2026-05-18T20:12:00Z");
        assert_eq!(read_json_value(&manifest_path)?, manifest_value);
        assert!(root.join("tiles").join("tile.png").is_file());
        Ok(())
    }

    #[test]
    fn zero_speedup_is_rejected() {
        let now = Utc.with_ymd_and_hms(2026, 5, 18, 0, 0, 0).unwrap();
        assert!(SimulationClockMap::new(now, now, 0).is_err());
    }
}
