// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use product_contracts::service_bulletins::{BulletinDocument, MAX_BYTES};
use std::io::{Read, Write};

fn main() {
    if let Err(error) = validate() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn validate() -> Result<(), String> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let document = BulletinDocument::decode(&bytes)?;
    let canonical = serde_json::to_vec(&document).map_err(|e| e.to_string())?;
    std::io::stdout()
        .write_all(&canonical)
        .map_err(|e| e.to_string())
}
