// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::Arc;

use crate::{
    local_documents::{LocalDocuments, STATION_DIRECTORY_DOCUMENT as KEY},
    weather_sources::StationDirectory,
};

#[derive(Default)]
pub(crate) struct WeatherDirectoryPersistence {
    queued_revision: u64,
}

impl WeatherDirectoryPersistence {
    pub fn load(&mut self, directory: &mut StationDirectory, storage: &Arc<LocalDocuments>) {
        match storage.read(KEY) {
            Ok(Some(bytes)) => match StationDirectory::decode_document(&bytes) {
                Ok(restored) => {
                    *directory = restored;
                    self.queued_revision = directory.revision();
                }
                Err(reason) => storage.protect_unreadable(KEY, reason),
            },
            // IO errors are already retained by LocalDocuments; absence is not an error.
            Ok(None) | Err(_) => {}
        }
    }

    /// Called after an ingestion batch, never once per observation. Revision is
    /// about geography/provenance, not weather recency. LocalDocuments owns IO
    /// completion, coalescing and retry once this version is queued.
    pub fn flush(&mut self, directory: &StationDirectory, storage: &Arc<LocalDocuments>) {
        if self.queued_revision == directory.revision() || storage.is_protected(KEY) {
            return;
        }
        match directory.encode_document() {
            Ok(bytes) => {
                if storage.replace(KEY, &bytes).is_ok() {
                    self.queued_revision = directory.revision();
                }
            }
            Err(reason) => storage.protect_unreadable(KEY, reason),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        local_documents::{DocumentCompletion, LocalDocumentBackend},
        weather_sources::*,
        AppResult,
    };
    use std::{collections::BTreeMap, sync::Mutex};

    #[derive(Default)]
    struct Host {
        files: Mutex<BTreeMap<String, Vec<u8>>>,
        pending: Mutex<Vec<(Option<Vec<u8>>, DocumentCompletion)>>,
    }
    impl LocalDocumentBackend for Host {
        fn read(&self, key: &str) -> AppResult<Option<Vec<u8>>> {
            Ok(self.files.lock().unwrap().get(key).cloned())
        }
        fn write(&self, key: &str, bytes: Option<Vec<u8>>, complete: DocumentCompletion) {
            assert_eq!(key, KEY);
            self.pending.lock().unwrap().push((bytes, complete));
        }
    }
    impl Host {
        fn finish(&self) {
            let (bytes, complete) = self.pending.lock().unwrap().remove(0);
            self.files
                .lock()
                .unwrap()
                .insert(KEY.into(), bytes.unwrap());
            complete(Ok(()));
        }
    }
    fn metadata(lat: f64) -> StationMetadata {
        StationMetadata {
            position: StationPosition {
                latitude: lat,
                longitude: -121.4,
            },
            source: StationMetadataSource::Internet,
        }
    }

    #[test]
    fn batches_metadata_writes_coalesces_changes_and_restores_geography_without_reports() {
        let host = Arc::new(Host::default());
        let storage = LocalDocuments::new(host.clone());
        let mut persistence = WeatherDirectoryPersistence::default();
        let mut weather = StationWeather::default();
        persistence.load(weather.directory_mut(), &storage);
        persistence.flush(weather.directory(), &storage);
        assert!(host.pending.lock().unwrap().is_empty());
        for id in ["PASS", "KPAE", "KSEA"] {
            weather
                .directory_mut()
                .discover(StationId::new(id).unwrap(), metadata(47.4))
                .unwrap();
        }
        persistence.flush(weather.directory(), &storage);
        assert_eq!(host.pending.lock().unwrap().len(), 1);
        // More metadata while IO is held must replace the pending version, not
        // race another write of this key or overwrite the newer directory later.
        weather
            .directory_mut()
            .correct_location(StationId::new("PASS").unwrap(), metadata(47.5))
            .unwrap();
        persistence.flush(weather.directory(), &storage);
        weather
            .directory_mut()
            .correct_location(StationId::new("PASS").unwrap(), metadata(47.6))
            .unwrap();
        persistence.flush(weather.directory(), &storage);
        assert_eq!(host.pending.lock().unwrap().len(), 1);
        host.finish();
        host.finish();
        for _ in 0..100 {
            weather
                .directory_mut()
                .discover(StationId::new("PASS").unwrap(), metadata(47.6))
                .unwrap();
            persistence.flush(weather.directory(), &storage);
        }
        assert!(host.pending.lock().unwrap().is_empty());
        let bytes = host.read(KEY).unwrap().unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            json.as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["schema_version", "stations"]
        );
        let restarted_storage = LocalDocuments::new(host.clone());
        let mut restarted = StationWeather::default();
        let mut restarted_persistence = WeatherDirectoryPersistence::default();
        restarted_persistence.load(restarted.directory_mut(), &restarted_storage);
        restarted_persistence.flush(restarted.directory(), &restarted_storage);
        assert!(host.pending.lock().unwrap().is_empty());
        assert_eq!(
            restarted
                .directory()
                .in_bounds(47.55, -122.0, 47.65, -121.0)
                .unwrap(),
            [&StationId::new("PASS").unwrap()]
        );
        assert!(restarted.query().metar("PASS").is_none());
        assert!(restarted.query().taf("PASS").is_none());
    }

