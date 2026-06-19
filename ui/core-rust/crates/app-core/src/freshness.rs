use chrono::{DateTime, NaiveDate, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataFreshnessPolicies {
    pub cycle_product: CycleProductFreshnessPolicy,
    pub live_feeds: LiveFeedFreshnessPolicies,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CycleProductFreshnessPolicy {
    pub warning_after_expiration: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveFeedFreshnessPolicies {
    pub metars: AgeFreshnessPolicy,
    pub tafs: AgeFreshnessPolicy,
    pub nexrad: AgeFreshnessPolicy,
    pub obstacles: AgeFreshnessPolicy,
    pub tfrs: AgeFreshnessPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgeFreshnessPolicy {
    pub info_after_ms: Option<i64>,
    pub warning_after_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreshnessSeverity {
    Info,
    Warning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FreshnessViolation {
    pub severity: FreshnessSeverity,
    pub age_ms: i64,
}

pub const MINUTE_MS: i64 = 60 * 1000;
pub const HOUR_MS: i64 = 60 * MINUTE_MS;
pub const DAY_MS: i64 = 24 * HOUR_MS;

pub const DATA_FRESHNESS_POLICIES: DataFreshnessPolicies = DataFreshnessPolicies {
    cycle_product: CycleProductFreshnessPolicy {
        warning_after_expiration: true,
    },
    live_feeds: LiveFeedFreshnessPolicies {
        metars: AgeFreshnessPolicy {
            info_after_ms: None,
            warning_after_ms: Some(30 * MINUTE_MS),
        },
        tafs: AgeFreshnessPolicy {
            info_after_ms: None,
            warning_after_ms: Some(8 * HOUR_MS),
        },
        nexrad: AgeFreshnessPolicy {
            info_after_ms: None,
            warning_after_ms: Some(10 * MINUTE_MS),
        },
        obstacles: AgeFreshnessPolicy {
            info_after_ms: Some(DAY_MS),
            warning_after_ms: Some(7 * DAY_MS),
        },
        tfrs: AgeFreshnessPolicy {
            info_after_ms: Some(HOUR_MS),
            warning_after_ms: Some(DAY_MS),
        },
    },
};

pub fn cycle_product_is_expired(expiration_utc: DateTime<Utc>, now_utc: DateTime<Utc>) -> bool {
    DATA_FRESHNESS_POLICIES
        .cycle_product
        .warning_after_expiration
        && now_utc > expiration_utc
}

pub fn evaluate_age(
    policy: AgeFreshnessPolicy,
    observed_utc: DateTime<Utc>,
    now_utc: DateTime<Utc>,
) -> Option<FreshnessViolation> {
    let age_ms = now_utc
        .timestamp_millis()
        .saturating_sub(observed_utc.timestamp_millis());
    if policy
        .warning_after_ms
        .is_some_and(|threshold| age_ms > threshold)
    {
        return Some(FreshnessViolation {
            severity: FreshnessSeverity::Warning,
            age_ms,
        });
    }
    if policy
        .info_after_ms
        .is_some_and(|threshold| age_ms > threshold)
    {
        return Some(FreshnessViolation {
            severity: FreshnessSeverity::Info,
            age_ms,
        });
    }
    None
}

pub fn parse_utc_instant(value: &str) -> Option<DateTime<Utc>> {
    if let Ok(value) = DateTime::parse_from_rfc3339(value) {
        return Some(value.with_timezone(&Utc));
    }
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()?;
    let midnight = date.and_hms_opt(0, 0, 0)?;
    Some(DateTime::<Utc>::from_naive_utc_and_offset(midnight, Utc))
}

pub fn format_age(age_ms: i64) -> String {
    let age_ms = age_ms.max(0);
    if age_ms >= DAY_MS {
        let days = age_ms.div_euclid(DAY_MS);
        let hours = age_ms.rem_euclid(DAY_MS).div_euclid(HOUR_MS);
        if hours == 0 {
            format!("{days}d")
        } else {
            format!("{days}d {hours}h")
        }
    } else if age_ms >= HOUR_MS {
        let hours = age_ms.div_euclid(HOUR_MS);
        let minutes = age_ms.rem_euclid(HOUR_MS).div_euclid(MINUTE_MS);
        if minutes == 0 {
            format!("{hours}h")
        } else {
            format!("{hours}h {minutes}m")
        }
    } else {
        let minutes = age_ms.div_euclid(MINUTE_MS);
        format!("{minutes}m")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policies_are_visible_in_one_place() {
        assert_eq!(
            DATA_FRESHNESS_POLICIES.live_feeds.metars.warning_after_ms,
            Some(30 * MINUTE_MS)
        );
        assert_eq!(
            DATA_FRESHNESS_POLICIES.live_feeds.tafs.warning_after_ms,
            Some(8 * HOUR_MS)
        );
        assert_eq!(
            DATA_FRESHNESS_POLICIES.live_feeds.nexrad.warning_after_ms,
            Some(10 * MINUTE_MS)
        );
        assert_eq!(
            DATA_FRESHNESS_POLICIES.live_feeds.obstacles.info_after_ms,
            Some(DAY_MS)
        );
        assert_eq!(
            DATA_FRESHNESS_POLICIES
                .live_feeds
                .obstacles
                .warning_after_ms,
            Some(7 * DAY_MS)
        );
        assert_eq!(
            DATA_FRESHNESS_POLICIES.live_feeds.tfrs.info_after_ms,
            Some(HOUR_MS)
        );
        assert_eq!(
            DATA_FRESHNESS_POLICIES.live_feeds.tfrs.warning_after_ms,
            Some(DAY_MS)
        );
    }

    #[test]
    fn evaluates_live_feed_age_thresholds() {
        let now = parse_utc_instant("2026-05-20T12:31:00Z").expect("now");
        let violation = evaluate_age(
            DATA_FRESHNESS_POLICIES.live_feeds.metars,
            parse_utc_instant("2026-05-20T12:00:00Z").expect("observed"),
            now,
        )
        .expect("old metar feed");

        assert_eq!(violation.severity, FreshnessSeverity::Warning);
        assert_eq!(format_age(violation.age_ms), "31m");

        let now = parse_utc_instant("2026-05-20T12:00:00Z").expect("now");
        let tfr_info = evaluate_age(
            DATA_FRESHNESS_POLICIES.live_feeds.tfrs,
            parse_utc_instant("2026-05-20T10:00:00Z").expect("observed"),
            now,
        )
        .expect("stale tfr feed");
        assert_eq!(tfr_info.severity, FreshnessSeverity::Info);

        let obstacle_info = evaluate_age(
            DATA_FRESHNESS_POLICIES.live_feeds.obstacles,
            parse_utc_instant("2026-05-18T11:00:00Z").expect("observed"),
            now,
        )
        .expect("stale obstacle feed");
        assert_eq!(obstacle_info.severity, FreshnessSeverity::Info);

        let obstacle_warning = evaluate_age(
            DATA_FRESHNESS_POLICIES.live_feeds.obstacles,
            parse_utc_instant("2026-05-12T12:00:00Z").expect("observed"),
            now,
        )
        .expect("old obstacle feed");
        assert_eq!(obstacle_warning.severity, FreshnessSeverity::Warning);
    }

    #[test]
    fn treats_cycle_dates_as_utc_expiration_instants() {
        let now = parse_utc_instant("2026-05-20T00:00:01Z").expect("now");

        assert!(cycle_product_is_expired(
            parse_utc_instant("2026-05-20").expect("expired"),
            now
        ));
        assert!(!cycle_product_is_expired(
            parse_utc_instant("2026-05-21").expect("current"),
            now
        ));
    }
}
