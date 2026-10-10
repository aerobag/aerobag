// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Station geography and observations have independent lifetimes. Queries use
//! a single directory, then compare source records without a merged-report cache.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

const MAX_STATIONS: usize = 65_536;
const MAX_REPORT_BYTES: usize = 16 * 1024;
const MAX_SOURCE_TEXT_BYTES: usize = 8 * 1024 * 1024;
const MAX_SNAPSHOT_BYTES: usize = 32 * 1024 * 1024;
const SPATIAL_CELL_DEGREES: f64 = 2.0;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct StationId(String);

impl StationId {
    pub fn new(value: &str) -> Result<Self, &'static str> {
        let value = value.trim().to_ascii_uppercase();
        if !(2..=16).contains(&value.len()) || !value.bytes().all(|c| c.is_ascii_alphanumeric()) {
            return Err("invalid weather station identifier");
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for StationId {
    type Error = &'static str;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(&value)
    }
}
impl From<StationId> for String {
    fn from(value: StationId) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeatherSource {
    Internet,
    Receiver,
}

/// Only exact station locations belong here, not an associated airport's ARP.
/// Ordering is an explicit metadata authority policy, never report recency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StationMetadataSource {
    CycleStationCatalog,
    Internet,
    InternetTaf,
    Receiver,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StationPosition {
    pub latitude: f64,
    pub longitude: f64,
}

impl StationPosition {
    fn validate(self) -> Result<(), &'static str> {
        if !(-90.0..=90.0).contains(&self.latitude) || !(-180.0..=180.0).contains(&self.longitude) {
            return Err("invalid weather station coordinates");
        }
        Ok(())
    }
    fn cell(self) -> (i16, i16) {
        (
            (self.latitude / SPATIAL_CELL_DEGREES).floor() as i16,
            (self.longitude / SPATIAL_CELL_DEGREES).floor() as i16,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StationMetadata {
    pub position: StationPosition,
    pub source: StationMetadataSource,
}

#[derive(Debug, Clone, Default)]
pub struct StationDirectory {
    stations: BTreeMap<StationId, StationMetadata>,
    spatial: BTreeMap<(i16, i16), BTreeSet<StationId>>,
}

impl StationDirectory {
    pub fn get(&self, id: &StationId) -> Option<&StationMetadata> {
        self.stations.get(id)
    }

    /// An observation may introduce an unknown station, but cannot relocate an
    /// existing station from the same authority. Corrections are explicit.
    pub fn discover(
        &mut self,
        id: StationId,
        metadata: StationMetadata,
    ) -> Result<bool, &'static str> {
        metadata.position.validate()?;
        if let Some(existing) = self.stations.get(&id) {
            if existing.source < metadata.source {
                return Ok(false);
            }
            if existing.source == metadata.source {
                if existing.position == metadata.position {
                    return Ok(false);
                }
                return Err("station location changed; metadata correction required");
            }
        }
        self.replace(id, metadata)
    }

    pub fn correct_location(
        &mut self,
        id: StationId,
        metadata: StationMetadata,
    ) -> Result<bool, &'static str> {
        metadata.position.validate()?;
        self.replace(id, metadata)
    }

    fn replace(&mut self, id: StationId, metadata: StationMetadata) -> Result<bool, &'static str> {
        if self.stations.get(&id) == Some(&metadata) {
            return Ok(false);
        }
        if self.stations.len() >= MAX_STATIONS && !self.stations.contains_key(&id) {
            return Err("station directory limit");
        }
        if let Some(previous) = self.stations.insert(id.clone(), metadata.clone()) {
            let cell = previous.position.cell();
            if let Some(ids) = self.spatial.get_mut(&cell) {
                ids.remove(&id);
                if ids.is_empty() {
                    self.spatial.remove(&cell);
                }
            }
        }
        self.spatial
            .entry(metadata.position.cell())
            .or_default()
            .insert(id);
        Ok(true)
    }

