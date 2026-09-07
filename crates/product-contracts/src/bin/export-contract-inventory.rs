// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

fn main() {
    println!(
        "{}",
        serde_json::to_string_pretty(&product_contracts::client_data_contract_inventory())
            .expect("serialize client data contract inventory")
    );
}
