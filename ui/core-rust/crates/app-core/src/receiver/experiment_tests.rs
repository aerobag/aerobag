// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;
use crate::receiver::{test_support::Temp, ApplicationRecord, ApplicationStream, Frame, Framer};
use md5::{Digest, Md5};
use std::collections::VecDeque;

fn clock(ms: u64) -> CaptureClock {
    CaptureClock {
        monotonic_ms: ms,
        wall_epoch_ms: 1791547200000 + ms as i64,
    }
}
fn action(runtime: &mut Runtime, id: &str, now: u64) -> ReceiverHostOutput {
    runtime.update(
        ReceiverHostEvent::Action {
            action_id: id.into(),
        },
        &[],
        clock(now),
    )
}
fn message(id: u32, data: &[u8]) -> Vec<u8> {
    [
        vec![1],
        ((data.len() + 8) as u16).to_le_bytes().to_vec(),
        vec![12],
        id.to_le_bytes().to_vec(),
        data.to_vec(),
    ]
    .concat()
}
fn container(parts: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = vec![1, 1, 1, 0, 0, 0, 0, 0];
    for index in 0..16 {
        bytes.extend_from_slice(&(parts.get(index).map_or(0, Vec::len) as u32).to_le_bytes());
    }
    for part in parts {
        bytes.extend(part);
    }
    bytes
}
fn compressed(bytes: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}

struct Peer {
    id: u64,
    seq: u8,
    ack: u8,
    now: u64,
    response: Vec<u8>,
}
impl Peer {
    fn settle(&mut self, runtime: &mut Runtime, output: ReceiverHostOutput) {
        let mut effects = VecDeque::from(output.effects);
        while let Some(effect) = effects.pop_front() {
            let ReceiverHostEffect::Write {
                connection_id,
                write_id,
                bytes_base64,
            } = effect
            else {
                panic!("unexpected effect")
            };
            assert_eq!(connection_id, self.id);
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(bytes_base64)
                .unwrap();
            for frame in Framer::default().feed(&bytes).unwrap() {
                if !frame.payload.is_empty() {
                    self.ack = frame.sequence;
                }
                if frame.kind == 2 {
                    for record in ApplicationStream::default().feed(&frame.payload).unwrap() {
                        if let ApplicationRecord::Message {
                            message_id: 3,
                            data,
                            ..
                        } = record
                        {
                            self.response = Md5::digest(
                                [
                                    &crate::receiver::compatibility::CHALLENGE_SEED[..],
                                    &data[20..],
                                ]
                                .concat(),
                            )
                            .to_vec();
                        }
                    }
                }
            }
            effects.extend(
                runtime
                    .update(
                        ReceiverHostEvent::Written {
                            connection_id,
                            write_id,
                        },
                        &[],
                        clock(self.now),
                    )
                    .effects,
            );
        }
    }
    fn rx(&mut self, runtime: &mut Runtime, kind: u8, payload: Vec<u8>) {
        self.seq += 1;
        self.now += 1;
        let raw = Frame {
            kind,
            sequence: self.seq,
            acknowledgement: self.ack,
            payload,
        }
        .encode()
        .unwrap();
        for chunk in raw.chunks(3) {
            let output = runtime.update(
                ReceiverHostEvent::Received {
                    connection_id: self.id,
                },
                chunk,
                clock(self.now),
            );
            self.settle(runtime, output);
        }
    }
    fn start(runtime: &mut Runtime, now: u64) -> Self {
        let output = action(runtime, "receiver:refresh", now);
        let ReceiverHostEffect::RefreshInventory { request_id } = output.effects[0] else {
            panic!()
        };
        let output = runtime.update(
            ReceiverHostEvent::Inventory {
                request_id,
                result: ReceiverInventoryResult::Ready {
                    devices: vec![app_ui_contracts::receiver::ReceiverDevice {
                        address: "TEST".into(),
                        name: "Synthetic".into(),
                    }],
                },
            },
            &[],
            clock(now),
        );
        let connect = output.panel.actions.last().unwrap();
        assert!(connect.enabled);
        let output = action(runtime, &connect.action_id, now);
        let ReceiverHostEffect::Connect { connection_id, .. } = output.effects[0] else {
            panic!()
        };
        let mut peer = Self {
            id: connection_id,
            seq: 0,
            ack: 0,
            now,
            response: Vec::new(),
        };
        let output = runtime.update(
            ReceiverHostEvent::Connected { connection_id },
            &[],
            clock(now),
        );
        peer.settle(runtime, output);
        peer.rx(runtime, 3, vec![1]);
        peer.rx(
            runtime,
            2,
            message(
                0,
                &[2u32, 4]
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            ),
        );
        peer.rx(runtime, 2, message(2, &[8; 16]));
        peer.rx(
            runtime,
            2,
            [
                message(4, &peer.response),
                message(
                    0,
                    &[
                        0u32, 0x1000, 0x2100, 0x2200, 0x20000000, 0x20000010, 0x20000090,
                    ]
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
                ),
            ]
            .concat(),
        );
        peer.rx(runtime, 2, message(0x1000, &[0x47, 6]));
        assert_eq!(
            runtime.connection.as_ref().unwrap().status().phase,
            ConnectionPhase::Protocol(ProtocolPhase::Receiving),
        );
        peer
    }
}

