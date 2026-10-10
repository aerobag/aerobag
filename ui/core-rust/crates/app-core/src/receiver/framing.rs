// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{BTreeMap, VecDeque};

use sha2::Digest;

use super::{DecodeError, Result, MAX_PRODUCT_BYTES};

const MAX_FRAME_BYTES: usize = u16::MAX as usize;
const MAX_ASSEMBLIES: usize = 64;

/// Shared by the live connection and passive capture replay. A retransmission
/// must not reach the application reassembler a second time.
#[derive(Default)]
pub(super) struct ReceiveSequence {
    last: Option<u8>,
    recent: VecDeque<(u8, [u8; 32])>,
}

impl ReceiveSequence {
    pub fn accept(&mut self, frame: &Frame) -> Result<bool> {
        let digest = sha2::Sha256::digest(&frame.payload).into();
        if self.recent.contains(&(frame.sequence, digest)) {
            return Ok(false);
        }
        if self
            .last
            .is_some_and(|last| frame.sequence != last.wrapping_add(1))
        {
            return Err(DecodeError("receiver transport sequence gap"));
        }
        self.last = Some(frame.sequence);
        self.recent.push_back((frame.sequence, digest));
        if self.recent.len() > 64 {
            self.recent.pop_front();
        }
        Ok(true)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Frame {
    pub kind: u8,
    pub sequence: u8,
    pub acknowledgement: u8,
    pub payload: Vec<u8>,
}

impl std::fmt::Debug for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Frame")
            .field("kind", &self.kind)
            .field("sequence", &self.sequence)
            .field("acknowledgement", &self.acknowledgement)
            .field("payload_bytes", &self.payload.len())
            .finish()
    }
}

impl Frame {
    pub fn encode(&self) -> Result<Vec<u8>> {
        let length = 8 + self.payload.len() + usize::from(!self.payload.is_empty());
        if length > MAX_FRAME_BYTES {
            return Err(DecodeError("frame too large"));
        }
        let mut bytes = vec![0xc0, 1];
        bytes.extend_from_slice(&(length as u16).to_le_bytes());
        bytes.extend_from_slice(&[self.kind, self.sequence, self.acknowledgement]);
        bytes.push(checksum(&bytes));
        bytes.extend_from_slice(&self.payload);
        if !self.payload.is_empty() {
            bytes.push(checksum(&self.payload));
        }
        Ok(bytes)
    }
}

fn checksum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0u8, |sum, byte| sum.wrapping_sub(*byte))
}

/// Incomplete input is bounded to one frame. A framing error poisons the stream
/// until reconnect/reset, rather than guessing where an authenticated stream resumes.
#[derive(Default)]
pub struct Framer {
    buffer: Vec<u8>,
    failed: bool,
}

impl Framer {
    pub fn feed(&mut self, mut bytes: &[u8]) -> Result<Vec<Frame>> {
        if self.failed {
            return Err(DecodeError("reset required after framing failure"));
        }
        let result = self.feed_inner(&mut bytes);
        if result.is_err() {
            self.failed = true;
            self.buffer.clear();
        }
        result
    }

    fn feed_inner(&mut self, bytes: &mut &[u8]) -> Result<Vec<Frame>> {
        let mut frames = Vec::new();
        // Bound output as well as partial input. OS adapters must submit bounded chunks.
        if bytes.len() > 4 * MAX_FRAME_BYTES {
            return Err(DecodeError("input chunk too large"));
        }
        while !bytes.is_empty() {
            let target = if self.buffer.len() < 8 {
                8
            } else {
                self.length()?
            };
            let count = (target - self.buffer.len()).min(bytes.len());
            self.buffer.extend_from_slice(&bytes[..count]);
            *bytes = &bytes[count..];
            if self.buffer.len() < 8 {
                continue;
            }
            let length = self.length()?;
            if self.buffer.len() < length {
                continue;
            }
            if checksum(&self.buffer[8..]) != 0 {
                return Err(DecodeError("payload checksum"));
            }
            frames.push(Frame {
                kind: self.buffer[4],
                sequence: self.buffer[5],
                acknowledgement: self.buffer[6],
                payload: if length == 8 {
                    Vec::new()
                } else {
                    self.buffer[8..length - 1].to_vec()
                },
            });
            self.buffer.clear();
        }
        Ok(frames)
    }

