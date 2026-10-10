// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Volatile receiver state. Weather survives restart; sensor fixes never do.

use std::sync::Arc;

use super::{capture::CaptureClock, Traffic};
use crate::{
    adsb::{AdsbAircraft, TrafficView, MAX_VISIBLE_POSITION_AGE_MS},
    LatLon, OwnshipSourceId, OwnshipSourceKind, OwnshipSourceRegistration, SituationSample,
};

pub(crate) const SOURCE_ID: &str = "receiver-gps";
const FIX_LIFETIME_MS: i64 = 10_000;
const PRESSURE_LIFETIME_MS: u64 = 5_000;

#[derive(Debug, Clone, PartialEq)]
struct PressureSample {
    altitude_ft: f64,
    clock: CaptureClock,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LiveSnapshot {
    pub(crate) available: bool,
    pub(crate) connected: bool,
    pub(crate) ownship: Option<SituationSample>,
    ownship_deadline_epoch_ms: Option<i64>,
    ownship_deadline_monotonic_ms: Option<u64>,
    pub(crate) aircraft: Arc<[AdsbAircraft]>,
    pressure: Option<PressureSample>,
    pub(crate) radar: Arc<super::radar::History>,
}

impl LiveSnapshot {
    pub(crate) fn pressure_at(&self, clock: CaptureClock) -> Option<(i64, f64)> {
        self.pressure
            .as_ref()
            .filter(|sample| {
                clock.monotonic_ms >= sample.clock.monotonic_ms
                    && clock.monotonic_ms - sample.clock.monotonic_ms < PRESSURE_LIFETIME_MS
                    && (0..PRESSURE_LIFETIME_MS as i64).contains(
                        &clock
                            .wall_epoch_ms
                            .saturating_sub(sample.clock.wall_epoch_ms),
                    )
            })
            .map(|sample| (sample.clock.wall_epoch_ms, sample.altitude_ft))
    }

    pub(crate) fn traffic(&self) -> TrafficView<'_> {
        TrafficView {
            aircraft: &self.aircraft,
        }
    }

    pub fn ownship_at(&self, clock: CaptureClock) -> Option<&SituationSample> {
        self.ownship.as_ref().filter(|sample| {
            clock.wall_epoch_ms >= sample.received_time_epoch_ms
                && self
                    .ownship_deadline_epoch_ms
                    .is_some_and(|deadline| clock.wall_epoch_ms < deadline)
                && self
                    .ownship_deadline_monotonic_ms
                    .is_some_and(|deadline| clock.monotonic_ms < deadline)
        })
    }

