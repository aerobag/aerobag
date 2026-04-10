pub use app_core::*;
use jni::objects::{JClass, JString};
use jni::sys::jstring;
use jni::JNIEnv;

pub fn load_catalog_json(catalog_json: &str) -> Result<String, String> {
    let handle =
        app_core::load_catalog(catalog_json).map_err(|err| err.to_string())?;
    serde_json::to_string(&handle).map_err(|err| err.to_string())
}

pub fn build_flight_plan_json(plan_json: &str) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let plan = app_core::build_flight_plan(plan).map_err(|err| err.to_string())?;
    serde_json::to_string(&plan).map_err(|err| err.to_string())
}

pub fn remove_flight_plan_leg_json(plan_json: &str, index: usize) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let plan = app_core::remove_flight_plan_leg(&plan, index).map_err(|err| err.to_string())?;
    serde_json::to_string(&plan).map_err(|err| err.to_string())
}

pub fn replace_flight_plan_state_json(
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

pub fn set_content_policy_state_json(
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

pub fn refresh_content_state_json(
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

pub fn chart_for_position_json(
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

pub fn derive_chart_page_json(
    resource_index_json: &str,
    plan_json: &str,
) -> Result<String, String> {
    let resource_index =
        app_core::load_resource_index_chart_page_input(resource_index_json).map_err(|err| err.to_string())?;
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let chart_page = app_core::derive_chart_page(&resource_index, &plan);
    serde_json::to_string(&chart_page).map_err(|err| err.to_string())
}

pub fn derive_chart_page_state_json(
    resource_index_json: &str,
    plan_json: &str,
    recent_airport_ids_json: &str,
    selected_airport_id_json: &str,
    selected_chart_id_json: &str,
) -> Result<String, String> {
    let resource_index =
        app_core::load_resource_index_chart_page_input(resource_index_json).map_err(|err| err.to_string())?;
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let recent_airport_ids: Vec<String> =
        serde_json::from_str(recent_airport_ids_json).map_err(|err| err.to_string())?;
    let selected_airport_id: Option<String> =
        serde_json::from_str(selected_airport_id_json).map_err(|err| err.to_string())?;
    let selected_chart_id: Option<String> =
        serde_json::from_str(selected_chart_id_json).map_err(|err| err.to_string())?;
    let state = app_core::derive_chart_page_state(
        &resource_index,
        &plan,
        &recent_airport_ids,
        selected_airport_id.as_deref(),
        selected_chart_id.as_deref(),
    );
    serde_json::to_string(&state).map_err(|err| err.to_string())
}

pub fn create_ui_session_json(
    catalog_json: &str,
    resource_index_json: &str,
    plan_json: &str,
    recent_airport_ids_json: &str,
    selected_airport_id_json: &str,
    selected_chart_id_json: &str,
) -> Result<String, String> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| err.to_string())?;
    let recent_airport_ids: Vec<String> =
        serde_json::from_str(recent_airport_ids_json).map_err(|err| err.to_string())?;
    let selected_airport_id: Option<String> =
        serde_json::from_str(selected_airport_id_json).map_err(|err| err.to_string())?;
    let selected_chart_id: Option<String> =
        serde_json::from_str(selected_chart_id_json).map_err(|err| err.to_string())?;
    let result = app_core::create_ui_session(
        catalog_json,
        resource_index_json,
        plan,
        &recent_airport_ids,
        selected_airport_id.as_deref(),
        selected_chart_id.as_deref(),
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&result).map_err(|err| err.to_string())
}

pub fn remove_leg_in_session_json(handle: u64, index: usize) -> Result<String, String> {
    let snapshot = app_core::remove_leg_in_session(handle, index).map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn move_waypoint_in_session_json(
    handle: u64,
    waypoint_index: usize,
    delta: isize,
) -> Result<String, String> {
    let snapshot = app_core::move_waypoint_in_session(handle, waypoint_index, delta)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn select_airport_in_session_json(handle: u64, airport_id_json: &str) -> Result<String, String> {
    let airport_id: String = serde_json::from_str(airport_id_json).map_err(|err| err.to_string())?;
    let snapshot =
        app_core::select_airport_in_session(handle, &airport_id).map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn select_chart_in_session_json(handle: u64, chart_id_json: &str) -> Result<String, String> {
    let chart_id: String = serde_json::from_str(chart_id_json).map_err(|err| err.to_string())?;
    let snapshot =
        app_core::select_chart_in_session(handle, &chart_id).map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn get_session_snapshot_json(handle: u64) -> Result<String, String> {
    let snapshot = app_core::get_session_snapshot(handle).map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn restore_chart_page_state_in_session_json(
    handle: u64,
    recent_airport_ids_json: &str,
    selected_airport_id_json: &str,
    selected_chart_id_json: &str,
) -> Result<String, String> {
    let recent_airport_ids: Vec<String> =
        serde_json::from_str(recent_airport_ids_json).map_err(|err| err.to_string())?;
    let selected_airport_id: Option<String> =
        serde_json::from_str(selected_airport_id_json).map_err(|err| err.to_string())?;
    let selected_chart_id: Option<String> =
        serde_json::from_str(selected_chart_id_json).map_err(|err| err.to_string())?;
    let snapshot = app_core::restore_chart_page_state_in_session(
        handle,
        &recent_airport_ids,
        selected_airport_id.as_deref(),
        selected_chart_id.as_deref(),
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_string(&snapshot).map_err(|err| err.to_string())
}

pub fn destroy_session_json(handle: u64) {
    app_core::destroy_session(handle);
}

fn get_java_string(env: &mut JNIEnv, value: JString) -> Result<String, String> {
    env.get_string(&value)
        .map(|s| s.into())
        .map_err(|err| err.to_string())
}

fn return_string(env: &mut JNIEnv, value: Result<String, String>) -> jstring {
    match value {
        Ok(text) => env
            .new_string(text)
            .map(|s| s.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(message) => {
            let _ = env.throw_new("java/lang/RuntimeException", message);
            std::ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_replaceFlightPlanStateJson(
    mut env: JNIEnv,
    _class: JClass,
    state_json: JString,
    catalog_json: JString,
    plan_json: JString,
) -> jstring {
    let result = (|| {
        let state = get_java_string(&mut env, state_json)?;
        let catalog = get_java_string(&mut env, catalog_json)?;
        let plan = get_java_string(&mut env, plan_json)?;
        replace_flight_plan_state_json(&state, &catalog, &plan)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_removeFlightPlanLegJson(
    mut env: JNIEnv,
    _class: JClass,
    plan_json: JString,
    index: i32,
) -> jstring {
    let result = (|| {
        let plan = get_java_string(&mut env, plan_json)?;
        remove_flight_plan_leg_json(&plan, index as usize)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_setContentPolicyStateJson(
    mut env: JNIEnv,
    _class: JClass,
    state_json: JString,
    catalog_json: JString,
    policy_json: JString,
) -> jstring {
    let result = (|| {
        let state = get_java_string(&mut env, state_json)?;
        let catalog = get_java_string(&mut env, catalog_json)?;
        let policy = get_java_string(&mut env, policy_json)?;
        set_content_policy_state_json(&state, &catalog, &policy)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_refreshContentStateJson(
    mut env: JNIEnv,
    _class: JClass,
    state_json: JString,
    catalog_json: JString,
    inventory_json: JString,
) -> jstring {
    let result = (|| {
        let state = get_java_string(&mut env, state_json)?;
        let catalog = get_java_string(&mut env, catalog_json)?;
        let inventory = get_java_string(&mut env, inventory_json)?;
        refresh_content_state_json(&state, &catalog, &inventory)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_chartForPositionJson(
    mut env: JNIEnv,
    _class: JClass,
    catalog_json: JString,
    geometry_json: JString,
    family_json: JString,
    lat: f64,
    lon: f64,
) -> jstring {
    let result = (|| {
        let catalog = get_java_string(&mut env, catalog_json)?;
        let geometry = get_java_string(&mut env, geometry_json)?;
        let family = get_java_string(&mut env, family_json)?;
        chart_for_position_json(&catalog, &geometry, &family, lat, lon)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_deriveChartPageJson(
    mut env: JNIEnv,
    _class: JClass,
    resource_index_json: JString,
    plan_json: JString,
) -> jstring {
    let result = (|| {
        let resource_index = get_java_string(&mut env, resource_index_json)?;
        let plan = get_java_string(&mut env, plan_json)?;
        derive_chart_page_json(&resource_index, &plan)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_deriveChartPageStateJson(
    mut env: JNIEnv,
    _class: JClass,
    resource_index_json: JString,
    plan_json: JString,
    recent_airport_ids_json: JString,
    selected_airport_id_json: JString,
    selected_chart_id_json: JString,
) -> jstring {
    let result = (|| {
        let resource_index = get_java_string(&mut env, resource_index_json)?;
        let plan = get_java_string(&mut env, plan_json)?;
        let recent_airport_ids = get_java_string(&mut env, recent_airport_ids_json)?;
        let selected_airport_id = get_java_string(&mut env, selected_airport_id_json)?;
        let selected_chart_id = get_java_string(&mut env, selected_chart_id_json)?;
        derive_chart_page_state_json(
            &resource_index,
            &plan,
            &recent_airport_ids,
            &selected_airport_id,
            &selected_chart_id,
        )
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_createUiSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    catalog_json: JString,
    resource_index_json: JString,
    plan_json: JString,
    recent_airport_ids_json: JString,
    selected_airport_id_json: JString,
    selected_chart_id_json: JString,
) -> jstring {
    let result = (|| {
        let catalog = get_java_string(&mut env, catalog_json)?;
        let resource_index = get_java_string(&mut env, resource_index_json)?;
        let plan = get_java_string(&mut env, plan_json)?;
        let recent_airport_ids = get_java_string(&mut env, recent_airport_ids_json)?;
        let selected_airport_id = get_java_string(&mut env, selected_airport_id_json)?;
        let selected_chart_id = get_java_string(&mut env, selected_chart_id_json)?;
        create_ui_session_json(
            &catalog,
            &resource_index,
            &plan,
            &recent_airport_ids,
            &selected_airport_id,
            &selected_chart_id,
        )
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_removeLegInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    index: i32,
) -> jstring {
    return_string(&mut env, remove_leg_in_session_json(handle as u64, index as usize))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_moveWaypointInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    waypoint_index: i32,
    delta: i32,
) -> jstring {
    return_string(
        &mut env,
        move_waypoint_in_session_json(handle as u64, waypoint_index as usize, delta as isize),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_selectAirportInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    airport_id_json: JString,
) -> jstring {
    let result = (|| {
        let airport_id = get_java_string(&mut env, airport_id_json)?;
        select_airport_in_session_json(handle as u64, &airport_id)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_selectChartInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    chart_id_json: JString,
) -> jstring {
    let result = (|| {
        let chart_id = get_java_string(&mut env, chart_id_json)?;
        select_chart_in_session_json(handle as u64, &chart_id)
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_getSessionSnapshotJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
) -> jstring {
    return_string(&mut env, get_session_snapshot_json(handle as u64))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_restoreChartPageStateInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    recent_airport_ids_json: JString,
    selected_airport_id_json: JString,
    selected_chart_id_json: JString,
) -> jstring {
    let result = (|| {
        let recent_airport_ids = get_java_string(&mut env, recent_airport_ids_json)?;
        let selected_airport_id = get_java_string(&mut env, selected_airport_id_json)?;
        let selected_chart_id = get_java_string(&mut env, selected_chart_id_json)?;
        restore_chart_page_state_in_session_json(
            handle as u64,
            &recent_airport_ids,
            &selected_airport_id,
            &selected_chart_id,
        )
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_jonh_aerobag_prototype_domain_NativeBindings_destroySession(
    _env: JNIEnv,
    _class: JClass,
    handle: i64,
) {
    destroy_session_json(handle as u64)
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