    fn length(&self) -> Result<usize> {
        if self.buffer[..2] != [0xc0, 1] || checksum(&self.buffer[..8]) != 0 {
            return Err(DecodeError("transport header/checksum"));
        }
        let length = u16::from_le_bytes([self.buffer[2], self.buffer[3]]) as usize;
        if length < 8 || length == 9 {
            return Err(DecodeError("invalid transport length"));
        }
        Ok(length)
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn is_incomplete(&self) -> bool {
        !self.buffer.is_empty()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum ApplicationRecord {
    Control {
        message_id: u32,
        commands: Vec<u8>,
    },
    Message {
        message_id: u32,
        attributes: u8,
        data: Vec<u8>,
    },
}

impl std::fmt::Debug for ApplicationRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Application bodies can include authentication bundles. Keep ordinary
        // diagnostics safe even when an assertion prints a mismatched record.
        match self {
            Self::Control {
                message_id,
                commands,
            } => f
                .debug_struct("Control")
                .field("message_id", message_id)
                .field("command_bytes", &commands.len())
                .finish(),
            Self::Message {
                message_id,
                attributes,
                data,
            } => f
                .debug_struct("Message")
                .field("message_id", message_id)
                .field("attributes", attributes)
                .field("data_bytes", &data.len())
                .finish(),
        }
    }
}

#[derive(Default)]
pub struct ApplicationStream {
    buffer: Vec<u8>,
    files: BTreeMap<u32, (u8, Vec<u8>)>,
    total_bytes: usize,
    failed: bool,
}

impl ApplicationStream {
    pub fn feed(&mut self, mut bytes: &[u8]) -> Result<Vec<ApplicationRecord>> {
        if self.failed {
            return Err(DecodeError("reset required after application failure"));
        }
        let result = self.feed_inner(&mut bytes);
        if result.is_err() {
            self.reset();
            self.failed = true;
        }
        result
    }

    fn feed_inner(&mut self, bytes: &mut &[u8]) -> Result<Vec<ApplicationRecord>> {
        if bytes.len() > MAX_PRODUCT_BYTES {
            return Err(DecodeError("application chunk too large"));
        }
        let mut output = Vec::new();
        while !bytes.is_empty() {
            let target = if self.buffer.len() < 4 {
                4
            } else {
                self.length()?
            };
            let count = (target - self.buffer.len()).min(bytes.len());
            self.buffer.extend_from_slice(&bytes[..count]);
            *bytes = &bytes[count..];
            if self.buffer.len() < 4 {
                continue;
            }
            let length = self.length()?;
            if self.buffer.len() < length {
                continue;
            }
            let raw = std::mem::take(&mut self.buffer);
            let flags = raw[3];
            let id = u32::from_le_bytes(raw[4..8].try_into().unwrap());
            let body = &raw[8..];
            if flags & 2 != 0 {
                output.push(ApplicationRecord::Control {
                    message_id: id,
                    commands: body.to_vec(),
                });
                continue;
            }
            if flags & 4 != 0 {
                if let Some((_, previous)) = self.files.remove(&id) {
                    self.total_bytes -= previous.len();
                }
                if self.files.len() == MAX_ASSEMBLIES {
                    return Err(DecodeError("too many unfinished products"));
                }
                self.files.insert(id, (flags & !0x0e, Vec::new()));
            }
            if self.total_bytes + body.len() > MAX_PRODUCT_BYTES {
                return Err(DecodeError("product reassembly limit"));
            }
            let (attributes, data) = self
                .files
                .get_mut(&id)
                .ok_or(DecodeError("fragment without start"))?;
            *attributes |= flags & !0x0e;
            data.extend_from_slice(body);
            self.total_bytes += body.len();
            if flags & 8 != 0 {
                let (attributes, data) = self.files.remove(&id).unwrap();
                self.total_bytes -= data.len();
                output.push(ApplicationRecord::Message {
                    message_id: id,
                    attributes,
                    data,
                });
            }
        }
        Ok(output)
    }

