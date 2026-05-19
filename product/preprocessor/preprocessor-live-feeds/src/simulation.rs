use anyhow::{bail, Context};
use chrono::{DateTime, Duration, Utc};

use crate::engine::{CompiledFixtureEvent, CompiledFixtureTimeline, UpstreamEvent, UpstreamSource};

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
