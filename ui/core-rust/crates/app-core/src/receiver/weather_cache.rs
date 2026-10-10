// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Latest receiver reports, independent of the Internet product/delta cache.
//! All IO runs on the receiver owner, never under a session or mailbox lock.

use crate::weather_sources::{StationReport, StationReportKind, StationReports};
use rusqlite::{params, Connection};
use std::{fs, path::Path};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub(super) struct WeatherCache {
    db: Connection,
    reports: StationReports,
    _lock: fs::File,
}

impl WeatherCache {
    pub fn open(root: &Path) -> Result<Self> {
        let mut directory = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            directory.mode(0o700);
        }
        match directory.create(root) {
            Ok(()) => fs::File::open(root.parent().ok_or("Missing cache parent")?)?.sync_all()?,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
        let metadata = fs::symlink_metadata(root)?;
        if !metadata.is_dir() {
            return Err("Receiver directory is not a directory".into());
        }
        super::capture_files::check_private(&metadata)?;
        let private_file = |name: &str| -> Result<fs::File> {
            let path = root.join(name);
            if let Ok(metadata) = fs::symlink_metadata(&path) {
                if !metadata.is_file() {
                    return Err("Invalid receiver cache file".into());
                }
                super::capture_files::check_private(&metadata)?;
            }
            Ok(super::capture_files::private_options()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(path)?)
        };
        let lock = private_file("weather.lock")?;
        fs2::FileExt::try_lock_exclusive(&lock)?;
        let file = private_file("weather.sqlite")?;
        if file.metadata()?.len() > 64 * 1024 * 1024 {
            return Err("Receiver weather cache exceeds size limit".into());
        }
        file.sync_all()?;
        fs::File::open(root)?.sync_all()?;
        let mut db = Connection::open(root.join("weather.sqlite"))?;
        // One owner and no concurrent readers: bounded rollback journal, not an
        // indefinitely growing WAL. One transaction per received weather batch.
        db.execute_batch(
            "PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL;
            PRAGMA trusted_schema=OFF;",
        )?;
        let page_size: u64 = db.pragma_query_value(None, "page_size", |row| row.get(0))?;
        db.pragma_update(None, "max_page_count", 64 * 1024 * 1024 / page_size)?;
        let version: i64 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
        match version {
            0 => {
                let transaction = db.transaction()?;
                transaction.execute_batch(
                    "CREATE TABLE reports (
                    station TEXT NOT NULL, kind TEXT NOT NULL,
                    body TEXT NOT NULL CHECK(length(body) <= 131072),
                    PRIMARY KEY(station, kind)) WITHOUT ROWID;
                    PRAGMA user_version=1;",
                )?;
                transaction.commit()?;
            }
            1 => {}
            _ => return Err("Unsupported receiver weather cache version".into()),
        }
        let mut reports = StationReports::default();
        {
            let mut statement = db.prepare("SELECT station, kind, body FROM reports")?;
            let mut rows = statement.query([])?;
            while let Some(row) = rows.next()? {
                let station: String = row.get(0)?;
                let kind: String = row.get(1)?;
                let text = row.get_ref(2)?.as_str()?;
                if text.len() > 131072 {
                    return Err("Oversized receiver weather cache row".into());
                }
                let report: StationReport = serde_json::from_str(text)?;
                if report.station.as_str() != station || kind_key(report.kind()) != kind {
                    return Err("Receiver weather cache key mismatch".into());
                }
                if !reports.ingest(report)? {
                    return Err("Duplicate receiver weather cache row".into());
                }
            }
        }
        Ok(Self {
            db,
            reports,
            _lock: lock,
        })
    }

    pub fn reports(&self) -> impl Iterator<Item = &StationReport> {
        self.reports.values()
    }

    pub fn ingest(&mut self, reports: Vec<StationReport>) -> Result<Vec<StationReport>> {
        let batch = self.reports.prepare_batch(reports)?;
        let changed = batch.changes().cloned().collect::<Vec<_>>();
        if changed.is_empty() {
            return Ok(changed);
        }
        let transaction = self.db.transaction()?;
        {
            let mut write = transaction.prepare_cached(
                "INSERT INTO reports (station, kind, body)
                VALUES (?1, ?2, ?3) ON CONFLICT(station, kind) DO UPDATE SET body=excluded.body",
            )?;
            for report in &changed {
                write.execute(params![
                    report.station.as_str(),
                    kind_key(report.kind()),
                    serde_json::to_string(report)?
                ])?;
            }
        }
        transaction.commit()?;
        // Memory and UI cannot see an update whose durable commit failed.
        batch.commit();
        Ok(changed)
    }
}

fn kind_key(kind: StationReportKind) -> &'static str {
    match kind {
        StationReportKind::Metar => "metar",
        StationReportKind::Taf => "taf",
    }
}