    /// West > east explicitly denotes an antimeridian-crossing viewport.
    pub fn in_bounds(
        &self,
        south: f64,
        west: f64,
        north: f64,
        east: f64,
    ) -> Result<Vec<&StationId>, &'static str> {
        StationPosition {
            latitude: south,
            longitude: west,
        }
        .validate()?;
        StationPosition {
            latitude: north,
            longitude: east,
        }
        .validate()?;
        if south > north {
            return Err("inverted latitude bounds");
        }
        let mut ids = Vec::new();
        let south_cell = (south / SPATIAL_CELL_DEGREES).floor() as i16;
        let north_cell = (north / SPATIAL_CELL_DEGREES).floor() as i16;
        for lat in south_cell..=north_cell {
            let ranges = if west <= east {
                vec![(west, east)]
            } else {
                vec![(west, 180.0), (-180.0, east)]
            };
            for (west_bound, east_bound) in ranges {
                let first = (west_bound / SPATIAL_CELL_DEGREES).floor() as i16;
                let last = (east_bound / SPATIAL_CELL_DEGREES).floor() as i16;
                for (_, candidates) in self.spatial.range((lat, first)..=(lat, last)) {
                    for id in candidates {
                        let position = self.stations[id].position;
                        if (south..=north).contains(&position.latitude)
                            && (west_bound..=east_bound).contains(&position.longitude)
                        {
                            ids.push(id);
                        }
                    }
                }
            }
        }
        ids.sort_unstable();
        ids.dedup();
        Ok(ids)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StationReportKind {
    Metar,
    Taf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportRevision {
    Original,
    Amended,
    Corrected,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StationReport {
    pub station: StationId,
    pub details: StationReportDetails,
    pub raw_text: String,
    pub report_time: Option<DateTime<Utc>>,
    pub received_at: DateTime<Utc>,
    pub revision: ReportRevision,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StationReportDetails {
    Metar {
        flight_category: Option<String>,
        cloud_symbol: Option<String>,
    },
    Taf,
}

impl StationReportDetails {
    fn unknown(kind: StationReportKind) -> Self {
        match kind {
            StationReportKind::Metar => Self::Metar {
                flight_category: None,
                cloud_symbol: None,
            },
            StationReportKind::Taf => Self::Taf,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportAge {
    Unknown,
    Past { age_ms: i64 },
    AheadOfClock { ahead_ms: i64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReportTiming {
    pub age: ReportAge,
    /// Preserve evidence of a discrepancy even after the local clock catches
    /// up. This does not identify which clock (or time convention) caused it.
    pub timestamp_ahead_of_receipt_ms: Option<i64>,
}

impl StationReport {
    pub fn kind(&self) -> StationReportKind {
        match self.details {
            StationReportDetails::Metar { .. } => StationReportKind::Metar,
            StationReportDetails::Taf => StationReportKind::Taf,
        }
    }

    pub fn flight_category(&self) -> Option<&str> {
        match &self.details {
            StationReportDetails::Metar {
                flight_category, ..
            } => flight_category.as_deref(),
            StationReportDetails::Taf => None,
        }
    }

    pub fn cloud_symbol(&self) -> Option<&str> {
        match &self.details {
            StationReportDetails::Metar { cloud_symbol, .. } => cloud_symbol.as_deref(),
            StationReportDetails::Taf => None,
        }
    }
    /// Only parse the identity/revision/time header here. Preserve the complete
    /// aviation text; container issue time is not the individual report time.
    pub fn from_receiver(
        kind: StationReportKind,
        report: &crate::receiver::TextReport,
        received_at: DateTime<Utc>,
    ) -> Result<Self, &'static str> {
        let station = StationId::new(
            report
                .station
                .as_deref()
                .ok_or("receiver report has no station identity")?,
        )?;
        let (mut words, mut revision) = report_header(&report.text, kind);
        if words.next() != Some(station.as_str()) {
            return Err("receiver weather station/text mismatch");
        }
        let time_word = words.next().ok_or("weather report timestamp missing")?;
        let digits = time_word
            .strip_suffix('Z')
            .filter(|digits| digits.len() == 6 && digits.bytes().all(|c| c.is_ascii_digit()))
            .ok_or("weather report timestamp malformed")?;
        let partial = crate::receiver::PartialUtc {
            day: digits[0..2].parse().map_err(|_| "weather report day")?,
            hour: digits[2..4].parse().map_err(|_| "weather report hour")?,
            minute: digits[4..6].parse().map_err(|_| "weather report minute")?,
        };
        let report_time = partial
            .resolve_nearest(received_at)
            .ok_or("weather report date cannot be resolved")?;
        if words.peek() == Some(&"COR") {
            revision = ReportRevision::Corrected;
        }
        let result = Self {
            station,
            details: StationReportDetails::unknown(kind),
            raw_text: report.text.clone(),
            report_time: Some(report_time),
            received_at,
            revision,
        };
        result.validate()?;
        Ok(result)
    }

    fn validate(&self) -> Result<(), &'static str> {
        if self.raw_text.is_empty() || self.raw_text.len() > MAX_REPORT_BYTES {
            return Err("weather report text size");
        }
        Ok(())
    }

    pub fn timing(&self, now: DateTime<Utc>) -> ReportTiming {
        let Some(reported) = self.report_time else {
            return ReportTiming {
                age: ReportAge::Unknown,
                timestamp_ahead_of_receipt_ms: None,
            };
        };
        let age_ms = now.signed_duration_since(reported).num_milliseconds();
        let ahead_ms = reported
            .signed_duration_since(self.received_at)
            .num_milliseconds();
        ReportTiming {
            age: if age_ms < 0 {
                ReportAge::AheadOfClock { ahead_ms: -age_ms }
            } else {
                ReportAge::Past { age_ms }
            },
            timestamp_ahead_of_receipt_ms: (ahead_ms > 0).then_some(ahead_ms),
        }
    }

    fn precedence(&self) -> (Option<DateTime<Utc>>, ReportRevision, &str) {
        // Reception order is deliberately absent. Equal-time, same-revision
        // conflicts use a deterministic tie break, not download opportunities.
        (self.report_time, self.revision, &self.raw_text)
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct StationReports {
    reports: BTreeMap<(StationId, StationReportKind), StationReport>,
    text_bytes: usize,
    metar_count: usize,
}

impl StationReports {
    pub(crate) fn values(&self) -> impl Iterator<Item = &StationReport> {
        self.reports.values()
    }
    pub fn get(&self, id: &StationId, kind: StationReportKind) -> Option<&StationReport> {
        self.reports.get(&(id.clone(), kind))
    }

    pub fn ingest(&mut self, report: StationReport) -> Result<bool, &'static str> {
        let update = self.prepare_batch([report])?;
        let changed = !update.changes.is_empty();
        update.commit();
        Ok(changed)
    }

    /// Prepare only changed rows, not a copy of the national dataset. Dropping
    /// the guard aborts; a durable owner can commit disk before publishing memory.
    pub(crate) fn prepare_batch(
        &mut self,
        reports: impl IntoIterator<Item = StationReport>,
    ) -> Result<ReportBatch<'_>, &'static str> {
        let mut changes = BTreeMap::<(StationId, StationReportKind), StationReport>::new();
        for report in reports {
            report.validate()?;
            let key = (report.station.clone(), report.kind());
            if changes
                .get(&key)
                .or_else(|| self.reports.get(&key))
                .is_none_or(|existing| existing.precedence() < report.precedence())
            {
                changes.insert(key, report);
            }
        }
        let mut text_bytes = self.text_bytes;
        let mut count = self.reports.len();
        let mut metar_count = self.metar_count;
        for (key, report) in &changes {
            if let Some(previous) = self.reports.get(key) {
                text_bytes -= previous.raw_text.len();
            } else {
                count += 1;
                metar_count += usize::from(report.kind() == StationReportKind::Metar);
            }
            text_bytes += report.raw_text.len();
        }
        if count > 2 * MAX_STATIONS {
            return Err("weather report count limit");
        }
        if text_bytes > MAX_SOURCE_TEXT_BYTES {
            return Err("weather source text size limit");
        }
        Ok(ReportBatch {
            target: self,
            changes,
            text_bytes,
            metar_count,
        })
    }

    pub fn remove(&mut self, id: &StationId, kind: StationReportKind) {
        if let Some(report) = self.reports.remove(&(id.clone(), kind)) {
            self.text_bytes -= report.raw_text.len();
            if kind == StationReportKind::Metar {
                self.metar_count -= 1;
            }
        }
    }
}

pub(crate) struct ReportBatch<'a> {
    target: &'a mut StationReports,
    changes: BTreeMap<(StationId, StationReportKind), StationReport>,
    text_bytes: usize,
    metar_count: usize,
}

impl ReportBatch<'_> {
    pub(crate) fn changes(&self) -> impl Iterator<Item = &StationReport> {
        self.changes.values()
    }

    pub(crate) fn commit(self) {
        self.target.reports.extend(self.changes);
        self.target.text_bytes = self.text_bytes;
        self.target.metar_count = self.metar_count;
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SelectedReport<'a> {
    pub source: WeatherSource,
    pub report: &'a StationReport,
    pub conflicting_same_time_report: bool,
    pub timing: ReportTiming,
}

/// No selected-record cache: callers borrow the two source indices.
#[derive(Clone, Copy)]
pub struct WeatherQuery<'a> {
    directory: &'a StationDirectory,
    internet: &'a StationReports,
    receiver: &'a StationReports,
}

impl<'a> WeatherQuery<'a> {
    pub fn station_position(&self, id: &str) -> Option<StationPosition> {
        self.directory
            .get(&StationId::new(id).ok()?)
            .map(|metadata| metadata.position)
    }

    pub fn in_bounds(&self, south: f64, west: f64, north: f64, east: f64) -> Vec<&'a StationId> {
        // Bounds come from the map projection, not from untrusted product data.
        self.directory
            .in_bounds(south, west, north, east)
            .expect("valid weather query bounds")
    }

    pub fn metar(&self, id: &str) -> Option<&'a StationReport> {
        self.select(&StationId::new(id).ok()?, StationReportKind::Metar)
            .map(|(_, report, _)| report)
    }

    pub fn taf(&self, id: &str) -> Option<&'a StationReport> {
        self.select(&StationId::new(id).ok()?, StationReportKind::Taf)
            .map(|(_, report, _)| report)
    }

    pub fn has_metars(&self) -> bool {
        self.internet.metar_count > 0 || self.receiver.metar_count > 0
    }

    pub fn has_reports(&self) -> bool {
        !self.internet.reports.is_empty() || !self.receiver.reports.is_empty()
    }

    pub fn report(
        &self,
        id: &StationId,
        kind: StationReportKind,
        now: DateTime<Utc>,
    ) -> Option<SelectedReport<'a>> {
        self.select(id, kind).map(
            |(source, report, conflicting_same_time_report)| SelectedReport {
                source,
                report,
                conflicting_same_time_report,
                timing: report.timing(now),
            },
        )
    }

    fn select(
        &self,
        id: &StationId,
        kind: StationReportKind,
    ) -> Option<(WeatherSource, &'a StationReport, bool)> {
        let candidates = [
            (WeatherSource::Internet, self.internet.get(id, kind)),
            (WeatherSource::Receiver, self.receiver.get(id, kind)),
        ];
        let mut best: Option<(WeatherSource, &'a StationReport, bool)> = None;
        for (source, report) in candidates {
            let Some(report) = report else {
                continue;
            };
            // Clock disagreement affects timing presentation, not visibility.
            // Neither receipt time nor "now" replaces the reported timestamp.
            match best.as_mut() {
                None => best = Some((source, report, false)),
                Some(previous) => {
                    let conflict = report.report_time == previous.1.report_time
                        && report.revision == previous.1.revision
                        && report.raw_text != previous.1.raw_text;
                    let ordering = (report.report_time, report.revision)
                        .cmp(&(previous.1.report_time, previous.1.revision));
                    if ordering.is_gt() {
                        *previous = (source, report, false);
                    } else {
                        previous.2 = conflict;
                    }
                }
            }
        }
        best
    }
}

/// A local cache envelope, not the cloud or internet publication/delta format.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot<R> {
    schema_version: u32,
    stations: Vec<(StationId, StationMetadata)>,
    internet: Vec<R>,
    receiver: Vec<R>,
    internet_products: BTreeMap<StationReportKind, WeatherProductMetadata>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeatherProductMetadata {
    pub version_label: String,
    pub generated_at_utc: Option<DateTime<Utc>>,
}

#[derive(Default)]
pub struct StationWeather {
    directory: StationDirectory,
    internet: StationReports,
    receiver: StationReports,
    internet_products: BTreeMap<StationReportKind, WeatherProductMetadata>,
}

impl StationWeather {
    pub fn internet_product(&self, kind: StationReportKind) -> Option<&WeatherProductMetadata> {
        self.internet_products.get(&kind)
    }

    pub fn clear_internet_product(&mut self, kind: StationReportKind) {
        let ids = self
            .internet
            .reports
            .keys()
            .filter(|(_, k)| *k == kind)
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in ids {
            self.internet.remove(&id, kind);
        }
        self.internet_products.remove(&kind);
    }

    pub fn install_metars(
        &mut self,
        payload: crate::MetarProductPayload,
        received_at: DateTime<Utc>,
    ) -> Result<(), &'static str> {
        let metadata = WeatherProductMetadata {
            version_label: payload.version_label,
            generated_at_utc: payload.generated_at_utc,
        };
        let mut reports = Vec::with_capacity(payload.metars_by_station.len());
        for (key, record) in payload.metars_by_station {
            let station = StationId::new(&record.station_id)?;
            if station != StationId::new(&key)? {
                return Err("METAR key/station mismatch");
            }
            let position = StationPosition {
                latitude: record.latitude,
                longitude: record.longitude,
            };
            let report_time = record
                .observed_at_utc
                .as_deref()
                .and_then(crate::freshness::parse_utc_instant);
            let revision = report_revision(&record.raw_text, StationReportKind::Metar);
            reports.push((
                StationReport {
                    station,
                    details: StationReportDetails::Metar {
                        flight_category: record.flight_category,
                        cloud_symbol: record.clouds.and_then(|clouds| clouds.symbol),
                    },
                    raw_text: record.raw_text,
                    report_time,
                    received_at,
                    revision,
                },
                position,
            ));
        }
        self.install_product(
            StationReportKind::Metar,
            StationMetadataSource::Internet,
            metadata,
            reports,
        )
    }

    pub fn install_tafs(
        &mut self,
        payload: crate::TafProductPayload,
        received_at: DateTime<Utc>,
    ) -> Result<(), &'static str> {
        let metadata = WeatherProductMetadata {
            version_label: payload.version_label,
            generated_at_utc: payload.generated_at_utc,
        };
        let mut reports = Vec::with_capacity(payload.tafs_by_station.len());
        for (key, record) in payload.tafs_by_station {
            let station = StationId::new(&record.station_id)?;
            if station != StationId::new(&key)? {
                return Err("TAF key/station mismatch");
            }
            let position = StationPosition {
                latitude: record.latitude,
                longitude: record.longitude,
            };
            let report_time = record
                .issued_at_utc
                .as_deref()
                .and_then(crate::freshness::parse_utc_instant);
            let revision = report_revision(&record.raw_text, StationReportKind::Taf);
            reports.push((
                StationReport {
                    station,
                    details: StationReportDetails::Taf,
                    raw_text: record.raw_text,
                    report_time,
                    received_at,
                    revision,
                },
                position,
            ));
        }
        self.install_product(
            StationReportKind::Taf,
            StationMetadataSource::InternetTaf,
            metadata,
            reports,
        )
    }

    fn install_product(
        &mut self,
        kind: StationReportKind,
        source: StationMetadataSource,
        metadata: WeatherProductMetadata,
        reports: Vec<(StationReport, StationPosition)>,
    ) -> Result<(), &'static str> {
        let mut directory = self.directory.clone();
        let mut replacement = StationReports::default();
        for (report, position) in reports {
            position.validate()?;
            let station = report.station.clone();
            if replacement.get(&station, kind).is_some() {
                return Err("duplicate weather station");
            }
            if directory
                .get(&station)
                .is_none_or(|existing| existing.source >= source)
            {
                directory.correct_location(station, StationMetadata { position, source })?;
            }
            replacement.ingest(report)?;
        }
        let retained_bytes = self
            .internet
            .reports
            .values()
            .filter(|report| report.kind() != kind)
            .map(|report| report.raw_text.len())
            .sum::<usize>();
        if retained_bytes + replacement.text_bytes > MAX_SOURCE_TEXT_BYTES {
            return Err("weather source text size limit");
        }
        self.clear_internet_product(kind);
        self.internet.text_bytes += replacement.text_bytes;
        self.internet.metar_count += replacement.metar_count;
        self.internet.reports.append(&mut replacement.reports);
        self.internet_products.insert(kind, metadata);
        self.directory = directory;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn test_products(
        metars: Option<&crate::MetarProductPayload>,
        tafs: Option<&crate::TafProductPayload>,
    ) -> Self {
        let mut state = Self::default();
        if let Some(payload) = metars {
            state
                .install_metars(payload.clone(), DateTime::UNIX_EPOCH)
                .unwrap();
        }
        if let Some(payload) = tafs {
            state
                .install_tafs(payload.clone(), DateTime::UNIX_EPOCH)
                .unwrap();
        }
        state
    }
    pub fn directory(&self) -> &StationDirectory {
        &self.directory
    }

    pub fn directory_mut(&mut self) -> &mut StationDirectory {
        &mut self.directory
    }

    fn source_mut(&mut self, source: WeatherSource) -> &mut StationReports {
        match source {
            WeatherSource::Internet => &mut self.internet,
            WeatherSource::Receiver => &mut self.receiver,
        }
    }

    pub fn ingest(
        &mut self,
        source: WeatherSource,
        report: StationReport,
    ) -> Result<bool, &'static str> {
        self.source_mut(source).ingest(report)
    }