    pub(crate) fn registration(&self) -> OwnshipSourceRegistration {
        OwnshipSourceRegistration {
            source_id: OwnshipSourceId(SOURCE_ID.into()),
            source_kind: OwnshipSourceKind::ExternalGps,
            display_name: "Receiver".into(),
            selectable: true,
            auto_eligible: false,
            stale_after_ms: Some(FIX_LIFETIME_MS),
            power_state: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::receiver::test_support::traffic;
    use crate::{adsb::TrafficOwnshipAltitude, MapSurfaceMetrics, MapViewport};

    fn clock(ms: u64) -> CaptureClock {
        CaptureClock {
            monotonic_ms: ms,
            wall_epoch_ms: 1_790_000_000_000 + ms as i64,
        }
    }
    fn metrics() -> MapSurfaceMetrics {
        MapSurfaceMetrics::new(
            MapViewport {
                center: LatLon {
                    lat: 47.45,
                    lon: -122.31,
                },
                zoom: 9.0,
                rotation_deg: 0.0,
                pitch_deg: 0.0,
            },
            800.0,
            600.0,
            1.0,
        )
    }

    #[test]
    fn msl_fusion_requires_same_fix_in_either_order_and_does_not_refresh_its_age() {
        let clock = clock(0);
        let report = traffic(clock.wall_epoch_ms);
        let utc = report.ownship.utc.unwrap();
        let sentence = format!(
            "$GPGGA,{},4727.000,N,12218.600,W,1,10,1.0,609.6,M,-20,M,,*00",
            utc.format("%H%M%S.00")
        );
        for nmea_first in [false, true] {
            let mut input = LiveInput::default();
            input.set_connected(true);
            if nmea_first {
                input.ingest_nmea(&sentence, clock);
            }
            input.ingest(&report, clock);
            if !nmea_first {
                input.ingest_nmea(&sentence, clock);
            }
            assert_eq!(
                input.snapshot().ownship_at(clock).unwrap().altitude_msl_ft,
                Some(2000.0)
            );
            let late = CaptureClock {
                monotonic_ms: clock.monotonic_ms + 10_000,
                wall_epoch_ms: clock.wall_epoch_ms + 10_000,
            };
            input.ingest_nmea(&sentence, late);
            assert!(input.snapshot().ownship_at(late).is_none());
            input.ingest(&traffic(late.wall_epoch_ms), late);
            assert_eq!(
                input.snapshot().ownship_at(late).unwrap().altitude_msl_ft,
                None
            );
            input.set_connected(false);
            input.set_connected(true);
            input.ingest(&report, clock);
            assert_eq!(
                input.snapshot().ownship_at(clock).unwrap().altitude_msl_ft,
                None
            );
        }
        let mut input = LiveInput::default();
        input.set_connected(true);
        input.ingest_nmea(&sentence.replace(",1,10,", ",0,10,"), clock);
        input.ingest(&report, clock);
        assert!(
            input.snapshot().ownship_at(clock).is_none(),
            "invalid GGA cannot be contradicted by same-fix traffic"
        );
        input.set_connected(false);
        input.set_connected(true);
        input.ingest_nmea(&sentence.replace("4727.000", "4827.000"), clock);
        input.ingest(&report, clock);
        assert_eq!(
            input.snapshot().ownship_at(clock).unwrap().altitude_msl_ft,
            None
        );
    }

    #[test]
    fn frozen_fix_clock_repeated_delivery_and_new_session_do_not_rejuvenate_ownship() {
        let mut input = LiveInput::default();
        input.set_connected(true);
        let report = traffic(clock(0).wall_epoch_ms);
        input.ingest(&report, clock(0));
        let leased = input.snapshot();
        let fix = leased.ownship_at(clock(0)).unwrap();
        assert_eq!(fix.pressure_altitude_ft, Some(2000.0));
        assert_eq!(fix.altitude_msl_ft, None, "geometric altitude is not MSL");
        for tick in [1000, 9000, 10_000, 20_000] {
            input.ingest(&report, clock(tick));
            assert_eq!(
                input.snapshot().ownship_at(clock(tick)).is_some(),
                tick < 10_000
            );
        }
        assert!(
            leased.ownship_at(clock(10_000)).is_none(),
            "old leased snapshot also expires"
        );
        assert!(LiveInput::default()
            .snapshot()
            .ownship_at(clock(0))
            .is_none());
        input.ingest(&traffic(clock(21_000).wall_epoch_ms), clock(21_000));
        assert!(input.snapshot().ownship_at(clock(21_000)).is_some());
    }

    #[test]
    fn invalid_fix_disconnect_reconnect_and_monotonic_expiry_remove_live_data() {
        let mut input = LiveInput::default();
        input.set_connected(true);
        input.ingest(&traffic(clock(0).wall_epoch_ms), clock(0));
        let mut invalid = traffic(clock(1000).wall_epoch_ms);
        invalid.ownship.latitude = None;
        input.ingest(&invalid, clock(1000));
        assert!(input.snapshot().ownship_at(clock(1000)).is_none());
        input.ingest(&traffic(clock(2000).wall_epoch_ms), clock(2000));
        let before = input.snapshot().aircraft[0].id.clone();
        input.set_connected(false);
        assert!(input.snapshot().aircraft.is_empty());
        assert!(input.snapshot().ownship.is_none());
        input.ingest(&traffic(clock(3000).wall_epoch_ms), clock(3000));
        assert!(
            input.snapshot().aircraft.is_empty(),
            "disconnected input cannot restore targets"
        );
        input.set_connected(true);
        assert!(input.snapshot().ownship.is_none());
        input.ingest(&traffic(clock(4000).wall_epoch_ms), clock(4000));
        assert_ne!(input.snapshot().aircraft[0].id, before);
        input.expire(CaptureClock {
            monotonic_ms: 14_000,
            wall_epoch_ms: clock(4000).wall_epoch_ms,
        });
        assert!(
            input.snapshot().ownship.is_none(),
            "wall clock rollback must not extend a fix"
        );
    }

    #[test]
    fn shared_map_and_inspector_obey_report_age_and_same_datum_altitudes() {
        let mut input = LiveInput::default();
        input.set_connected(true);
        let mut report = traffic(clock(0).wall_epoch_ms);
        report.age_seconds = Some(2.0);
        input.ingest(&report, clock(0));
        let snapshot = input.snapshot();
        let own = TrafficOwnshipAltitude {
            pressure_altitude_ft: Some(2000.0),
            altitude_msl_ft: Some(10_000.0),
        };
        let visible = snapshot
            .traffic()
            .visible_traffic(metrics(), own, clock(0).wall_epoch_ms);
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].detail_label, "+05");
        assert_eq!(
            visible[0].symbol_points,
            vec![(0.0, -9.0), (9.0, 0.0), (0.0, 9.0), (-9.0, 0.0)]
        );
        assert_eq!(
            visible[0].track_deg_true, None,
            "unknown direction datum is not a track"
        );
        let items = snapshot
            .traffic()
            .traffic_selection_category(
                metrics(),
                own,
                LatLon {
                    lat: 47.451,
                    lon: -122.31,
                },
                clock(0).wall_epoch_ms,
            )
            .items;
        assert_eq!(items.len(), 1);
        assert!(
            items[0].actions.iter().all(|action| !action.enabled),
            "no accidental Internet Follow polling"
        );
        assert_eq!(
            snapshot
                .traffic()
                .next_expiry_epoch_ms(clock(0).wall_epoch_ms),
            Some(clock(12_001).wall_epoch_ms)
        );
        for now in [clock(12_001).wall_epoch_ms, clock(60_000).wall_epoch_ms] {
            assert!(snapshot
                .traffic()
                .visible_traffic(metrics(), own, now)
                .is_empty());
            assert!(snapshot
                .traffic()
                .traffic_selection_category(
                    metrics(),
                    own,
                    LatLon {
                        lat: 47.451,
                        lon: -122.31
                    },
                    now
                )
                .items
                .is_empty());
        }
        report.targets[0].state_age_seconds = None;
        input.ingest(&report, clock(1000));
        assert!(input
            .snapshot()
            .traffic()
            .visible_traffic(metrics(), own, clock(1000).wall_epoch_ms)
            .is_empty());
    }
}

