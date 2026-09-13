// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let legacy =
        || serde_json::to_string_pretty(&product_contracts::client_data_contract_inventory());
    let compatibility =
        || serde_json::to_string_pretty(&product_contracts::live_feed_compatibility_descriptor());
    match args.as_slice() {
        [] => println!("{}", legacy()?),
        [flag] if flag == "--live-feed-compatibility" => println!("{}", compatibility()?),
        [flag, directory] if flag == "--output-dir" => {
            let directory = std::path::Path::new(directory);
            std::fs::create_dir_all(directory)?;
            std::fs::write(
                directory.join("client-data-contracts.json"),
                format!("{}\n", legacy()?),
            )?;
            std::fs::write(
                directory.join("live-feed-compatibility.json"),
                format!("{}\n", compatibility()?),
            )?;
        }
        _ => {
            return Err(
                "usage: export-contract-inventory [--live-feed-compatibility | --output-dir DIR]"
                    .into(),
            )
        }
    }
    Ok(())
}
