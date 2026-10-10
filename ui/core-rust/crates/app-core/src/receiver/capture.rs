// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Private append-only capture format. A complete record has a length, clocks,
//! kind, bytes, and SHA-256. Recovery accepts a torn final record, never a corrupt
//! complete record. Files may include authentication material and must not be
//! sent to normal application logging or public artifact storage.

use std::io::{self, Read, Write};

use sha2::{Digest, Sha256};

const MAGIC: &[u8; 8] = b"ABRX\x01\0\0\0";
pub const MAX_CHUNK_BYTES: usize = 65_535;
const RECORD_METADATA_BYTES: usize = 25;
const HASH_BYTES: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CaptureKind {
    Received = 0,
    Transmitted = 1,
    Connected = 2,
    Disconnected = 3,
    Diagnostic = 4,
    /// Payload: u64 write ID followed by requested socket bytes. Not proof of TX.
    WriteRequested = 5,
    /// Payload: u64 write ID. Only emitted after the complete socket write.
    WriteCompleted = 6,
    /// Payload: u64 connection ID. Recorded before opening the socket.
    ConnectionRequested = 7,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureClock {
    pub monotonic_ms: u64,
    pub wall_epoch_ms: i64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CaptureProgress {
    pub written_records: u64,
    pub durable_records: u64,
    pub written_bytes: u64,
}

/// The connection owner records before interpretation or dispatch. Implementors
/// must latch IO failure; a successful later flush cannot repair missing bytes.
pub trait Recorder {
    fn record(&mut self, kind: CaptureKind, clock: CaptureClock, bytes: &[u8]) -> io::Result<()>;
    fn checkpoint(&mut self) -> io::Result<()>;
    fn progress(&self) -> CaptureProgress;
}

/// The platform supplies storage only. sync_data must make prior writes durable.
pub trait CaptureSink: Write {
    fn sync_data(&mut self) -> io::Result<()>;
}

pub struct CaptureWriter<S> {
    sink: S,
    bytes_written: u64,
    max_bytes: u64,
    sequence: u64,
    last_monotonic_ms: Option<u64>,
    failed: bool,
}

impl<S: CaptureSink> CaptureWriter<S> {
    pub fn bytes_written(&self) -> u64 {
        self.bytes_written
    }

    pub fn encoded_record_bytes(payload_bytes: usize) -> u64 {
        (4 + RECORD_METADATA_BYTES + payload_bytes + HASH_BYTES) as u64
    }

    pub fn fits(&self, payload_bytes: usize) -> bool {
        self.bytes_written
            .saturating_add(Self::encoded_record_bytes(payload_bytes))
            <= self.max_bytes
    }

    /// Sink must be a newly created private file, never an existing capture.
    pub fn new(mut sink: S, max_bytes: u64) -> io::Result<Self> {
        if max_bytes < MAGIC.len() as u64 + RECORD_METADATA_BYTES as u64 + HASH_BYTES as u64 + 4 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "capture quota too small",
            ));
        }
        sink.write_all(MAGIC)?;
        sink.sync_data()?;
        Ok(Self {
            sink,
            bytes_written: MAGIC.len() as u64,
            max_bytes,
            sequence: 0,
            last_monotonic_ms: None,
            failed: false,
        })
    }

    pub fn append(
        &mut self,
        kind: CaptureKind,
        monotonic_ms: u64,
        wall_epoch_ms: i64,
        bytes: &[u8],
    ) -> io::Result<()> {
        if self.failed {
            return Err(io::Error::other(
                "capture writer failed; open a new capture",
            ));
        }
        let result = self.append_inner(kind, monotonic_ms, wall_epoch_ms, bytes);
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn append_inner(
        &mut self,
        kind: CaptureKind,
        monotonic_ms: u64,
        wall_epoch_ms: i64,
        bytes: &[u8],
    ) -> io::Result<()> {
        if bytes.len() > MAX_CHUNK_BYTES
            || self
                .last_monotonic_ms
                .is_some_and(|previous| monotonic_ms < previous)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid capture chunk/clock",
            ));
        }
        let size = RECORD_METADATA_BYTES + bytes.len();
        let mut record = Vec::with_capacity(4 + size + HASH_BYTES);
        record.extend_from_slice(&(size as u32).to_le_bytes());
        record.extend_from_slice(&self.sequence.to_le_bytes());
        record.push(kind as u8);
        record.extend_from_slice(&monotonic_ms.to_le_bytes());
        record.extend_from_slice(&wall_epoch_ms.to_le_bytes());
        record.extend_from_slice(bytes);
        let hash = Sha256::digest(&record);
        record.extend_from_slice(&hash);
        if self.bytes_written.saturating_add(record.len() as u64) > self.max_bytes {
            return Err(io::Error::other("capture quota reached"));
        }
        self.sink.write_all(&record)?;
        self.bytes_written += record.len() as u64;
        self.sequence += 1;
        self.last_monotonic_ms = Some(monotonic_ms);
        Ok(())
    }

    /// IO owner calls periodically and at disconnect, off the UI thread.
    /// Status must distinguish written bytes from this durable boundary.
    pub fn checkpoint(&mut self) -> io::Result<u64> {
        // Even after a quota or append failure, preserve the complete prefix.
        // Do not confuse that best-effort sync with a healthy recording stream.
        if let Err(error) = self.sink.flush().and_then(|_| self.sink.sync_data()) {
            self.failed = true;
            return Err(error);
        }
        if self.failed {
            return Err(io::Error::other("capture writer failed"));
        }
        Ok(self.sequence)
    }

    pub fn finish(mut self) -> io::Result<S> {
        self.checkpoint()?;
        Ok(self.sink)
    }
}

