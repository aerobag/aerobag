// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Native private capture storage. A kernel lock covers inventory, quota and
//! append ownership. Closed segments are never reopened, rewritten or deleted.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use super::capture::{
    CaptureClock, CaptureKind, CaptureProgress, CaptureSink, CaptureWriter, Recorder,
};

#[derive(Clone, Copy)]
pub struct CapturePolicy {
    pub segment_bytes: u64,
    pub total_bytes: u64,
    pub max_segments: usize,
}

impl Default for CapturePolicy {
    fn default() -> Self {
        Self {
            segment_bytes: 16 * 1024 * 1024,
            total_bytes: 256 * 1024 * 1024,
            max_segments: 512,
        }
    }
}

struct FileSink(File);

impl Write for FileSink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl CaptureSink for FileSink {
    fn sync_data(&mut self) -> io::Result<()> {
        self.0.sync_data()
    }
}

pub struct CaptureArchive {
    directory: PathBuf,
    _lock: File,
    policy: CapturePolicy,
    session: String,
    next_segment: usize,
    segment_count: usize,
    stored_bytes: u64,
    writer: Option<CaptureWriter<FileSink>>,
    progress: CaptureProgress,
    failed: bool,
    last_clock_ms: Option<u64>,
}

impl CaptureArchive {
    /// Root must be beneath the application's private storage. No shared or
    /// remotely supplied path belongs here. Other files are rejected, not erased.
    pub fn open(directory: &Path, policy: CapturePolicy) -> io::Result<Self> {
        if policy.segment_bytes < 128
            || policy.total_bytes < policy.segment_bytes
            || policy.max_segments == 0
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid receiver capture policy",
            ));
        }
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        match builder.create(directory) {
            Ok(()) => {
                if let Some(parent) = directory.parent() {
                    File::open(parent)?.sync_all()?;
                }
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
        let metadata = fs::symlink_metadata(directory)?;
        if !metadata.is_dir() {
            return Err(io::Error::other("capture root is not a directory"));
        }
        check_private(&metadata)?;
        let lock_path = directory.join(".lock");
        if let Ok(metadata) = fs::symlink_metadata(&lock_path) {
            if !metadata.is_file() {
                return Err(io::Error::other("invalid capture lock"));
            }
            check_private(&metadata)?;
        }
        let mut options = private_options();
        let lock = options
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)?;
        fs2::FileExt::try_lock_exclusive(&lock)?;
        File::open(directory)?.sync_all()?;
        let mut stored_bytes = 0u64;
        let mut segment_count = 0usize;
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            if entry.file_name() == ".lock" {
                continue;
            }
            let metadata = fs::symlink_metadata(entry.path())?;
            if !metadata.is_file()
                || entry
                    .path()
                    .extension()
                    .is_none_or(|extension| extension != "abrc")
            {
                return Err(io::Error::other("unexpected entry in capture directory"));
            }
            check_private(&metadata)?;
            stored_bytes = stored_bytes
                .checked_add(metadata.len())
                .ok_or_else(|| io::Error::other("capture size overflow"))?;
            segment_count += 1;
            if stored_bytes > policy.total_bytes || segment_count > policy.max_segments {
                return Err(io::Error::new(
                    io::ErrorKind::StorageFull,
                    "receiver capture quota reached; export and remove captures",
                ));
            }
        }
        let mut random = [0u8; 16];
        getrandom::getrandom(&mut random)
            .map_err(|_| io::Error::other("capture ID generation failed"))?;
        let session = random.iter().map(|byte| format!("{byte:02x}")).collect();
        Ok(Self {
            directory: directory.to_path_buf(),
            _lock: lock,
            policy,
            session,
            next_segment: 0,
            segment_count,
            stored_bytes,
            writer: None,
            progress: CaptureProgress::default(),
            failed: false,
            last_clock_ms: None,
        })
    }

    fn append(&mut self, kind: CaptureKind, clock: CaptureClock, bytes: &[u8]) -> io::Result<()> {
        if self
            .last_clock_ms
            .is_some_and(|last| clock.monotonic_ms < last)
        {
            return Err(io::Error::other("capture monotonic clock regressed"));
        }
        let size = CaptureWriter::<FileSink>::encoded_record_bytes(bytes.len());
        if bytes.len() > super::capture::MAX_CHUNK_BYTES || size + 8 > self.policy.segment_bytes {
            return Err(io::Error::other("capture record exceeds segment limit"));
        }
        if self
            .writer
            .as_ref()
            .is_some_and(|writer| !writer.fits(bytes.len()))
        {
            self.checkpoint()?;
            self.writer = None;
        }
        let header_bytes = if self.writer.is_none() { 8 } else { 0 };
        if self.stored_bytes.saturating_add(size + header_bytes) > self.policy.total_bytes {
            return Err(io::Error::new(
                io::ErrorKind::StorageFull,
                "receiver capture quota reached; export and remove captures",
            ));
        }
        if self.writer.is_none() {
            if self.segment_count == self.policy.max_segments {
                return Err(io::Error::new(
                    io::ErrorKind::StorageFull,
                    "receiver capture segment limit reached",
                ));
            }
            let path = self
                .directory
                .join(format!("{}-{:08}.abrc", self.session, self.next_segment));
            let file = private_options().write(true).create_new(true).open(path)?;
            // Directory durability is part of opening a segment, not a best-effort
            // exit hook. A crash must not unlink an already synced capture.
            let writer = CaptureWriter::new(FileSink(file), self.policy.segment_bytes)?;
            File::open(&self.directory)?.sync_all()?;
            self.writer = Some(writer);
            self.next_segment += 1;
            self.segment_count += 1;
            self.stored_bytes += 8;
            self.progress.written_bytes += 8;
        }
        self.writer.as_mut().unwrap().append(
            kind,
            clock.monotonic_ms,
            clock.wall_epoch_ms,
            bytes,
        )?;
        self.stored_bytes += size;
        self.progress.written_bytes += size;
        self.progress.written_records += 1;
        self.last_clock_ms = Some(clock.monotonic_ms);
        Ok(())
    }

    /// Flush first, then expose immutable paths for an explicit private export.
    /// Never put them in logs or public build artifacts. Rotation prevents a
    /// continuing capture from changing files handed to the export operation.
    pub fn seal_for_export(&mut self) -> io::Result<Vec<PathBuf>> {
        self.checkpoint()?;
        self.writer = None;
        let mut paths = fs::read_dir(&self.directory)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<io::Result<Vec<_>>>()?;
        paths.retain(|path| {
            path.extension()
                .is_some_and(|extension| extension == "abrc")
        });
        paths.sort();
        Ok(paths)
    }
}

