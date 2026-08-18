// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use prost::Message as _;
use sha2::{Digest, Sha256};

const PREFIX: &str = "AB3.";
const CHECKSUM_BYTES: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeviceSetupCode {
    pub root_secret: [u8; 32],
    pub provider: DeviceSetupProvider,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DeviceSetupProvider {
    AerobagCloud {
        base_url: String,
        account_locator: [u8; 32],
    },
}

pub(crate) fn encode_device_setup_code(value: &DeviceSetupCode) -> String {
    let wire = WireDeviceSetupCodeV3 {
        root_secret: value.root_secret.to_vec(),
        provider: Some(match &value.provider {
            DeviceSetupProvider::AerobagCloud {
                base_url,
                account_locator,
            } => wire_device_setup_code_v3::Provider::AerobagCloud(WireAerobagCloudSetup {
                base_url: base_url.clone(),
                account_locator: account_locator.to_vec(),
            }),
        }),
    };
    encode_wire_bytes(&wire.encode_to_vec())
}

pub(crate) fn decode_device_setup_code(encoded: &str) -> Result<DeviceSetupCode, String> {
    let encoded = encoded.trim();
    let body = encoded
        .strip_prefix(PREFIX)
        .ok_or_else(|| "Device Setup Code has an unsupported format".to_string())?;
    let bytes = URL_SAFE_NO_PAD
        .decode(body)
        .map_err(|_| "Device Setup Code is not valid base64".to_string())?;
    if bytes.len() <= CHECKSUM_BYTES {
        return Err("Device Setup Code content is truncated".to_string());
    }
    let payload_len = bytes.len() - CHECKSUM_BYTES;
    let (payload, supplied_checksum) = bytes.split_at(payload_len);
    let expected_checksum = Sha256::digest(payload);
    if supplied_checksum != &expected_checksum[..CHECKSUM_BYTES] {
        return Err("Device Setup Code checksum does not match".to_string());
    }

    let wire = WireDeviceSetupCodeV3::decode(payload)
        .map_err(|_| "Device Setup Code protobuf is invalid".to_string())?;
    let root_secret = fixed_32(
        wire.root_secret,
        "Device Setup Code root secret must be 256 bits",
    )?;
    let provider = match wire.provider {
        Some(wire_device_setup_code_v3::Provider::AerobagCloud(provider)) => {
            if provider.base_url.trim().is_empty() {
                return Err("Aerobag Cloud provider URL is missing".to_string());
            }
            DeviceSetupProvider::AerobagCloud {
                base_url: provider.base_url,
                account_locator: fixed_32(
                    provider.account_locator,
                    "Aerobag Cloud account locator must be 256 bits",
                )?,
            }
        }
        None => return Err("Device Setup Code has no storage provider".to_string()),
    };
    Ok(DeviceSetupCode {
        root_secret,
        provider,
    })
}

fn fixed_32(bytes: Vec<u8>, error: &str) -> Result<[u8; 32], String> {
    bytes.try_into().map_err(|_| error.to_string())
}

fn encode_wire_bytes(payload: &[u8]) -> String {
    let checksum = Sha256::digest(payload);
    let mut body = Vec::with_capacity(payload.len() + CHECKSUM_BYTES);
    body.extend_from_slice(payload);
    body.extend_from_slice(&checksum[..CHECKSUM_BYTES]);
    format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(body))
}

// These derives intentionally mirror proto/device_setup_code_v3.proto without
// requiring protoc in every product build.
#[derive(Clone, PartialEq, prost::Message)]
struct WireDeviceSetupCodeV3 {
    #[prost(bytes = "vec", tag = "1")]
    root_secret: Vec<u8>,
    #[prost(oneof = "wire_device_setup_code_v3::Provider", tags = "11")]
    provider: Option<wire_device_setup_code_v3::Provider>,
}

mod wire_device_setup_code_v3 {
    #[derive(Clone, PartialEq, prost::Oneof)]
    pub(super) enum Provider {
        #[prost(message, tag = "11")]
        AerobagCloud(super::WireAerobagCloudSetup),
    }
}

#[derive(Clone, PartialEq, prost::Message)]
struct WireAerobagCloudSetup {
    #[prost(string, tag = "1")]
    base_url: String,
    #[prost(bytes = "vec", tag = "2")]
    account_locator: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acs() -> DeviceSetupCode {
        DeviceSetupCode {
            root_secret: [0x42; 32],
            provider: DeviceSetupProvider::AerobagCloud {
                base_url: "https://aerobag.org/cloud/".to_string(),
                account_locator: [0xa3; 32],
            },
        }
    }

    #[test]
    fn aerobag_cloud_round_trip() {
        let acs = acs();
        assert_eq!(
            decode_device_setup_code(&encode_device_setup_code(&acs)).unwrap(),
            acs
        );
    }

    #[test]
    fn checksum_and_format_reject_corruption_and_ab2() {
        let mut corrupted = encode_device_setup_code(&acs()).into_bytes();
        corrupted[12] = if corrupted[12] == b'A' { b'B' } else { b'A' };
        let corrupted = String::from_utf8(corrupted).unwrap();
        assert_eq!(
            decode_device_setup_code(&corrupted).unwrap_err(),
            "Device Setup Code checksum does not match"
        );
        assert_eq!(
            decode_device_setup_code("AB2.not-supported.deadbeef").unwrap_err(),
            "Device Setup Code has an unsupported format"
        );
    }

    #[test]
    fn protobuf_wire_fixture_is_stable() {
        assert_eq!(
            encode_device_setup_code(&acs()),
            "AB3.CiBCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQlo-ChpodHRwczovL2Flcm9iYWcub3JnL2Nsb3VkLxIgo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6PIpSJUl80gIQ"
        );
    }
}
