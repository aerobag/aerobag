// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::Serialize;

pub const MINUTE_MS: i64 = 60 * 1_000;
pub const HOUR_MS: i64 = 60 * MINUTE_MS;
pub const DAY_MS: i64 = 24 * HOUR_MS;
pub const LIVE_FEED_FAILED_RESOURCE_RETRY_DELAY_MS: i64 = 5 * MINUTE_MS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveFeedProducerKind {
    PollingTask,
    ExternalCollector,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct LiveFeedProducerPolicy {
    pub kind: LiveFeedProducerKind,
    pub nominal_interval_seconds: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LiveFeedCachePolicy {
    RecordJson {
        records_key: &'static str,
        count_key: Option<&'static str>,
    },
    RecordJsonArray {
        records_key: &'static str,
        record_id_key: &'static str,
        count_key: Option<&'static str>,
    },
    FullJson,
    NavKv,
    NexradPackage,
    Notam,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveFeedPreparationPolicy {
    None,
    Metars,
    Tafs,
    Pireps,
    Tfrs,
    Notams,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveFeedDeltaPolicy {
    None,
    RecordJson,
    NavKv,
    Notam,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveFeedUiInvalidationPolicy {
    None,
    MapOverlay,
    NexradOverlay,
}

impl LiveFeedPreparationPolicy {
    pub const fn is_prepared(self) -> bool {
        !matches!(self, Self::None)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct LiveFeedAgePolicy {
    pub info_after_ms: Option<i64>,
    pub warning_after_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct LiveFeedOperatorHealthPolicy {
    pub warning_after_seconds: u64,
    pub critical_after_seconds: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct LiveFeedProductPolicy {
    pub product_id: &'static str,
    pub display_name: &'static str,
    pub status_order: u8,
    pub producer: LiveFeedProducerPolicy,
    pub retention_seconds: u64,
    pub cache: LiveFeedCachePolicy,
    pub preparation: LiveFeedPreparationPolicy,
    pub delta: LiveFeedDeltaPolicy,
    pub ui_invalidation: LiveFeedUiInvalidationPolicy,
    pub user_freshness: Option<LiveFeedAgePolicy>,
    pub operator_health: LiveFeedOperatorHealthPolicy,
}

impl LiveFeedProductPolicy {
    pub const fn is_polling_task(self) -> bool {
        matches!(self.producer.kind, LiveFeedProducerKind::PollingTask)
    }
}

const THREE_HOURS: u64 = 3 * 60 * 60;
const SEVEN_DAYS: u64 = 7 * 24 * 60 * 60;

/// The authoritative roster and policy for every public live-feed product.
///
/// Producer scheduling, publication retention, core decoding, user-facing
/// freshness, and operator health deliberately remain separate fields: they
/// answer different questions, but they must agree on the product identity.
pub const LIVE_FEED_PRODUCT_POLICIES: &[LiveFeedProductPolicy] = &[
    LiveFeedProductPolicy {
        product_id: "tfrs",
        display_name: "TFRs",
        status_order: 10,
        producer: LiveFeedProducerPolicy {
            kind: LiveFeedProducerKind::PollingTask,
            nominal_interval_seconds: 5 * 60,
        },
        retention_seconds: THREE_HOURS,
        cache: LiveFeedCachePolicy::RecordJsonArray {
            records_key: "areas",
            record_id_key: "area_id",
            count_key: Some("area_group_count"),
        },
        preparation: LiveFeedPreparationPolicy::Tfrs,
        delta: LiveFeedDeltaPolicy::RecordJson,
        ui_invalidation: LiveFeedUiInvalidationPolicy::MapOverlay,
        user_freshness: Some(LiveFeedAgePolicy {
            info_after_ms: Some(HOUR_MS),
            warning_after_ms: Some(DAY_MS),
        }),
        operator_health: LiveFeedOperatorHealthPolicy {
            warning_after_seconds: 3 * 60 * 60,
            critical_after_seconds: 6 * 60 * 60,
        },
    },
    LiveFeedProductPolicy {
        product_id: "notams",
        display_name: "NOTAMs",
        status_order: 20,
        producer: LiveFeedProducerPolicy {
            kind: LiveFeedProducerKind::ExternalCollector,
            nominal_interval_seconds: 3 * 60,
        },
        retention_seconds: THREE_HOURS,
        cache: LiveFeedCachePolicy::Notam,
        preparation: LiveFeedPreparationPolicy::Notams,
        delta: LiveFeedDeltaPolicy::Notam,
        ui_invalidation: LiveFeedUiInvalidationPolicy::None,
        user_freshness: Some(LiveFeedAgePolicy {
            info_after_ms: Some(HOUR_MS),
            warning_after_ms: Some(DAY_MS),
        }),
        operator_health: LiveFeedOperatorHealthPolicy {
            warning_after_seconds: 5 * 60,
            critical_after_seconds: 15 * 60,
        },
    },
    LiveFeedProductPolicy {
        product_id: "metars",
        display_name: "METARs",
        status_order: 30,
        producer: LiveFeedProducerPolicy {
            kind: LiveFeedProducerKind::PollingTask,
            nominal_interval_seconds: 5 * 60,
        },
        retention_seconds: THREE_HOURS,
        cache: LiveFeedCachePolicy::RecordJson {
            records_key: "metars_by_station",
            count_key: Some("metar_count"),
        },
        preparation: LiveFeedPreparationPolicy::Metars,
        delta: LiveFeedDeltaPolicy::RecordJson,
        ui_invalidation: LiveFeedUiInvalidationPolicy::MapOverlay,
        user_freshness: Some(LiveFeedAgePolicy {
            info_after_ms: None,
            warning_after_ms: Some(30 * MINUTE_MS),
        }),
        operator_health: LiveFeedOperatorHealthPolicy {
            warning_after_seconds: 7 * 60,
            critical_after_seconds: 30 * 60,
        },
    },
    LiveFeedProductPolicy {
        product_id: "pireps",
        display_name: "PIREPs",
        status_order: 40,
        producer: LiveFeedProducerPolicy {
            kind: LiveFeedProducerKind::PollingTask,
            nominal_interval_seconds: 5 * 60,
        },
        retention_seconds: THREE_HOURS,
        cache: LiveFeedCachePolicy::RecordJson {
            records_key: "pireps_by_id",
            count_key: Some("pirep_count"),
        },
        preparation: LiveFeedPreparationPolicy::Pireps,
        delta: LiveFeedDeltaPolicy::RecordJson,
        ui_invalidation: LiveFeedUiInvalidationPolicy::MapOverlay,
        user_freshness: Some(LiveFeedAgePolicy {
            info_after_ms: None,
            warning_after_ms: Some(30 * MINUTE_MS),
        }),
        operator_health: LiveFeedOperatorHealthPolicy {
            warning_after_seconds: 15 * 60,
            critical_after_seconds: 30 * 60,
        },
    },
    LiveFeedProductPolicy {
        product_id: "tafs",
        display_name: "TAFs",
        status_order: 50,
        producer: LiveFeedProducerPolicy {
            kind: LiveFeedProducerKind::PollingTask,
            nominal_interval_seconds: 5 * 60,
        },
        retention_seconds: THREE_HOURS,
        cache: LiveFeedCachePolicy::RecordJson {
            records_key: "tafs_by_station",
            count_key: Some("taf_count"),
        },
        preparation: LiveFeedPreparationPolicy::Tafs,
        delta: LiveFeedDeltaPolicy::RecordJson,
        ui_invalidation: LiveFeedUiInvalidationPolicy::MapOverlay,
        user_freshness: Some(LiveFeedAgePolicy {
            info_after_ms: None,
            warning_after_ms: Some(8 * HOUR_MS),
        }),
        operator_health: LiveFeedOperatorHealthPolicy {
            warning_after_seconds: 60 * 60,
            critical_after_seconds: 3 * 60 * 60,
        },
    },
    LiveFeedProductPolicy {
        product_id: "nexrad",
        display_name: "NEXRAD",
        status_order: 60,
        producer: LiveFeedProducerPolicy {
            kind: LiveFeedProducerKind::PollingTask,
            nominal_interval_seconds: 5 * 60,
        },
        retention_seconds: 34 * 60,
        cache: LiveFeedCachePolicy::NexradPackage,
        preparation: LiveFeedPreparationPolicy::None,
        delta: LiveFeedDeltaPolicy::None,
        ui_invalidation: LiveFeedUiInvalidationPolicy::NexradOverlay,
        user_freshness: Some(LiveFeedAgePolicy {
            info_after_ms: None,
            warning_after_ms: Some(10 * MINUTE_MS),
        }),
        operator_health: LiveFeedOperatorHealthPolicy {
            warning_after_seconds: 700,
            critical_after_seconds: 15 * 60,
        },
    },
    LiveFeedProductPolicy {
        product_id: "obstacles",
        display_name: "Obstacles",
        status_order: 70,
        producer: LiveFeedProducerPolicy {
            kind: LiveFeedProducerKind::PollingTask,
            nominal_interval_seconds: 6 * 60 * 60,
        },
        retention_seconds: SEVEN_DAYS,
        cache: LiveFeedCachePolicy::NavKv,
        preparation: LiveFeedPreparationPolicy::None,
        delta: LiveFeedDeltaPolicy::NavKv,
        ui_invalidation: LiveFeedUiInvalidationPolicy::MapOverlay,
        user_freshness: Some(LiveFeedAgePolicy {
            info_after_ms: Some(DAY_MS),
            warning_after_ms: Some(7 * DAY_MS),
        }),
        operator_health: LiveFeedOperatorHealthPolicy {
            warning_after_seconds: 2 * 24 * 60 * 60,
            critical_after_seconds: 7 * 24 * 60 * 60,
        },
    },
    LiveFeedProductPolicy {
        product_id: "winds-aloft",
        display_name: "Winds aloft",
        status_order: 80,
        producer: LiveFeedProducerPolicy {
            kind: LiveFeedProducerKind::PollingTask,
            nominal_interval_seconds: 60 * 60,
        },
        retention_seconds: SEVEN_DAYS,
        cache: LiveFeedCachePolicy::NavKv,
        preparation: LiveFeedPreparationPolicy::None,
        delta: LiveFeedDeltaPolicy::None,
        ui_invalidation: LiveFeedUiInvalidationPolicy::None,
        // Forecast validity, not simple sample age, controls the pilot warning.
        user_freshness: None,
        operator_health: LiveFeedOperatorHealthPolicy {
            warning_after_seconds: 12 * 60 * 60,
            critical_after_seconds: 18 * 60 * 60,
        },
    },
];

pub fn live_feed_product_policy(product_id: &str) -> Option<&'static LiveFeedProductPolicy> {
    LIVE_FEED_PRODUCT_POLICIES
        .iter()
        .find(|policy| policy.product_id == product_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn product_ids_and_status_order_are_unique() {
        let mut ids = BTreeSet::new();
        let mut status_order = BTreeSet::new();
        for policy in LIVE_FEED_PRODUCT_POLICIES {
            assert!(
                ids.insert(policy.product_id),
                "duplicate {}",
                policy.product_id
            );
            assert!(
                status_order.insert(policy.status_order),
                "duplicate status order {}",
                policy.status_order
            );
            assert!(
                policy.operator_health.warning_after_seconds
                    < policy.operator_health.critical_after_seconds
            );
            match policy.delta {
                LiveFeedDeltaPolicy::RecordJson => {
                    assert!(matches!(
                        policy.cache,
                        LiveFeedCachePolicy::RecordJson { .. }
                            | LiveFeedCachePolicy::RecordJsonArray { .. }
                    ));
                    assert!(policy.preparation.is_prepared());
                }
                LiveFeedDeltaPolicy::NavKv => {
                    assert_eq!(policy.cache, LiveFeedCachePolicy::NavKv);
                }
                LiveFeedDeltaPolicy::Notam => {
                    assert_eq!(policy.cache, LiveFeedCachePolicy::Notam);
                    assert_eq!(policy.preparation, LiveFeedPreparationPolicy::Notams);
                }
                LiveFeedDeltaPolicy::None => {}
            }
        }
    }
}
