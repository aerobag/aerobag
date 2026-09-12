// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! A genuinely different, hermetic successor contract for mixed-client tests.
//! Never included in ordinary application builds.
use super::*;

pub(crate) static NEXT: AccountFormat = AccountFormat {
    version: 2,
    predecessor: Some(&CURRENT),
    decode_node: |value| serde_json::from_value(value).map_err(cloud_json_error),
    decode_page: |value| {
        let page: TestPage = serde_json::from_value(value).map_err(cloud_json_error)?;
        if page.version != 2 {
            return Err(cloud_error("Expected test account page 2"));
        }
        let page = page_for_records(&page.entries);
        validate_cloud_page(&page)?;
        check_marker(&page.records)?;
        Ok(page)
    },
    encode_page: |page| {
        validate_cloud_page(page)?;
        check_marker(&page.records)?;
        serde_json::to_value(TestPage {
            version: 2,
            entries: page.records.clone(),
        })
        .map_err(cloud_json_error)
    },
    migrate_records: |records| {
        if let Some(marker) = records.get_mut("test/format_marker") {
            if marker.value != "old marker" || marker.schema_version != 1 {
                return Err(cloud_error("Invalid source migration marker"));
            }
            marker.schema_version = 2;
            marker.value = serde_json::json!({"migrated_marker": "new marker"});
        }
        Ok(())
    },
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TestPage {
    version: u32,
    entries: BTreeMap<String, CloudRecord>,
}

fn check_marker(records: &BTreeMap<String, CloudRecord>) -> AppResult<()> {
    if records.get("test/format_marker").is_some_and(|marker| {
        marker.schema_version != 2
            || marker.value != serde_json::json!({"migrated_marker": "new marker"})
    }) {
        return Err(cloud_error("Unmigrated marker in new-format output"));
    }
    Ok(())
}
