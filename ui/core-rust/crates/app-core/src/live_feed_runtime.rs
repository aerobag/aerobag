// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use product_contracts::{SseTransportPolicy, AEROBAG_SSE_TRANSPORT_POLICY};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveFeedConnectionEventKind {
    Connecting,
    Connected,
    Message,
    Error,
    Closed,
    NetworkStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveFeedNetworkStatus {
    Unmetered,
    Metered,
    NoActiveNetwork,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveFeedConnectionEvent {
    pub kind: LiveFeedConnectionEventKind,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub status_url: Option<String>,
    #[serde(default)]
    pub network_status: Option<LiveFeedNetworkStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveFeedRuntimeEventKind {
    Start,
    NetworkStatus,
    Connecting,
    Connected,
    Message,
    Error,
    Closed,
    IdleTimeout,
    Online,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveFeedRuntimeInput {
    pub kind: LiveFeedRuntimeEventKind,
    pub now_ms: i64,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub status_url: Option<String>,
    #[serde(default)]
    pub network_status: Option<LiveFeedNetworkStatus>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveFeedRuntimeDecision {
    pub transport_policy: SseTransportPolicy,
    #[serde(default)]
    pub connection_event: Option<LiveFeedConnectionEvent>,
    pub commands: Vec<LiveFeedRuntimeCommand>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LiveFeedRuntimeCommand {
    Reconnect { delay_ms: i64 },
    RetryResources { delay_ms: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LiveFeedRuntimeState {
    consecutive_errors: u32,
}

pub fn live_feed_runtime_decision(
    state: &mut LiveFeedRuntimeState,
    input: LiveFeedRuntimeInput,
) -> LiveFeedRuntimeDecision {
    use LiveFeedConnectionEventKind as ConnectionKind;
    use LiveFeedRuntimeEventKind as RuntimeKind;

    let connection_kind = match input.kind {
        RuntimeKind::Connecting => Some(ConnectionKind::Connecting),
        RuntimeKind::Connected => Some(ConnectionKind::Connected),
        RuntimeKind::Message => Some(ConnectionKind::Message),
        RuntimeKind::Error => Some(ConnectionKind::Error),
        RuntimeKind::Closed | RuntimeKind::IdleTimeout => Some(ConnectionKind::Closed),
        RuntimeKind::NetworkStatus => Some(ConnectionKind::NetworkStatus),
        RuntimeKind::Start | RuntimeKind::Online => None,
    };
    let connection_event = connection_kind.map(|kind| LiveFeedConnectionEvent {
        kind,
        message: input.message.clone(),
        source_url: input.source_url.clone(),
        status_url: input.status_url.clone(),
        network_status: input.network_status,
    });
    let reconnect_delay_ms = match input.kind {
        RuntimeKind::Error | RuntimeKind::Closed => {
            state.consecutive_errors = state.consecutive_errors.saturating_add(1);
            Some(reconnect_delay_for_consecutive_errors(
                state.consecutive_errors,
            ))
        }
        RuntimeKind::IdleTimeout | RuntimeKind::Online => Some(0),
        RuntimeKind::Connected | RuntimeKind::Message => {
            state.consecutive_errors = 0;
            None
        }
        RuntimeKind::Start | RuntimeKind::NetworkStatus | RuntimeKind::Connecting => None,
    };

    LiveFeedRuntimeDecision {
        transport_policy: AEROBAG_SSE_TRANSPORT_POLICY,
        connection_event,
        commands: reconnect_delay_ms
            .map(|delay_ms| LiveFeedRuntimeCommand::Reconnect { delay_ms })
            .into_iter()
            .collect(),
    }
}

fn reconnect_delay_for_consecutive_errors(consecutive_errors: u32) -> i64 {
    AEROBAG_SSE_TRANSPORT_POLICY.reconnect_delay_ms(consecutive_errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connected_waits_for_stream_catalog() {
        let mut state = LiveFeedRuntimeState::default();
        let decision = live_feed_runtime_decision(
            &mut state,
            LiveFeedRuntimeInput {
                kind: LiveFeedRuntimeEventKind::Connected,
                now_ms: 0,
                message: None,
                source_url: Some("http://example.test".to_string()),
                status_url: Some("http://example.test/live-feeds/status.html".to_string()),
                network_status: Some(LiveFeedNetworkStatus::Unmetered),
            },
        );

        assert!(decision.commands.is_empty());
        let event = decision.connection_event.unwrap();
        assert_eq!(event.kind, LiveFeedConnectionEventKind::Connected);
        assert_eq!(event.source_url.as_deref(), Some("http://example.test"));
    }

    #[test]
    fn start_does_not_block_on_current_catalog_refresh() {
        let mut state = LiveFeedRuntimeState::default();
        let decision = live_feed_runtime_decision(
            &mut state,
            LiveFeedRuntimeInput {
                kind: LiveFeedRuntimeEventKind::Start,
                now_ms: 0,
                message: None,
                source_url: Some("http://example.test".to_string()),
                status_url: Some("http://example.test/live-feeds/status.html".to_string()),
                network_status: Some(LiveFeedNetworkStatus::Unmetered),
            },
        );

        assert!(decision.commands.is_empty());
        assert!(decision.connection_event.is_none());
    }

    #[test]
    fn errors_report_and_back_off_exponentially_to_idle_timeout() {
        let mut state = LiveFeedRuntimeState::default();
        let mut delays = Vec::new();
        for _ in 0..6 {
            let decision = live_feed_runtime_decision(
                &mut state,
                LiveFeedRuntimeInput {
                    kind: LiveFeedRuntimeEventKind::Error,
                    now_ms: 0,
                    message: Some("boom".to_string()),
                    source_url: None,
                    status_url: None,
                    network_status: None,
                },
            );
            let LiveFeedRuntimeCommand::Reconnect { delay_ms } = decision.commands[0] else {
                panic!("expected reconnect command");
            };
            delays.push(delay_ms);
        }

        assert_eq!(delays, vec![5_000, 10_000, 20_000, 40_000, 65_000, 65_000]);
    }

    #[test]
    fn errors_report_connection_event() {
        let mut state = LiveFeedRuntimeState::default();
        let decision = live_feed_runtime_decision(
            &mut state,
            LiveFeedRuntimeInput {
                kind: LiveFeedRuntimeEventKind::Error,
                now_ms: 0,
                message: Some("boom".to_string()),
                source_url: None,
                status_url: None,
                network_status: None,
            },
        );

        assert_eq!(
            decision.commands,
            vec![LiveFeedRuntimeCommand::Reconnect {
                delay_ms: AEROBAG_SSE_TRANSPORT_POLICY.reconnect_initial_delay_ms,
            }]
        );
        let event = decision.connection_event.unwrap();
        assert_eq!(event.kind, LiveFeedConnectionEventKind::Error);
        assert_eq!(event.message.as_deref(), Some("boom"));
    }

    #[test]
    fn successful_message_resets_backoff() {
        let mut state = LiveFeedRuntimeState::default();
        for _ in 0..3 {
            live_feed_runtime_decision(
                &mut state,
                LiveFeedRuntimeInput {
                    kind: LiveFeedRuntimeEventKind::Error,
                    now_ms: 0,
                    message: Some("boom".to_string()),
                    source_url: None,
                    status_url: None,
                    network_status: None,
                },
            );
        }
        live_feed_runtime_decision(
            &mut state,
            LiveFeedRuntimeInput {
                kind: LiveFeedRuntimeEventKind::Message,
                now_ms: 0,
                message: None,
                source_url: None,
                status_url: None,
                network_status: None,
            },
        );
        let decision = live_feed_runtime_decision(
            &mut state,
            LiveFeedRuntimeInput {
                kind: LiveFeedRuntimeEventKind::Error,
                now_ms: 0,
                message: Some("boom".to_string()),
                source_url: None,
                status_url: None,
                network_status: None,
            },
        );

        assert_eq!(
            decision.commands,
            vec![LiveFeedRuntimeCommand::Reconnect {
                delay_ms: AEROBAG_SSE_TRANSPORT_POLICY.reconnect_initial_delay_ms,
            }]
        );
    }
}