#[cfg(test)]
mod tests {
    use super::super::{test_support::Temp, TextReport};
    use super::*;
    use chrono::{TimeZone, Utc};

    fn report(station: &str, minute: u8) -> StationReport {
        StationReport::from_receiver(
            StationReportKind::Metar,
            &TextReport {
                station: Some(station.into()),
                text: format!("{station} 1012{minute:02}Z 00000KT 10SM CLR 10/05 A3000"),
                notam_identifier: None,
                record_type_raw: None,
            },
            Utc.with_ymd_and_hms(2026, 10, 10, 12, 0, 0).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn restart_restores_latest_reports_without_changing_receipt_or_future_timestamps() {
        let root = Temp::new();
        let mut cache = WeatherCache::open(root.path()).unwrap();
        assert_eq!(
            cache
                .ingest(vec![
                    report("KPAE", 1),
                    report("KPAE", 0),
                    report("KSEA", 2)
                ])
                .unwrap()
                .len(),
            2
        );
        assert!(
            WeatherCache::open(root.path()).is_err(),
            "one writer across owners"
        );
        let expected = cache.reports().cloned().collect::<Vec<_>>();
        drop(cache);
        let mut cache = WeatherCache::open(root.path()).unwrap();
        assert_eq!(cache.reports().cloned().collect::<Vec<_>>(), expected);
        assert!(cache.ingest(vec![report("KPAE", 0)]).unwrap().is_empty());
        assert_eq!(
            cache.ingest(vec![report("KPAE", 3)]).unwrap(),
            [report("KPAE", 3)]
        );
        assert_eq!(
            cache.reports().count(),
            2,
            "updates replace rows rather than append history"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(root.path().join("weather.sqlite"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn transaction_failure_publishes_nothing_and_restores_prior_disk_and_memory() {
        let root = Temp::new();
        let mut cache = WeatherCache::open(root.path()).unwrap();
        cache.ingest(vec![report("KPAE", 0)]).unwrap();
        cache
            .db
            .execute_batch(
                "CREATE TRIGGER fail_insert BEFORE INSERT ON reports WHEN new.station='KSEA'
            BEGIN SELECT RAISE(ABORT, 'injected storage failure'); END;",
            )
            .unwrap();
        assert!(cache
            .ingest(vec![report("KPAE", 1), report("KSEA", 1)])
            .is_err());
        assert_eq!(
            cache.reports().cloned().collect::<Vec<_>>(),
            [report("KPAE", 0)]
        );
        drop(cache);
        let cache = WeatherCache::open(root.path()).unwrap();
        assert_eq!(
            cache.reports().cloned().collect::<Vec<_>>(),
            [report("KPAE", 0)]
        );
    }

    #[test]
    fn actual_sqlite_full_error_preserves_the_previous_report_and_releases_transaction() {
        let root = Temp::new();
        let mut cache = WeatherCache::open(root.path()).unwrap();
        cache.ingest(vec![report("KPAE", 0)]).unwrap();
        let pages: i64 = cache
            .db
            .pragma_query_value(None, "page_count", |row| row.get(0))
            .unwrap();
        cache
            .db
            .pragma_update(None, "max_page_count", pages)
            .unwrap();
        let mut large = report("KPAE", 1);
        large.raw_text.push_str(&"X".repeat(12_000));
        let error = cache.ingest(vec![large.clone()]).unwrap_err();
        let error = error.downcast_ref::<rusqlite::Error>().unwrap();
        assert_eq!(
            error.sqlite_error_code(),
            Some(rusqlite::ErrorCode::DiskFull)
        );
        assert_eq!(
            cache.reports().cloned().collect::<Vec<_>>(),
            [report("KPAE", 0)]
        );
        drop(cache);
        let mut reopened = WeatherCache::open(root.path()).unwrap();
        assert_eq!(
            reopened.reports().cloned().collect::<Vec<_>>(),
            [report("KPAE", 0)]
        );
        assert_eq!(reopened.ingest(vec![large.clone()]).unwrap(), [large]);
    }

    #[test]
    fn corrupt_or_unknown_cache_is_reported_not_silently_reset() {
        let root = Temp::new();
        let cache = WeatherCache::open(root.path()).unwrap();
        cache.db.pragma_update(None, "user_version", 999).unwrap();
        drop(cache);
        assert!(WeatherCache::open(root.path()).is_err());
        let db = Connection::open(root.path().join("weather.sqlite")).unwrap();
        assert_eq!(
            db.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
                .unwrap(),
            999
        );
        drop(db);
        fs::write(root.path().join("weather.sqlite"), b"broken sqlite").unwrap();
        assert!(WeatherCache::open(root.path()).is_err());
        assert_eq!(
            fs::read(root.path().join("weather.sqlite")).unwrap(),
            b"broken sqlite"
        );
    }
}
