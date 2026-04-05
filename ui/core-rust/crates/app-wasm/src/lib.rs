use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn load_catalog(catalog_json: &str) -> Result<String, JsValue> {
    let handle = app_core::load_catalog(catalog_json)
        .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&handle).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen]
pub fn build_flight_plan(plan_json: &str) -> Result<String, JsValue> {
    let plan: app_core::FlightPlan =
        serde_json::from_str(plan_json).map_err(|err| JsValue::from_str(&err.to_string()))?;
    let plan = app_core::build_flight_plan(plan)
        .map_err(|err| JsValue::from_str(&err.to_string()))?;
    serde_json::to_string(&plan).map_err(|err| JsValue::from_str(&err.to_string()))
}
