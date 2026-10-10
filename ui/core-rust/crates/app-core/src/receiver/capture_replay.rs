// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Passive replay of an app-exported private archive. No extraction, credentials,
//! radio access, or writes. Authentication is observed, not independently verified.

use std::{
    collections::BTreeMap,
    io::{self, Read, Seek},
};

use super::{
    capture::{CaptureClock, CaptureKind, CaptureRead, CaptureReader},
    capture_files::CapturePolicy,
    framing::ReceiveSequence,
    ApplicationRecord, ApplicationStream, Frame, Framer,
};

#[derive(Debug, Default, serde::Serialize)]
pub struct ReplaySummary {
    pub sessions: usize,
    pub segments: usize,
    pub records: u64,
    pub connections: u64,
    pub received_bytes: u64,
    pub write_requests: u64,
    pub completed_writes: u64,
    pub messages: u64,
    pub duplicate_frames: u64,
    pub torn_tails: u64,
    pub incomplete_connections: u64,
}

pub enum ReplayEvent {
    Connected,
    Disconnected,
    Message(ApplicationRecord),
}

#[derive(Default)]
struct Stream {
    connected: bool,
    framer: Framer,
    sequence: ReceiveSequence,
    application: ApplicationStream,
    clock: Option<CaptureClock>,
}

impl Stream {
    fn finish(
        &mut self,
        summary: &mut ReplaySummary,
        emit: &mut impl FnMut(CaptureClock, ReplayEvent),
    ) {
        if self.framer.is_incomplete() || self.application.is_incomplete() {
            summary.incomplete_connections += 1;
        }
        if self.connected {
            emit(self.clock.unwrap(), ReplayEvent::Disconnected);
        }
        self.connected = false;
        self.framer.reset();
        self.application.reset();
        self.sequence = ReceiveSequence::default();
    }

    fn frame(
        &mut self,
        frame: Frame,
        clock: CaptureClock,
        summary: &mut ReplaySummary,
        emit: &mut impl FnMut(CaptureClock, ReplayEvent),
    ) -> io::Result<()> {
        if !matches!(frame.kind, 2 | 3) {
            return Err(invalid("unknown transport kind"));
        }
        if frame.payload.is_empty() {
            return Ok(());
        }
        if !self.sequence.accept(&frame).map_err(invalid)? {
            summary.duplicate_frames += 1;
            return Ok(());
        }
        if frame.kind == 3 {
            self.application.reset();
            return Ok(());
        }
        for record in self.application.feed(&frame.payload).map_err(invalid)? {
            if matches!(record, ApplicationRecord::Message { .. }) {
                summary.messages += 1;
                emit(clock, ReplayEvent::Message(record));
            }
        }
        Ok(())
    }

    fn segment(
        &mut self,
        input: impl Read,
        summary: &mut ReplaySummary,
        emit: &mut impl FnMut(CaptureClock, ReplayEvent),
    ) -> io::Result<bool> {
        let mut reader = CaptureReader::new(input)?;
        loop {
            let record = match reader.next_record()? {
                CaptureRead::End => return Ok(false),
                CaptureRead::TornTail => {
                    summary.torn_tails += 1;
                    return Ok(true);
                }
                CaptureRead::Record(record) => record,
            };
            let clock = CaptureClock {
                monotonic_ms: record.monotonic_ms,
                wall_epoch_ms: record.wall_epoch_ms,
            };
            if self
                .clock
                .is_some_and(|previous| previous.monotonic_ms > clock.monotonic_ms)
            {
                return Err(invalid("capture clock regressed across segments"));
            }
            self.clock = Some(clock);
            summary.records += 1;
            match record.kind {
                CaptureKind::Connected => {
                    self.finish(summary, emit);
                    self.connected = true;
                    summary.connections += 1;
                    emit(clock, ReplayEvent::Connected);
                }
                CaptureKind::ConnectionRequested | CaptureKind::Disconnected => {
                    self.finish(summary, emit)
                }
                CaptureKind::Received => {
                    if !self.connected {
                        return Err(invalid("RX outside a captured connection"));
                    }
                    summary.received_bytes += record.bytes.len() as u64;
                    for frame in self.framer.feed(&record.bytes).map_err(invalid)? {
                        self.frame(frame, clock, summary, emit)?;
                    }
                }
                CaptureKind::WriteRequested => summary.write_requests += 1,
                CaptureKind::WriteCompleted => summary.completed_writes += 1,
                CaptureKind::Transmitted | CaptureKind::Diagnostic => {}
            }
        }
    }
}