    fn length(&self) -> Result<usize> {
        let length = u16::from_le_bytes([self.buffer[1], self.buffer[2]]) as usize;
        if self.buffer[0] != 1 || length < 8 {
            return Err(DecodeError("application header"));
        }
        Ok(length)
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn is_incomplete(&self) -> bool {
        !self.buffer.is_empty() || !self.files.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: u32, flags: u8, body: &[u8]) -> Vec<u8> {
        let mut out = vec![1];
        out.extend_from_slice(&((8 + body.len()) as u16).to_le_bytes());
        out.push(flags);
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(body);
        out
    }

    #[test]
    fn every_transport_split_and_coalesced_frames_preserve_payloads() {
        let frames = [
            Frame {
                kind: 2,
                sequence: 255,
                acknowledgement: 1,
                payload: vec![1, 2, 3],
            },
            Frame {
                kind: 2,
                sequence: 0,
                acknowledgement: 2,
                payload: vec![],
            },
        ];
        let bytes: Vec<_> = frames.iter().flat_map(|f| f.encode().unwrap()).collect();
        for split in 0..=bytes.len() {
            let mut decoder = Framer::default();
            let mut result = decoder.feed(&bytes[..split]).unwrap();
            result.extend(decoder.feed(&bytes[split..]).unwrap());
            assert_eq!(result, frames);
            assert!(!decoder.is_incomplete());
        }
    }

    #[test]
    fn interleaved_products_survive_every_application_split() {
        let bytes = [
            record(10, 4, b"abc"),
            record(11, 12, b"def"),
            record(10, 8, b"ghi"),
        ]
        .concat();
        for split in 0..=bytes.len() {
            let mut decoder = ApplicationStream::default();
            let mut result = decoder.feed(&bytes[..split]).unwrap();
            result.extend(decoder.feed(&bytes[split..]).unwrap());
            assert_eq!(
                result,
                vec![
                    ApplicationRecord::Message {
                        message_id: 11,
                        attributes: 0,
                        data: b"def".to_vec()
                    },
                    ApplicationRecord::Message {
                        message_id: 10,
                        attributes: 0,
                        data: b"abcghi".to_vec()
                    },
                ]
            );
            assert!(!decoder.is_incomplete());
        }
    }

    #[test]
    fn corruption_requires_reset_and_never_reuses_partial_product() {
        let mut decoder = ApplicationStream::default();
        decoder.feed(&record(1, 4, b"old")).unwrap();
        assert!(decoder.feed(&[0, 8, 0, 0]).is_err());
        assert!(decoder.feed(&record(1, 8, b"new")).is_err());
        decoder.reset();
        assert!(decoder.feed(&record(1, 8, b"new")).is_err());
        let mut framer = Framer::default();
        let mut raw = Frame {
            kind: 2,
            sequence: 1,
            acknowledgement: 0,
            payload: vec![7],
        }
        .encode()
        .unwrap();
        raw[8] ^= 1;
        assert!(framer.feed(&raw).is_err());
        assert!(framer.feed(&[]).is_err());
    }

    #[test]
    fn unfinished_empty_products_are_bounded_too() {
        let mut stream = ApplicationStream::default();
        for id in 0..MAX_ASSEMBLIES as u32 {
            stream.feed(&record(id, 4, b"")).unwrap();
        }
        assert!(stream.feed(&record(100, 4, b"")).is_err());
    }
}