    #[test]
    fn changing_or_removing_reports_does_not_rewrite_learned_station_metadata() {
        let host = Arc::new(Host::default());
        let storage = LocalDocuments::new(host.clone());
        let mut persistence = WeatherDirectoryPersistence::default();
        let mut weather = StationWeather::default();
        persistence.load(weather.directory_mut(), &storage);
        let now = chrono::DateTime::parse_from_rfc3339("2026-10-10T12:00:00Z")
            .unwrap()
            .to_utc();
        for minute in 0..10 {
            let payload = serde_json::from_value(serde_json::json!({
                "schema_version":3, "version_label":format!("v{minute}"),
                "metars_by_station": {
                    "PASS": {"station_id":"PASS", "latitude":47.4, "longitude":-121.4,
                        "raw_text":format!("METAR PASS 1012{minute:02}Z 00000KT 10SM CLR 10/08 A3000")},
                    "KPAE": {"station_id":"KPAE", "latitude":47.9, "longitude":-122.3,
                        "raw_text":format!("METAR KPAE 1012{minute:02}Z 00000KT 10SM CLR 10/08 A3000")}
                }
            })).unwrap();
            weather.install_metars(payload, now).unwrap();
            persistence.flush(weather.directory(), &storage);
            if minute == 0 {
                assert_eq!(host.pending.lock().unwrap().len(), 1);
                host.finish();
            } else {
                assert!(host.pending.lock().unwrap().is_empty());
            }
        }
        let empty = serde_json::from_value(serde_json::json!({
            "schema_version":3, "version_label":"empty", "metars_by_station":{}
        }))
        .unwrap();
        weather.install_metars(empty, now).unwrap();
        persistence.flush(weather.directory(), &storage);
        assert!(host.pending.lock().unwrap().is_empty());
        assert!(weather.query().metar("PASS").is_none());
        // Later, offline, the receiver has text but no coordinates. The learned
        // station still participates in the same spatial query after restart.
        let mut restarted = StationWeather::default();
        WeatherDirectoryPersistence::default()
            .load(restarted.directory_mut(), &LocalDocuments::new(host));
        restarted
            .ingest(
                WeatherSource::Receiver,
                StationReport::from_receiver(
                    StationReportKind::Metar,
                    &crate::receiver::TextReport {
                        text: "METAR PASS 101210Z 00000KT 10SM CLR 10/08 A3000".into(),
                        station: Some("PASS".into()),
                        notam_identifier: None,
                        record_type_raw: None,
                    },
                    now,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(
            restarted
                .directory()
                .in_bounds(47.0, -122.0, 47.8, -121.0)
                .unwrap(),
            [&StationId::new("PASS").unwrap()]
        );
        assert!(restarted.query().metar("PASS").is_some());
    }

    #[test]
    fn corrupt_directory_is_reported_and_protected_without_blocking_live_learning() {
        for bytes in [
            b"broken".as_slice(),
            br#"{"schema_version":2,"stations":[]}"#,
        ] {
            let host = Arc::new(Host::default());
            host.files
                .lock()
                .unwrap()
                .insert(KEY.into(), bytes.to_vec());
            let storage = LocalDocuments::new(host.clone());
            let mut directory = StationDirectory::default();
            let mut persistence = WeatherDirectoryPersistence::default();
            persistence.load(&mut directory, &storage);
            directory
                .discover(StationId::new("PASS").unwrap(), metadata(47.4))
                .unwrap();
            persistence.flush(&directory, &storage);
            assert!(storage.is_protected(KEY));
            assert!(!storage.errors().is_empty());
            assert!(host.pending.lock().unwrap().is_empty());
            assert_eq!(host.read(KEY).unwrap().unwrap(), bytes);
        }
    }

    #[test]
    fn directory_decoder_rejects_duplicate_stations_and_invalid_coordinates() {
        let station = (StationId::new("PASS").unwrap(), metadata(47.4));
        for (version, stations) in [
            (2, vec![station.clone()]),
            (1, vec![station.clone(), station]),
            (1, vec![(StationId::new("PASS").unwrap(), metadata(91.0))]),
        ] {
            let bytes = serde_json::to_vec(
                &serde_json::json!({"schema_version":version, "stations":stations}),
            )
            .unwrap();
            assert!(StationDirectory::decode_document(&bytes).is_err());
        }
    }
}
