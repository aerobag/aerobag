// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

use app_ui_contracts::session::ClientBuildInfo;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupMode {
    #[default]
    Normal,
    OmitSetupPayload,
}

impl SetupMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "Normal: both blobs",
            Self::OmitSetupPayload => "Experiment: small blob only",
        }
    }
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

/// ABRX v2 setup record, durable before the socket is opened. Synthetic and
/// standalone core callers may not have a platform build identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupMetadata {
    pub mode: SetupMode,
    pub build: Option<ClientBuildInfo>,
    pub challenge_seed: BlobFingerprint,
    pub setup_payload: BlobFingerprint,
}
