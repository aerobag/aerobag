// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

pub use app_ui_contracts::session::MapInspectionCommand;

const INSPECTOR_IDLE_MS: i64 = 10_000;

#[derive(Clone, Default)]
pub(crate) struct MapInspection {
    open: bool,
    detail_open: bool,
    touching: bool,
    deadline: Option<i64>,
    pub dismissal_revision: u64,
}

impl MapInspection {
    pub fn apply(&mut self, command: MapInspectionCommand, now: i64) {
        use MapInspectionCommand::*;
        match command {
            Open => {
                self.open = true;
                self.detail_open = false;
                self.touching = false;
                self.deadline = Some(now.saturating_add(INSPECTOR_IDLE_MS));
            }
            Dismiss => self.dismiss(),
            MapGesture if !self.detail_open => self.dismiss(),
            DetailOpened => {
                self.open = true;
                self.detail_open = true;
                self.deadline = None;
            }
            TouchStarted if self.open && !self.detail_open => {
                self.touching = true;
                self.deadline = None;
            }
            TouchEnded | Activity if self.open && !self.detail_open => {
                if command == TouchEnded {
                    self.touching = false;
                }
                if !self.touching {
                    self.deadline = Some(now.saturating_add(INSPECTOR_IDLE_MS));
                }
            }
            _ => {}
        }
    }

    fn dismiss(&mut self) {
        if self.open {
            self.open = false;
            self.deadline = None;
            self.dismissal_revision += 1;
        }
    }

    pub fn refresh(&mut self, now: i64) {
        if self.deadline.is_some_and(|deadline| now >= deadline) {
            self.dismiss();
        }
    }

    pub fn next_refresh(&self) -> Option<i64> {
        self.deadline
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use MapInspectionCommand::*;

    #[test]
    fn accidental_inspection_expires_even_while_background_updates_continue() {
        let mut state = MapInspection::default();
        state.apply(Open, 1_000);
        assert_eq!(state.next_refresh(), Some(11_000));
        for now in [2_000, 7_000, 10_999] {
            state.refresh(now);
            assert_eq!(state.dismissal_revision, 0);
        }
        state.refresh(11_000);
        assert_eq!(state.dismissal_revision, 1);
        assert_eq!(state.next_refresh(), None);
        state.refresh(12_000);
        state.apply(Activity, 13_000);
        assert_eq!(state.dismissal_revision, 1);
        assert_eq!(state.next_refresh(), None);
    }

    #[test]
    fn interaction_restarts_idle_time_and_a_held_touch_cannot_time_out() {
        let mut state = MapInspection::default();
        state.apply(Open, 1_000);
        state.apply(Activity, 9_000);
        state.refresh(11_000);
        assert_eq!(state.dismissal_revision, 0);
        assert_eq!(state.next_refresh(), Some(19_000));
        state.apply(TouchStarted, 18_000);
        state.apply(Activity, 18_500);
        state.refresh(40_000);
        assert_eq!(state.dismissal_revision, 0);
        assert_eq!(state.next_refresh(), None);
        state.apply(TouchEnded, 41_000);
        state.refresh(51_000);
        assert_eq!(state.dismissal_revision, 1);
    }

    #[test]
    fn backdrop_gesture_dismisses_inspector_but_not_a_full_detail_dialog() {
        let mut state = MapInspection::default();
        state.apply(Open, 1_000);
        state.apply(MapGesture, 2_000);
        assert_eq!(state.dismissal_revision, 1);
        state.apply(Open, 3_000);
        state.apply(DetailOpened, 4_000);
        state.apply(TouchEnded, 5_000);
        state.apply(MapGesture, 6_000);
        state.refresh(90_000);
        assert_eq!(state.dismissal_revision, 1);
        assert_eq!(state.next_refresh(), None);
        state.apply(Dismiss, 91_000);
        assert_eq!(state.dismissal_revision, 2);
    }

    #[test]
    fn automatically_opened_detail_can_be_dismissed_without_an_intermediate_tray() {
        let mut state = MapInspection::default();
        state.apply(MapInspectionCommand::DetailOpened, 1_000);
        state.refresh(99_000);
        assert_eq!(state.dismissal_revision, 0);
        state.apply(MapInspectionCommand::Dismiss, 100_000);
        assert_eq!(state.dismissal_revision, 1);
    }
}