#[derive(Default)]
pub struct LiveInput {
    snapshot: Arc<LiveSnapshot>,
    generation: u64,
    last_fix_epoch_ms: Option<i64>,
    fix_deadline_monotonic_ms: Option<u64>,
    traffic_deadline_monotonic_ms: Option<u64>,
    gga: Option<super::nmea::Gga>,
}

impl LiveInput {
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn restore_radar(&mut self, radar: super::radar::History) {
        let snapshot = Arc::make_mut(&mut self.snapshot);
        snapshot.available |= !radar.frames.is_empty();
        snapshot.radar = Arc::new(radar);
    }

    pub fn snapshot(&self) -> Arc<LiveSnapshot> {
        self.snapshot.clone()
    }

    pub fn set_connected(&mut self, connected: bool) {
        if self.snapshot.connected == connected {
            return;
        }
        self.generation += 1;
        self.last_fix_epoch_ms = None;
        self.fix_deadline_monotonic_ms = None;
        self.traffic_deadline_monotonic_ms = None;
        self.gga = None;
        self.snapshot = Arc::new(LiveSnapshot {
            available: true,
            connected,
            radar: self.snapshot.radar.clone(),
            ..LiveSnapshot::default()
        });
    }

    pub fn ingest(&mut self, traffic: &Traffic, clock: CaptureClock) {
        if !self.snapshot.connected {
            return;
        }
        let mut next = (*self.snapshot).clone();
        let own = &traffic.ownship;
        next.pressure = own
            .pressure_altitude_feet
            .map(|altitude_ft| PressureSample { altitude_ft, clock });
        // A valid coordinate with an old/frozen GPS clock is not a new fix.
        let fix_time = own.utc.map(|utc| utc.timestamp_millis());
        if let Some(time) = fix_time.filter(|time| {
            self.last_fix_epoch_ms.is_none_or(|last| *time > last)
                && (-1_000..FIX_LIFETIME_MS).contains(&clock.wall_epoch_ms.saturating_sub(*time))
        }) {
            self.last_fix_epoch_ms = Some(time);
            let lifetime = FIX_LIFETIME_MS - clock.wall_epoch_ms.saturating_sub(time).max(0);
            self.fix_deadline_monotonic_ms =
                Some(clock.monotonic_ms.saturating_add(lifetime as u64));
            next.ownship_deadline_monotonic_ms = self.fix_deadline_monotonic_ms;
            next.ownship_deadline_epoch_ms = Some(clock.wall_epoch_ms.saturating_add(lifetime));
            next.ownship = own
                .latitude
                .zip(own.longitude)
                .map(|(lat, lon)| SituationSample {
                    source_id: OwnshipSourceId(SOURCE_ID.into()),
                    source_kind: OwnshipSourceKind::ExternalGps,
                    event_time_epoch_ms: time,
                    received_time_epoch_ms: clock.wall_epoch_ms,
                    position: Some(LatLon { lat, lon }),
                    horizontal_accuracy_m: None,
                    vertical_accuracy_m: None,
                    track_deg_true: own.true_track_degrees,
                    heading_deg_true: own.true_heading_degrees,
                    ground_speed_kt: own.ground_speed_knots,
                    // Only a matching GGA may supply MSL, never geometric height.
                    altitude_msl_ft: None,
                    pressure_altitude_ft: own.pressure_altitude_feet,
                    vertical_speed_fpm: None,
                });
        }
        if own.latitude.is_none() || own.longitude.is_none() || fix_time.is_none() {
            next.ownship = None;
        }
        self.fuse_msl(&mut next);
        next.aircraft = traffic
            .targets
            .iter()
            .map(|target| {
                // Receiver track IDs, not unqualified addresses, identify targets.
                let id = format!(
                    "receiver-{}-{}-{}-{:06x}",
                    self.generation,
                    target.receiver_track_id,
                    target.address_qualifier_raw,
                    target.address
                );
                let age = traffic
                    .age_seconds
                    .zip(target.state_age_seconds)
                    .map(|(snapshot, state)| snapshot + state);
                AdsbAircraft {
                    id,
                    registration: None,
                    callsign: target.callsign.clone(),
                    aircraft_type: None,
                    position: target
                        .latitude
                        .zip(target.longitude)
                        .map(|(lat, lon)| LatLon { lat, lon }),
                    position_epoch_ms: age
                        .filter(|age| {
                            age.is_finite()
                                && (0.0..=MAX_VISIBLE_POSITION_AGE_MS as f64 / 1000.0).contains(age)
                        })
                        .map(|age| {
                            clock
                                .wall_epoch_ms
                                .saturating_sub((age * 1000.0).ceil() as i64)
                        }),
                    // Direction datum / vertical source qualifiers are not established.
                    track_deg_true: None,
                    ground_speed_kt: target.ground_speed_knots,
                    pressure_altitude_ft: target.pressure_altitude_feet,
                    altitude_msl_ft: None,
                    vertical_speed_fpm: None,
                    on_ground: !target.airborne,
                }
            })
            .collect();
        self.traffic_deadline_monotonic_ms = next
            .aircraft
            .iter()
            .filter_map(|aircraft| aircraft.position_epoch_ms)
            .max()
            .map(|latest| {
                clock.monotonic_ms.saturating_add(
                    latest
                        .saturating_add(MAX_VISIBLE_POSITION_AGE_MS + 1)
                        .saturating_sub(clock.wall_epoch_ms)
                        .max(0) as u64,
                )
            });
        self.snapshot = Arc::new(next);
        self.expire(clock);
    }

