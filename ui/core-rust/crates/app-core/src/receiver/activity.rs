// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Bounded reception history. Only decoded socket/replay input belongs here,
//! never restored caches, Internet products, or UI reads.
use std::collections::VecDeque;

use app_ui_contracts::receiver::{
    UiReceiverActivity, UiReceiverActivityLegend, UiReceiverActivityRect, UiReceiverActivityStrip,
};

use super::{Product, Weather, WeatherKind};

const MINUTE_MS: u64 = 60_000;
const BUCKETS: usize = 60;
const SERIES: [(&str, &str); 6] = [
    ("traffic", "Traffic"),
    ("sensors", "Ownship / sensors"),
    ("metar", "METAR"),
    ("taf", "TAF"),
    ("radar", "Radar"),
    ("other", "Other weather"),
];

#[derive(Debug, Clone, Default)]
struct Bucket {
    minute: u64,
    counts: [u64; 6],
    disconnected: bool,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Activity {
    buckets: VecDeque<Bucket>,
    last_seen: [Option<u64>; 6],
    connected: bool,
    last_ms: Option<u64>,
}

impl Activity {
    /// The previous connection state owns the interval before this observation.
    /// Monotonic time keeps wall-clock adjustments out of the reception graph.
    pub fn advance(&mut self, now: u64, connected: bool) {
        if self.last_ms.is_none() && !connected {
            return;
        }
        assert!(self.last_ms.is_none_or(|last| now >= last));
        let minute = now / MINUTE_MS;
        let first = minute.saturating_sub((BUCKETS - 1) as u64);
        while self.buckets.front().is_some_and(|b| b.minute < first) {
            self.buckets.pop_front();
        }
        let next = self.buckets.back().map_or_else(
            || {
                self.last_ms
                    .map_or(minute, |last| (last / MINUTE_MS).max(first))
            },
            |b| b.minute + 1,
        );
        for minute in next..=minute {
            self.buckets.push_back(Bucket {
                minute,
                ..Default::default()
            });
        }
        if !self.connected {
            if let Some(last) = self.last_ms {
                for bucket in &mut self.buckets {
                    let start = last.max(bucket.minute * MINUTE_MS);
                    let end = now.min((bucket.minute + 1) * MINUTE_MS);
                    // Only elapsed disconnection belongs to history. A zero-time
                    // tick before reconnect must not stain the following minute.
                    bucket.disconnected |= start < end;
                }
            }
        }
        self.connected = connected;
        self.last_ms = Some(now);
    }

    pub fn observe(&mut self, product: &Product, now: u64) {
        self.advance(now, self.connected);
        let series = match product {
            Product::Traffic(_) => 0,
            Product::Attitude(_) | Product::PressureUnverified { .. } | Product::Nmea { .. } => 1,
            Product::Weather(Weather::Text { kind, reports, .. }) if !reports.is_empty() => {
                match kind {
                    WeatherKind::Metar => 2,
                    WeatherKind::Taf => 3,
                    _ => 5,
                }
            }
            Product::Weather(Weather::Radar(_)) => 4,
            // Empty responses, source declarations and undecoded payloads are not data.
            _ => return,
        };
        if let Some(bucket) = self.buckets.back_mut() {
            bucket.counts[series] = bucket.counts[series].saturating_add(1);
            self.last_seen[series] = Some(now);
        }
    }