#[test]
fn bundled_handshake_captures_delivers_and_preserves_products_with_wire_proof() {
    exercise_reception(false);
}

#[test]
fn capture_open_failure_still_delivers_traffic_weather_and_radar_with_visible_diagnostic() {
    exercise_reception(true);
}

fn exercise_reception(capture_unavailable: bool) {
    for legacy_files in [false, true] {
        let root = Temp::new();
        drop(WeatherCache::open(root.path()).unwrap());
        if legacy_files {
            // Former imports cannot override the bundled protocol, even when invalid.
            for name in [
                "vendor-challenge-seed.bin",
                "vendor-setup-payload.bin",
                "auth-token.bin",
                "auth-user.bin",
            ] {
                fs::write(root.path().join(name), b"not protocol data").unwrap();
            }
        }
        if capture_unavailable {
            fs::write(root.path().join("captures"), b"not a directory").unwrap();
        }
        let mut runtime = Runtime::new(root.path().into());
        assert!(
            runtime.weather_error.is_none(),
            "{:?}",
            runtime.weather_error
        );
        runtime.build = Some(app_ui_contracts::session::ClientBuildInfo {
            platform: "test".into(),
            version: "test-build".into(),
            built_at_utc: None,
            commit: None,
            dirty: false,
        });
        runtime.attach(1).unwrap();
        let mut peer = Peer::start(&mut runtime, 1);
        if capture_unavailable {
            assert_eq!(runtime.panel().status, "Receiving; recording stopped");
            assert!(runtime
                .panel()
                .detail
                .contains("Recording stopped (storage unavailable)"));
        } else {
            assert_eq!(runtime.panel().status, "Receiving and recording");
        }
        peer.rx(&mut runtime, 2, message(0x2100, &[0; 16]));
        peer.rx(&mut runtime, 2, message(0x2200, &1234f32.to_le_bytes()));
        let mut traffic = vec![0u8; 268];
        traffic[0] = 1;
        traffic[116..120].copy_from_slice(&1u32.to_le_bytes());
        traffic[120..124].copy_from_slice(&7u32.to_le_bytes());
        traffic[124..128].copy_from_slice(&2u32.to_le_bytes());
        traffic[144..152].copy_from_slice(&47.45f64.to_radians().to_le_bytes());
        traffic[152..160].copy_from_slice(&(-122.31f64).to_radians().to_le_bytes());
        traffic[264..268].copy_from_slice(&64u32.to_le_bytes());
        peer.rx(&mut runtime, 2, message(0x20000000, &traffic));
        peer.rx(&mut runtime, 2, message(0x20000010, &[]));
        let text = b"KPAE 091200Z 00000KT 10SM CLR 10/05 A3000";
        let mut index = vec![0; 77];
        index[..4].copy_from_slice(b"KPAE");
        index[69..73].copy_from_slice(&(text.len() as u32).to_le_bytes());
        peer.rx(
            &mut runtime,
            2,
            message(
                0x20000010,
                &container(&[vec![9, 12, 0, 1, 0], compressed(&index), compressed(text)]),
            ),
        );
        let mut radar = vec![0; 26];
        radar[8..14].copy_from_slice(&[9, 12, 0, 9, 12, 0]);
        radar[18..26].copy_from_slice(&[2, 0, 1, 0, 0x3c, 7, 0xda, 10]);
        peer.rx(
            &mut runtime,
            2,
            message(
                0x20000090,
                &container(&[radar, compressed(&[1, 0, 24, 1, 0, 7]), compressed(&[0; 4])]),
            ),
        );
        let panel = runtime.panel();
        assert!(panel.detail.contains("1 traffic snapshots, 1 with targets"));
        assert!(panel.detail.contains("METAR: 1 nonempty / 1 empty"));
        assert!(panel
            .detail
            .contains("Regional NEXRAD: 1 nonempty / 0 empty"));
        assert_eq!(runtime.normalized, 1);
        assert!(runtime.rejected == 0 && runtime.weather_error.is_none());
        assert_eq!(
            runtime
                .connection
                .as_ref()
                .unwrap()
                .status()
                .malformed_messages,
            0
        );
        assert!(!runtime
            .weather_cache
            .as_ref()
            .unwrap()
            .radar()
            .frames
            .is_empty());
        // UI-session replacement (rotation/reload) preserves reception and delivery.
        runtime.attach(2).unwrap();
        let delivery = runtime.mailbox.lock().unwrap().begin_for(2).unwrap();
        assert_eq!(delivery.reports.len(), 1);
        assert_eq!(delivery.live.aircraft.len(), 1);
        assert_eq!(
            runtime.connection.as_ref().unwrap().status().phase,
            ConnectionPhase::Protocol(ProtocolPhase::Receiving)
        );
        let reception = runtime.connection.as_ref().unwrap().status().reception;
        if capture_unavailable {
            assert!(runtime.panel().detail.contains("Recording stopped"));
            assert_eq!(
                runtime
                    .connection
                    .as_ref()
                    .unwrap()
                    .status()
                    .captured
                    .written_records,
                0
            );
        }
        action(&mut runtime, "receiver:stop", peer.now);
        if !capture_unavailable {
            let path = runtime.export_capture().unwrap();
            let summary = crate::receiver::capture_replay::replay_export(
                fs::File::open(path).unwrap(),
                |_, _| {},
            )
            .unwrap();
            assert_eq!(summary.attempts.len(), 1);
            let attempt = &summary.attempts[0];
            assert_eq!(
                attempt.setup.as_ref().unwrap(),
                &crate::receiver::setup::SetupMetadata::current(runtime.build.clone())
            );
            assert_eq!(
                attempt
                    .setup
                    .as_ref()
                    .unwrap()
                    .build
                    .as_ref()
                    .unwrap()
                    .version,
                "test-build"
            );
            assert_eq!(attempt.setup_payload_messages, 0);
            assert_eq!(attempt.incomplete_writes, 0);
            assert_eq!(attempt.reception, reception);
        }
        // Restart needs no imported files and preserves received weather.
        drop(runtime);
        let mut runtime = Runtime::new(root.path().into());
        assert_eq!(runtime.weather_cache.as_ref().unwrap().reports().count(), 1);
        let _peer = Peer::start(&mut runtime, peer.now + 1);
        assert!(
            runtime.panel().detail.contains("Weather: no responses yet"),
            "cached weather must not be reported as newly received"
        );
    }
}