    pub fn ingest_batch(
        &mut self,
        source: WeatherSource,
        reports: impl IntoIterator<Item = StationReport>,
    ) -> Result<bool, &'static str> {
        let batch = self.source_mut(source).prepare_batch(reports)?;
        let changed = batch.changes().next().is_some();
        batch.commit();
        Ok(changed)
    }

    pub fn remove(&mut self, source: WeatherSource, id: &StationId, kind: StationReportKind) {
        self.source_mut(source).remove(id, kind);
    }

    /// Replace one authoritative source snapshot atomically. Missing records
    /// disappear only from that source, never from station metadata or another
    /// source's observations. Do not use for partial receiver coverage updates.
    pub fn replace_source(
        &mut self,
        source: WeatherSource,
        reports: impl IntoIterator<Item = StationReport>,
    ) -> Result<(), &'static str> {
        let mut replacement = StationReports::default();
        for report in reports {
            let key = (report.station.clone(), report.kind());
            if replacement.reports.contains_key(&key) {
                return Err("duplicate report in weather source snapshot");
            }
            replacement.ingest(report)?;
        }
        *self.source_mut(source) = replacement;
        if source == WeatherSource::Internet {
            self.internet_products.clear();
        }
        Ok(())
    }

    pub fn query(&self) -> WeatherQuery<'_> {
        WeatherQuery {
            directory: &self.directory,
            internet: &self.internet,
            receiver: &self.receiver,
        }
    }

    pub fn snapshot(&self) -> Result<Vec<u8>, &'static str> {
        let snapshot = Snapshot {
            schema_version: 2,
            stations: self
                .directory
                .stations
                .iter()
                .map(|(id, meta)| (id.clone(), meta.clone()))
                .collect(),
            internet: self.internet.values().collect(),
            receiver: self.receiver.values().collect(),
            internet_products: self.internet_products.clone(),
        };
        let bytes = serde_json::to_vec(&snapshot).map_err(|_| "weather cache encoding")?;
        if bytes.len() > MAX_SNAPSHOT_BYTES {
            return Err("weather cache size limit");
        }
        Ok(bytes)
    }

    pub fn restore(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() > MAX_SNAPSHOT_BYTES {
            return Err("weather cache size limit");
        }
        let snapshot: Snapshot<StationReport> =
            serde_json::from_slice(bytes).map_err(|_| "invalid weather cache")?;
        if snapshot.schema_version != 2 {
            return Err("unsupported weather cache version");
        }
        let mut state = Self::default();
        for (id, metadata) in snapshot.stations {
            if state.directory.get(&id).is_some() {
                return Err("duplicate station in weather cache");
            }
            state.directory.discover(id, metadata)?;
        }
        state.replace_source(WeatherSource::Internet, snapshot.internet)?;
        state.replace_source(WeatherSource::Receiver, snapshot.receiver)?;
        state.internet_products = snapshot.internet_products;
        Ok(state)
    }
}

