// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{env, fs, path::PathBuf};

use app_ui_contracts::{cloud, home, nexrad, session, UI_WIRE_CONTRACT_VERSION};
use schemars::{schema_for, JsonSchema};
use serde_json::{json, Map, Value};

struct ContractSchema {
    filename: &'static str,
    id: &'static str,
    description: &'static str,
    export_order: &'static [&'static str],
    tagged_unions: &'static [(&'static str, &'static str)],
    schema: fn() -> Value,
}

fn schema<T: JsonSchema>() -> Value {
    serde_json::to_value(schema_for!(T)).expect("serialize generated JSON schema")
}

fn cloud_schema() -> Value {
    let mut root = schema::<cloud::UiCloudPageState>();
    add_definition::<cloud::CloudProviderKind>(&mut root, "CloudProviderKind");
    add_definition::<cloud::CloudProviderPrincipal>(&mut root, "CloudProviderPrincipal");
    add_definition::<cloud::CloudAuthorizationMode>(&mut root, "CloudAuthorizationMode");
    add_definition::<cloud::CloudAuthorizationRequest>(&mut root, "CloudAuthorizationRequest");
    add_definition::<cloud::CloudAuthorizationResponse>(&mut root, "CloudAuthorizationResponse");
    add_definition::<cloud::CloudUiFieldValue>(&mut root, "CloudUiFieldValue");
    add_definition::<cloud::CloudHttpMethod>(&mut root, "CloudHttpMethod");
    add_definition::<cloud::CloudHttpHeader>(&mut root, "CloudHttpHeader");
    add_definition::<cloud::CloudHttpRequest>(&mut root, "CloudHttpRequest");
    add_definition::<cloud::CloudHttpResponse>(&mut root, "CloudHttpResponse");
    add_definition::<cloud::CloudEventStreamPlan>(&mut root, "CloudEventStreamPlan");
    add_definition::<cloud::CloudEventStreamEventKind>(&mut root, "CloudEventStreamEventKind");
    add_definition::<cloud::CloudEventStreamEvent>(&mut root, "CloudEventStreamEvent");
    root
}

fn session_schema() -> Value {
    let mut root = schema::<session::UiSessionPageContracts>();
    add_definition::<session::FlightEstimateKind>(&mut root, "FlightEstimateKind");
    add_definition::<session::FlightDataColumn>(&mut root, "FlightDataColumn");
    add_definition::<session::FlightDataBannerModel>(&mut root, "FlightDataBannerModel");
    add_definition::<session::MapLayerId>(&mut root, "MapLayerId");
    add_definition::<session::DebugFlagId>(&mut root, "DebugFlagId");
    root
}

fn add_definition<T: JsonSchema>(root: &mut Value, name: &str) {
    let mut generated = schema::<T>();
    let generated_object = generated
        .as_object_mut()
        .expect("generated definition must be an object");
    generated_object.remove("$schema");
    let nested = generated_object
        .remove("$defs")
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();

    let definitions = root
        .as_object_mut()
        .expect("schema root must be an object")
        .entry("$defs")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .expect("$defs must be an object");
    for (nested_name, nested_schema) in nested {
        definitions.entry(nested_name).or_insert(nested_schema);
    }
    definitions.entry(name.to_string()).or_insert(generated);
}