    pub fn ingest_radar(
        &mut self,
        radar: &super::Radar,
        clock: CaptureClock,
    ) -> Result<bool, &'static str> {
        let now = chrono::DateTime::from_timestamp_millis(clock.wall_epoch_ms)
            .ok_or("invalid radar receipt time")?;
        Arc::make_mut(&mut Arc::make_mut(&mut self.snapshot).radar).ingest(radar, now)
    }

    pub fn ingest_nmea(&mut self, sentence: &str, clock: CaptureClock) {
        if !self.snapshot.connected {
            return;
        }
        let Some(gga) = super::nmea::gga(sentence, clock.wall_epoch_ms) else {
            return;
        };
        if !(-1_000..FIX_LIFETIME_MS).contains(&clock.wall_epoch_ms.saturating_sub(gga.time))
            || self.gga.as_ref().is_some_and(|last| gga.time <= last.time)
        {
            return;
        }
        self.gga = Some(gga);
        let mut next = (*self.snapshot).clone();
        self.fuse_msl(&mut next);
        if next != *self.snapshot {
            self.snapshot = Arc::new(next);
        }
        self.expire(clock);
    }

    fn fuse_msl(&self, next: &mut LiveSnapshot) {
        let (Some(gga), Some(sample)) = (&self.gga, next.ownship.as_mut()) else {
            return;
        };
        if gga.fix.is_none()
            && gga.time.div_euclid(1000) >= sample.event_time_epoch_ms.div_euclid(1000)
        {
            next.ownship = None;
            return;
        }
        // Join messages from the same fix, in either arrival order. Never use
        // an altitude from an earlier second as the aircraft continues moving.
        if gga.time.div_euclid(1000) != sample.event_time_epoch_ms.div_euclid(1000) {
            return;
        }
        sample.altitude_msl_ft = gga.fix.and_then(|(position, altitude)| {
            sample
                .position
                .filter(|own| crate::geodesy::great_circle_distance_nm(*own, position) < 0.05)
                .and(altitude)
        });
    }

    pub fn expire(&mut self, clock: CaptureClock) {
        if self.snapshot.pressure.is_some() && self.snapshot.pressure_at(clock).is_none() {
            Arc::make_mut(&mut self.snapshot).pressure = None;
        }
        if self
            .traffic_deadline_monotonic_ms
            .is_some_and(|deadline| clock.monotonic_ms >= deadline)
        {
            Arc::make_mut(&mut self.snapshot).aircraft = Arc::default();
            self.traffic_deadline_monotonic_ms = None;
        }
        if self
            .fix_deadline_monotonic_ms
            .is_some_and(|deadline| clock.monotonic_ms >= deadline)
            || self
                .snapshot
                .ownship_deadline_epoch_ms
                .is_some_and(|deadline| clock.wall_epoch_ms >= deadline)
        {
            if self.snapshot.ownship.is_some() {
                Arc::make_mut(&mut self.snapshot).ownship = None;
            }
            self.fix_deadline_monotonic_ms = None;
        }
    }

    pub fn next_wake_ms(&self) -> Option<u64> {
        self.fix_deadline_monotonic_ms
            .into_iter()
            .chain(self.traffic_deadline_monotonic_ms)
            .chain(self.snapshot.pressure.as_ref().map(|sample| {
                sample
                    .clock
                    .monotonic_ms
                    .saturating_add(PRESSURE_LIFETIME_MS)
            }))
            .min()
    }
}
