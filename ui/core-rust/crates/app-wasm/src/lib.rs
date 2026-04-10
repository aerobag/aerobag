use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn load_catalog(catalog_json: &str) -> Result<String, JsValue> {
    load_catalog_json(catalog_json).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn build_flight_plan(plan_json: &str) -> Result<String, JsValue> {
    build_flight_plan_json(plan_json).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn remove_flight_plan_leg(plan_json: &str, index: usize) -> Result<String, JsValue> {
    remove_flight_plan_leg_json(plan_json, index).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn replace_flight_plan_state(
    state_json: &str,
    catalog_json: &str,
    plan_json: &str,
) -> Result<String, JsValue> {
    replace_flight_plan_state_json(state_json, catalog_json, plan_json)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn set_content_policy_state(
    state_json: &str,
    catalog_json: &str,
    policy_json: &str,
) -> Result<String, JsValue> {
    set_content_policy_state_json(state_json, catalog_json, policy_json)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn refresh_content_state(
    state_json: &str,
    catalog_json: &str,
    inventory_json: &str,
) -> Result<String, JsValue> {
    refresh_content_state_json(state_json, catalog_json, inventory_json)
        .map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen]
pub fn chart_for_position(
    catalog_json: &str,
    geometry_json: &str,
    family_json: &str,
    lat: f64,
    lon: f64,
) -> Result<String, JsValue> {
    chart_for_position_json(catalog_json, geometry_json, family_json, lat, lon)
        .map_err(|err| JsValue::from_str(&err))
}

fn load_catalog_json(catalog_json: &str) -> Result<String, String> {
    let handle =
        app_core::load_catalog(catalog_json).map_err(|err| err.to_string())?;
    serde_json::to_string(&handle).map_err(|err| err.to_string())
}

fn build_flight_plan_json(plan_json: &str) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let plan = app_core::build_flight_plan(plan).map_err(|err| err.to_string())?;
    serde_json::to_string(&plan).map_err(|err| err.to_string())
}

fn remove_flight_plan_leg_json(plan_json: &str, index: usize) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let plan = app_core::remove_flight_plan_leg(&plan, index).map_err(|err| err.to_string())?;
    serde_json::to_string(&plan).map_err(|err| err.to_string())
}

fn replace_flight_plan_state_json(
    state_json: &str,
    catalog_json: &str,
    plan_json: &str,
) -> Result<String, String> {
    let state: app_core::AppState =
        serde_json::from_str(state_json).map_err(|err| err.to_string())?;
    let catalog = app_core::load_catalog(catalog_json).map_err(|err| err.to_string())?;
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let next = app_core::state::reduce(
        &state,
        app_core::AppEvent::ReplaceFlightPlan(plan),
        &catalog,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&next).map_err(|err| err.to_string())
}

fn set_content_policy_state_json(
    state_json: &str,
    catalog_json: &str,
    policy_json: &str,
) -> Result<String, String> {
    let state: app_core::AppState =
        serde_json::from_str(state_json).map_err(|err| err.to_string())?;
    let catalog = app_core::load_catalog(catalog_json).map_err(|err| err.to_string())?;
    let policy: app_core::ContentPolicy =
        serde_json::from_str(policy_json).map_err(|err| err.to_string())?;
    let next = app_core::state::reduce(
        &state,
        app_core::AppEvent::SetContentPolicy(policy),
        &catalog,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&next).map_err(|err| err.to_string())
}

fn refresh_content_state_json(
    state_json: &str,
    catalog_json: &str,
    inventory_json: &str,
) -> Result<String, String> {
    let state: app_core::AppState =
        serde_json::from_str(state_json).map_err(|err| err.to_string())?;
    let catalog = app_core::load_catalog(catalog_json).map_err(|err| err.to_string())?;
    let inventory: app_core::ContentInventory =
        serde_json::from_str(inventory_json).map_err(|err| err.to_string())?;
    let next = app_core::state::reduce(
        &state,
        app_core::AppEvent::RefreshContent { inventory },
        &catalog,
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&next).map_err(|err| err.to_string())
}

fn chart_for_position_json(
    catalog_json: &str,
    geometry_json: &str,
    family_json: &str,
    lat: f64,
    lon: f64,
) -> Result<String, String> {
    let catalog = app_core::load_catalog(catalog_json).map_err(|err| err.to_string())?;
    let geometry: app_core::GeometryBundle =
        serde_json::from_str(geometry_json).map_err(|err| err.to_string())?;
    let family: app_core::ChartFamilyId =
        serde_json::from_str(family_json).map_err(|err| err.to_string())?;
    let chart =
        app_core::chart_for_position(&catalog, &geometry, family, lat, lon).map_err(|err| err.to_string())?;
    serde_json::to_string(&chart).map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_catalog_json() -> String {
        serde_json::json!({
            "schema_version": 1,
            "cycle": "2026-04-16",
            "catalog_revision": "2026-04-05T22:00:00Z",
            "families": [
                {
                    "id": "sectional",
                    "display_name": "VFR Sectional Charts",
                    "kind": "tiled_raster",
                    "max_zoom": 10,
                    "tile_size": 512
                }
            ],
            "regions": [
                {
                    "id": "ne",
                    "display_name": "Northeast",
                    "sort_order": 0
                }
            ],
            "packages": [
                {
                    "id": {
                        "region": "ne",
                        "family": "sectional",
                        "cycle": "2026-04-16"
                    },
                    "package_name": "NE_SEC",
                    "family_id": "sectional",
                    "region_id": "ne",
                    "cycle": "2026-04-16",
                    "artifact_kind": "zip",
                    "relative_url": "/2026-04-16/NE_SEC.zip",
                    "manifest_name": "NE_SEC",
                    "size_bytes": null,
                    "checksum_sha256": null
                }
            ],
            "charts": [
                {
                    "id": {
                        "family": "sectional",
                        "name": "Boston",
                        "cycle": "2026-04-16"
                    },
                    "family_id": "sectional",
                    "name": "Boston",
                    "display_name": "Boston",
                    "cycle": "2026-04-16",
                    "region_ids": ["ne"],
                    "max_zoom": 10,
                    "tile_path_template": "tiles/{chart_index}/{z}/{x}/{y}",
                    "coverage": {
                        "kind": "polygon_ref",
                        "value": {
                            "polygon_id": "sectional:boston"
                        }
                    }
                }
            ],
            "plates": [
                {
                    "id": {
                        "airport_id": "KBOS",
                        "procedure_code": "IAP-ILS-RWY-04R",
                        "page": 1,
                        "cycle": "2026-04-16"
                    },
                    "airport_id": "KBOS",
                    "region_id": "ne",
                    "cycle": "2026-04-16",
                    "procedure_code": "IAP-ILS-RWY-04R",
                    "display_name": "ILS OR LOC RWY 04R",
                    "kind": "approach",
                    "georeferenced": true,
                    "page_count": 1,
                    "asset_base_path": "plates/KBOS/IAP-ILS-RWY-04R"
                }
            ],
            "supplements": []
        })
        .to_string()
    }

    fn empty_state_json() -> String {
        serde_json::to_string(&app_core::AppState::default()).unwrap()
    }

    fn sample_geometry_json() -> String {
        serde_json::json!({
            "schema_version": 1,
            "polygons": [
                {
                    "id": "sectional:boston",
                    "points": [
                        [-72.0, 41.0],
                        [-70.0, 41.0],
                        [-70.0, 43.0],
                        [-72.0, 43.0],
                        [-72.0, 41.0]
                    ]
                }
            ]
        })
        .to_string()
    }

    fn sample_plan_json() -> String {
        serde_json::json!({
            "id": "plan-1",
            "name": "KBOS local",
            "legs": [
                {
                    "from": {"Airport": "KBOS"},
                    "to": {"Airport": "KBOS"},
                    "airway": null
                }
            ],
            "departure": "KBOS",
            "destination": "KBOS",
            "alternate": null,
            "cruise_altitude_ft": 3000,
            "notes": null,
            "updated_at_epoch_ms": 0,
            "version": 1
        })
        .to_string()
    }

    #[test]
    fn replace_flight_plan_state_json_populates_requirements() {
        let next_json = replace_flight_plan_state_json(
            &empty_state_json(),
            &sample_catalog_json(),
            &sample_plan_json(),
        )
        .unwrap();
        let next: app_core::AppState = serde_json::from_str(&next_json).unwrap();

        assert!(next.active_plan.is_some());
        assert_eq!(next.last_content_requirements.len(), 1);
    }

    #[test]
    fn stream_allowed_policy_survives_json_boundary() {
        let with_plan_json = replace_flight_plan_state_json(
            &empty_state_json(),
            &sample_catalog_json(),
            &sample_plan_json(),
        )
        .unwrap();

        let web_state_json = set_content_policy_state_json(
            &with_plan_json,
            &sample_catalog_json(),
            &serde_json::to_string(&app_core::ContentPolicy::StreamAllowed).unwrap(),
        )
        .unwrap();

        let refreshed_json = refresh_content_state_json(
            &web_state_json,
            &sample_catalog_json(),
            &serde_json::json!({
                "installed_packages": [],
                "cached_tilesets": [],
                "cached_plates": []
            })
            .to_string(),
        )
        .unwrap();

        let refreshed: app_core::AppState = serde_json::from_str(&refreshed_json).unwrap();
        assert!(refreshed.last_content_report.as_ref().unwrap().fully_satisfied);
    }

    #[test]
    fn chart_for_position_json_returns_matching_chart() {
        let chart_json = chart_for_position_json(
            &sample_catalog_json(),
            &sample_geometry_json(),
            &serde_json::to_string(&app_core::ChartFamilyId::Sectional).unwrap(),
            42.0,
            -71.0,
        )
        .unwrap();
        let chart: Option<app_core::ChartRecord> = serde_json::from_str(&chart_json).unwrap();

        assert_eq!(chart.unwrap().display_name, "Boston");
    }

    #[test]
    fn chart_for_position_json_returns_null_outside_coverage() {
        let chart_json = chart_for_position_json(
            &sample_catalog_json(),
            &sample_geometry_json(),
            &serde_json::to_string(&app_core::ChartFamilyId::Sectional).unwrap(),
            35.0,
            -71.0,
        )
        .unwrap();
        let chart: Option<app_core::ChartRecord> = serde_json::from_str(&chart_json).unwrap();

        assert!(chart.is_none());
    }
}
