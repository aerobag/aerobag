// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

pub(crate) const MIN_GEOMETRY_DISTANCE_NM: f64 = 0.05;
pub(crate) const MIN_ARC_SWEEP_DEG: f64 = 0.5;
pub(crate) const POSITION_EPSILON_DEG: f64 = 0.0005;
pub(crate) const MAX_APPROACH_DISPLAY_ELEMENT_DISTANCE_NM: f64 = 40.0;
pub(crate) const MAX_ENROUTE_TRANSITION_DISPLAY_ELEMENT_DISTANCE_NM: f64 = 200.0;
pub(crate) const MAX_PUBLISHED_HOLD_OR_MISSED_SEGMENT_DISTANCE_NM: f64 = 60.0;
pub(crate) const EXPLICIT_MISSED_TURN_SOURCE_PREFIX: &str = "explicit_missed_turn@";
pub(crate) const INFERRED_MISSED_TURN_SOURCE_PREFIX: &str = "inferred_missed_turn@";
pub(crate) const PLATE_EXCEPTION_MISSED_TURN_SOURCE_PREFIX: &str = "plate_exception_missed_turn@";
pub(crate) const BORROWED_LATER_HOLD_FOR_PI_SOURCE_PREFIX: &str = "borrowed_later_hold_for_pi@";
pub(crate) const INVENTED_PI_ENTRY_REVERSAL_SOURCE_PREFIX: &str = "invented_pi_entry_reversal@";
pub(crate) const CF_DIRECT_SHORTCUT_MISALIGNED_SOURCE_PREFIX: &str =
    "cf_direct_shortcut_misaligned@";