/// Session IDs are random, not temporal. Each session is replayed independently;
/// only its numbered segments have ordering. Never merge across boot clocks.
pub fn replay_export(
    input: impl Read + Seek,
    mut emit: impl FnMut(CaptureClock, ReplayEvent),
) -> io::Result<ReplaySummary> {
    let mut archive = zip::ZipArchive::new(input).map_err(invalid)?;
    let limits = CapturePolicy::default();
    if archive.is_empty() || archive.len() > limits.max_segments {
        return Err(invalid("invalid capture archive segment count"));
    }
    let mut sessions = BTreeMap::<String, BTreeMap<u32, usize>>::new();
    let mut bytes = 0u64;
    for index in 0..archive.len() {
        let file = archive.by_index(index).map_err(invalid)?;
        let name = file.name();
        let (session, segment) = name
            .strip_suffix(".abrc")
            .and_then(|name| name.split_once('-'))
            .ok_or_else(|| invalid("unexpected capture archive entry"))?;
        if session.len() != 32
            || !session.bytes().all(|b| b.is_ascii_hexdigit())
            || segment.len() != 8
            || !segment.bytes().all(|b| b.is_ascii_digit())
            || file.is_dir()
            || file.is_symlink()
            || file.size() > limits.segment_bytes
        {
            return Err(invalid("invalid capture archive entry"));
        }
        bytes = bytes
            .checked_add(file.size())
            .ok_or_else(|| invalid("capture archive size overflow"))?;
        if bytes > limits.total_bytes {
            return Err(invalid("capture archive exceeds size limit"));
        }
        let number = segment.parse().map_err(invalid)?;
        if sessions
            .entry(session.into())
            .or_default()
            .insert(number, index)
            .is_some()
        {
            return Err(invalid("duplicate capture segment"));
        }
    }
    let mut summary = ReplaySummary {
        sessions: sessions.len(),
        segments: archive.len(),
        ..Default::default()
    };
    for segments in sessions.values() {
        let mut stream = Stream::default();
        let mut torn = false;
        for (expected, (number, index)) in segments.iter().enumerate() {
            if *number != expected as u32 || torn {
                return Err(invalid("missing segment or data after torn capture tail"));
            }
            let file = archive.by_index(*index).map_err(invalid)?;
            torn = stream.segment(file.take(limits.segment_bytes + 1), &mut summary, &mut emit)?;
        }
        stream.finish(&mut summary, &mut emit);
    }
    Ok(summary)
}

