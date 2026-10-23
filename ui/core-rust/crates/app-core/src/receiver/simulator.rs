// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Development-only wire peer. Uses the real connection, framing and decoder.
//! Manual setup steps hold connection timers; receiving/retry use real clocks.
//! No positions or usable weather are fabricated, and no Bluetooth IO escapes.
use super::*;
use crate::receiver::{ApplicationRecord, ApplicationStream, Frame, Framer};
use md5::{Digest, Md5};
use std::collections::VecDeque;

#[derive(Default)]
pub(super) struct Peer {
    saved_activity: crate::receiver::activity::Activity,
    id: Option<u64>,
    step: u8,
    sequence: u8,
    ack: u8,
    response: Vec<u8>,
    last_sample: Option<u64>,
}

impl Peer {
    pub(super) fn new(saved_activity: crate::receiver::activity::Activity) -> Self {
        Self {
            saved_activity,
            ..Default::default()
        }
    }
    fn receive(&mut self, kind: u8, payload: Vec<u8>) -> (ReceiverHostEvent, Vec<u8>) {
        self.sequence = self.sequence.wrapping_add(1);
        (
            ReceiverHostEvent::Received {
                connection_id: self.id.unwrap(),
            },
            Frame {
                kind,
                sequence: self.sequence,
                acknowledgement: self.ack,
                payload,
            }
            .encode()
            .unwrap(),
        )
    }

    fn advance(&mut self) -> Option<(ReceiverHostEvent, Vec<u8>)> {
        let id = self.id?;
        let event = match self.step {
            0 => (ReceiverHostEvent::Connected { connection_id: id }, vec![]),
            1 => self.receive(3, vec![1]),
            2 => self.receive(
                2,
                message(
                    0,
                    &[2u32, 4]
                        .into_iter()
                        .flat_map(u32::to_le_bytes)
                        .collect::<Vec<_>>(),
                ),
            ),
            3 => self.receive(2, message(2, &[8; 16])),
            4 => self.receive(
                2,
                [
                    message(4, &self.response),
                    message(
                        0,
                        &[0u32, 0x1000, 0x2100, 0x20000000, 0x20000010]
                            .into_iter()
                            .flat_map(u32::to_le_bytes)
                            .collect::<Vec<_>>(),
                    ),
                ]
                .concat(),
            ),
            5 => self.receive(2, message(0x1000, &[0x47, 6])),
            _ => return None,
        };
        self.step += 1;
        Some(event)
    }

