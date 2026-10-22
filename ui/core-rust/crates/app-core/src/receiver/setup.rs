// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

use app_ui_contracts::session::ClientBuildInfo;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Frozen ABRX v2 values. Normal is retained for reading historical captures,
/// not as a selectable connection mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupMode {
    Normal,
    OmitSetupPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlobFingerprint {
    pub length: usize,
    pub sha256: String,
}

impl BlobFingerprint {
    pub fn of(bytes: &[u8]) -> Self {
        Self {
            length: bytes.len(),
            sha256: format!("{:x}", Sha256::digest(bytes)),
        }
    }
}

/// ABRX v2 setup record, durable before socket open while recording is healthy.
/// Standalone core callers may not have a platform build identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupMetadata {
    pub mode: SetupMode,
    pub build: Option<ClientBuildInfo>,
    pub challenge_seed: BlobFingerprint,
    pub setup_payload: BlobFingerprint,
}

impl SetupMetadata {
    pub(super) fn current(build: Option<ClientBuildInfo>) -> Self {
        Self {
            mode: SetupMode::OmitSetupPayload,
            build,
            challenge_seed: BlobFingerprint::of(&super::compatibility::CHALLENGE_SEED),
            setup_payload: BlobFingerprint::of(&[]),
        }
    }
}
