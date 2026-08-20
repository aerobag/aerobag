// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::io;

use notam_state::NotamState;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    serde_json::to_writer_pretty(io::stdout().lock(), &NotamState::empty().checkpoint())?;
    Ok(())
}
