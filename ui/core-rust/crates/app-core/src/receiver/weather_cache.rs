// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Latest receiver reports, independent of the Internet product/delta cache.
//! All IO runs on the receiver owner, never under a session or mailbox lock.

use super::radar::{Frame, History, Image, MAX_HISTORY_BYTES};
use crate::weather_sources::{StationReport, StationReportKind, StationReports};
use rusqlite::{params, Connection};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    sync::Arc,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub(super) struct WeatherCache {
    db: Connection,
    reports: StationReports,
    radar: History,
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
            1 | 2 => {}
            _ => return Err("Unsupported receiver weather cache version".into()),
        }
        if version < 2 {
            let transaction = db.transaction()?;
            transaction.execute_batch(
                "CREATE TABLE radar_images (
                    id TEXT PRIMARY KEY, observed TEXT NOT NULL, issued TEXT NOT NULL,
                    regional INTEGER NOT NULL, manifest TEXT NOT NULL);
                 CREATE TABLE radar_tiles (
                    image TEXT NOT NULL REFERENCES radar_images(id), url TEXT NOT NULL,
                    png BLOB NOT NULL, PRIMARY KEY(image,url)) WITHOUT ROWID;
                 CREATE TABLE radar_frames (
                    ordinal INTEGER NOT NULL, layer INTEGER NOT NULL,
                    image TEXT NOT NULL REFERENCES radar_images(id),
                    PRIMARY KEY(ordinal,layer)) WITHOUT ROWID;
                 PRAGMA user_version=2;",
            )?;
            transaction.commit()?;
        }
        db.execute_batch("PRAGMA foreign_keys=ON;")?;
        let radar = read_radar(&db)?;
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
                let mut report: StationReport = serde_json::from_str(text)?;
                if report.station.as_str() != station || kind_key(report.kind()) != kind {
                    return Err("Receiver weather cache key mismatch".into());
                }
                // Derived display fields are disposable. Reinterpret original
                // observations on upgrade without changing their dates.
                report.details = crate::weather_sources::StationReportDetails::from_raw(
                    report.kind(),
                    &report.raw_text,
                );
                if !reports.ingest(report)? {
                    return Err("Duplicate receiver weather cache row".into());
                }
            }
        }
        Ok(Self {
            db,
            reports,
            radar,
            _lock: lock,
        })
    }

    pub fn reports(&self) -> impl Iterator<Item = &StationReport> {
        self.reports.values()
    }

    pub(super) fn radar(&self) -> &History {
        &self.radar
    }

    pub(super) fn store_radar(&mut self, history: History) -> Result<()> {
        let old_ids: BTreeSet<_> = self
            .radar
            .frames
            .iter()
            .flat_map(|f| &f.layers)
            .map(|i| i.id.as_str())
            .collect();
        let images: BTreeMap<_, _> = history
            .frames
            .iter()
            .flat_map(|f| &f.layers)
            .map(|i| (i.id.as_str(), i))
            .collect();
        let transaction = self.db.transaction()?;
        // Evict inside this transaction before allocating replacement tiles.
        // Failure rolls the whole change back, while successful replacement
        // reuses pages instead of needing room for two complete histories.
        transaction.execute("DELETE FROM radar_frames", [])?;
        for id in old_ids.iter().filter(|id| !images.contains_key(**id)) {
            transaction.execute("DELETE FROM radar_tiles WHERE image=?1", [id])?;
            transaction.execute("DELETE FROM radar_images WHERE id=?1", [id])?;
        }
        for (id, image) in &images {
            if old_ids.contains(id) {
                continue;
            }
            transaction.execute(
                "INSERT INTO radar_images VALUES (?1,?2,?3,?4,?5)",
                params![
                    id,
                    image.observed.to_rfc3339(),
                    image.issue.to_rfc3339(),
                    image.regional,
                    serde_json::to_string(&image.manifest)?
                ],
            )?;
            let mut insert =
                transaction.prepare_cached("INSERT INTO radar_tiles VALUES (?1,?2,?3)")?;
            for (url, png) in &image.tiles {
                insert.execute(params![id, url, png.as_ref()])?;
            }
        }
        {
            let mut insert =
                transaction.prepare_cached("INSERT INTO radar_frames VALUES (?1,?2,?3)")?;
            for (ordinal, frame) in history.frames.iter().enumerate() {
                for (layer, image) in frame.layers.iter().enumerate() {
                    insert.execute(params![ordinal as i64, layer as i64, image.id])?;
                }
            }
        }
        transaction.commit()?;
        self.radar = history;
        Ok(())
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

fn read_radar(db: &Connection) -> Result<History> {
    let mut images = BTreeMap::new();
    let mut total_bytes = 0;
    let mut statement =
        db.prepare("SELECT id, observed, issued, regional, manifest FROM radar_images")?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        if images.len() >= 2 * crate::live_feeds::NEXRAD_FRAME_WINDOW_SIZE {
            return Err("Too many cached radar images".into());
        }
        let id: String = row.get(0)?;
        if id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("Invalid cached radar identity".into());
        }
        let observed = chrono::DateTime::parse_from_rfc3339(row.get_ref(1)?.as_str()?)?.to_utc();
        let issue = chrono::DateTime::parse_from_rfc3339(row.get_ref(2)?.as_str()?)?.to_utc();
        let manifest_text = row.get_ref(4)?.as_str()?;
        if manifest_text.len() > 65536 {
            return Err("Oversized radar manifest".into());
        }
        let manifest: serde_json::Value = serde_json::from_str(manifest_text)?;
        if manifest["state_id"].as_str() != Some(id.as_str()) {
            return Err("Cached radar manifest identity mismatch".into());
        }
        let mut tiles = BTreeMap::new();
        let mut tile_statement = db.prepare("SELECT url, png FROM radar_tiles WHERE image=?1")?;
        let mut tile_rows = tile_statement.query([&id])?;
        let mut bytes = 0;
        while let Some(tile) = tile_rows.next()? {
            let png = tile.get_ref(1)?.as_blob()?;
            total_bytes += png.len();
            if total_bytes > MAX_HISTORY_BYTES || png.len() > 1024 * 1024 || tiles.len() >= 1024 {
                return Err("Receiver radar cache exceeds display limits".into());
            }
            let url: String = tile.get(0)?;
            if !url.starts_with(&format!("{}{id}/", super::radar::RESOURCE_PREFIX)) {
                return Err("Cached radar tile identity mismatch".into());
            }
            bytes += png.len();
            tiles.insert(url, Arc::from(png));
        }
        if tiles.is_empty() {
            return Err("Cached radar image has no tiles".into());
        }
        images.insert(
            id.clone(),
            Arc::new(Image {
                id,
                observed,
                issue,
                regional: row.get(3)?,
                manifest,
                tiles,
                bytes,
            }),
        );
    }
    let mut frames: Vec<Frame> = Vec::new();
    let mut used = BTreeSet::new();
    let mut statement =
        db.prepare("SELECT ordinal, layer, image FROM radar_frames ORDER BY ordinal, layer")?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let ordinal: usize = row.get(0)?;
        let layer: usize = row.get(1)?;
        if layer == 0
            && ordinal == frames.len()
            && ordinal < crate::live_feeds::NEXRAD_FRAME_WINDOW_SIZE
        {
            frames.push(Frame {
                id: String::new(),
                layers: Vec::new(),
            });
        }
        let frame = frames
            .get_mut(ordinal)
            .ok_or("Invalid cached radar frame order")?;
        if layer != frame.layers.len() || layer >= 2 {
            return Err("Invalid cached radar layer order".into());
        }
        let id: String = row.get(2)?;
        let image = images.get(&id).ok_or("Missing cached radar image")?;
        if frame
            .layers
            .last()
            .is_some_and(|last| last.regional >= image.regional)
        {
            return Err("Invalid cached radar layer kinds".into());
        }
        used.insert(id);
        frame.layers.push(image.clone());
    }
    if used.len() != images.len() {
        return Err("Unreferenced cached radar image".into());
    }
    for frame in &mut frames {
        *frame = Frame::new(std::mem::take(&mut frame.layers));
    }
    Ok(History { frames })
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

    #[test]
    fn radar_restart_preserves_pixels_dates_frame_ids_and_bounded_history() {
        let root = Temp::new();
        let now = Utc.with_ymd_and_hms(2026, 10, 10, 12, 0, 0).unwrap();
        let mut cache = WeatherCache::open(root.path()).unwrap();
        let mut history = History::default();
        let mut radar = super::super::test_support::radar();
        for minute in 0..20 {
            radar.issue_time.minute = minute;
            radar.precipitation_time.minute = minute;
            radar.kind = if minute % 2 == 0 {
                super::super::WeatherKind::RegionalRadar
            } else {
                super::super::WeatherKind::ConusRadar
            };
            history
                .ingest(&radar, now + chrono::Duration::minutes(minute.into()))
                .unwrap();
            cache.store_radar(history.clone()).unwrap();
        }
        let n: usize = cache
            .db
            .query_row("SELECT count(*) FROM radar_images", [], |r| r.get(0))
            .unwrap();
        assert!(n <= crate::live_feeds::NEXRAD_FRAME_WINDOW_SIZE + 1);
        drop(cache);
        let cache = WeatherCache::open(root.path()).unwrap();
        assert_eq!(cache.radar(), &history);
        drop(cache);
        let runtime = super::super::runtime::Runtime::new(root.path().to_path_buf());
        let snapshot = runtime.test_snapshot();
        assert_eq!(*snapshot.radar, history);
        assert!(snapshot.ownship.is_none());
        assert!(snapshot.aircraft.is_empty());
        assert!(!snapshot.connected);
    }

    #[test]
    fn replacing_radar_history_reuses_pages_under_a_tight_disk_quota() {
        let root = Temp::new();
        let now = Utc.with_ymd_and_hms(2026, 10, 10, 12, 0, 0).unwrap();
        let mut radar = super::super::test_support::radar();
        radar.rows = 512;
        radar.columns = 512;
        let mut seed = 12345_u32;
        radar.grid = (0..512 * 512)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                (seed % 8) as u8
            })
            .collect();
        let mut cache = WeatherCache::open(root.path()).unwrap();
        let mut first = History::default();
        first.ingest(&radar, now).unwrap();
        cache.store_radar(first).unwrap();
        let pages: u64 = cache
            .db
            .pragma_query_value(None, "page_count", |r| r.get(0))
            .unwrap();
        cache
            .db
            .pragma_update(None, "max_page_count", pages + 4)
            .unwrap();
        // Replace an evicted history with a similarly sized frame. Neither
        // image is individually too large; only temporary duplication is.
        radar.issue_time.minute = 1;
        let mut next = History::default();
        next.ingest(&radar, now + chrono::Duration::minutes(1))
            .unwrap();
        cache.store_radar(next.clone()).unwrap();
        drop(cache);
        assert_eq!(WeatherCache::open(root.path()).unwrap().radar(), &next);
    }

    #[test]
    fn failed_radar_commit_leaves_old_complete_frame_on_disk_and_in_memory() {
        let root = Temp::new();
        let now = Utc.with_ymd_and_hms(2026, 10, 10, 12, 0, 0).unwrap();
        let mut cache = WeatherCache::open(root.path()).unwrap();
        let mut old = History::default();
        let mut radar = super::super::test_support::radar();
        old.ingest(&radar, now).unwrap();
        cache.store_radar(old.clone()).unwrap();
        cache.db.execute_batch("CREATE TRIGGER fail_frame BEFORE INSERT ON radar_frames BEGIN SELECT RAISE(ABORT, 'injected failure'); END;").unwrap();
        radar.issue_time.minute += 1;
        let mut next = History::default();
        next.ingest(&radar, now).unwrap();
        assert!(cache.store_radar(next).is_err());
        assert_eq!(cache.radar(), &old);
        drop(cache);
        let cache = WeatherCache::open(root.path()).unwrap();
        assert_eq!(cache.radar(), &old);
    }

    #[test]
    fn old_receiver_cache_upgrades_without_losing_reports_and_rederives_symbols() {
        let root = Temp::new();
        let mut cache = WeatherCache::open(root.path()).unwrap();
        let expected = report("KPAE", 0);
        cache.ingest(vec![expected.clone()]).unwrap();
        cache.db.execute_batch("UPDATE reports SET body=json_set(body,'$.details.flight_category',NULL,'$.details.cloud_symbol',NULL);
            DROP TABLE radar_frames; DROP TABLE radar_tiles; DROP TABLE radar_images; PRAGMA user_version=1;").unwrap();
        drop(cache);
        let cache = WeatherCache::open(root.path()).unwrap();
        assert_eq!(cache.reports().cloned().collect::<Vec<_>>(), [expected]);
        assert!(cache.radar().frames.is_empty());
    }

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
