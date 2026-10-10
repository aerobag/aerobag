// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{BTreeSet, VecDeque};

use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};

use super::{
    decode_product, ApplicationRecord, ApplicationStream, DecodeError, Frame, Framer, Product,
    Result, WeatherKind,
};

const SETUP_TIMEOUT_MS: u64 = 15_000;
const RETRANSMIT_MS: u64 = 1_000;
const MAX_RETRIES: usize = 3;
const MAX_PENDING: usize = 32;
const NEGOTIATION: &[u8] = &[0x40, 0x0a, 1, 0, 4, 0xf4, 1, 0x96, 0, 0xb8, 0xab, 3, 0];

/// Intentionally neither Debug nor Serialize: never put authentication material
/// in app state snapshots, telemetry, or public fixtures.
pub struct Credentials {
    token: [u8; 48],
    bundle: Vec<u8>,
}

impl Credentials {
    pub(super) fn for_connection(&self) -> Self {
        Self {
            token: self.token,
            bundle: self.bundle.clone(),
        }
    }

    pub fn new(token: &[u8], bundle: &[u8]) -> Result<Self> {
        if token.len() != 48 || !bundle.starts_with(b"GRM_SALT") || bundle.len() > 4096 {
            return Err(DecodeError("invalid private receiver credentials"));
        }
        Ok(Self {
            token: token.try_into().unwrap(),
            bundle: bundle.to_vec(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolPhase {
    Negotiating,
    Capabilities,
    Challenge,
    ServerAuthentication,
    Product,
    Receiving,
    Failed,
}

#[derive(Debug)]
pub struct ReceivedMessage {
    pub message_id: u32,
    pub attributes: u8,
    pub product: Result<Product>,
}

#[derive(Default)]
pub struct ProtocolOutput {
    /// Write verbatim to the socket, and record privately, never to logcat.
    pub transmissions: Vec<Vec<u8>>,
    pub messages: Vec<ReceivedMessage>,
}

struct Pending {
    frame: Frame,
    last_sent_ms: u64,
    retries: usize,
}

/// All time arguments use one connection's monotonic clock. Create a fresh
/// Protocol on reconnect; no assembly, sequence, or nonce survives a connection.
pub struct Protocol {
    phase: ProtocolPhase,
    credentials: Credentials,
    nonce: [u8; 16],
    expected_response: Option<[u8; 16]>,
    framer: Framer,
    application: ApplicationStream,
    advertised: BTreeSet<u32>,
    next_tx_sequence: u8,
    receive_sequence: super::framing::ReceiveSequence,
    acknowledgement: u8,
    pending: VecDeque<Pending>,
    phase_since_ms: u64,
    last_clock_ms: u64,
}

impl Protocol {
    pub fn connect(credentials: Credentials, now_ms: u64) -> Result<(Self, ProtocolOutput)> {
        let mut nonce = [0u8; 16];
        getrandom::getrandom(&mut nonce)
            .map_err(|_| DecodeError("receiver nonce generation failed"))?;
        Self::with_nonce(credentials, now_ms, nonce)
    }

    fn with_nonce(
        credentials: Credentials,
        now_ms: u64,
        nonce: [u8; 16],
    ) -> Result<(Self, ProtocolOutput)> {
        let mut state = Self {
            phase: ProtocolPhase::Negotiating,
            credentials,
            nonce,
            expected_response: None,
            framer: Framer::default(),
            application: ApplicationStream::default(),
            advertised: BTreeSet::new(),
            next_tx_sequence: 1,
            receive_sequence: super::framing::ReceiveSequence::default(),
            acknowledgement: 0,
            pending: VecDeque::new(),
            phase_since_ms: now_ms,
            last_clock_ms: now_ms,
        };
        let mut output = ProtocolOutput::default();
        state.send(3, NEGOTIATION.to_vec(), now_ms, &mut output)?;
        Ok((state, output))
    }

    pub fn phase(&self) -> ProtocolPhase {
        self.phase
    }
    pub fn advertised_ids(&self) -> &BTreeSet<u32> {
        &self.advertised
    }

    /// Raw RX must already be recorded by the IO owner. Product errors are
    /// diagnostic events; framing/auth/order errors require a fresh connection.
    pub fn receive(&mut self, bytes: &[u8], now_ms: u64) -> Result<ProtocolOutput> {
        let result = self.receive_inner(bytes, now_ms);
        if result.is_err() {
            self.fail();
        }
        result
    }

    fn receive_inner(&mut self, bytes: &[u8], now_ms: u64) -> Result<ProtocolOutput> {
        self.check_clock(now_ms)?;
        let mut output = ProtocolOutput::default();
        for frame in self.framer.feed(bytes)? {
            if frame.kind != 2 && frame.kind != 3 {
                return Err(DecodeError("unknown transport kind"));
            }
            if let Some(index) = self
                .pending
                .iter()
                .position(|pending| pending.frame.sequence == frame.acknowledgement)
            {
                self.pending.drain(..=index);
            }
            if frame.payload.is_empty() {
                continue;
            }
            if !self.receive_sequence.accept(&frame)? {
                self.send(2, Vec::new(), now_ms, &mut output)?;
                continue;
            }
            self.acknowledgement = frame.sequence;
            let before_tx = output.transmissions.len();
            if self.phase == ProtocolPhase::Negotiating {
                if frame.kind != 3 {
                    return Err(DecodeError("expected receiver negotiation"));
                }
                self.send(2, control(&[subscription(0, 20)]), now_ms, &mut output)?;
                self.transition(ProtocolPhase::Capabilities, now_ms);
                continue;
            }
            if frame.kind != 2 {
                return Err(DecodeError("unexpected repeated negotiation"));
            }
            let records = self.application.feed(&frame.payload)?;
            // Inspect the complete batch before acting on auth: the post-auth
            // supported-ID list can occur later in this same transport payload.
            for record in &records {
                if let ApplicationRecord::Message {
                    message_id: 0,
                    attributes: 0,
                    data,
                } = record
                {
                    if data.len() % 4 != 0 || data.len() > 4096 * 4 {
                        return Err(DecodeError("supported message list length"));
                    }
                    self.advertised = data
                        .chunks_exact(4)
                        .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
                        .collect();
                }
            }
            if self.phase == ProtocolPhase::Capabilities
                && self.advertised.contains(&2)
                && self.advertised.contains(&4)
            {
                self.send(
                    2,
                    [
                        control(&[subscription(4, 19), subscription(2, 19)]),
                        supported_ids(),
                    ]
                    .concat(),
                    now_ms,
                    &mut output,
                )?;
                self.transition(ProtocolPhase::Challenge, now_ms);
            }
            for record in records {
                match record {
                    ApplicationRecord::Control { commands, .. }
                        if self.phase == ProtocolPhase::Receiving =>
                    {
                        let requests = notifications(&commands)?
                            .into_iter()
                            .filter(|id| {
                                WeatherKind::from_message_id(*id).is_some()
                                    && self.advertised.contains(id)
                            })
                            .collect::<BTreeSet<_>>()
                            .into_iter()
                            .map(|id| [vec![1], id.to_le_bytes().to_vec()].concat())
                            .collect::<Vec<_>>();
                        if !requests.is_empty() {
                            self.send(2, control(&requests), now_ms, &mut output)?;
                        }
                    }
                    ApplicationRecord::Message {
                        message_id: 2,
                        attributes: 0,
                        data,
                    } if self.phase == ProtocolPhase::Challenge => {
                        if data.len() != 16 {
                            return Err(DecodeError("authentication challenge length"));
                        }
                        let mut digest = Md5::new();
                        digest.update(self.credentials.token);
                        digest.update(&data);
                        let response = [
                            1u32.to_le_bytes().as_slice(),
                            digest.finalize().as_slice(),
                            &self.nonce,
                        ]
                        .concat();
                        let mut digest = Md5::new();
                        digest.update(self.credentials.token);
                        digest.update(self.nonce);
                        self.expected_response = Some(digest.finalize().into());
                        self.send(2, message(3, &response), now_ms, &mut output)?;
                        self.transition(ProtocolPhase::ServerAuthentication, now_ms);
                    }
                    ApplicationRecord::Message {
                        message_id: 4,
                        attributes: 0,
                        data,
                    } if self.phase == ProtocolPhase::ServerAuthentication => {
                        let expected = self
                            .expected_response
                            .take()
                            .ok_or(DecodeError("authentication state"))?;
                        if data.len() != expected.len()
                            || data
                                .iter()
                                .zip(expected)
                                .fold(0u8, |diff, (a, b)| diff | (*a ^ b))
                                != 0
                        {
                            return Err(DecodeError("receiver authentication failed"));
                        }
                        let requests = [
                            (0x1000, 1),
                            (0x2010, 1),
                            (0x2110, 1),
                            (0x20001100, 10),
                            (0x20001104, 10),
                        ]
                        .into_iter()
                        .filter(|(id, _)| self.advertised.contains(id))
                        .map(|(id, rate)| subscription(id, rate))
                        .collect::<Vec<_>>();
                        if !self.advertised.contains(&0x1000) {
                            return Err(DecodeError("receiver has no product identity message"));
                        }
                        self.send(
                            2,
                            [
                                control(&requests),
                                supported_ids(),
                                message(1, &self.credentials.bundle),
                            ]
                            .concat(),
                            now_ms,
                            &mut output,
                        )?;
                        self.transition(ProtocolPhase::Product, now_ms);
                    }
                    ApplicationRecord::Message {
                        message_id,
                        attributes,
                        data,
                    } => {
                        if message_id == 0x1000 && self.phase == ProtocolPhase::Product {
                            let mut subscriptions = [
                                (0x2100, 1),
                                (0x2200, 1),
                                (0x3010, 1),
                                (0x20000000, 10),
                                (0x2001, 1),
                                (0x2003, 1),
                                (0x2005, 1),
                            ]
                            .into_iter()
                            .filter(|(id, _)| self.advertised.contains(id))
                            .map(|(id, rate)| subscription(id, rate))
                            .collect::<Vec<_>>();
                            for id in &self.advertised {
                                if WeatherKind::from_message_id(*id).is_some() {
                                    subscriptions.push(subscription(*id, 255));
                                }
                            }
                            self.send(2, control(&subscriptions), now_ms, &mut output)?;
                            self.transition(ProtocolPhase::Receiving, now_ms);
                        }
                        // Authentication/control payloads do not enter public diagnostics.
                        if message_id > 4 && self.phase == ProtocolPhase::Receiving {
                            let product = if attributes != 0 {
                                Err(DecodeError("unverified application file attributes"))
                            } else {
                                decode_product(message_id, &data)
                            };
                            output.messages.push(ReceivedMessage {
                                message_id,
                                attributes,
                                product,
                            });
                        }
                    }
                    _ => {}
                }
            }
            if output.transmissions.len() == before_tx {
                self.send(2, Vec::new(), now_ms, &mut output)?;
            }
        }
        Ok(output)
    }

    pub fn tick(&mut self, now_ms: u64) -> Result<ProtocolOutput> {
        let result = self.tick_inner(now_ms);
        if result.is_err() {
            self.fail();
        }
        result
    }

    fn tick_inner(&mut self, now_ms: u64) -> Result<ProtocolOutput> {
        self.check_clock(now_ms)?;
        if self.phase != ProtocolPhase::Receiving
            && now_ms - self.phase_since_ms >= SETUP_TIMEOUT_MS
        {
            return Err(DecodeError("receiver setup deadline"));
        }
        let mut output = ProtocolOutput::default();
        if self
            .pending
            .front()
            .is_some_and(|first| now_ms - first.last_sent_ms >= RETRANSMIT_MS)
        {
            for pending in &mut self.pending {
                if pending.retries >= MAX_RETRIES {
                    return Err(DecodeError("receiver acknowledgement deadline"));
                }
                pending.frame.acknowledgement = self.acknowledgement;
                output.transmissions.push(pending.frame.encode()?);
                pending.last_sent_ms = now_ms;
                pending.retries += 1;
            }
        }
        Ok(output)
    }

    fn check_clock(&mut self, now_ms: u64) -> Result<()> {
        if self.phase == ProtocolPhase::Failed {
            return Err(DecodeError("receiver reconnect required"));
        }
        if now_ms < self.last_clock_ms {
            return Err(DecodeError("receiver monotonic clock regressed"));
        }
        self.last_clock_ms = now_ms;
        Ok(())
    }
    fn transition(&mut self, phase: ProtocolPhase, now_ms: u64) {
        self.phase = phase;
        self.phase_since_ms = now_ms;
    }
    fn fail(&mut self) {
        self.phase = ProtocolPhase::Failed;
        self.pending.clear();
        self.framer.reset();
        self.application.reset();
    }

    fn send(
        &mut self,
        kind: u8,
        payload: Vec<u8>,
        now_ms: u64,
        output: &mut ProtocolOutput,
    ) -> Result<()> {
        let frame = Frame {
            kind,
            sequence: self.next_tx_sequence,
            acknowledgement: self.acknowledgement,
            payload,
        };
        if !frame.payload.is_empty() && self.pending.len() == MAX_PENDING {
            return Err(DecodeError("receiver send window limit"));
        }
        output.transmissions.push(frame.encode()?);
        if !frame.payload.is_empty() {
            self.next_tx_sequence = self.next_tx_sequence.wrapping_add(1);
            self.pending.push_back(Pending {
                frame,
                last_sent_ms: now_ms,
                retries: 0,
            });
        }
        Ok(())
    }
}

fn record(flags: u8, id: u32, body: &[u8]) -> Vec<u8> {
    let mut bytes = vec![1];
    bytes.extend_from_slice(&((body.len() + 8) as u16).to_le_bytes());
    bytes.push(flags);
    bytes.extend_from_slice(&id.to_le_bytes());
    bytes.extend_from_slice(body);
    bytes
}
fn message(id: u32, body: &[u8]) -> Vec<u8> {
    record(12, id, body)
}
fn control(commands: &[Vec<u8>]) -> Vec<u8> {
    record(2, 0, &commands.concat())
}
fn subscription(id: u32, rate: u8) -> Vec<u8> {
    [vec![0], id.to_le_bytes().to_vec(), vec![1, rate]].concat()
}
fn supported_ids() -> Vec<u8> {
    message(
        0,
        &[0u32, 0x102, 3, 1]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>(),
    )
}

fn notifications(mut bytes: &[u8]) -> Result<Vec<u32>> {
    let mut ids = Vec::new();
    while !bytes.is_empty() {
        let size = match bytes[0] {
            0 => 7,
            1..=3 => 5,
            _ => return Err(DecodeError("unknown control syntax")),
        };
        if bytes.len() < size {
            return Err(DecodeError("truncated control command"));
        }
        if bytes[0] == 2 {
            ids.push(u32::from_le_bytes(bytes[1..5].try_into().unwrap()));
        }
        bytes = &bytes[size..];
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client() -> Protocol {
        Protocol::with_nonce(
            Credentials::new(&[7; 48], b"GRM_SALTsynthetic").unwrap(),
            0,
            [9; 16],
        )
        .unwrap()
        .0
    }
    fn rx(client: &mut Protocol, kind: u8, seq: u8, payload: Vec<u8>) -> Result<ProtocolOutput> {
        client.receive(
            &Frame {
                kind,
                sequence: seq,
                acknowledgement: client.next_tx_sequence.wrapping_sub(1),
                payload,
            }
            .encode()?,
            seq as u64,
        )
    }
    fn authenticate() -> Protocol {
        let mut client = client();
        rx(&mut client, 3, 1, NEGOTIATION.to_vec()).unwrap();
        rx(
            &mut client,
            2,
            2,
            message(
                0,
                &[2u32, 4]
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            ),
        )
        .unwrap();
        rx(&mut client, 2, 3, message(2, &[8; 16])).unwrap();
        assert_eq!(client.phase(), ProtocolPhase::ServerAuthentication);
        let response = client.expected_response.unwrap();
        let ids = [0u32, 0x1000, 0x2100, 0x2200, 0x20000010, 0x20000000];
        rx(
            &mut client,
            2,
            4,
            [
                message(4, &response),
                message(
                    0,
                    &ids.into_iter()
                        .flat_map(u32::to_le_bytes)
                        .collect::<Vec<_>>(),
                ),
            ]
            .concat(),
        )
        .unwrap();
        assert_eq!(client.phase(), ProtocolPhase::Product);
        client
    }

    #[test]
    fn same_packet_capabilities_are_used_before_authentication_response() {
        let mut client = authenticate();
        let output = rx(&mut client, 2, 5, message(0x1000, &[0x47, 6])).unwrap();
        assert_eq!(client.phase(), ProtocolPhase::Receiving);
        let frames = Framer::default()
            .feed(&output.transmissions.concat())
            .unwrap();
        let mut app = ApplicationStream::default();
        let mut requested = BTreeSet::new();
        for frame in frames {
            for record in app.feed(&frame.payload).unwrap() {
                if let ApplicationRecord::Control { commands, .. } = record {
                    for command in commands.chunks_exact(7) {
                        requested.insert(u32::from_le_bytes(command[1..5].try_into().unwrap()));
                    }
                }
            }
        }
        assert_eq!(
            requested,
            BTreeSet::from([0x2100, 0x2200, 0x20000010, 0x20000000])
        );
        assert!(
            !requested.contains(&0x2001),
            "unsupported GPS subscriptions broke the prototype"
        );
    }

    #[test]
    fn authentication_failure_never_sends_the_user_bundle() {
        let mut client = client();
        rx(&mut client, 3, 1, NEGOTIATION.to_vec()).unwrap();
        rx(
            &mut client,
            2,
            2,
            message(
                0,
                &[2u32, 4]
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            ),
        )
        .unwrap();
        rx(&mut client, 2, 3, message(2, &[8; 16])).unwrap();
        assert!(rx(&mut client, 2, 4, message(4, &[0; 16])).is_err());
        assert_eq!(client.phase(), ProtocolPhase::Failed);
        assert!(client.tick(1000).is_err());
    }

    #[test]
    fn unauthenticated_sensor_payloads_are_not_application_inputs() {
        let mut client = client();
        rx(&mut client, 3, 1, NEGOTIATION.to_vec()).unwrap();
        let output = rx(&mut client, 2, 2, message(0x2100, &[0; 16])).unwrap();
        assert!(output.messages.is_empty());
        assert_eq!(client.phase(), ProtocolPhase::Capabilities);
    }

    #[test]
    fn duplicates_do_not_repeat_products_and_sequence_gaps_fail_closed() {
        let mut client = authenticate();
        rx(&mut client, 2, 5, message(0x1000, &[0x47, 6])).unwrap();
        let output = rx(&mut client, 2, 6, message(0x2100, &[0; 16])).unwrap();
        assert_eq!(output.messages.len(), 1);
        assert!(rx(&mut client, 2, 6, message(0x2100, &[0; 16]))
            .unwrap()
            .messages
            .is_empty());
        assert!(rx(&mut client, 2, 8, message(0x2100, &[0; 16])).is_err());
        assert_eq!(client.phase(), ProtocolPhase::Failed);
    }

    #[test]
    fn stalled_send_is_bounded_but_idle_receiving_is_not_a_failure() {
        let mut stalled = client();
        for now in [1000, 2000, 3000] {
            assert_eq!(stalled.tick(now).unwrap().transmissions.len(), 1);
        }
        assert!(stalled.tick(4000).is_err());
        let mut ready = authenticate();
        rx(&mut ready, 2, 5, message(0x1000, &[0x47, 6])).unwrap();
        rx(&mut ready, 2, 6, Vec::new()).unwrap();
        assert!(ready.tick(1_000_000).unwrap().transmissions.is_empty());
        assert_eq!(ready.phase(), ProtocolPhase::Receiving);
    }
}
