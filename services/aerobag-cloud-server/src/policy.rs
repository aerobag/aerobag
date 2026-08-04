// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{fs, net::IpAddr, path::Path};

use anyhow::{bail, Context as _};
use serde::{Deserialize, Serialize};

use crate::{StoreConfig, TokenBucketConfig};

pub const ACS_POLICY_SCHEMA_VERSION: u32 = 3;

#[cfg(test)]
pub(crate) fn checked_in_test_policy() -> AcsRuntimePolicy {
    let policy: AcsRuntimePolicy =
        serde_json::from_str(include_str!("../../../deploy/aerobag-cloud-policy.json")).unwrap();
    policy.validate().unwrap();
    policy
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcsRuntimePolicy {
    pub schema_version: u32,
    pub trusted_proxy_ips: Vec<IpAddr>,
    pub request: RequestPolicy,
    pub storage: StoragePolicy,
    pub rate_limits: RateLimitPolicy,
    pub sse: SsePolicy,
    pub garbage_collection: GarbageCollectionPolicy,
    pub backup: BackupPolicy,
    pub monitoring: MonitoringPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestPolicy {
    pub max_body_bytes: u64,
    pub max_concurrent_requests: u64,
    pub max_target_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoragePolicy {
    pub anonymous_account_quota_bytes: u64,
    pub anonymous_account_object_limit: u64,
    pub global_storage_limit_bytes: u64,
    pub inline_ciphertext_threshold_bytes: u64,
    pub retained_sse_events: u64,
    pub write_resume_headroom_bytes: u64,
    pub write_resume_min_filesystem_free_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RateLimitPolicy {
    pub account_creation_per_network: TokenBucketPolicy,
    pub account_creation_global: TokenBucketPolicy,
    pub operations_global: TokenBucketPolicy,
    pub operations_per_network: TokenBucketPolicy,
    pub operations_per_account: TokenBucketPolicy,
    pub ingress_bytes_global: TokenBucketPolicy,
    pub ingress_bytes_per_network: TokenBucketPolicy,
    pub ingress_bytes_per_account: TokenBucketPolicy,
    pub egress_bytes_global: TokenBucketPolicy,
    pub egress_bytes_per_network: TokenBucketPolicy,
    pub egress_bytes_per_account: TokenBucketPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenBucketPolicy {
    pub capacity: u64,
    pub refill_amount: u64,
    pub refill_period_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SsePolicy {
    pub max_connections_global: u64,
    pub max_connections_per_account: u64,
    pub max_connections_per_network: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GarbageCollectionPolicy {
    pub interval_seconds: u64,
    pub orphan_grace_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackupPolicy {
    pub interval_seconds: u64,
    pub retained_snapshots: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MonitoringPolicy {
    pub stored_bytes_warning: u64,
    pub stored_bytes_critical: u64,
    pub filesystem_free_bytes_warning: u64,
    pub filesystem_free_bytes_critical: u64,
    pub sse_connections_warning: u64,
    pub sse_connections_critical: u64,
    pub gc_database_pause_ms_warning: u64,
    pub gc_database_pause_ms_critical: u64,
    pub gc_elapsed_ms_warning: u64,
    pub gc_elapsed_ms_critical: u64,
    pub backup_age_seconds_warning: u64,
    pub backup_age_seconds_critical: u64,
    pub backup_elapsed_ms_warning: u64,
    pub backup_elapsed_ms_critical: u64,
    pub backup_sqlite_snapshot_ms_warning: u64,
    pub backup_sqlite_snapshot_ms_critical: u64,
    pub backup_wal_growth_bytes_warning: u64,
    pub backup_wal_growth_bytes_critical: u64,
    pub backup_linked_blob_count_warning: u64,
    pub backup_linked_blob_count_critical: u64,
    pub backup_linked_blob_bytes_warning: u64,
    pub backup_linked_blob_bytes_critical: u64,
}

impl AcsRuntimePolicy {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let bytes = fs::read(path)
            .with_context(|| format!("read ACS runtime policy {}", path.display()))?;
        let policy: Self = serde_json::from_slice(&bytes)
            .with_context(|| format!("decode ACS runtime policy {}", path.display()))?;
        policy.validate()?;
        Ok(policy)
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if self.schema_version != ACS_POLICY_SCHEMA_VERSION {
            bail!(
                "unsupported ACS policy schema {}; expected {}",
                self.schema_version,
                ACS_POLICY_SCHEMA_VERSION
            );
        }
        positive("request.max_body_bytes", self.request.max_body_bytes)?;
        positive(
            "request.max_concurrent_requests",
            self.request.max_concurrent_requests,
        )?;
        positive("request.max_target_bytes", self.request.max_target_bytes)?;
        usize::try_from(self.request.max_body_bytes)
            .context("request.max_body_bytes exceeds this platform")?;
        usize::try_from(self.request.max_concurrent_requests)
            .context("request.max_concurrent_requests exceeds this platform")?;
        usize::try_from(self.request.max_target_bytes)
            .context("request.max_target_bytes exceeds this platform")?;
        positive(
            "storage.anonymous_account_quota_bytes",
            self.storage.anonymous_account_quota_bytes,
        )?;
        positive(
            "storage.anonymous_account_object_limit",
            self.storage.anonymous_account_object_limit,
        )?;
        positive(
            "storage.global_storage_limit_bytes",
            self.storage.global_storage_limit_bytes,
        )?;
        positive(
            "storage.inline_ciphertext_threshold_bytes",
            self.storage.inline_ciphertext_threshold_bytes,
        )?;
        positive(
            "storage.retained_sse_events",
            self.storage.retained_sse_events,
        )?;
        positive(
            "storage.write_resume_headroom_bytes",
            self.storage.write_resume_headroom_bytes,
        )?;
        positive(
            "storage.write_resume_min_filesystem_free_bytes",
            self.storage.write_resume_min_filesystem_free_bytes,
        )?;
        if self.storage.anonymous_account_quota_bytes > self.storage.global_storage_limit_bytes {
            bail!("anonymous account quota exceeds the global storage limit");
        }
        if self.storage.write_resume_headroom_bytes >= self.storage.global_storage_limit_bytes {
            bail!("write resume headroom must be below the global storage limit");
        }
        for (label, bucket) in [
            (
                "rate_limits.account_creation_per_network",
                self.rate_limits.account_creation_per_network,
            ),
            (
                "rate_limits.account_creation_global",
                self.rate_limits.account_creation_global,
            ),
            (
                "rate_limits.operations_global",
                self.rate_limits.operations_global,
            ),
            (
                "rate_limits.operations_per_network",
                self.rate_limits.operations_per_network,
            ),
            (
                "rate_limits.operations_per_account",
                self.rate_limits.operations_per_account,
            ),
            (
                "rate_limits.ingress_bytes_global",
                self.rate_limits.ingress_bytes_global,
            ),
            (
                "rate_limits.ingress_bytes_per_network",
                self.rate_limits.ingress_bytes_per_network,
            ),
            (
                "rate_limits.ingress_bytes_per_account",
                self.rate_limits.ingress_bytes_per_account,
            ),
            (
                "rate_limits.egress_bytes_global",
                self.rate_limits.egress_bytes_global,
            ),
            (
                "rate_limits.egress_bytes_per_network",
                self.rate_limits.egress_bytes_per_network,
            ),
            (
                "rate_limits.egress_bytes_per_account",
                self.rate_limits.egress_bytes_per_account,
            ),
        ] {
            bucket.validate(label)?;
        }
        positive(
            "sse.max_connections_global",
            self.sse.max_connections_global,
        )?;
        positive(
            "sse.max_connections_per_account",
            self.sse.max_connections_per_account,
        )?;
        positive(
            "sse.max_connections_per_network",
            self.sse.max_connections_per_network,
        )?;
        if self.sse.max_connections_per_account > self.sse.max_connections_global
            || self.sse.max_connections_per_network > self.sse.max_connections_global
        {
            bail!("per-account/per-network SSE limits must not exceed the global SSE limit");
        }
        positive(
            "garbage_collection.interval_seconds",
            self.garbage_collection.interval_seconds,
        )?;
        positive("backup.interval_seconds", self.backup.interval_seconds)?;
        positive("backup.retained_snapshots", self.backup.retained_snapshots)?;
        if self.monitoring.backup_age_seconds_warning < self.backup.interval_seconds {
            bail!("backup age warning must not be shorter than the backup interval");
        }
        validate_thresholds(
            "monitoring.stored_bytes",
            self.monitoring.stored_bytes_warning,
            self.monitoring.stored_bytes_critical,
            self.storage.global_storage_limit_bytes,
        )?;
        validate_low_thresholds(
            "monitoring.filesystem_free_bytes",
            self.monitoring.filesystem_free_bytes_warning,
            self.monitoring.filesystem_free_bytes_critical,
        )?;
        validate_thresholds(
            "monitoring.sse_connections",
            self.monitoring.sse_connections_warning,
            self.monitoring.sse_connections_critical,
            self.sse.max_connections_global,
        )?;
        validate_pair(
            "monitoring.gc_database_pause_ms",
            self.monitoring.gc_database_pause_ms_warning,
            self.monitoring.gc_database_pause_ms_critical,
        )?;
        validate_pair(
            "monitoring.gc_elapsed_ms",
            self.monitoring.gc_elapsed_ms_warning,
            self.monitoring.gc_elapsed_ms_critical,
        )?;
        for (label, warning, critical) in [
            (
                "monitoring.backup_age_seconds",
                self.monitoring.backup_age_seconds_warning,
                self.monitoring.backup_age_seconds_critical,
            ),
            (
                "monitoring.backup_elapsed_ms",
                self.monitoring.backup_elapsed_ms_warning,
                self.monitoring.backup_elapsed_ms_critical,
            ),
            (
                "monitoring.backup_sqlite_snapshot_ms",
                self.monitoring.backup_sqlite_snapshot_ms_warning,
                self.monitoring.backup_sqlite_snapshot_ms_critical,
            ),
            (
                "monitoring.backup_wal_growth_bytes",
                self.monitoring.backup_wal_growth_bytes_warning,
                self.monitoring.backup_wal_growth_bytes_critical,
            ),
            (
                "monitoring.backup_linked_blob_count",
                self.monitoring.backup_linked_blob_count_warning,
                self.monitoring.backup_linked_blob_count_critical,
            ),
            (
                "monitoring.backup_linked_blob_bytes",
                self.monitoring.backup_linked_blob_bytes_warning,
                self.monitoring.backup_linked_blob_bytes_critical,
            ),
        ] {
            validate_pair(label, warning, critical)?;
        }
        Ok(())
    }

    pub fn store_config(&self, storage_root: std::path::PathBuf) -> StoreConfig {
        StoreConfig {
            storage_root,
            anonymous_quota_bytes: self.storage.anonymous_account_quota_bytes,
            anonymous_object_limit: self.storage.anonymous_account_object_limit,
            global_storage_limit_bytes: self.storage.global_storage_limit_bytes,
            inline_threshold_bytes: self.storage.inline_ciphertext_threshold_bytes,
            event_retention: self.storage.retained_sse_events,
            global_operation_bucket: self.rate_limits.operations_global.into(),
            network_operation_bucket: self.rate_limits.operations_per_network.into(),
            account_operation_bucket: self.rate_limits.operations_per_account.into(),
            global_ingress_bucket: self.rate_limits.ingress_bytes_global.into(),
            network_ingress_bucket: self.rate_limits.ingress_bytes_per_network.into(),
            account_ingress_bucket: self.rate_limits.ingress_bytes_per_account.into(),
            global_egress_bucket: self.rate_limits.egress_bytes_global.into(),
            network_egress_bucket: self.rate_limits.egress_bytes_per_network.into(),
            account_egress_bucket: self.rate_limits.egress_bytes_per_account.into(),
            global_sse_limit: self.sse.max_connections_global,
            account_sse_limit: self.sse.max_connections_per_account,
            network_sse_limit: self.sse.max_connections_per_network,
            creation_network_bucket: self.rate_limits.account_creation_per_network.into(),
            creation_global_bucket: self.rate_limits.account_creation_global.into(),
            stored_bytes_warning: self.monitoring.stored_bytes_warning,
            stored_bytes_critical: self.monitoring.stored_bytes_critical,
            filesystem_free_bytes_warning: self.monitoring.filesystem_free_bytes_warning,
            filesystem_free_bytes_critical: self.monitoring.filesystem_free_bytes_critical,
            sse_connections_warning: self.monitoring.sse_connections_warning,
            sse_connections_critical: self.monitoring.sse_connections_critical,
            gc_database_pause_ms_warning: self.monitoring.gc_database_pause_ms_warning,
            gc_database_pause_ms_critical: self.monitoring.gc_database_pause_ms_critical,
            gc_elapsed_ms_warning: self.monitoring.gc_elapsed_ms_warning,
            gc_elapsed_ms_critical: self.monitoring.gc_elapsed_ms_critical,
            write_resume_headroom_bytes: self.storage.write_resume_headroom_bytes,
            write_resume_min_filesystem_free_bytes: self
                .storage
                .write_resume_min_filesystem_free_bytes,
            backup_interval_seconds: self.backup.interval_seconds,
            backup_retained_snapshots: self.backup.retained_snapshots,
            backup_age_seconds_warning: self.monitoring.backup_age_seconds_warning,
            backup_age_seconds_critical: self.monitoring.backup_age_seconds_critical,
            backup_elapsed_ms_warning: self.monitoring.backup_elapsed_ms_warning,
            backup_elapsed_ms_critical: self.monitoring.backup_elapsed_ms_critical,
            backup_sqlite_snapshot_ms_warning: self.monitoring.backup_sqlite_snapshot_ms_warning,
            backup_sqlite_snapshot_ms_critical: self.monitoring.backup_sqlite_snapshot_ms_critical,
            backup_wal_growth_bytes_warning: self.monitoring.backup_wal_growth_bytes_warning,
            backup_wal_growth_bytes_critical: self.monitoring.backup_wal_growth_bytes_critical,
            backup_linked_blob_count_warning: self.monitoring.backup_linked_blob_count_warning,
            backup_linked_blob_count_critical: self.monitoring.backup_linked_blob_count_critical,
            backup_linked_blob_bytes_warning: self.monitoring.backup_linked_blob_bytes_warning,
            backup_linked_blob_bytes_critical: self.monitoring.backup_linked_blob_bytes_critical,
        }
    }
}

impl TokenBucketPolicy {
    fn validate(self, label: &str) -> anyhow::Result<()> {
        positive(&format!("{label}.capacity"), self.capacity)?;
        positive(&format!("{label}.refill_amount"), self.refill_amount)?;
        positive(
            &format!("{label}.refill_period_seconds"),
            self.refill_period_seconds,
        )?;
        let refill_period_ms = self
            .refill_period_seconds
            .checked_mul(1_000)
            .with_context(|| format!("{label} refill period overflows milliseconds"))?;
        self.capacity
            .checked_mul(refill_period_ms)
            .with_context(|| format!("{label} values overflow"))?;
        Ok(())
    }
}

impl From<TokenBucketPolicy> for TokenBucketConfig {
    fn from(value: TokenBucketPolicy) -> Self {
        Self {
            capacity: value.capacity,
            refill_amount: value.refill_amount,
            refill_period_ms: value.refill_period_seconds.saturating_mul(1_000),
        }
    }
}

fn positive(label: &str, value: u64) -> anyhow::Result<()> {
    if value == 0 {
        bail!("{label} must be positive");
    }
    Ok(())
}

fn validate_thresholds(label: &str, warning: u64, critical: u64, hard: u64) -> anyhow::Result<()> {
    positive(&format!("{label}_warning"), warning)?;
    if warning >= critical || critical >= hard {
        bail!("{label} thresholds must satisfy warning < critical < hard limit");
    }
    Ok(())
}

fn validate_low_thresholds(label: &str, warning: u64, critical: u64) -> anyhow::Result<()> {
    positive(&format!("{label}_critical"), critical)?;
    if warning <= critical {
        bail!("{label} thresholds must satisfy warning > critical");
    }
    Ok(())
}

fn validate_pair(label: &str, warning: u64, critical: u64) -> anyhow::Result<()> {
    positive(&format!("{label}_warning"), warning)?;
    if warning >= critical {
        bail!("{label} thresholds must satisfy warning < critical");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checked_in_policy() -> AcsRuntimePolicy {
        checked_in_test_policy()
    }

    #[test]
    fn checked_in_policy_is_complete_and_valid() {
        checked_in_policy().validate().unwrap();
    }

    #[test]
    fn invalid_threshold_order_is_rejected() {
        let mut policy = checked_in_policy();
        policy.monitoring.stored_bytes_critical = policy.monitoring.stored_bytes_warning;
        assert!(policy.validate().is_err());
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let text = include_str!("../../../deploy/aerobag-cloud-policy.json").replace(
            "\"schema_version\": 3,",
            "\"schema_version\": 3, \"surprise\": true,",
        );
        assert!(serde_json::from_str::<AcsRuntimePolicy>(&text).is_err());
    }
}
