// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Code identities describe the executable, not a possibly different checkout.
use std::sync::OnceLock;

fn get(kind: &str, relative: &str) -> anyhow::Result<String> {
    static SOURCES: OnceLock<serde_json::Value> = OnceLock::new();
    let sources = SOURCES.get_or_init(|| {
        serde_json::from_str(include_str!(concat!(
            env!("OUT_DIR"),
            "/compiled-sources.json"
        )))
        .expect("compiled source identities")
    });
    sources[kind][relative]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| {
            anyhow::anyhow!("source identity missing from executable: {kind}/{relative}")
        })
}

pub fn file(relative: &str) -> anyhow::Result<String> {
    get("files", relative)
}
pub fn tree(relative: &str) -> anyhow::Result<String> {
    get("trees", relative)
}