fn main() {
    let output_dir = output_dir();
    fs::create_dir_all(&output_dir).expect("create schema output directory");

    let contracts = [
        ContractSchema {
            filename: "home-page-wire.schema.json",
            id: "org.aerobag.ui-wire.home-page",
            description: "Core-owned Home-page navigation contract.",
            export_order: &["UiHomeDestination", "UiHomePageButton", "UiHomePageState"],
            tagged_unions: &[],
            schema: schema::<home::UiHomePageState>,
        },
        ContractSchema {
            filename: "nexrad-overlay-wire.schema.json",
            id: "org.aerobag.ui-wire.nexrad-overlay",
            description: "Core-owned wire contract for NEXRAD overlay queries emitted by app-core.",
            export_order: &[
                "NexradOverlayStatus",
                "NexradOverlayAnimationPhase",
                "NexradOverlayAnimation",
                "NexradOverlayScreenPoint",
                "NexradOverlayTileCorners",
                "NexradOverlayTile",
                "NexradOverlayStats",
                "NexradOverlayQueryResult",
            ],
            tagged_unions: &[("NexradOverlayStatus", "state")],
            schema: schema::<nexrad::NexradOverlayQueryResult>,
        },
        ContractSchema {
            filename: "cloud-wire.schema.json",
            id: "org.aerobag.ui-wire.cloud",
            description: "Core-owned Cloud UI and provider-effect wire contract.",
            export_order: &[
                "CloudProviderKind",
                "CloudProviderPrincipal",
                "CloudAuthorizationMode",
                "CloudAuthorizationRequest",
                "CloudAuthorizationResponse",
                "CloudUiActionId",
                "CloudUiFieldId",
                "CloudUiFieldValue",
                "UiQrCode",
                "CloudPlatformEffect",
                "UiCloudPanelState",
                "UiCloudAction",
                "UiCloudPanelControl",
                "UiCloudTimeFact",
                "UiCloudPanel",
                "UiCloudPageState",
                "CloudHttpMethod",
                "CloudHttpHeader",
                "CloudHttpRequest",
                "CloudHttpResponse",
                "CloudEventStreamPlan",
                "CloudEventStreamEventKind",
                "CloudEventStreamEvent",
            ],
            tagged_unions: &[
                ("CloudAuthorizationResponse", "state"),
                ("CloudPlatformEffect", "kind"),
                ("UiCloudPanelControl", "kind"),
                ("CloudHttpResponse", "result"),
            ],
            schema: cloud_schema,
        },
        ContractSchema {
            filename: "session-page-wire.schema.json",
            id: "org.aerobag.ui-wire.session-page",
            description:
                "Core-owned session page, platform capability, and settings-action wire contract.",
            export_order: &[
                "FlightDataCellTone",
                "FlightEstimateKind",
                "FlightDataCell",
                "FlightDataColumn",
                "FlightDataBannerModel",
                "UiStatusSeverity",
                "UiStatusActionStyle",
                "UiStatusAction",
                "UiDataStatusBox",
                "UiDataStatusState",
                "UiDataStatusPageTimeDisplay",
                "UiDataStatusPageFact",
                "UiDataStatusPageRow",
                "UiDataStatusPageState",
                "UiChartPageState",
                "UiMapLayerToggleState",
                "UiMapLayerState",
                "MapLayerId",
                "DebugFlagId",
                "UiDebugState",
                "UiSettingsSliderStop",
                "UiSettingsGridItem",
                "UiSettingsPageRow",
                "UiSettingsPageState",
                "UiDisplayPolicy",
                "UiDisclaimerState",
                "UiSettingsAction",
                "UiPlaybackPanelState",
                "UiNavDbIdentity",
                "PlatformDisplayPolicyCapability",
                "PlatformOfflinePackagesCapability",
                "PlatformCloudCapability",
                "LiveFeedAcquisitionPolicy",
                "PlatformLiveFeedsCapability",
                "ClientBuildInfo",
                "PlatformCapabilities",
            ],
            tagged_unions: &[],
            schema: session_schema,
        },
    ];

    for contract in contracts {
        let mut generated = (contract.schema)();
        let object = generated
            .as_object_mut()
            .expect("schemars root must be an object");
        object.insert("$id".to_string(), json!(contract.id));
        object.insert(
            "x-contract-version".to_string(),
            json!(UI_WIRE_CONTRACT_VERSION),
        );
        object.insert("description".to_string(), json!(contract.description));
        object.insert("x-export-order".to_string(), json!(contract.export_order));
        for (name, discriminator) in contract.tagged_unions {
            definition_mut(object, name).insert(
                "x-discriminator".to_string(),
                Value::String((*discriminator).to_string()),
            );
        }
        let mut bytes = serde_json::to_vec_pretty(&generated).expect("encode JSON schema");
        bytes.push(b'\n');
        fs::write(output_dir.join(contract.filename), bytes).expect("write JSON schema");
    }
}

fn definition_mut<'a>(root: &'a mut Map<String, Value>, name: &str) -> &'a mut Map<String, Value> {
    root.get_mut("$defs")
        .and_then(Value::as_object_mut)
        .and_then(|definitions| definitions.get_mut(name))
        .and_then(Value::as_object_mut)
        .unwrap_or_else(|| panic!("generated schema is missing $defs/{name}"))
}

fn output_dir() -> PathBuf {
    let mut args = env::args().skip(1);
    while let Some(argument) = args.next() {
        if argument == "--out-dir" {
            return PathBuf::from(args.next().expect("--out-dir requires a path"));
        }
        panic!("unknown argument {argument}");
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("schemas")
}
