// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::weather_controller::NexradSource;

pub(crate) const SWITCH_AGE_MS: i64 = 10 * 60_000;
pub(crate) const FRESHNESS_GAIN_MS: i64 = 5 * 60_000;
pub(crate) const MAX_DISPLAY_AGE_MS: i64 = 60 * 60_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Auto,
    Only(NexradSource),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Selection {
    pub mode: Mode,
    pub preferred: NexradSource,
    pub effective: Option<NexradSource>,
}

impl Default for Selection {
    fn default() -> Self {
        Self {
            mode: Mode::Auto,
            preferred: NexradSource::Internet,
            effective: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Transition {
    pub from: Option<NexradSource>,
    pub to: Option<NexradSource>,
    pub reason: &'static str,
    pub observed: [Option<i64>; 2],
}

impl Selection {
    pub fn requested(self) -> NexradSource {
        match self.mode {
            Mode::Only(source) => source,
            Mode::Auto => self.effective.unwrap_or(self.preferred),
        }
    }

    pub fn choose(&mut self, mode: Mode, preferred: NexradSource) {
        self.mode = mode;
        self.preferred = preferred;
        self.effective = match mode {
            Mode::Only(source) => Some(source),
            Mode::Auto => None,
        };
    }

    /// Inputs are observation times of drawable imagery, never receipt times.
    pub fn update(&mut self, now: i64, observed: [Option<i64>; 2]) -> Option<Transition> {
        let available = observed
            .map(|time| time.filter(|time| now.saturating_sub(*time) <= MAX_DISPLAY_AGE_MS));
        let from = self.effective;
        let (to, reason) = match self.mode {
            Mode::Only(source) => (Some(source), "explicit source"),
            Mode::Auto => {
                let selected = from.filter(|source| available[source.index()].is_some());
                match selected {
                    None => (
                        [self.preferred, self.preferred.other()]
                            .into_iter()
                            .find(|source| available[source.index()].is_some()),
                        if from.is_some() {
                            "source expired"
                        } else {
                            "initial source"
                        },
                    ),
                    Some(source) => {
                        let current = available[source.index()].unwrap();
                        let other = source.other();
                        if now.saturating_sub(current) >= SWITCH_AGE_MS
                            && available[other.index()].is_some_and(|time| {
                                time.saturating_sub(current) >= FRESHNESS_GAIN_MS
                            })
                        {
                            (Some(other), "fresher by hysteresis margin")
                        } else {
                            (Some(source), "retain source")
                        }
                    }
                }
            }
        };
        self.effective = to;
        (from != to).then_some(Transition {
            from,
            to,
            reason,
            observed,
        })
    }
}

impl NexradSource {
    pub const fn index(self) -> usize {
        match self {
            Self::Internet => 0,
            Self::Receiver => 1,
        }
    }

    pub const fn other(self) -> Self {
        match self {
            Self::Internet => Self::Receiver,
            Self::Receiver => Self::Internet,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const M: i64 = 60_000;

    #[test]
    fn hysteresis_is_symmetric_and_does_not_chase_nominal_updates() {
        for initial in [NexradSource::Internet, NexradSource::Receiver] {
            let mut state = Selection {
                preferred: initial,
                ..Selection::default()
            };
            let pair = |a, b| {
                let mut result = [None; 2];
                result[initial.index()] = a;
                result[initial.other().index()] = b;
                result
            };
            state.update(0, pair(Some(0), Some(0)));
            for minute in 1..100 {
                let now = minute * M;
                let times = if minute % 2 == 0 {
                    pair(Some(now - 2 * M), Some(now))
                } else {
                    pair(Some(now), Some(now - 2 * M))
                };
                assert!(state.update(now, times).is_none());
                assert_eq!(state.effective, Some(initial));
            }
            assert!(state
                .update(110 * M - 1, pair(Some(100 * M), Some(105 * M)))
                .is_none());
            assert!(state
                .update(110 * M, pair(Some(100 * M), Some(105 * M - 1)))
                .is_none());
            assert!(state
                .update(110 * M, pair(Some(100 * M), Some(105 * M)))
                .is_some());
            assert_eq!(state.effective, Some(initial.other()));
            // Preferred source recovers, but the selected source is still recent.
            state.update(111 * M, pair(Some(111 * M), Some(105 * M)));
            assert_eq!(state.effective, Some(initial.other()));
        }
    }

    #[test]
    fn readiness_expiry_and_explicit_locks_do_not_mean_connectivity() {
        let mut state = Selection::default();
        assert!(state.update(0, [None, None]).is_none());
        state.update(0, [None, Some(0)]); // Internet manifest without pixels.
        assert_eq!(state.effective, Some(NexradSource::Receiver));
        state.update(60 * M, [None, Some(0)]);
        assert_eq!(state.effective, Some(NexradSource::Receiver));
        state.update(60 * M + 1, [Some(M), Some(0)]);
        assert_eq!(state.effective, Some(NexradSource::Internet));
        state.update(62 * M, [Some(M), Some(0)]);
        assert_eq!(state.effective, None);
        for source in [NexradSource::Internet, NexradSource::Receiver] {
            state.choose(Mode::Only(source), source);
            state.update(100 * M, [None, None]);
            assert_eq!(state.effective, Some(source));
            state.update(100 * M, [Some(100 * M), Some(100 * M)]);
            assert_eq!(state.effective, Some(source));
        }
    }
}