fn report_header(
    text: &str,
    kind: StationReportKind,
) -> (
    std::iter::Peekable<std::str::SplitAsciiWhitespace<'_>>,
    ReportRevision,
) {
    let mut words = text.split_ascii_whitespace().peekable();
    let mut revision = ReportRevision::Original;
    if let Some(prefix) = words.peek().copied() {
        let is_prefix = match kind {
            StationReportKind::Metar => matches!(prefix, "METAR" | "SPECI"),
            StationReportKind::Taf => matches!(prefix, "TAF" | "TAF.AMD" | "TAF.COR"),
        };
        if is_prefix {
            words.next();
            revision = match prefix {
                "TAF.AMD" => ReportRevision::Amended,
                "TAF.COR" => ReportRevision::Corrected,
                _ => ReportRevision::Original,
            };
        }
    }
    if matches!(words.peek(), Some(&"AMD") | Some(&"COR")) {
        revision = if words.next() == Some("COR") {
            ReportRevision::Corrected
        } else {
            ReportRevision::Amended
        };
    }
    (words, revision)
}

fn report_revision(raw_text: &str, kind: StationReportKind) -> ReportRevision {
    let (mut words, revision) = report_header(raw_text, kind);
    words.next(); // Station.
    words.next(); // Observation/issue time.
    if words.next() == Some("COR") {
        ReportRevision::Corrected
    } else {
        revision
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn time(minute: u32) -> DateTime<Utc> {
        chrono::NaiveDate::from_ymd_opt(2026, 10, 6)
            .unwrap()
            .and_hms_opt(1, minute, 0)
            .unwrap()
            .and_utc()
    }
    fn report(minute: u32) -> StationReport {
        StationReport {
            station: StationId::new("PASS").unwrap(),
            details: StationReportDetails::unknown(StationReportKind::Metar),
            raw_text: format!("METAR PASS 0601{minute:02}Z"),
            report_time: Some(time(minute)),
            received_at: time(55),
            revision: ReportRevision::Original,
        }
    }

    #[test]
    fn feed_only_station_survives_receiver_update_and_offline_restart() {
        let mut state = StationWeather::default();
        let id = report(10).station;
        state
            .directory
            .discover(
                id.clone(),
                StationMetadata {
                    position: StationPosition {
                        latitude: 47.4,
                        longitude: -121.4,
                    },
                    source: StationMetadataSource::Internet,
                },
            )
            .unwrap();
        state.internet.ingest(report(10)).unwrap();
        state.receiver.ingest(report(40)).unwrap();
        // No cycle catalog, airport alias, network connection, or merged record.
        let restored = StationWeather::restore(&state.snapshot().unwrap()).unwrap();
        let ids = restored
            .directory
            .in_bounds(47.0, -122.0, 48.0, -121.0)
            .unwrap();
        assert_eq!(ids, vec![&id]);
        let map_report = restored
            .query()
            .report(ids[0], StationReportKind::Metar, time(58))
            .unwrap();
        let detail = restored
            .query()
            .report(&id, StationReportKind::Metar, time(58))
            .unwrap();
        assert_eq!(map_report.report, detail.report);
        assert_eq!(detail.source, WeatherSource::Receiver);
        assert_eq!(
            (time(58) - detail.report.report_time.unwrap()).num_minutes(),
            18
        );
        assert_eq!(
            restored
                .internet
                .get(&id, StationReportKind::Metar)
                .unwrap()
                .report_time,
            Some(time(10))
        );
    }

    #[test]
    fn failed_batch_cannot_partially_replace_weather_or_consume_storage_budget() {
        let mut state = StationWeather::default();
        state.ingest(WeatherSource::Receiver, report(10)).unwrap();
        let before = state.snapshot().unwrap();
        let mut invalid = report(30);
        invalid.raw_text.clear();
        assert!(state
            .ingest_batch(WeatherSource::Receiver, [report(20), invalid])
            .is_err());
        assert_eq!(state.snapshot().unwrap(), before);
        assert!(state
            .ingest_batch(WeatherSource::Receiver, [report(20), report(15)])
            .unwrap());
        assert_eq!(state.query().metar("PASS"), Some(&report(20)));
        assert!(!state
            .ingest_batch(WeatherSource::Receiver, [report(20), report(15)])
            .unwrap());
    }

    #[test]
    fn internet_install_is_atomic_and_never_replaces_receiver_reports_or_learned_locations() {
        let mut state = StationWeather::default();
        let metars = |latitude| crate::MetarProductPayload {
            schema_version: 3,
            version_label: "metars-v1".into(),
            generated_at_utc: Some(time(10)),
            observed_at_utc: None,
            metar_count: None,
            metars_by_station: std::collections::HashMap::from([(
                "PASS".into(),
                crate::MetarRecord {
                    station_id: "PASS".into(),
                    latitude,
                    longitude: -121.4,
                    raw_text: "METAR PASS 060110Z 00000KT 10SM CLR 10/08 A3000".into(),
                    observed_at_utc: Some(time(10).to_rfc3339()),
                    flight_category: Some("VFR".into()),
                    clouds: None,
                },
            )]),
        };
        state.install_metars(metars(47.4), time(20)).unwrap();
        state.ingest(WeatherSource::Receiver, report(40)).unwrap();
        let before = state.snapshot().unwrap();
        assert!(state.install_metars(metars(91.0), time(20)).is_err());
        assert_eq!(
            state.snapshot().unwrap(),
            before,
            "invalid products do not partially install"
        );
        let tafs = crate::TafProductPayload {
            schema_version: 1,
            version_label: "tafs-v1".into(),
            generated_at_utc: Some(time(20)),
            taf_count: None,
            tafs_by_station: std::collections::HashMap::from([(
                "PASS".into(),
                crate::TafRecord {
                    station_id: "PASS".into(),
                    latitude: 0.0,
                    longitude: 0.0,
                    raw_text: "TAF PASS 060120Z 0601/0701 00000KT P6SM SCT020".into(),
                    issued_at_utc: Some(time(20).to_rfc3339()),
                },
            )]),
        };
        state.install_tafs(tafs, time(30)).unwrap();
        assert_eq!(
            state.query().station_position("PASS").unwrap().latitude,
            47.4,
            "TAF cannot relocate a METAR station"
        );
        let restored = StationWeather::restore(&state.snapshot().unwrap()).unwrap();
        assert_eq!(
            restored
                .internet_product(StationReportKind::Metar)
                .unwrap()
                .version_label,
            "metars-v1"
        );
        assert_eq!(
            restored
                .internet_product(StationReportKind::Taf)
                .unwrap()
                .version_label,
            "tafs-v1"
        );
        state.clear_internet_product(StationReportKind::Metar);
        assert_eq!(
            state.query().metar("PASS").unwrap().report_time,
            Some(time(40))
        );
        assert!(state.query().taf("PASS").is_some());
        state.remove(
            WeatherSource::Receiver,
            &StationId::new("PASS").unwrap(),
            StationReportKind::Metar,
        );
        assert!(!state.query().has_metars());
        assert!(state.query().has_reports());
        assert_eq!(
            state.query().station_position("PASS"),
            restored.query().station_position("PASS")
        );
    }

    #[test]
    fn raw_report_without_station_coordinates_is_readable_but_not_placed_on_map() {
        let mut state = StationWeather::default();
        state.ingest(WeatherSource::Receiver, report(40)).unwrap();
        let restored = StationWeather::restore(&state.snapshot().unwrap()).unwrap();
        assert!(restored.query().metar("PASS").is_some());
        assert!(restored.query().station_position("PASS").is_none());
        assert!(restored
            .query()
            .in_bounds(-90.0, -180.0, 90.0, 180.0)
            .is_empty());
    }

    #[test]
    fn internet_and_receiver_use_the_same_revision_header_rules() {
        for (kind, raw, revision) in [
            (
                StationReportKind::Metar,
                "METAR COR PASS 060100Z 00000KT 10SM CLR",
                ReportRevision::Corrected,
            ),
            (
                StationReportKind::Metar,
                "SPECI PASS 060100Z COR 00000KT 10SM CLR",
                ReportRevision::Corrected,
            ),
            (
                StationReportKind::Taf,
                "TAF AMD PASS 060100Z 0601/0701 00000KT P6SM SCT020",
                ReportRevision::Amended,
            ),
            (
                StationReportKind::Taf,
                "TAF.COR PASS 060100Z 0601/0701 00000KT P6SM SCT020",
                ReportRevision::Corrected,
            ),
        ] {
            let receiver = StationReport::from_receiver(
                kind,
                &crate::receiver::TextReport {
                    station: Some("PASS".into()),
                    text: raw.into(),
                    notam_identifier: None,
                    record_type_raw: None,
                },
                time(20),
            )
            .unwrap();
            assert_eq!(receiver.revision, revision);
            assert_eq!(report_revision(raw, kind), revision);
        }
    }

    #[test]
    fn receive_order_does_not_change_freshness_and_source_removal_is_local() {
        let mut state = StationWeather::default();
        let id = report(1).station;
        state.receiver.ingest(report(40)).unwrap();
        assert!(!state.receiver.ingest(report(20)).unwrap());
        state.internet.ingest(report(30)).unwrap();
        assert_eq!(
            state
                .query()
                .report(&id, StationReportKind::Metar, time(55))
                .unwrap()
                .source,
            WeatherSource::Receiver
        );
        state.internet.remove(&id, StationReportKind::Metar);
        assert_eq!(
            state
                .query()
                .report(&id, StationReportKind::Metar, time(55))
                .unwrap()
                .source,
            WeatherSource::Receiver
        );
        state.internet.ingest(report(50)).unwrap();
        assert_eq!(
            state
                .query()
                .report(&id, StationReportKind::Metar, time(55))
                .unwrap()
                .source,
            WeatherSource::Internet
        );
    }

    #[test]
    fn unknown_report_times_do_not_win_by_download_time() {
        let mut state = StationWeather::default();
        let mut unknown = report(1);
        let id = unknown.station.clone();
        unknown.report_time = None;
        state.receiver.ingest(unknown).unwrap();
        state.internet.ingest(report(10)).unwrap();
        assert_eq!(
            state
                .query()
                .report(&id, StationReportKind::Metar, time(55))
                .unwrap()
                .source,
            WeatherSource::Internet
        );
        let selected = state
            .query()
            .report(&id, StationReportKind::Metar, time(55))
            .unwrap();
        assert_eq!(
            selected.timing.age,
            ReportAge::Past {
                age_ms: 45 * 60 * 1000
            }
        );
        state.remove(WeatherSource::Internet, &id, StationReportKind::Metar);
        assert_eq!(
            state
                .query()
                .report(&id, StationReportKind::Metar, time(55))
                .unwrap()
                .timing
                .age,
            ReportAge::Unknown
        );
    }

    #[test]
    fn station_metadata_is_not_chosen_by_weather_recency() {
        let mut directory = StationDirectory::default();
        let id = report(1).station;
        let original = StationMetadata {
            position: StationPosition {
                latitude: 47.4,
                longitude: -121.4,
            },
            source: StationMetadataSource::Internet,
        };
        directory.discover(id.clone(), original.clone()).unwrap();
        let moved = StationMetadata {
            position: StationPosition {
                latitude: 0.0,
                longitude: 0.0,
            },
            ..original.clone()
        };
        assert!(directory.discover(id.clone(), moved.clone()).is_err());
        assert_eq!(directory.get(&id), Some(&original));
        directory.correct_location(id.clone(), moved).unwrap();
        assert!(directory
            .in_bounds(47.0, -122.0, 48.0, -121.0)
            .unwrap()
            .is_empty());
        assert_eq!(
            directory.in_bounds(-1.0, -1.0, 1.0, 1.0).unwrap(),
            vec![&id]
        );
    }

    #[test]
    fn spatial_query_wraps_dateline_and_deduplicates_station_identity() {
        let mut directory = StationDirectory::default();
        for (ident, longitude) in [("EAST", 179.0), ("WEST", -179.0), ("AWAY", 0.0)] {
            directory
                .discover(
                    StationId::new(ident).unwrap(),
                    StationMetadata {
                        position: StationPosition {
                            latitude: 0.0,
                            longitude,
                        },
                        source: StationMetadataSource::Internet,
                    },
                )
                .unwrap();
        }
        let ids = directory.in_bounds(-1.0, 178.0, 1.0, -178.0).unwrap();
        assert_eq!(
            ids.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
            ["EAST", "WEST"]
        );
        assert!(directory.in_bounds(f64::NAN, 0.0, 1.0, 1.0).is_err());
    }

    #[test]
    fn corrections_win_and_same_time_disagreement_is_explicit() {
        let mut state = StationWeather::default();
        let id = report(10).station;
        state.internet.ingest(report(10)).unwrap();
        let mut corrected = report(10);
        corrected.revision = ReportRevision::Corrected;
        corrected.raw_text += " COR";
        state.receiver.ingest(corrected).unwrap();
        assert_eq!(
            state
                .query()
                .report(&id, StationReportKind::Metar, time(55))
                .unwrap()
                .source,
            WeatherSource::Receiver
        );
        let mut other = report(10);
        other.revision = ReportRevision::Corrected;
        other.raw_text += " COR DIFFERENT";
        state.internet.ingest(other).unwrap();
        assert!(
            state
                .query()
                .report(&id, StationReportKind::Metar, time(55))
                .unwrap()
                .conflicting_same_time_report
        );
    }

    #[test]
    fn receiver_report_uses_its_header_not_container_or_receipt_time() {
        let raw = crate::receiver::TextReport {
            text: "METAR PASS 060110Z AUTO 00000KT 10SM CLR 10/08 A3000=".into(),
            station: Some("PASS".into()),
            notam_identifier: None,
            record_type_raw: None,
        };
        let report =
            StationReport::from_receiver(StationReportKind::Metar, &raw, time(55)).unwrap();
        assert_eq!(report.report_time, Some(time(10)));
        assert_eq!(report.raw_text, raw.text);
        let mut wrong = raw.clone();
        wrong.station = Some("OTHER".into());
        assert!(StationReport::from_receiver(StationReportKind::Metar, &wrong, time(55)).is_err());
        wrong = raw.clone();
        wrong.text = "TAF.COR PASS 060110Z 0601/0701 00000KT P6SM SKC=".into();
        assert_eq!(
            StationReport::from_receiver(StationReportKind::Taf, &wrong, time(55))
                .unwrap()
                .revision,
            ReportRevision::Corrected
        );
    }

    #[test]
    fn future_dated_reports_are_readable_immediately_and_survive_offline_restart() {
        for ahead_seconds in [22, 198] {
            for kind in [StationReportKind::Metar, StationReportKind::Taf] {
                let observed = time(40);
                let received = observed - chrono::Duration::seconds(ahead_seconds);
                let text = match kind {
                    StationReportKind::Metar => "METAR PASS 060140Z 00000KT 10SM CLR 10/08 A3000=",
                    StationReportKind::Taf => "TAF PASS 060140Z 0602/0702 00000KT P6SM SKC=",
                };
                let raw = crate::receiver::TextReport {
                    text: text.into(),
                    station: Some("PASS".into()),
                    notam_identifier: None,
                    record_type_raw: None,
                };
                let record = StationReport::from_receiver(kind, &raw, received)
                    .expect("clock discrepancy must not discard readable weather");
                let id = record.station.clone();
                let mut state = StationWeather::default();
                let mut older = report(10);
                older.details = StationReportDetails::unknown(kind);
                state.ingest(WeatherSource::Internet, older).unwrap();
                state.ingest(WeatherSource::Receiver, record).unwrap();
                let restored = StationWeather::restore(&state.snapshot().unwrap()).unwrap();
                for state in [&state, &restored] {
                    for now in [received, observed, observed + chrono::Duration::minutes(1)] {
                        let selected = state
                            .query()
                            .report(&id, kind, now)
                            .expect("no timer gate on weather visibility");
                        assert_eq!(selected.source, WeatherSource::Receiver);
                        assert_eq!(selected.report.raw_text, raw.text);
                        assert_eq!(selected.report.report_time, Some(observed));
                        assert_eq!(selected.report.received_at, received);
                        assert_eq!(
                            selected.timing.timestamp_ahead_of_receipt_ms,
                            Some(ahead_seconds * 1000)
                        );
                        assert_eq!(
                            selected.timing.age,
                            if now < observed {
                                ReportAge::AheadOfClock {
                                    ahead_ms: ahead_seconds * 1000,
                                }
                            } else {
                                ReportAge::Past {
                                    age_ms: (now - observed).num_milliseconds(),
                                }
                            }
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn future_dated_report_remains_available_after_clock_regression() {
        let mut state = StationWeather::default();
        let record = report(40);
        let id = record.station.clone();
        state
            .ingest(WeatherSource::Internet, record.clone())
            .unwrap();
        let selected = state
            .query()
            .report(&id, StationReportKind::Metar, time(39))
            .expect("a backwards clock step must not hide stored weather");
        assert_eq!(selected.report, &record);
        assert_eq!(
            selected.timing.age,
            ReportAge::AheadOfClock { ahead_ms: 60_000 }
        );
        assert_eq!(selected.timing.timestamp_ahead_of_receipt_ms, None);
    }

    #[test]
    fn future_dated_report_keeps_next_year_and_still_rejects_malformed_time() {
        let received = DateTime::parse_from_rfc3339("2026-12-31T23:59:38Z")
            .unwrap()
            .to_utc();
        let mut raw = crate::receiver::TextReport {
            text: "METAR PASS 010000Z 00000KT 10SM CLR 10/08 A3000=".into(),
            station: Some("PASS".into()),
            notam_identifier: None,
            record_type_raw: None,
        };
        let normalized =
            StationReport::from_receiver(StationReportKind::Metar, &raw, received).unwrap();
        assert_eq!(
            normalized.report_time,
            Some(
                DateTime::parse_from_rfc3339("2027-01-01T00:00:00Z")
                    .unwrap()
                    .to_utc()
            )
        );
        assert_eq!(
            normalized.timing(received).age,
            ReportAge::AheadOfClock { ahead_ms: 22_000 }
        );
        for timestamp in ["000000Z", "012400Z", "010060Z", "330000Z", "banana"] {
            raw.text = format!("METAR PASS {timestamp} 00000KT 10SM CLR=");
            assert!(
                StationReport::from_receiver(StationReportKind::Metar, &raw, received).is_err()
            );
        }
    }

    #[test]
    fn full_snapshot_replacement_and_deltas_are_source_local() {
        let mut state = StationWeather::default();
        let id = report(10).station;
        state.ingest(WeatherSource::Internet, report(10)).unwrap();
        state.ingest(WeatherSource::Receiver, report(30)).unwrap();
        state
            .replace_source(WeatherSource::Internet, [report(20)])
            .unwrap();
        assert_eq!(
            state
                .query()
                .report(&id, StationReportKind::Metar, time(55))
                .unwrap()
                .source,
            WeatherSource::Receiver
        );
        state.replace_source(WeatherSource::Internet, []).unwrap();
        assert_eq!(
            state
                .query()
                .report(&id, StationReportKind::Metar, time(55))
                .unwrap()
                .source,
            WeatherSource::Receiver
        );
        state.ingest(WeatherSource::Internet, report(40)).unwrap();
        state.remove(WeatherSource::Receiver, &id, StationReportKind::Metar);
        assert_eq!(
            state
                .query()
                .report(&id, StationReportKind::Metar, time(55))
                .unwrap()
                .source,
            WeatherSource::Internet
        );
        let before = state.snapshot().unwrap();
        assert!(
            state
                .replace_source(WeatherSource::Internet, [report(41), report(42)])
                .is_err(),
            "duplicate keys must reject the whole authoritative snapshot"
        );
        assert_eq!(state.snapshot().unwrap(), before);
        let mut invalid = report(59);
        invalid.station = StationId::new("INVALID").unwrap();
        invalid.raw_text.clear();
        assert!(state
            .replace_source(WeatherSource::Internet, [report(41), invalid])
            .is_err());
        assert_eq!(state.snapshot().unwrap(), before);
    }

    #[test]
    fn source_memory_limit_is_transactional_and_removal_reclaims_capacity() {
        let mut state = StationWeather::default();
        let full_text = "x".repeat(MAX_REPORT_BYTES);
        for index in 0..(MAX_SOURCE_TEXT_BYTES / MAX_REPORT_BYTES) {
            let mut record = report(10);
            record.station = StationId::new(&format!("S{index}")).unwrap();
            record.raw_text = full_text.clone();
            state.ingest(WeatherSource::Receiver, record).unwrap();
        }
        assert!(state.ingest(WeatherSource::Receiver, report(20)).is_err());
        assert!(state
            .query()
            .report(&report(20).station, StationReportKind::Metar, time(55))
            .is_none());
        state.remove(
            WeatherSource::Receiver,
            &StationId::new("S0").unwrap(),
            StationReportKind::Metar,
        );
        assert!(state.ingest(WeatherSource::Receiver, report(20)).unwrap());
        assert!(
            state.ingest(WeatherSource::Internet, report(30)).unwrap(),
            "source budgets are independent"
        );
    }

    #[test]
    fn cache_does_not_silently_resolve_duplicate_rows() {
        let snapshot = Snapshot {
            schema_version: 2,
            stations: Vec::new(),
            internet: vec![report(10), report(20)],
            receiver: Vec::new(),
            internet_products: BTreeMap::new(),
        };
        assert!(StationWeather::restore(&serde_json::to_vec(&snapshot).unwrap()).is_err());
    }
}
