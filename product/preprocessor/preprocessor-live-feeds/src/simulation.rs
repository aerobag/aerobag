use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context};
use chrono::{DateTime, Duration, Utc};

use crate::{
    engine::{
        read_json_value, BuiltLiveFeedState, CompiledFixtureEvent, CompiledFixtureTimeline,
        CycleDataProvider, DeltaPolicy, LiveFeedVersionManifest, ProductBuilder, UpstreamEvent,
        UpstreamSource,
    },
    products::{directory_live_feed_state, json_live_feed_state, live_nexrad_tile_count},
};

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
        let elapsed = source_time
            .signed_duration_since(self.source_start_utc)
            .num_milliseconds();
        let scaled = elapsed / i64::from(self.speedup);
        self.simulation_start_utc
            .checked_add_signed(Duration::milliseconds(scaled))
            .context("simulation time overflow")
    }
}

#[derive(Debug, Clone)]
struct ScheduledFixtureEvent {
    simulation_time_utc: DateTime<Utc>,
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
        clock_map: SimulationClockMap,
    ) -> anyhow::Result<Self> {
        let product_id = product_id.into();
        let mut events = timeline
            .events
            .into_iter()
            .filter(|event| event.product == product_id)
            .map(|event| {
                let simulation_time_utc =
                    clock_map.source_to_simulation_time(event.observed_at_utc)?;
                Ok(ScheduledFixtureEvent {
                    simulation_time_utc,
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
                source_id: scheduled.event.version.clone(),
                observed_at_utc: scheduled.event.observed_at_utc,
                payload_path: None,
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
        _scratch_dir: &Path,
        _cycle_data: &dyn CycleDataProvider,
    ) -> anyhow::Result<BuiltLiveFeedState> {
        if event.product != self.product_id {
            bail!(
                "fixture builder for {} received {} event",
                self.product_id,
                event.product
            );
        }
        let version_manifest_path = self
            .fixture_root
            .join("versions")
            .join(&event.product)
            .join(format!("{}.json", event.source_id));
        let manifest: LiveFeedVersionManifest = serde_json::from_slice(
            &fs::read(&version_manifest_path)
                .with_context(|| format!("failed to read {}", version_manifest_path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", version_manifest_path.display()))?;
        let state_path = self.fixture_root.join(&manifest.state.url);
        let state_value = read_json_value(&state_path)?;
        if state_path.file_name().and_then(|name| name.to_str()) == Some("manifest.json")
            && state_path.parent().is_some_and(Path::is_dir)
        {
            let state_root = state_path
                .parent()
                .context("directory state manifest has no parent")?
                .to_path_buf();
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
            let manifest: LiveFeedVersionManifest =
                serde_json::from_slice(&fs::read(&version_manifest_path).with_context(|| {
                    format!("failed to read {}", version_manifest_path.display())
                })?)
                .with_context(|| format!("failed to parse {}", version_manifest_path.display()))?;
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
    use chrono::TimeZone;

    fn event(product: &str, version: &str, minute: u32) -> CompiledFixtureEvent {
        CompiledFixtureEvent {
            product: product.to_string(),
            version: version.to_string(),
            observed_at_utc: Utc.with_ymd_and_hms(2026, 5, 18, 12, minute, 0).unwrap(),
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
        let source_start = fixture_source_start(&timeline)?;
        let simulation_start = Utc.with_ymd_and_hms(2026, 5, 18, 20, 0, 0).unwrap();
        let clock_map = SimulationClockMap::new(source_start, simulation_start, 60)?;
        let mut source = SimulatedLiveFeedSource::from_timeline("metars", timeline, clock_map)?;

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
            vec!["m1"]
        );
        assert_eq!(source.remaining(), 1);
        Ok(())
    }

    #[test]
    fn zero_speedup_is_rejected() {
        let now = Utc.with_ymd_and_hms(2026, 5, 18, 0, 0, 0).unwrap();
        assert!(SimulationClockMap::new(now, now, 0).is_err());
    }
}
