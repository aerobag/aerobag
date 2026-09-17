// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotamBadgeUiView {
    pub label: String,
    pub count: usize,
    pub action_id: String,
    pub accessibility_label: String,
    pub detail: NotamDetailUiView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotamDetailUiView {
    pub title: String,
    pub advisory_text: String,
    pub empty_text: String,
    pub notams: Vec<crate::AirportNotamUiView>,
}
