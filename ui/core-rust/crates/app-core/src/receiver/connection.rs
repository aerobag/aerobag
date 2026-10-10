// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Single background owner of one receiver connection. Effects are plumbing,
//! not policy: adapters execute them and return tagged completions. Never run
//! this owner under the UI session mutex; recording and decoding can block.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};

use super::capture::{CaptureClock, CaptureKind, CaptureProgress, Recorder, MAX_CHUNK_BYTES};
use super::{Credentials, Protocol, ProtocolOutput, ProtocolPhase, ReceivedMessage, SERVICE_UUID};

const CONNECT_TIMEOUT_MS: u64 = 15_000;
const WRITE_TIMEOUT_MS: u64 = 5_000;
const CHECKPOINT_MS: u64 = 1_000;
const CHECKPOINT_BYTES: u64 = 256 * 1024;
const RETRY_INITIAL_MS: u64 = 5_000;
const RETRY_MAX_MS: u64 = 65_000;
const STABLE_CONNECTION_MS: u64 = 60_000;
const MAX_QUEUED_WRITE_BYTES: usize = 1024 * 1024;
// Owner recreation (not merely socket reconnect) must not recycle callback IDs.
static NEXT_CONNECTION_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectionId(pub(super) u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteId(pub(super) u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoFailure {
    Connection,
    Read,
    Write,
    EndOfStream,
    Permission,
    BluetoothDisabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    Io(IoFailure),
    Protocol(&'static str),
    ConnectDeadline,
    WriteDeadline,
    WriteQueueLimit,
    Capture,
    CaptureFull,
    ClockRegressed,
    UnexpectedCompletion,
    ConnectionIdsExhausted,
}

impl Fault {
    pub fn description(self) -> &'static str {
        match self {
            Self::Io(IoFailure::Connection) => "Receiver connection failed",
            Self::Io(IoFailure::Read | IoFailure::EndOfStream) => "Receiver connection lost",
            Self::Io(IoFailure::Write) => "Receiver write failed; transmitted extent unknown",
            Self::Io(IoFailure::Permission) => "Bluetooth permission required",
            Self::Io(IoFailure::BluetoothDisabled) => "Bluetooth is disabled",
            Self::Protocol(reason) => reason,
            Self::ConnectDeadline => "Receiver connection timed out",
            Self::WriteDeadline => "Receiver write timed out; transmitted extent unknown",
            Self::WriteQueueLimit => "Receiver output queue limit reached",
            Self::Capture => {
                "Receiver capture unavailable; connection stopped to preserve evidence"
            }
            Self::CaptureFull => {
                "Receiver capture storage is full; export and remove captures before reconnecting"
            }
            Self::ClockRegressed => "Receiver monotonic clock regressed",
            Self::UnexpectedCompletion => "Unexpected receiver IO completion",
            Self::ConnectionIdsExhausted => {
                "Receiver connection IDs exhausted; restart application"
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionPhase {
    Stopped,
    Connecting,
    Protocol(ProtocolPhase),
    WaitingToRetry { at_monotonic_ms: u64 },
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionStatus {
    pub phase: ConnectionPhase,
    pub last_fault: Option<Fault>,
    pub last_receive_monotonic_ms: Option<u64>,
    pub captured: CaptureProgress,
    pub decoded_messages: u64,
    pub malformed_messages: u64,
    pub ignored_stale_callbacks: u64,
}

// Neither Debug nor Serialize: commands contain authentication and raw data.
pub enum Effect {
    Connect {
        id: ConnectionId,
        device: String,
        service_uuid: &'static str,
    },
    Close {
        id: ConnectionId,
    },
    Write {
        id: ConnectionId,
        write_id: WriteId,
        bytes: Vec<u8>,
    },
}

pub enum Event<'a> {
    Connected(ConnectionId),
    Received(ConnectionId, &'a [u8]),
    Written(ConnectionId, WriteId),
    Failed(ConnectionId, IoFailure),
    Tick,
}

#[derive(Default)]
pub struct Output {
    pub effects: Vec<Effect>,
    /// Move to product consumers after processing effects. A UI stall must not
    /// stop raw capture; the application handoff needs its own bounded mailbox.
    pub messages: Vec<ReceivedMessage>,
}

struct PendingWrite {
    id: WriteId,
    since_ms: u64,
}

pub struct Connection<R: Recorder> {
    credentials: Credentials,
    recorder: R,
    device: Option<String>,
    active: Option<ConnectionId>,
    protocol: Option<Protocol>,
    status: ConnectionStatus,
    pending_write: Option<PendingWrite>,
    writes: VecDeque<Vec<u8>>,
    next_write_id: u64,
    queued_bytes: usize,
    connect_since_ms: u64,
    receiving_since_ms: Option<u64>,
    next_retry_ms: u64,
    last_clock_ms: u64,
    last_checkpoint_ms: u64,
    checkpoint_bytes: u64,
}

impl<R: Recorder> Connection<R> {
    pub fn new(credentials: Credentials, recorder: R, clock: CaptureClock) -> Self {
        let captured = recorder.progress();
        Self {
            credentials,
            recorder,
            device: None,
            active: None,
            protocol: None,
            status: ConnectionStatus {
                phase: ConnectionPhase::Stopped,
                last_fault: None,
                last_receive_monotonic_ms: None,
                captured,
                decoded_messages: 0,
                malformed_messages: 0,
                ignored_stale_callbacks: 0,
            },
            pending_write: None,
            writes: VecDeque::new(),
            next_write_id: 0,
            queued_bytes: 0,
            connect_since_ms: clock.monotonic_ms,
            receiving_since_ms: None,
            next_retry_ms: RETRY_INITIAL_MS,
            last_clock_ms: clock.monotonic_ms,
            last_checkpoint_ms: clock.monotonic_ms,
            checkpoint_bytes: captured.written_bytes,
        }
    }

    pub fn status(&self) -> ConnectionStatus {
        let mut status = self.status.clone();
        status.captured = self.recorder.progress();
        status
    }

    /// The adapter schedules this deadline, never chooses its own retry period.
    pub fn next_wake_ms(&self) -> Option<u64> {
        let periodic = self.last_clock_ms.saturating_add(250);
        match self.status.phase {
            ConnectionPhase::Stopped | ConnectionPhase::Blocked => None,
            ConnectionPhase::WaitingToRetry { at_monotonic_ms } => Some(at_monotonic_ms),
            _ => Some(periodic.min(self.last_checkpoint_ms.saturating_add(CHECKPOINT_MS))),
        }
    }

    pub fn start(&mut self, device: String, clock: CaptureClock) -> Output {
        let mut output = self.stop(clock);
        // Storage/clock failure needs a new, successfully opened owner, not a
        // button which makes the same broken recorder appear healthy again.
        if self.status.phase == ConnectionPhase::Blocked {
            return output;
        }
        self.device = Some(device);
        self.next_retry_ms = RETRY_INITIAL_MS;
        self.open(clock, &mut output);
        output
    }

    pub fn stop(&mut self, clock: CaptureClock) -> Output {
        let mut output = Output::default();
        if !self.clock(clock, &mut output) {
            return output;
        }
        self.device = None;
        self.disconnect(clock, &mut output);
        if self.status.phase != ConnectionPhase::Blocked {
            self.status.phase = ConnectionPhase::Stopped;
        }
        output
    }

    /// Clock is sampled inside the serialized owner at consumption time, not in
    /// an arbitrary socket callback before it queues behind a newer timer event.
    pub fn update(&mut self, event: Event<'_>, clock: CaptureClock) -> Output {
        let mut output = Output::default();
        let id = match &event {
            Event::Connected(id)
            | Event::Received(id, _)
            | Event::Written(id, _)
            | Event::Failed(id, _) => Some(*id),
            Event::Tick => None,
        };
        if id.is_some() && id != self.active {
            self.status.ignored_stale_callbacks += 1;
            if let Event::Connected(id) = event {
                output.effects.push(Effect::Close { id });
            }
            return output;
        }
        if !self.clock(clock, &mut output) {
            return output;
        }
        if matches!(
            self.status.phase,
            ConnectionPhase::Blocked | ConnectionPhase::Stopped
        ) {
            return output;
        }
        match event {
            Event::Connected(_) => {
                if self.status.phase != ConnectionPhase::Connecting {
                    self.fail(Fault::UnexpectedCompletion, clock, &mut output);
                } else if self.record(CaptureKind::Connected, clock, &[], &mut output) {
                    match Protocol::connect(self.credentials.for_connection(), clock.monotonic_ms) {
                        Ok((protocol, batch)) => {
                            self.protocol = Some(protocol);
                            self.protocol_output(batch, clock, &mut output);
                        }
                        Err(error) => self.fail(Fault::Protocol(error.0), clock, &mut output),
                    }
                }
            }
            Event::Received(_, bytes) => {
                // The IO adapter must bound individual reads. Split large reads
                // here too so framing errors never bypass raw recording.
                for chunk in bytes.chunks(MAX_CHUNK_BYTES) {
                    if !self.record(CaptureKind::Received, clock, chunk, &mut output) {
                        return output;
                    }
                }
                self.status.last_receive_monotonic_ms = Some(clock.monotonic_ms);
                if let Some(protocol) = &mut self.protocol {
                    match protocol.receive(bytes, clock.monotonic_ms) {
                        Ok(batch) => self.protocol_output(batch, clock, &mut output),
                        Err(error) => self.fail(Fault::Protocol(error.0), clock, &mut output),
                    }
                } else {
                    self.fail(Fault::UnexpectedCompletion, clock, &mut output);
                }
            }
            Event::Written(_, write_id) => {
                if !self
                    .pending_write
                    .as_ref()
                    .is_some_and(|write| write.id == write_id)
                {
                    self.fail(Fault::UnexpectedCompletion, clock, &mut output);
                } else if self.record(
                    CaptureKind::WriteCompleted,
                    clock,
                    &write_id.0.to_le_bytes(),
                    &mut output,
                ) {
                    self.pending_write = None;
                    self.dispatch_write(clock, &mut output);
                }
            }
            Event::Failed(_, reason) => self.fail(Fault::Io(reason), clock, &mut output),
            Event::Tick => self.tick(clock, &mut output),
        }
        if self.active.is_some()
            && (clock.monotonic_ms.saturating_sub(self.last_checkpoint_ms) >= CHECKPOINT_MS
                || self
                    .recorder
                    .progress()
                    .written_bytes
                    .saturating_sub(self.checkpoint_bytes)
                    >= CHECKPOINT_BYTES)
        {
            self.checkpoint(clock, &mut output);
        }
        output
    }

    fn tick(&mut self, clock: CaptureClock, output: &mut Output) {
        if let ConnectionPhase::WaitingToRetry { at_monotonic_ms } = self.status.phase {
            if clock.monotonic_ms >= at_monotonic_ms {
                self.open(clock, output);
            }
            return;
        }
        if self.status.phase == ConnectionPhase::Connecting {
            if clock.monotonic_ms.saturating_sub(self.connect_since_ms) >= CONNECT_TIMEOUT_MS {
                self.fail(Fault::ConnectDeadline, clock, output);
            }
            return;
        }
        if let Some(write) = &self.pending_write {
            if clock.monotonic_ms.saturating_sub(write.since_ms) >= WRITE_TIMEOUT_MS {
                self.fail(Fault::WriteDeadline, clock, output);
            }
            return;
        }
        if let Some(protocol) = &mut self.protocol {
            match protocol.tick(clock.monotonic_ms) {
                Ok(batch) => self.protocol_output(batch, clock, output),
                Err(error) => self.fail(Fault::Protocol(error.0), clock, output),
            }
        }
    }

    fn open(&mut self, clock: CaptureClock, output: &mut Output) {
        let Ok(id) =
            NEXT_CONNECTION_ID.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
        else {
            self.block(Fault::ConnectionIdsExhausted, output);
            return;
        };
        let id = ConnectionId(id);
        if !self.record(
            CaptureKind::ConnectionRequested,
            clock,
            &id.0.to_le_bytes(),
            output,
        ) || !self.checkpoint(clock, output)
        {
            return;
        }
        self.active = Some(id);
        self.connect_since_ms = clock.monotonic_ms;
        self.status.phase = ConnectionPhase::Connecting;
        output.effects.push(Effect::Connect {
            id,
            device: self
                .device
                .as_ref()
                .expect("open has selected device")
                .clone(),
            service_uuid: SERVICE_UUID,
        });
    }

    fn protocol_output(&mut self, batch: ProtocolOutput, clock: CaptureClock, output: &mut Output) {
        let phase = self
            .protocol
            .as_ref()
            .expect("protocol output has protocol")
            .phase();
        self.status.phase = ConnectionPhase::Protocol(phase);
        if phase == ProtocolPhase::Receiving {
            let since = *self.receiving_since_ms.get_or_insert(clock.monotonic_ms);
            if clock.monotonic_ms.saturating_sub(since) >= STABLE_CONNECTION_MS {
                self.next_retry_ms = RETRY_INITIAL_MS;
            }
            self.status.last_fault = None;
        }
        for bytes in batch.transmissions {
            if bytes.len() + 8 > MAX_CHUNK_BYTES
                || self.queued_bytes.saturating_add(bytes.len()) > MAX_QUEUED_WRITE_BYTES
            {
                self.fail(Fault::WriteQueueLimit, clock, output);
                return;
            }
            self.queued_bytes += bytes.len();
            self.writes.push_back(bytes);
        }
        self.dispatch_write(clock, output);
        if self.active.is_some() {
            for message in batch.messages {
                if message.product.is_ok() {
                    self.status.decoded_messages += 1;
                } else {
                    self.status.malformed_messages += 1;
                }
                output.messages.push(message);
            }
        }
    }

    fn dispatch_write(&mut self, clock: CaptureClock, output: &mut Output) {
        if self.pending_write.is_some() {
            return;
        }
        let Some(bytes) = self.writes.pop_front() else {
            return;
        };
        self.queued_bytes -= bytes.len();
        self.next_write_id += 1;
        let write_id = WriteId(self.next_write_id);
        let mut intent = write_id.0.to_le_bytes().to_vec();
        intent.extend_from_slice(&bytes);
        if !self.record(CaptureKind::WriteRequested, clock, &intent, output) {
            return;
        }
        self.pending_write = Some(PendingWrite {
            id: write_id,
            since_ms: clock.monotonic_ms,
        });
        output.effects.push(Effect::Write {
            id: self.active.expect("write has connection"),
            write_id,
            bytes,
        });
    }

    fn record(
        &mut self,
        kind: CaptureKind,
        clock: CaptureClock,
        bytes: &[u8],
        output: &mut Output,
    ) -> bool {
        if let Err(error) = self.recorder.record(kind, clock, bytes) {
            self.block(capture_fault(&error), output);
            return false;
        }
        true
    }

    fn checkpoint(&mut self, clock: CaptureClock, output: &mut Output) -> bool {
        if let Err(error) = self.recorder.checkpoint() {
            self.block(capture_fault(&error), output);
            return false;
        }
        self.last_checkpoint_ms = clock.monotonic_ms;
        self.checkpoint_bytes = self.recorder.progress().written_bytes;
        true
    }

    fn clock(&mut self, clock: CaptureClock, output: &mut Output) -> bool {
        if clock.monotonic_ms < self.last_clock_ms {
            self.block(Fault::ClockRegressed, output);
            return false;
        }
        self.last_clock_ms = clock.monotonic_ms;
        true
    }

    fn clear_connection(&mut self, output: &mut Output) {
        if let Some(id) = self.active.take() {
            output.effects.push(Effect::Close { id });
        }
        self.protocol = None;
        self.pending_write = None;
        self.writes.clear();
        self.queued_bytes = 0;
        self.receiving_since_ms = None;
    }

    fn disconnect(&mut self, clock: CaptureClock, output: &mut Output) {
        if self.active.is_some() {
            self.record(CaptureKind::Disconnected, clock, &[], output);
            self.checkpoint(clock, output);
        }
        self.clear_connection(output);
    }

    fn fail(&mut self, fault: Fault, clock: CaptureClock, output: &mut Output) {
        self.status.last_fault = Some(fault);
        if !self.record(
            CaptureKind::Diagnostic,
            clock,
            fault.description().as_bytes(),
            output,
        ) {
            return;
        }
        self.disconnect(clock, output);
        if self.status.phase != ConnectionPhase::Blocked {
            self.status.phase = ConnectionPhase::WaitingToRetry {
                at_monotonic_ms: clock.monotonic_ms.saturating_add(self.next_retry_ms),
            };
            self.next_retry_ms = (self.next_retry_ms * 2).min(RETRY_MAX_MS);
        }
        // A command prepared earlier in this turn must not escape a later fault.
        output
            .effects
            .retain(|effect| matches!(effect, Effect::Close { .. }));
        output.messages.clear();
    }

    fn block(&mut self, fault: Fault, output: &mut Output) {
        self.status.last_fault = Some(fault);
        self.status.phase = ConnectionPhase::Blocked;
        // Best effort preserves a complete prefix, without claiming the failed
        // recorder is healthy. No automatic deletion/retry can hide this fault.
        let _ = self.recorder.checkpoint();
        self.clear_connection(output);
        output
            .effects
            .retain(|effect| matches!(effect, Effect::Close { .. }));
        output.messages.clear();
    }
}

fn capture_fault(error: &std::io::Error) -> Fault {
    if error.kind() == std::io::ErrorKind::StorageFull {
        Fault::CaptureFull
    } else {
        Fault::Capture
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::receiver::{ApplicationRecord, ApplicationStream, Frame, Framer, Product};
    use md5::{Digest, Md5};
    use std::{cell::RefCell, io, rc::Rc};

    #[derive(Default)]
    struct Storage {
        records: Vec<(CaptureKind, Vec<u8>)>,
        durable: u64,
        bytes: u64,
        fail_append: bool,
        fail_sync: bool,
    }
    #[derive(Clone, Default)]
    struct Memory(Rc<RefCell<Storage>>);
    impl Recorder for Memory {
        fn record(&mut self, kind: CaptureKind, _: CaptureClock, bytes: &[u8]) -> io::Result<()> {
            let mut store = self.0.borrow_mut();
            if store.fail_append {
                return Err(io::Error::other("injected append failure"));
            }
            store.records.push((kind, bytes.to_vec()));
            store.bytes += bytes.len() as u64 + 61;
            Ok(())
        }
        fn checkpoint(&mut self) -> io::Result<()> {
            let mut store = self.0.borrow_mut();
            if store.fail_sync {
                return Err(io::Error::other("injected sync failure"));
            }
            store.durable = store.records.len() as u64;
            Ok(())
        }
        fn progress(&self) -> CaptureProgress {
            let store = self.0.borrow();
            CaptureProgress {
                written_records: store.records.len() as u64,
                durable_records: store.durable,
                written_bytes: store.bytes,
            }
        }
    }
    fn clock(ms: u64) -> CaptureClock {
        CaptureClock {
            monotonic_ms: ms,
            wall_epoch_ms: 123_000 + ms as i64,
        }
    }
    fn connection() -> (Connection<Memory>, Memory) {
        let memory = Memory::default();
        let connection = Connection::new(
            Credentials::new(&[7; 48], b"GRM_SALTsynthetic").unwrap(),
            memory.clone(),
            clock(0),
        );
        (connection, memory)
    }
    fn open(connection: &mut Connection<Memory>, now: u64) -> ConnectionId {
        let output = connection.start("paired-device".into(), clock(now));
        let Effect::Connect {
            id, service_uuid, ..
        } = output.effects.last().unwrap()
        else {
            panic!()
        };
        assert_eq!(*service_uuid, SERVICE_UUID);
        *id
    }
    fn write(output: Output) -> (ConnectionId, WriteId, Vec<u8>) {
        assert_eq!(output.effects.len(), 1);
        let Effect::Write {
            id,
            write_id,
            bytes,
        } = output.effects.into_iter().next().unwrap()
        else {
            panic!()
        };
        (id, write_id, bytes)
    }
    fn message(id: u32, bytes: &[u8]) -> Vec<u8> {
        let mut record = vec![1];
        record.extend_from_slice(&((bytes.len() + 8) as u16).to_le_bytes());
        record.push(12);
        record.extend_from_slice(&id.to_le_bytes());
        record.extend_from_slice(bytes);
        record
    }

    struct Peer {
        id: ConnectionId,
        sequence: u8,
        acknowledgement: u8,
        now: u64,
        auth_response: Vec<u8>,
    }
    impl Peer {
        fn settle(
            &mut self,
            connection: &mut Connection<Memory>,
            output: Output,
        ) -> Vec<ReceivedMessage> {
            let mut effects = VecDeque::from(output.effects);
            let mut messages = output.messages;
            while let Some(effect) = effects.pop_front() {
                let Effect::Write {
                    id,
                    write_id,
                    bytes,
                } = effect
                else {
                    panic!("unexpected effect")
                };
                assert_eq!(id, self.id);
                for frame in Framer::default().feed(&bytes).unwrap() {
                    if !frame.payload.is_empty() {
                        self.acknowledgement = frame.sequence;
                    }
                    if frame.kind == 2 {
                        for record in ApplicationStream::default().feed(&frame.payload).unwrap() {
                            if let ApplicationRecord::Message {
                                message_id: 3,
                                data,
                                ..
                            } = record
                            {
                                let mut digest = Md5::new();
                                digest.update([7; 48]);
                                digest.update(&data[20..]);
                                self.auth_response = digest.finalize().to_vec();
                            }
                        }
                    }
                }
                let output = connection.update(Event::Written(id, write_id), clock(self.now));
                effects.extend(output.effects);
                messages.extend(output.messages);
            }
            messages
        }
        fn receive(
            &mut self,
            connection: &mut Connection<Memory>,
            kind: u8,
            payload: Vec<u8>,
        ) -> Vec<ReceivedMessage> {
            self.now += 1;
            self.sequence = self.sequence.wrapping_add(1);
            let bytes = Frame {
                kind,
                sequence: self.sequence,
                acknowledgement: self.acknowledgement,
                payload,
            }
            .encode()
            .unwrap();
            let mut messages = Vec::new();
            for chunk in bytes.chunks(3) {
                let output = connection.update(Event::Received(self.id, chunk), clock(self.now));
                messages.extend(self.settle(connection, output));
            }
            messages
        }
        fn authenticated(connection: &mut Connection<Memory>) -> Self {
            let id = open(connection, 0);
            let mut peer = Self {
                id,
                sequence: 0,
                acknowledgement: 0,
                now: 0,
                auth_response: Vec::new(),
            };
            let output = connection.update(Event::Connected(id), clock(0));
            peer.settle(connection, output);
            peer.receive(connection, 3, vec![1]);
            peer.receive(
                connection,
                2,
                message(
                    0,
                    &[2u32, 4]
                        .into_iter()
                        .flat_map(u32::to_le_bytes)
                        .collect::<Vec<_>>(),
                ),
            );
            peer.receive(connection, 2, message(2, &[8; 16]));
            assert!(!peer.auth_response.is_empty());
            peer.receive(
                connection,
                2,
                [
                    message(4, &peer.auth_response),
                    message(
                        0,
                        &[0u32, 0x1000, 0x2200]
                            .into_iter()
                            .flat_map(u32::to_le_bytes)
                            .collect::<Vec<_>>(),
                    ),
                ]
                .concat(),
            );
            peer.receive(connection, 2, message(0x1000, &[0x47, 6]));
            assert_eq!(
                connection.status().phase,
                ConnectionPhase::Protocol(ProtocolPhase::Receiving)
            );
            // Acknowledge final subscriptions without inventing a data payload.
            let ack = Frame {
                kind: 2,
                sequence: 0,
                acknowledgement: peer.acknowledgement,
                payload: Vec::new(),
            }
            .encode()
            .unwrap();
            assert!(connection
                .update(Event::Received(id, &ack), clock(peer.now))
                .effects
                .is_empty());
            peer
        }
    }

    #[test]
    fn real_handshake_split_reads_and_sensor_delivery_are_captured_before_consumption() {
        let (mut connection, memory) = connection();
        let mut peer = Peer::authenticated(&mut connection);
        let messages = peer.receive(&mut connection, 2, message(0x2200, &1234f32.to_le_bytes()));
        assert!(matches!(
            messages[0].product,
            Ok(Product::PressureUnverified { raw: Some(1234.0) })
        ));
        let records = &memory.0.borrow().records;
        assert!(records
            .iter()
            .any(|(kind, _)| *kind == CaptureKind::Received));
        assert_eq!(
            records
                .iter()
                .filter(|(kind, _)| *kind == CaptureKind::WriteRequested)
                .count(),
            records
                .iter()
                .filter(|(kind, _)| *kind == CaptureKind::WriteCompleted)
                .count()
        );
        assert!(!records
            .iter()
            .any(|(kind, _)| *kind == CaptureKind::Transmitted));
    }

    #[test]
    fn quiet_weather_does_not_disconnect_receiving_but_malformed_products_remain_diagnostic() {
        let (mut connection, _) = connection();
        let mut peer = Peer::authenticated(&mut connection);
        let messages = peer.receive(&mut connection, 2, message(0x2200, &[1]));
        assert!(messages[0].product.is_err());
        for now in [1000, 60_000, 120_000, 3_600_000] {
            let output = connection.update(Event::Tick, clock(now));
            assert!(output.effects.is_empty());
            assert_eq!(
                connection.status().phase,
                ConnectionPhase::Protocol(ProtocolPhase::Receiving)
            );
        }
        assert_eq!(connection.status().malformed_messages, 1);
        assert_eq!(
            connection.status().captured.durable_records,
            connection.status().captured.written_records
        );
    }

    #[test]
    fn failed_connection_backoff_caps_and_stop_prevents_reopen() {
        let (mut connection, _) = connection();
        let mut id = open(&mut connection, 0);
        let mut now = 0;
        for delay in [5000, 10_000, 20_000, 40_000, 65_000, 65_000] {
            let output = connection.update(Event::Failed(id, IoFailure::Connection), clock(now));
            assert!(matches!(output.effects.as_slice(), [Effect::Close { .. }]));
            assert_eq!(connection.next_wake_ms(), Some(now + delay));
            assert!(connection
                .update(Event::Tick, clock(now + delay - 1))
                .effects
                .is_empty());
            now += delay;
            let output = connection.update(Event::Tick, clock(now));
            let Effect::Connect { id: next, .. } = output.effects[0] else {
                panic!()
            };
            assert_ne!(id, next);
            id = next;
        }
        connection.stop(clock(now));
        assert_eq!(connection.next_wake_ms(), None);
        assert!(connection
            .update(Event::Tick, clock(now + 100_000))
            .effects
            .is_empty());
    }

    #[test]
    fn successful_socket_open_does_not_reset_repeated_setup_failure_backoff() {
        let (mut connection, _) = connection();
        let id = open(&mut connection, 0);
        connection.update(Event::Failed(id, IoFailure::Connection), clock(1));
        let output = connection.update(Event::Tick, clock(5001));
        let Effect::Connect { id, .. } = output.effects[0] else {
            panic!()
        };
        connection.update(Event::Connected(id), clock(5002));
        connection.update(Event::Failed(id, IoFailure::Read), clock(5003));
        assert_eq!(connection.next_wake_ms(), Some(15_003));
    }

    #[test]
    fn delayed_old_callbacks_never_touch_new_protocol_or_capture() {
        let (mut connection, memory) = connection();
        let old = open(&mut connection, 0);
        let (_, old_write, _) = write(connection.update(Event::Connected(old), clock(1)));
        let current = open(&mut connection, 2);
        let before = memory.0.borrow().records.len();
        for event in [
            Event::Received(old, b"stale"),
            Event::Written(old, old_write),
            Event::Failed(old, IoFailure::Read),
        ] {
            // Even a stale callback's old clock cannot poison the new owner.
            assert!(connection.update(event, clock(0)).effects.is_empty());
        }
        let output = connection.update(Event::Connected(old), clock(0));
        assert!(matches!(output.effects.as_slice(), [Effect::Close { id }] if *id == old));
        assert_eq!(memory.0.borrow().records.len(), before);
        assert_eq!(connection.active, Some(current));
        assert_eq!(connection.status().phase, ConnectionPhase::Connecting);
        assert_eq!(connection.status().ignored_stale_callbacks, 4);
    }

    #[test]
    fn recreated_owner_cannot_accept_callbacks_from_destroyed_owner() {
        let (mut old, _) = connection();
        let previous = open(&mut old, 0);
        drop(old);
        let (mut replacement, memory) = connection();
        let current = open(&mut replacement, 0);
        assert_ne!(previous, current);
        let recorded = memory.0.borrow().records.len();
        let output = replacement.update(Event::Connected(previous), clock(0));
        assert!(matches!(output.effects.as_slice(), [Effect::Close { id }] if *id == previous));
        assert_eq!(replacement.active, Some(current));
        assert_eq!(memory.0.borrow().records.len(), recorded);
    }

    #[test]
    fn capture_failure_latches_blocks_outputs_and_requires_new_owner() {
        for sync_failure in [false, true] {
            let (mut connection, memory) = connection();
            let id = open(&mut connection, 0);
            let (_, write_id, _) = write(connection.update(Event::Connected(id), clock(1)));
            if sync_failure {
                memory.0.borrow_mut().fail_sync = true;
            } else {
                memory.0.borrow_mut().fail_append = true;
            }
            let event = if sync_failure {
                Event::Tick
            } else {
                Event::Written(id, write_id)
            };
            let output = connection.update(event, clock(1000));
            assert!(
                matches!(output.effects.as_slice(), [Effect::Close { id: closed }] if *closed == id)
            );
            assert_eq!(connection.status().phase, ConnectionPhase::Blocked);
            assert_eq!(connection.status().last_fault, Some(Fault::Capture));
            memory.0.borrow_mut().fail_sync = false;
            memory.0.borrow_mut().fail_append = false;
            assert!(connection
                .start("another-device".into(), clock(2000))
                .effects
                .is_empty());
            assert!(connection
                .update(Event::Tick, clock(100_000))
                .effects
                .is_empty());
        }
    }

    #[test]
    fn capture_must_open_and_sync_before_socket_is_requested() {
        for sync_failure in [true, false] {
            let (mut connection, memory) = connection();
            memory.0.borrow_mut().fail_sync = sync_failure;
            memory.0.borrow_mut().fail_append = !sync_failure;
            assert!(connection
                .start("device".into(), clock(1))
                .effects
                .is_empty());
            assert_eq!(connection.status().last_fault, Some(Fault::Capture));
        }
    }

    #[test]
    fn failed_or_timed_out_write_is_not_recorded_as_transmitted_and_cannot_block_stop() {
        for fail_write in [true, false] {
            let (mut connection, memory) = connection();
            let id = open(&mut connection, 0);
            let (_, write_id, _) = write(connection.update(Event::Connected(id), clock(1)));
            assert!(memory
                .0
                .borrow()
                .records
                .iter()
                .any(|(kind, _)| *kind == CaptureKind::WriteRequested));
            let event = if fail_write {
                Event::Failed(id, IoFailure::Write)
            } else {
                Event::Tick
            };
            let output = connection.update(event, clock(5001));
            assert!(matches!(output.effects.as_slice(), [Effect::Close { .. }]));
            assert!(!memory
                .0
                .borrow()
                .records
                .iter()
                .any(|(kind, _)| *kind == CaptureKind::WriteCompleted));
            assert!(connection
                .update(Event::Written(id, write_id), clock(5002))
                .effects
                .is_empty());
            connection.stop(clock(5003));
            assert_eq!(connection.status().phase, ConnectionPhase::Stopped);
        }
    }

    #[test]
    fn framing_error_preserves_raw_bytes_and_connect_and_clock_deadlines_are_explicit() {
        let (mut connection, memory) = connection();
        let id = open(&mut connection, 0);
        let (id, write_id, _) = write(connection.update(Event::Connected(id), clock(1)));
        connection.update(Event::Written(id, write_id), clock(2));
        let mut bad = Frame {
            kind: 3,
            sequence: 1,
            acknowledgement: 1,
            payload: vec![5],
        }
        .encode()
        .unwrap();
        bad[7] ^= 1;
        connection.update(Event::Received(id, &bad), clock(3));
        assert!(memory
            .0
            .borrow()
            .records
            .iter()
            .any(|(kind, bytes)| *kind == CaptureKind::Received && bytes == &bad));
        assert!(matches!(
            connection.status().last_fault,
            Some(Fault::Protocol(_))
        ));
        let id = open(&mut connection, 4);
        let output = connection.update(Event::Tick, clock(15_004));
        assert!(
            matches!(output.effects.as_slice(), [Effect::Close { id: closed }] if *closed == id)
        );
        assert_eq!(connection.status().last_fault, Some(Fault::ConnectDeadline));
        connection.update(Event::Tick, clock(3));
        assert_eq!(connection.status().last_fault, Some(Fault::ClockRegressed));
        assert_eq!(connection.status().phase, ConnectionPhase::Blocked);
    }
}