fn invalid(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::super::capture::{CaptureSink, CaptureWriter};
    use super::*;
    use std::io::{Cursor, Write};

    #[derive(Default)]
    struct Sink(Vec<u8>);
    impl Write for Sink {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    impl CaptureSink for Sink {
        fn sync_data(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn segment(records: &[(CaptureKind, u64, &[u8])]) -> Vec<u8> {
        let mut writer = CaptureWriter::new(Sink::default(), 4096).unwrap();
        for (kind, time, bytes) in records {
            writer
                .append(*kind, *time, 1000 + *time as i64, bytes)
                .unwrap();
        }
        writer.finish().unwrap().0
    }
    fn archive(entries: Vec<(String, Vec<u8>)>) -> Cursor<Vec<u8>> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in entries {
            writer
                .start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(&bytes).unwrap();
        }
        writer.finish().unwrap()
    }
    fn name(session: u32, number: u32) -> String {
        format!("{session:032x}-{number:08}.abrc")
    }
    fn frame(sequence: u8) -> Vec<u8> {
        // One complete application message, intentionally uninterpreted.
        let mut payload = vec![0x01, 11, 0, 12];
        payload.extend_from_slice(&0x1234u32.to_le_bytes());
        payload.extend_from_slice(&[1, 2, 3]);
        Frame {
            kind: 2,
            sequence,
            acknowledgement: 0,
            payload,
        }
        .encode()
        .unwrap()
    }

    #[test]
    fn segmented_reads_retransmission_and_reconnect_replay_exactly_once() {
        let raw = frame(254);
        let a = segment(&[
            (CaptureKind::Connected, 10, &[]),
            (CaptureKind::Received, 11, &raw[..5]),
        ]);
        let b = segment(&[
            (CaptureKind::Received, 12, &raw[5..]),
            (CaptureKind::Received, 13, &raw),
            (CaptureKind::WriteRequested, 14, &[0; 8]),
            (CaptureKind::Disconnected, 15, &[]),
        ]);
        let c = segment(&[
            (CaptureKind::Connected, 1, &[]),
            (CaptureKind::Received, 2, &raw),
        ]);
        let mut messages = Vec::new();
        let summary = replay_export(
            archive(vec![(name(1, 1), b), (name(2, 0), c), (name(1, 0), a)]),
            |clock, event| {
                if let ReplayEvent::Message(message) = event {
                    messages.push((clock.monotonic_ms, message));
                }
            },
        )
        .unwrap();
        assert_eq!(summary.connections, 2);
        assert_eq!(summary.messages, 2);
        assert_eq!(summary.duplicate_frames, 1);
        assert_eq!(summary.write_requests, 1);
        assert_eq!(summary.completed_writes, 0, "requested is not completed");
        assert_eq!(summary.incomplete_connections, 0);
        assert_eq!(
            messages.iter().map(|(time, _)| *time).collect::<Vec<_>>(),
            [12, 2]
        );
    }

    #[test]
    fn torn_tail_retains_prefix_but_corruption_and_missing_segments_are_errors() {
        let raw = frame(1);
        let mut bytes = segment(&[
            (CaptureKind::Connected, 0, &[]),
            (CaptureKind::Received, 1, &raw),
            (CaptureKind::Disconnected, 2, &[]),
        ]);
        bytes.truncate(bytes.len() - 5);
        let summary = replay_export(archive(vec![(name(1, 0), bytes.clone())]), |_, _| {}).unwrap();
        assert_eq!((summary.messages, summary.torn_tails), (1, 1));
        assert!(replay_export(archive(vec![(name(1, 1), bytes.clone())]), |_, _| {}).is_err());
        assert!(replay_export(
            archive(vec![
                (name(1, 0), bytes.clone()),
                (name(1, 1), segment(&[]))
            ]),
            |_, _| {}
        )
        .is_err());
        bytes[40] ^= 1;
        assert!(replay_export(archive(vec![(name(1, 0), bytes)]), |_, _| {}).is_err());
    }

    #[test]
    fn sequence_gaps_and_cross_segment_clock_regressions_are_not_healed() {
        let first = frame(1);
        let gap = frame(3);
        let a = segment(&[
            (CaptureKind::Connected, 10, &[]),
            (CaptureKind::Received, 11, &first),
        ]);
        let b = segment(&[(CaptureKind::Received, 12, &gap)]);
        assert!(replay_export(
            archive(vec![(name(1, 0), a.clone()), (name(1, 1), b)]),
            |_, _| {}
        )
        .unwrap_err()
        .to_string()
        .contains("sequence gap"));
        let b = segment(&[(CaptureKind::Disconnected, 5, &[])]);
        assert!(
            replay_export(archive(vec![(name(1, 0), a), (name(1, 1), b)]), |_, _| {})
                .unwrap_err()
                .to_string()
                .contains("clock regressed")
        );
    }

    #[test]
    fn archive_paths_are_never_extracted_or_interpreted() {
        assert!(replay_export(
            archive(vec![(format!("../{}", name(1, 0)), segment(&[]))]),
            |_, _| {}
        )
        .is_err());
    }
}