    pub fn view(&self, now: u64) -> Option<UiReceiverActivity> {
        self.last_ms?;
        let last_minute = now / MINUTE_MS;
        let strips = [("Flight data", 0..2), ("Weather", 2..6)]
            .into_iter()
            .map(|(title, series)| {
                let max = self
                    .buckets
                    .iter()
                    .map(|b| series.clone().map(|i| b.counts[i]).sum::<u64>())
                    .max()
                    .unwrap_or(0);
                let scale = max.max(1).checked_next_power_of_two().unwrap_or(u64::MAX);
                let mut rects = Vec::new();
                for slot in 0..BUCKETS {
                    let minute = last_minute.checked_sub((BUCKETS - 1 - slot) as u64);
                    let bucket = minute.and_then(|m| {
                        self.buckets
                            .front()
                            .and_then(|first| m.checked_sub(first.minute))
                            .and_then(|i| self.buckets.get(i as usize))
                    });
                    let x = slot as u32 * 1000;
                    let background = match bucket {
                        None => Some("unobserved"),
                        Some(b)
                            if b.disconnected || (b.minute == last_minute && !self.connected) =>
                        {
                            Some("disconnected")
                        }
                        _ => None,
                    };
                    if let Some(color) = background {
                        rects.push(rect(x, 0, 1000, 1000, color));
                    }
                    if let Some(bucket) = bucket {
                        let mut total = 0u128;
                        for i in series.clone() {
                            let bottom = (total * 1000 / u128::from(scale)) as u32;
                            total += u128::from(bucket.counts[i]);
                            let top = (total * 1000 / u128::from(scale)) as u32;
                            if top > bottom {
                                rects.push(rect(
                                    x + 100,
                                    1000 - top,
                                    800,
                                    top - bottom,
                                    SERIES[i].0,
                                ));
                            }
                        }
                    }
                }
                let legend = series
                    .map(|i| UiReceiverActivityLegend {
                        color_key: SERIES[i].0.into(),
                        text: format!(
                            "{}: {}",
                            SERIES[i].1,
                            self.last_seen[i].map_or_else(
                                || "not received".into(),
                                |last| {
                                    let seconds = now.saturating_sub(last) / 1000;
                                    if seconds < 60 {
                                        format!("last received {seconds}s ago")
                                    } else {
                                        format!("last received {}m ago", seconds / 60)
                                    }
                                }
                            )
                        ),
                    })
                    .collect();
                UiReceiverActivityStrip {
                    title: title.into(),
                    scale_label: format!("0-{scale} messages/min"),
                    rects,
                    legend,
                }
            })
            .collect();
        Some(UiReceiverActivity {
            title: "Reception activity".into(),
            axis_start: "60 minutes ago".into(),
            axis_end: "Now".into(),
            strips,
        })
    }
}

fn rect(x: u32, y: u32, width: u32, height: u32, color: &str) -> UiReceiverActivityRect {
    UiReceiverActivityRect {
        x,
        y,
        width,
        height,
        color_key: color.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn traffic() -> Product {
        Product::Traffic(super::super::test_support::traffic(0))
    }

    #[test]
    fn minute_boundaries_gaps_and_hour_expiration_are_bounded() {
        let mut a = Activity::default();
        assert!(a.view(0).is_none());
        a.advance(0, true);
        a.observe(&traffic(), 59_999);
        a.observe(&traffic(), 60_000);
        assert_eq!(a.buckets[0].counts[0], 1);
        assert_eq!(a.buckets[1].counts[0], 1);
        a.advance(65_000, false);
        a.advance(180_000, true);
        assert!(a.buckets[1].disconnected);
        assert!(a.buckets[2].disconnected);
        assert!(!a.buckets[3].disconnected);
        a.advance(3_600_000, true);
        assert_eq!(a.buckets.len(), 60);
        assert_eq!(a.buckets.front().unwrap().minute, 1);
        a.advance(10 * 3_600_000, false);
        assert_eq!(a.buckets.len(), 60);
        assert!(a.buckets.iter().all(|b| b.counts == [0; 6]));
    }

    #[test]
    fn reconnect_on_boundary_does_not_depend_on_extra_clock_ticks() {
        let mut direct = Activity::default();
        direct.advance(0, true);
        direct.advance(500, false);
        let mut ticked = direct.clone();
        direct.advance(60_000, true);
        ticked.advance(60_000, false);
        ticked.advance(60_000, true);
        assert_eq!(direct.view(60_000), ticked.view(60_000));
    }

    #[test]
    fn duplicates_count_but_empty_and_uninterpreted_weather_do_not() {
        let mut a = Activity::default();
        a.advance(0, true);
        for _ in 0..3 {
            a.observe(&traffic(), 100);
        }
        a.observe(
            &Product::Weather(Weather::Empty {
                kind: WeatherKind::Metar,
            }),
            200,
        );
        a.observe(
            &Product::Uninterpreted {
                message_id: 1,
                length: 99,
            },
            200,
        );
        a.observe(
            &Product::Weather(Weather::Radar(super::super::test_support::radar())),
            300,
        );
        assert_eq!(a.buckets[0].counts, [3, 0, 0, 0, 1, 0]);
        let view = a.view(300).unwrap();
        assert_eq!(view.strips[0].scale_label, "0-4 messages/min");
        assert_eq!(view.strips[1].scale_label, "0-1 messages/min");
        assert_eq!(
            view.strips[0].legend[0].text,
            "Traffic: last received 0s ago"
        );
        assert_eq!(view.strips[1].legend[0].text, "METAR: not received");
        let theme: serde_json::Value =
            serde_json::from_str(include_str!("../../../../../shared-fixtures/ui-theme.json"))
                .unwrap();
        for strip in view.strips {
            for r in strip.rects {
                assert!(r.x + r.width <= 60_000 && r.y + r.height <= 1000);
                assert!(theme["receiver_activity"][r.color_key].is_string());
            }
        }
    }
}