    fn effect(
        &mut self,
        effect: ReceiverHostEffect,
        queue: &mut VecDeque<(Option<ReceiverHostEvent>, Vec<u8>)>,
    ) -> Option<ReceiverHostEffect> {
        match effect {
            ReceiverHostEffect::Connect {
                connection_id,
                device,
                ..
            } => {
                assert_eq!(device, "SIMULATED-GTX345");
                *self = Self {
                    id: Some(connection_id),
                    saved_activity: std::mem::take(&mut self.saved_activity),
                    ..Default::default()
                };
            }
            ReceiverHostEffect::Close { .. } => self.id = None,
            ReceiverHostEffect::Write {
                connection_id,
                write_id,
                bytes_base64,
            } => {
                assert_eq!(Some(connection_id), self.id);
                let bytes = STANDARD.decode(bytes_base64).unwrap();
                for frame in Framer::default().feed(&bytes).unwrap() {
                    if !frame.payload.is_empty() {
                        self.ack = frame.sequence;
                        // Acknowledge transport immediately; application replies are stepped.
                        let ack = Frame {
                            kind: 2,
                            sequence: 0,
                            acknowledgement: self.ack,
                            payload: vec![],
                        }
                        .encode()
                        .unwrap();
                        queue.push_back((Some(ReceiverHostEvent::Received { connection_id }), ack));
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
                queue.push_front((
                    Some(ReceiverHostEvent::Written {
                        connection_id,
                        write_id,
                    }),
                    vec![],
                ));
            }
            other => return Some(other),
        }
        None
    }
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

impl Runtime {
    pub(super) fn update_with_simulator(
        &mut self,
        event: ReceiverHostEvent,
        bytes: &[u8],
        clock: CaptureClock,
    ) -> ReceiverHostOutput {
        let holding = self
            .simulator
            .as_ref()
            .is_some_and(|s| s.id.is_some() && s.step < 6);
        let input = match (&event, &mut self.simulator) {
            (ReceiverHostEvent::Action { action_id }, Some(peer))
                if action_id == "receiver:sim-next" =>
            {
                peer.advance()
                    .map(|(e, b)| (Some(e), b))
                    .unwrap_or((None, vec![]))
            }
            (ReceiverHostEvent::Action { action_id }, Some(peer))
                if action_id == "receiver:sim-interrupt" =>
            {
                (
                    peer.id.map(|connection_id| ReceiverHostEvent::Failed {
                        connection_id,
                        failure: ReceiverIoFailure::Read,
                    }),
                    vec![],
                )
            }
            (ReceiverHostEvent::Tick, _) if holding => (None, vec![]),
            _ => (Some(event), bytes.to_vec()),
        };
        let mut queue = VecDeque::from([input]);
        let mut host_effects = vec![];
        let mut delivery_ready = false;
        let mut output;
        loop {
            let (event, bytes) = queue.pop_front().unwrap();
            output = self.update_inner(event, &bytes, clock);
            delivery_ready |= output.delivery_ready;
            for effect in std::mem::take(&mut output.effects) {
                if let Some(peer) = &mut self.simulator {
                    if let Some(effect) = peer.effect(effect, &mut queue) {
                        host_effects.push(effect);
                    }
                } else {
                    host_effects.push(effect);
                }
            }
            if queue.is_empty() {
                break;
            }
        }
        if self.connection.is_none() {
            if let Some(peer) = self.simulator.take() {
                self.activity = peer.saved_activity;
                self.activity_view = None;
                self.activity_view_second = None;
            }
        }
        // Publish after transport effects too: Connect/Close change the peer's
        // available lab actions, even when they produce no immediate callbacks.
        output = self.update_inner(None, &[], clock);
        delivery_ready |= output.delivery_ready;
        if let Some(peer) = &mut self.simulator {
            if peer.id.is_some()
                && peer.step == 6
                && peer
                    .last_sample
                    .is_none_or(|last| clock.monotonic_ms.saturating_sub(last) >= 1000)
            {
                peer.last_sample = Some(clock.monotonic_ms);
                let mut traffic = vec![0; 120];
                traffic[0] = 1;
                // Zero validity bits, no targets: activity without invented navigation.
                let (event, bytes) = peer.receive(
                    2,
                    [
                        message(0x2100, &[0; 16]),
                        message(0x20000000, &traffic),
                        message(0x20000010, &[]),
                    ]
                    .concat(),
                );
                let samples = self.update_with_simulator(event, &bytes, clock);
                delivery_ready |= samples.delivery_ready;
                host_effects.extend(samples.effects);
                output.panel = samples.panel;
            }
            // A manual setup step must remain inspectable indefinitely. Never
            // forward its already-due protocol deadline into an OS busy loop.
            output.next_wake_monotonic_ms = Some(clock.monotonic_ms + 1000);
        }
        output.effects = host_effects;
        output.delivery_ready |= delivery_ready;
        output
    }

    pub(super) fn simulator_panel(&self, panel: &mut UiReceiverPanel) {
        if self.simulator_enabled {
            if let Some(chooser) = &mut panel.chooser {
                chooser
                    .devices
                    .push(action("receiver:simulate", "Simulated GTX345 (UI lab)"));
                chooser.empty_text = None;
            }
        }
        if let Some(peer) = &self.simulator {
            panel.title = "ADS-B receiver - SIMULATION".into();
            panel.detail = if let Some(ConnectionPhase::WaitingToRetry { at_monotonic_ms }) = self
                .connection
                .as_ref()
                .map(|connection| connection.status().phase)
            {
                format!(
                    "Retry in {}s.",
                    at_monotonic_ms.saturating_sub(self.now_ms).div_ceil(1000)
                )
            } else if peer.step < 6 && peer.id.is_some() {
                let step = [
                    "Connect Bluetooth",
                    "Negotiate transport",
                    "Exchange capabilities",
                    "Send challenge",
                    "Verify response",
                    "Start data stream",
                ][usize::from(peer.step)];
                format!("Step {}/6: {step}.", peer.step + 1)
            } else {
                "Simulated activity only.".into()
            };
            let mut next = action("receiver:sim-next", "Advance simulated peer");
            next.enabled = peer.id.is_some() && peer.step < 6;
            next.disabled_reason = (!next.enabled).then(|| {
                "The simulated peer is already receiving, or waiting to reconnect.".into()
            });
            panel.actions.push(next);
            let mut interrupt = action("receiver:sim-interrupt", "Interrupt simulated link");
            interrupt.enabled = peer.id.is_some();
            interrupt.disabled_reason =
                (!interrupt.enabled).then(|| "The simulated link is already disconnected.".into());
            panel.actions.push(interrupt);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::receiver::test_support::Temp;

    fn send(runtime: &mut Runtime, action_id: Option<&str>, now: u64) -> ReceiverHostOutput {
        let output = runtime.update(
            action_id.map_or(ReceiverHostEvent::Tick, |id| ReceiverHostEvent::Action {
                action_id: id.into(),
            }),
            &[],
            CaptureClock {
                monotonic_ms: now,
                wall_epoch_ms: 1791547200000 + now as i64,
            },
        );
        assert!(
            output.effects.is_empty(),
            "simulated IO must not reach the OS"
        );
        output
    }

    fn take_panel(runtime: &Runtime) -> UiReceiverPanel {
        let mut mailbox = runtime.mailbox.lock().unwrap();
        let delivery = mailbox.begin().expect("UI update available");
        let panel = delivery.panel.clone();
        mailbox.acknowledge(&delivery);
        panel
    }

    #[test]
    fn every_manual_setup_advance_has_visible_feedback() {
        let root = Temp::new();
        let mut runtime = Runtime::new(root.path().into());
        runtime.simulator_enabled = true;
        send(&mut runtime, Some("receiver:simulate"), 0);
        let mut prior = take_panel(&runtime);
        for step in 1..=6 {
            let output = send(&mut runtime, Some("receiver:sim-next"), step * 1000);
            let panel = take_panel(&runtime);
            assert_ne!(
                panel.detail, prior.detail,
                "advance {step} must visibly acknowledge progress, not repeat the same setup text"
            );
            assert!(
                output.delivery_ready,
                "progress must be delivered to the UI"
            );
            prior = panel;
        }
        assert_eq!(prior.status, "Receiving");
    }

    #[test]
    fn interruption_at_every_setup_step_can_reconnect_and_finish_receiving() {
        for interrupted_after in 0..=6 {
            let root = Temp::new();
            let mut runtime = Runtime::new(root.path().into());
            runtime.simulator_enabled = true;
            send(&mut runtime, Some("receiver:simulate"), 0);
            for step in 1..=interrupted_after {
                send(&mut runtime, Some("receiver:sim-next"), step * 1000);
            }
            let now = (interrupted_after + 1) * 1000;
            let interrupted = send(&mut runtime, Some("receiver:sim-interrupt"), now);
            assert_eq!(interrupted.panel.status, "Retrying");
            assert!(
                !interrupted
                    .panel
                    .actions
                    .iter()
                    .find(|a| a.action_id == "receiver:sim-next")
                    .unwrap()
                    .enabled
            );
            let premature = send(&mut runtime, Some("receiver:sim-next"), now + 1);
            assert_eq!(
                premature.panel.status, "Retrying",
                "an early advance cannot skip reconnect"
            );
            let waiting = send(&mut runtime, None, now + 1000);
            assert_eq!(waiting.panel.status, "Retrying");
            assert_ne!(
                waiting.panel.detail, interrupted.panel.detail,
                "retry countdown advances"
            );
            assert_eq!(
                send(&mut runtime, None, now + 65_000).panel.status,
                "Connecting"
            );
            for step in 1..=6 {
                send(
                    &mut runtime,
                    Some("receiver:sim-next"),
                    now + 65_000 + step * 1000,
                );
            }
            let finished = send(&mut runtime, None, now + 72_000);
            assert_eq!(
                finished.panel.status, "Receiving",
                "interrupted after step {interrupted_after}"
            );
            assert!(take_panel(&runtime).activity.is_some());
        }
    }

    #[test]
    fn simulator_walks_real_protocol_holds_steps_retries_and_never_emits_bluetooth_io_or_flight_data(
    ) {
        let root = Temp::new();
        let mut runtime = Runtime::new(root.path().into());
        runtime.simulator_enabled = true;
        let mut now = 0;
        let mut run = |runtime: &mut Runtime, id: Option<&str>| {
            now += 60_000; // User can inspect a setup step for longer than protocol timeout.
            let result = runtime.update(
                id.map_or(ReceiverHostEvent::Tick, |id| ReceiverHostEvent::Action {
                    action_id: id.into(),
                }),
                &[],
                CaptureClock {
                    monotonic_ms: now,
                    wall_epoch_ms: 1791547200000 + now as i64,
                },
            );
            assert!(
                result.effects.is_empty(),
                "simulated IO must never reach the OS"
            );
            result
        };
        let initial = run(&mut runtime, Some("receiver:simulate"));
        assert_eq!(initial.panel.status, "Connecting");
        assert_eq!(run(&mut runtime, None).panel.status, "Connecting");
        for _ in 0..5 {
            assert_eq!(
                run(&mut runtime, Some("receiver:sim-next")).panel.status,
                "Setting up"
            );
            assert_eq!(run(&mut runtime, None).panel.status, "Setting up");
        }
        assert_eq!(
            run(&mut runtime, Some("receiver:sim-next")).panel.status,
            "Receiving"
        );
        run(&mut runtime, None);
        assert!(runtime.activity_view.is_some());
        assert!(runtime.live.snapshot().ownship.is_none());
        assert!(runtime.live.snapshot().aircraft.is_empty());
        assert_eq!(runtime.normalized, 0);
        assert_eq!(
            run(&mut runtime, Some("receiver:sim-interrupt"))
                .panel
                .status,
            "Retrying"
        );
        assert_eq!(run(&mut runtime, None).panel.status, "Connecting");
        let stopped = run(&mut runtime, Some("receiver:stop"));
        assert_eq!(stopped.panel.status, "Disconnected");
        assert!(!stopped.panel.title.contains("SIMULATION"));
        assert!(
            runtime.activity_view.is_none(),
            "simulation history is not real reception history"
        );
        assert!(runtime
            .weather_cache
            .as_ref()
            .unwrap()
            .reports()
            .next()
            .is_none());
        assert!(root.path().join("simulator-captures").is_dir());
        assert!(!root.path().join("captures").exists());
    }

    #[test]
    fn normal_runtime_has_no_simulator_choice_or_action() {
        let root = Temp::new();
        let mut runtime = Runtime::new(root.path().into());
        let output = runtime.update(
            ReceiverHostEvent::Action {
                action_id: "receiver:simulate".into(),
            },
            &[],
            CaptureClock {
                monotonic_ms: 0,
                wall_epoch_ms: 0,
            },
        );
        assert!(!output.keep_alive);
        assert!(output.effects.is_empty());
        assert!(output.panel.chooser.unwrap().devices.is_empty());
    }
}
