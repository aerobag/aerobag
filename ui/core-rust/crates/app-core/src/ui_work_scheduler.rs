// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiSessionWorkKind {
    MapOverlay,
    MapSelection,
    MapSelectionForNavRef,
}

impl UiSessionWorkKind {
    fn is_input_priority(self) -> bool {
        matches!(
            self,
            UiSessionWorkKind::MapSelection | UiSessionWorkKind::MapSelectionForNavRef
        )
    }

    fn is_viewport_coalesced(self) -> bool {
        matches!(self, UiSessionWorkKind::MapOverlay)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiSessionWorkRequest {
    pub id: u64,
    pub kind: UiSessionWorkKind,
    pub coalesce_key: Option<String>,
    pub requested_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UiSessionWorkRequestDecision {
    Start { request: UiSessionWorkRequest },
    Queued { replaced_request_id: Option<u64> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UiSessionWorkResultAction {
    Land,
    Drop { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiSessionWorkCompletionDecision {
    pub result_action: UiSessionWorkResultAction,
    pub next: Option<UiSessionWorkRequest>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UiSessionWorkScheduler {
    active_input: Option<UiSessionWorkRequest>,
    active_overlay: Option<UiSessionWorkRequest>,
    pending_input: Option<UiSessionWorkRequest>,
    pending_overlay: Option<UiSessionWorkRequest>,
}

impl UiSessionWorkScheduler {
    pub fn request(&mut self, request: UiSessionWorkRequest) -> UiSessionWorkRequestDecision {
        if request.kind.is_input_priority() {
            if self.active_input.is_none() {
                self.active_input = Some(request.clone());
                return UiSessionWorkRequestDecision::Start { request };
            }
            let replaced_request_id = self.pending_input.replace(request).map(|old| old.id);
            return UiSessionWorkRequestDecision::Queued {
                replaced_request_id,
            };
        }
        if request.kind.is_viewport_coalesced() {
            if self.active_overlay.is_none() && self.active_input.is_none() {
                self.active_overlay = Some(request.clone());
                return UiSessionWorkRequestDecision::Start { request };
            }
            let replaced_request_id = self.pending_overlay.replace(request).map(|old| old.id);
            return UiSessionWorkRequestDecision::Queued {
                replaced_request_id,
            };
        }
        if self.active_input.is_none() {
            self.active_input = Some(request.clone());
            return UiSessionWorkRequestDecision::Start { request };
        }
        let replaced_request_id = self.pending_input.replace(request).map(|old| old.id);
        UiSessionWorkRequestDecision::Queued {
            replaced_request_id,
        }
    }

    pub fn complete(&mut self, request_id: u64) -> UiSessionWorkCompletionDecision {
        if self
            .active_input
            .as_ref()
            .is_some_and(|active| active.id == request_id)
        {
            let active = self
                .active_input
                .take()
                .expect("active input checked above");
            return self.complete_active_request(active);
        }
        if self
            .active_overlay
            .as_ref()
            .is_some_and(|active| active.id == request_id)
        {
            let active = self
                .active_overlay
                .take()
                .expect("active overlay checked above");
            return self.complete_active_request(active);
        }
        if self.active_input.is_none() && self.active_overlay.is_none() {
            return UiSessionWorkCompletionDecision {
                result_action: UiSessionWorkResultAction::Drop {
                    reason: "no_active_request".to_string(),
                },
                next: None,
            };
        }
        UiSessionWorkCompletionDecision {
            result_action: UiSessionWorkResultAction::Drop {
                reason: "request_is_not_active".to_string(),
            },
            next: None,
        }
    }

    fn complete_active_request(
        &mut self,
        active: UiSessionWorkRequest,
    ) -> UiSessionWorkCompletionDecision {
        let result_action = if active.kind.is_input_priority() && self.pending_input.is_some() {
            UiSessionWorkResultAction::Drop {
                reason: "superseded_by_newer_input".to_string(),
            }
        } else if active.kind.is_viewport_coalesced() && self.pending_overlay.is_some() {
            UiSessionWorkResultAction::Drop {
                reason: "superseded_by_newer_overlay".to_string(),
            }
        } else {
            UiSessionWorkResultAction::Land
        };
        let next = self.next_request_to_start();
        UiSessionWorkCompletionDecision {
            result_action,
            next,
        }
    }

    fn next_request_to_start(&mut self) -> Option<UiSessionWorkRequest> {
        if self.active_input.is_none() {
            if let Some(next_input) = self.pending_input.take() {
                self.active_input = Some(next_input.clone());
                return Some(next_input);
            }
        }
        if self.active_input.is_none() && self.active_overlay.is_none() {
            if let Some(next_overlay) = self.pending_overlay.take() {
                self.active_overlay = Some(next_overlay.clone());
                return Some(next_overlay);
            }
        }
        None
    }

    pub fn active_request(&self) -> Option<&UiSessionWorkRequest> {
        self.active_input
            .as_ref()
            .or_else(|| self.active_overlay.as_ref())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionSnapshotRefreshPriority {
    Timely,
    LowPriority,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SessionSnapshotRefreshDecision {
    Idle,
    Schedule { delay_ms: u64, reason: String },
    Start { reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSnapshotRefreshSchedulerConfig {
    pub background_debounce_ms: u64,
    pub post_viewport_activity_idle_ms: u64,
    pub active_gesture_retry_ms: u64,
}

impl Default for SessionSnapshotRefreshSchedulerConfig {
    fn default() -> Self {
        Self {
            background_debounce_ms: 250,
            post_viewport_activity_idle_ms: 400,
            active_gesture_retry_ms: 100,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingSessionSnapshotRefresh {
    reason: String,
    priority: SessionSnapshotRefreshPriority,
    requested_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSnapshotRefreshScheduler {
    config: SessionSnapshotRefreshSchedulerConfig,
    pending: Option<PendingSessionSnapshotRefresh>,
    in_flight: bool,
    viewport_gesture_active: bool,
    viewport_quiet_at_ms: u64,
}

impl SessionSnapshotRefreshScheduler {
    pub fn new(config: SessionSnapshotRefreshSchedulerConfig) -> Self {
        Self {
            config,
            pending: None,
            in_flight: false,
            viewport_gesture_active: false,
            viewport_quiet_at_ms: 0,
        }
    }

    pub fn request(
        &mut self,
        now_ms: u64,
        priority: SessionSnapshotRefreshPriority,
        reason: impl Into<String>,
    ) -> SessionSnapshotRefreshDecision {
        let reason = reason.into();
        match &mut self.pending {
            Some(pending) => {
                pending.reason = reason;
                pending.requested_at_ms = now_ms;
                if matches!(priority, SessionSnapshotRefreshPriority::Timely) {
                    pending.priority = priority;
                }
            }
            None => {
                self.pending = Some(PendingSessionSnapshotRefresh {
                    reason,
                    priority,
                    requested_at_ms: now_ms,
                });
            }
        }
        self.decide(now_ms)
    }

    pub fn viewport_gesture_active_changed(
        &mut self,
        now_ms: u64,
        active: bool,
    ) -> SessionSnapshotRefreshDecision {
        self.viewport_gesture_active = active;
        if !active {
            self.record_viewport_activity_quiet_window(now_ms);
        }
        self.decide(now_ms)
    }

    pub fn viewport_activity(&mut self, now_ms: u64) -> SessionSnapshotRefreshDecision {
        self.record_viewport_activity_quiet_window(now_ms);
        self.decide(now_ms)
    }

    pub fn refresh_completed(&mut self, now_ms: u64) -> SessionSnapshotRefreshDecision {
        self.in_flight = false;
        self.decide(now_ms)
    }

    pub fn poll(&mut self, now_ms: u64) -> SessionSnapshotRefreshDecision {
        self.decide(now_ms)
    }

    fn record_viewport_activity_quiet_window(&mut self, now_ms: u64) {
        self.viewport_quiet_at_ms = self
            .viewport_quiet_at_ms
            .max(now_ms.saturating_add(self.config.post_viewport_activity_idle_ms));
    }

    fn decide(&mut self, now_ms: u64) -> SessionSnapshotRefreshDecision {
        let Some(pending) = self.pending.as_ref() else {
            return SessionSnapshotRefreshDecision::Idle;
        };
        if self.in_flight {
            return SessionSnapshotRefreshDecision::Schedule {
                delay_ms: self.config.active_gesture_retry_ms,
                reason: pending.reason.clone(),
            };
        }
        if matches!(pending.priority, SessionSnapshotRefreshPriority::Timely) {
            return self.start_pending();
        }
        let debounce_ready_at_ms = pending
            .requested_at_ms
            .saturating_add(self.config.background_debounce_ms);
        let debounce_remaining_ms = debounce_ready_at_ms.saturating_sub(now_ms);
        let quiet_remaining_ms = self.viewport_quiet_at_ms.saturating_sub(now_ms);
        let gesture_retry_ms = if self.viewport_gesture_active {
            self.config.active_gesture_retry_ms
        } else {
            0
        };
        let delay_ms = debounce_remaining_ms
            .max(quiet_remaining_ms)
            .max(gesture_retry_ms);
        if delay_ms > 0 {
            return SessionSnapshotRefreshDecision::Schedule {
                delay_ms,
                reason: pending.reason.clone(),
            };
        }
        self.start_pending()
    }

    fn start_pending(&mut self) -> SessionSnapshotRefreshDecision {
        let Some(pending) = self.pending.take() else {
            return SessionSnapshotRefreshDecision::Idle;
        };
        self.in_flight = true;
        SessionSnapshotRefreshDecision::Start {
            reason: pending.reason,
        }
    }
}

impl Default for SessionSnapshotRefreshScheduler {
    fn default() -> Self {
        Self::new(SessionSnapshotRefreshSchedulerConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work_request(id: u64, kind: UiSessionWorkKind) -> UiSessionWorkRequest {
        UiSessionWorkRequest {
            id,
            kind,
            coalesce_key: Some(format!("{kind:?}")),
            requested_at_ms: id * 10,
        }
    }

    #[test]
    fn session_work_starts_first_request_immediately() {
        let mut scheduler = UiSessionWorkScheduler::default();
        let request = work_request(1, UiSessionWorkKind::MapOverlay);
        assert_eq!(
            scheduler.request(request.clone()),
            UiSessionWorkRequestDecision::Start {
                request: request.clone()
            }
        );
        assert_eq!(scheduler.active_request(), Some(&request));
    }

    #[test]
    fn session_work_coalesces_overlay_to_latest_pending_request() {
        let mut scheduler = UiSessionWorkScheduler::default();
        assert!(matches!(
            scheduler.request(work_request(1, UiSessionWorkKind::MapOverlay)),
            UiSessionWorkRequestDecision::Start { .. }
        ));
        assert_eq!(
            scheduler.request(work_request(2, UiSessionWorkKind::MapOverlay)),
            UiSessionWorkRequestDecision::Queued {
                replaced_request_id: None
            }
        );
        assert_eq!(
            scheduler.request(work_request(3, UiSessionWorkKind::MapOverlay)),
            UiSessionWorkRequestDecision::Queued {
                replaced_request_id: Some(2)
            }
        );
        assert_eq!(
            scheduler.complete(1),
            UiSessionWorkCompletionDecision {
                result_action: UiSessionWorkResultAction::Drop {
                    reason: "superseded_by_newer_overlay".to_string(),
                },
                next: Some(work_request(3, UiSessionWorkKind::MapOverlay)),
            }
        );
        assert_eq!(
            scheduler.complete(3),
            UiSessionWorkCompletionDecision {
                result_action: UiSessionWorkResultAction::Land,
                next: None,
            }
        );
    }

    #[test]
    fn session_work_prioritizes_pending_input_over_pending_overlay() {
        let mut scheduler = UiSessionWorkScheduler::default();
        assert!(matches!(
            scheduler.request(work_request(1, UiSessionWorkKind::MapOverlay)),
            UiSessionWorkRequestDecision::Start { .. }
        ));
        assert_eq!(
            scheduler.request(work_request(2, UiSessionWorkKind::MapOverlay)),
            UiSessionWorkRequestDecision::Queued {
                replaced_request_id: None
            }
        );
        assert_eq!(
            scheduler.request(work_request(3, UiSessionWorkKind::MapSelection)),
            UiSessionWorkRequestDecision::Start {
                request: work_request(3, UiSessionWorkKind::MapSelection)
            }
        );
        assert_eq!(
            scheduler.complete(1),
            UiSessionWorkCompletionDecision {
                result_action: UiSessionWorkResultAction::Drop {
                    reason: "superseded_by_newer_overlay".to_string(),
                },
                next: None,
            }
        );
        assert_eq!(
            scheduler.complete(3),
            UiSessionWorkCompletionDecision {
                result_action: UiSessionWorkResultAction::Land,
                next: Some(work_request(2, UiSessionWorkKind::MapOverlay)),
            }
        );
    }

    #[test]
    fn session_work_drops_superseded_input_result() {
        let mut scheduler = UiSessionWorkScheduler::default();
        assert!(matches!(
            scheduler.request(work_request(1, UiSessionWorkKind::MapSelection)),
            UiSessionWorkRequestDecision::Start { .. }
        ));
        assert_eq!(
            scheduler.request(work_request(2, UiSessionWorkKind::MapSelectionForNavRef)),
            UiSessionWorkRequestDecision::Queued {
                replaced_request_id: None
            }
        );
        assert_eq!(
            scheduler.complete(1),
            UiSessionWorkCompletionDecision {
                result_action: UiSessionWorkResultAction::Drop {
                    reason: "superseded_by_newer_input".to_string(),
                },
                next: Some(work_request(2, UiSessionWorkKind::MapSelectionForNavRef)),
            }
        );
    }

    #[test]
    fn session_work_starts_input_while_overlay_is_active() {
        let mut scheduler = UiSessionWorkScheduler::default();
        assert_eq!(
            scheduler.request(work_request(1, UiSessionWorkKind::MapOverlay)),
            UiSessionWorkRequestDecision::Start {
                request: work_request(1, UiSessionWorkKind::MapOverlay)
            }
        );
        assert_eq!(
            scheduler.request(work_request(2, UiSessionWorkKind::MapSelection)),
            UiSessionWorkRequestDecision::Start {
                request: work_request(2, UiSessionWorkKind::MapSelection)
            }
        );
        assert_eq!(
            scheduler.complete(2),
            UiSessionWorkCompletionDecision {
                result_action: UiSessionWorkResultAction::Land,
                next: None,
            }
        );
        assert_eq!(
            scheduler.complete(1),
            UiSessionWorkCompletionDecision {
                result_action: UiSessionWorkResultAction::Land,
                next: None,
            }
        );
    }

    fn scheduler() -> SessionSnapshotRefreshScheduler {
        SessionSnapshotRefreshScheduler::default()
    }

    #[test]
    fn low_priority_refresh_waits_for_debounce() {
        let mut scheduler = scheduler();
        assert_eq!(
            scheduler.request(
                1_000,
                SessionSnapshotRefreshPriority::LowPriority,
                "live-feed"
            ),
            SessionSnapshotRefreshDecision::Schedule {
                delay_ms: 250,
                reason: "live-feed".to_string(),
            }
        );
        assert_eq!(
            scheduler.poll(1_249),
            SessionSnapshotRefreshDecision::Schedule {
                delay_ms: 1,
                reason: "live-feed".to_string(),
            }
        );
        assert_eq!(
            scheduler.poll(1_250),
            SessionSnapshotRefreshDecision::Start {
                reason: "live-feed".to_string(),
            }
        );
    }

    #[test]
    fn low_priority_refresh_waits_for_viewport_quiet() {
        let mut scheduler = scheduler();
        assert_eq!(
            scheduler.viewport_activity(1_000),
            SessionSnapshotRefreshDecision::Idle
        );
        assert_eq!(
            scheduler.request(1_010, SessionSnapshotRefreshPriority::LowPriority, "status"),
            SessionSnapshotRefreshDecision::Schedule {
                delay_ms: 390,
                reason: "status".to_string(),
            }
        );
        assert_eq!(
            scheduler.poll(1_260),
            SessionSnapshotRefreshDecision::Schedule {
                delay_ms: 140,
                reason: "status".to_string(),
            }
        );
        assert_eq!(
            scheduler.poll(1_400),
            SessionSnapshotRefreshDecision::Start {
                reason: "status".to_string(),
            }
        );
    }

    #[test]
    fn active_gesture_keeps_low_priority_refresh_pending() {
        let mut scheduler = scheduler();
        assert_eq!(
            scheduler.viewport_gesture_active_changed(1_000, true),
            SessionSnapshotRefreshDecision::Idle
        );
        assert_eq!(
            scheduler.request(1_000, SessionSnapshotRefreshPriority::LowPriority, "status"),
            SessionSnapshotRefreshDecision::Schedule {
                delay_ms: 250,
                reason: "status".to_string(),
            }
        );
        assert_eq!(
            scheduler.poll(1_300),
            SessionSnapshotRefreshDecision::Schedule {
                delay_ms: 100,
                reason: "status".to_string(),
            }
        );
        assert_eq!(
            scheduler.viewport_gesture_active_changed(1_350, false),
            SessionSnapshotRefreshDecision::Schedule {
                delay_ms: 400,
                reason: "status".to_string(),
            }
        );
        assert_eq!(
            scheduler.poll(1_750),
            SessionSnapshotRefreshDecision::Start {
                reason: "status".to_string(),
            }
        );
    }

    #[test]
    fn timely_refresh_starts_even_during_gesture() {
        let mut scheduler = scheduler();
        assert_eq!(
            scheduler.viewport_gesture_active_changed(1_000, true),
            SessionSnapshotRefreshDecision::Idle
        );
        assert_eq!(
            scheduler.request(1_010, SessionSnapshotRefreshPriority::Timely, "command"),
            SessionSnapshotRefreshDecision::Start {
                reason: "command".to_string(),
            }
        );
    }

    #[test]
    fn completed_refresh_runs_coalesced_timely_work_next() {
        let mut scheduler = scheduler();
        assert_eq!(
            scheduler.request(1_000, SessionSnapshotRefreshPriority::Timely, "first"),
            SessionSnapshotRefreshDecision::Start {
                reason: "first".to_string(),
            }
        );
        assert_eq!(
            scheduler.request(
                1_010,
                SessionSnapshotRefreshPriority::LowPriority,
                "background"
            ),
            SessionSnapshotRefreshDecision::Schedule {
                delay_ms: 100,
                reason: "background".to_string(),
            }
        );
        assert_eq!(
            scheduler.request(1_020, SessionSnapshotRefreshPriority::Timely, "second"),
            SessionSnapshotRefreshDecision::Schedule {
                delay_ms: 100,
                reason: "second".to_string(),
            }
        );
        assert_eq!(
            scheduler.refresh_completed(1_030),
            SessionSnapshotRefreshDecision::Start {
                reason: "second".to_string(),
            }
        );
    }
}