impl Recorder for CaptureArchive {
    fn record(&mut self, kind: CaptureKind, clock: CaptureClock, bytes: &[u8]) -> io::Result<()> {
        if self.failed {
            return Err(io::Error::other("receiver capture remains failed"));
        }
        let result = self.append(kind, clock, bytes);
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn checkpoint(&mut self) -> io::Result<()> {
        if let Some(writer) = &mut self.writer {
            if let Err(error) = writer.checkpoint() {
                self.failed = true;
                return Err(error);
            }
        }
        self.progress.durable_records = self.progress.written_records;
        if self.failed {
            return Err(io::Error::other("receiver capture remains failed"));
        }
        Ok(())
    }

    fn progress(&self) -> CaptureProgress {
        self.progress
    }
}

impl Drop for CaptureArchive {
    fn drop(&mut self) {
        let _ = self.checkpoint();
    }
}

pub(super) fn private_options() -> OpenOptions {
    #[allow(unused_mut)]
    let mut options = OpenOptions::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
}

pub(super) fn check_private(metadata: &fs::Metadata) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(io::Error::other("receiver capture storage is not private"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::capture::{CaptureRead, CaptureReader};
    use super::super::test_support::Temp;
    use super::*;

    fn clock(ms: u64) -> CaptureClock {
        CaptureClock {
            monotonic_ms: ms,
            wall_epoch_ms: ms as i64,
        }
    }
    fn records(path: &Path) -> Vec<Vec<u8>> {
        let mut reader = CaptureReader::new(File::open(path).unwrap()).unwrap();
        let mut result = Vec::new();
        loop {
            match reader.next_record().unwrap() {
                CaptureRead::Record(record) => result.push(record.bytes),
                CaptureRead::End => return result,
                CaptureRead::TornTail => panic!("unexpected torn record"),
            }
        }
    }

    #[test]
    fn rotates_seals_exports_and_counts_existing_files_after_restart() {
        let temp = Temp::new();
        let policy = CapturePolicy {
            segment_bytes: 160,
            total_bytes: 500,
            max_segments: 10,
        };
        let mut archive = CaptureArchive::open(&temp.0, policy).unwrap();
        for i in 0..3 {
            archive
                .record(CaptureKind::Received, clock(i), &[i as u8; 10])
                .unwrap();
        }
        let sealed = archive.seal_for_export().unwrap();
        assert_eq!(sealed.len(), 2);
        assert_eq!(records(&sealed[0]), vec![vec![0; 10], vec![1; 10]]);
        let immutable = fs::read(&sealed[1]).unwrap();
        archive
            .record(CaptureKind::Received, clock(4), &[4; 10])
            .unwrap();
        assert_eq!(fs::read(&sealed[1]).unwrap(), immutable);
        assert_eq!(archive.progress().written_records, 4);
        assert_eq!(archive.progress().durable_records, 3);
        drop(archive);
        let mut reopened = CaptureArchive::open(&temp.0, policy).unwrap();
        assert_eq!(reopened.stored_bytes, 4 * 71 + 3 * 8);
        reopened
            .record(CaptureKind::Received, clock(0), &[5; 10])
            .unwrap();
        reopened
            .record(CaptureKind::Received, clock(1), &[6; 10])
            .unwrap();
        assert!(reopened
            .record(CaptureKind::Received, clock(2), &[7; 10])
            .is_err());
        assert!(reopened
            .record(CaptureKind::Received, clock(3), &[])
            .is_err());
        assert!(reopened.checkpoint().is_err());
        assert_eq!(fs::read(&sealed[1]).unwrap(), immutable);
    }

    #[test]
    fn only_one_writer_and_reopen_never_appends_to_a_torn_segment() {
        let temp = Temp::new();
        let mut archive = CaptureArchive::open(&temp.0, CapturePolicy::default()).unwrap();
        assert!(CaptureArchive::open(&temp.0, CapturePolicy::default()).is_err());
        archive
            .record(CaptureKind::Received, clock(1), b"complete")
            .unwrap();
        let paths = archive.seal_for_export().unwrap();
        drop(archive);
        let mut file = OpenOptions::new().append(true).open(&paths[0]).unwrap();
        file.write_all(&[30, 0]).unwrap();
        file.sync_all().unwrap();
        let torn = fs::read(&paths[0]).unwrap();
        let mut archive = CaptureArchive::open(&temp.0, CapturePolicy::default()).unwrap();
        archive
            .record(CaptureKind::Received, clock(0), b"restart")
            .unwrap();
        assert_eq!(archive.seal_for_export().unwrap().len(), 2);
        assert_eq!(fs::read(&paths[0]).unwrap(), torn);
        let mut reader = CaptureReader::new(torn.as_slice()).unwrap();
        assert!(matches!(
            reader.next_record().unwrap(),
            CaptureRead::Record(_)
        ));
        assert!(matches!(
            reader.next_record().unwrap(),
            CaptureRead::TornTail
        ));
    }

    #[test]
    fn segment_count_limit_and_clock_order_are_latched() {
        let temp = Temp::new();
        let policy = CapturePolicy {
            segment_bytes: 128,
            total_bytes: 1000,
            max_segments: 1,
        };
        let mut archive = CaptureArchive::open(&temp.0, policy).unwrap();
        archive
            .record(CaptureKind::Received, clock(1), &[3; 59])
            .unwrap();
        assert!(archive
            .record(CaptureKind::Received, clock(2), &[])
            .is_err());
        drop(archive);
        assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 2); // segment and lock
        let temp = Temp::new();
        let mut archive = CaptureArchive::open(&temp.0, policy).unwrap();
        archive
            .record(CaptureKind::Received, clock(2), &[])
            .unwrap();
        assert!(archive
            .record(CaptureKind::Received, clock(1), &[])
            .is_err());
        assert!(archive
            .record(CaptureKind::Received, clock(3), &[])
            .is_err());
    }

    #[test]
    fn real_archive_quota_stops_connection_without_claiming_transmission_or_retrying() {
        use super::super::connection::{Connection, ConnectionPhase, Effect, Event, Fault};
        use super::super::Credentials;
        let temp = Temp::new();
        let policy = CapturePolicy {
            segment_bytes: 256,
            total_bytes: 256,
            max_segments: 5,
        };
        let archive = CaptureArchive::open(&temp.0, policy).unwrap();
        let mut connection = Connection::new(
            Credentials::new(&[7; 48], b"GRM_SALTsynthetic").unwrap(),
            archive,
            clock(0),
        );
        let output = connection.start("paired-device".into(), clock(0));
        let Effect::Connect { id, .. } = output.effects[0] else {
            panic!()
        };
        let output = connection.update(Event::Connected(id), clock(1));
        let Effect::Write { write_id, .. } = output.effects[0] else {
            panic!()
        };
        let output = connection.update(Event::Written(id, write_id), clock(2));
        assert!(matches!(output.effects.as_slice(), [Effect::Close { .. }]));
        assert_eq!(connection.status().phase, ConnectionPhase::Blocked);
        assert_eq!(connection.status().last_fault, Some(Fault::CaptureFull));
        assert!(connection
            .update(Event::Tick, clock(100_000))
            .effects
            .is_empty());
        assert_eq!(connection.status().captured.durable_records, 3);
        drop(connection);
        let path = fs::read_dir(&temp.0)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| path.extension().is_some_and(|ext| ext == "abrc"))
            .unwrap();
        let mut reader = CaptureReader::new(File::open(path).unwrap()).unwrap();
        let mut kinds = Vec::new();
        while let CaptureRead::Record(record) = reader.next_record().unwrap() {
            kinds.push(record.kind);
        }
        assert_eq!(
            kinds,
            vec![
                CaptureKind::ConnectionRequested,
                CaptureKind::Connected,
                CaptureKind::WriteRequested
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn refuses_public_storage_and_symlink_entries() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let temp = Temp::new();
        fs::create_dir(&temp.0).unwrap();
        fs::set_permissions(&temp.0, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(CaptureArchive::open(&temp.0, CapturePolicy::default()).is_err());
        fs::set_permissions(&temp.0, fs::Permissions::from_mode(0o700)).unwrap();
        symlink("/dev/null", temp.0.join("trick.abrc")).unwrap();
        assert!(CaptureArchive::open(&temp.0, CapturePolicy::default()).is_err());
    }
}