// Deliberately no Debug: the payload can contain secrets and flight locations.
pub struct CaptureRecord {
    pub sequence: u64,
    pub kind: CaptureKind,
    pub monotonic_ms: u64,
    pub wall_epoch_ms: i64,
    pub bytes: Vec<u8>,
}

pub enum CaptureRead {
    Record(CaptureRecord),
    End,
    TornTail,
}

pub struct CaptureReader<R> {
    input: R,
    next_sequence: u64,
    last_monotonic_ms: Option<u64>,
    ended: bool,
}

impl<R: Read> CaptureReader<R> {
    pub fn new(mut input: R) -> io::Result<Self> {
        let mut magic = [0u8; 8];
        input.read_exact(&mut magic)?;
        if &magic != MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unknown receiver capture format",
            ));
        }
        Ok(Self {
            input,
            next_sequence: 0,
            last_monotonic_ms: None,
            ended: false,
        })
    }

    pub fn next_record(&mut self) -> io::Result<CaptureRead> {
        if self.ended {
            return Ok(CaptureRead::End);
        }
        let result = self.read_record();
        if !matches!(result, Ok(CaptureRead::Record(_))) {
            self.ended = true;
        }
        result
    }

    fn read_record(&mut self) -> io::Result<CaptureRead> {
        let mut length = [0u8; 4];
        // read_exact distinguishes interrupted reads correctly; only an EOF
        // before the first byte denotes a clean record boundary.
        match self.input.read_exact(&mut length[..1]) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
                return Ok(CaptureRead::End)
            }
            Err(error) => return Err(error),
        }
        match self.input.read_exact(&mut length[1..]) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
                return Ok(CaptureRead::TornTail)
            }
            Err(error) => return Err(error),
        }
        let size = u32::from_le_bytes(length) as usize;
        if !(RECORD_METADATA_BYTES..=RECORD_METADATA_BYTES + MAX_CHUNK_BYTES).contains(&size) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid receiver capture record size",
            ));
        }
        let mut body = vec![0u8; size + HASH_BYTES];
        match self.input.read_exact(&mut body) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
                return Ok(CaptureRead::TornTail)
            }
            Err(error) => return Err(error),
        }
        let mut digest = Sha256::new();
        digest.update(length);
        digest.update(&body[..size]);
        if digest.finalize().as_slice() != &body[size..] {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "receiver capture checksum mismatch",
            ));
        }
        let sequence = u64::from_le_bytes(body[..8].try_into().unwrap());
        let kind = match body[8] {
            0 => CaptureKind::Received,
            1 => CaptureKind::Transmitted,
            2 => CaptureKind::Connected,
            3 => CaptureKind::Disconnected,
            4 => CaptureKind::Diagnostic,
            5 => CaptureKind::WriteRequested,
            6 => CaptureKind::WriteCompleted,
            7 => CaptureKind::ConnectionRequested,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "unknown capture record kind",
                ))
            }
        };
        let monotonic_ms = u64::from_le_bytes(body[9..17].try_into().unwrap());
        let wall_epoch_ms = i64::from_le_bytes(body[17..25].try_into().unwrap());
        if sequence != self.next_sequence
            || self
                .last_monotonic_ms
                .is_some_and(|previous| monotonic_ms < previous)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "receiver capture ordering",
            ));
        }
        self.next_sequence += 1;
        self.last_monotonic_ms = Some(monotonic_ms);
        Ok(CaptureRead::Record(CaptureRecord {
            sequence,
            kind,
            monotonic_ms,
            wall_epoch_ms,
            bytes: body[25..size].to_vec(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Memory {
        bytes: Vec<u8>,
        synced: usize,
    }
    impl Write for Memory {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.bytes.extend(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    impl CaptureSink for Memory {
        fn sync_data(&mut self) -> io::Result<()> {
            self.synced = self.bytes.len();
            Ok(())
        }
    }

    #[test]
    fn every_torn_tail_preserves_all_preceding_records() {
        let mut writer = CaptureWriter::new(Memory::default(), 1_000_000).unwrap();
        writer
            .append(CaptureKind::Received, 100, 123, b"first")
            .unwrap();
        assert_eq!(writer.checkpoint().unwrap(), 1);
        let durable = writer.sink.synced;
        writer
            .append(CaptureKind::Transmitted, 101, -456, b"second")
            .unwrap();
        let sink = writer.finish().unwrap();
        for end in durable..sink.bytes.len() {
            let mut reader = CaptureReader::new(&sink.bytes[..end]).unwrap();
            let CaptureRead::Record(first) = reader.next_record().unwrap() else {
                panic!()
            };
            assert_eq!(first.bytes, b"first");
            assert!(matches!(
                reader.next_record().unwrap(),
                CaptureRead::End | CaptureRead::TornTail
            ));
        }
        let mut reader = CaptureReader::new(sink.bytes.as_slice()).unwrap();
        assert!(matches!(
            reader.next_record().unwrap(),
            CaptureRead::Record(_)
        ));
        let CaptureRead::Record(second) = reader.next_record().unwrap() else {
            panic!()
        };
        assert_eq!(
            second.wall_epoch_ms, -456,
            "wall-clock adjustment must not reorder monotonic capture"
        );
        assert_eq!(second.bytes, b"second");
    }

    #[test]
    fn corruption_and_quota_are_not_reported_as_successful_capture() {
        let mut writer = CaptureWriter::new(Memory::default(), 1000).unwrap();
        writer
            .append(CaptureKind::Received, 1, 1, b"payload")
            .unwrap();
        let mut bytes = writer.finish().unwrap().bytes;
        bytes[40] ^= 1;
        let mut reader = CaptureReader::new(bytes.as_slice()).unwrap();
        assert!(reader.next_record().is_err());
        let mut writer = CaptureWriter::new(Memory::default(), 100).unwrap();
        writer
            .append(CaptureKind::Received, 1, 1, b"prior evidence")
            .unwrap();
        assert!(writer.sink.synced < writer.sink.bytes.len());
        assert!(writer
            .append(CaptureKind::Received, 2, 2, &[0; 200])
            .is_err());
        assert!(writer.checkpoint().is_err());
        assert_eq!(writer.sink.synced, writer.sink.bytes.len());
        assert!(writer
            .append(CaptureKind::Received, 3, 3, b"must stay failed")
            .is_err());
    }
}
