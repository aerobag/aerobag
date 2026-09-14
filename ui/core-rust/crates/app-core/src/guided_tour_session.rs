// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;
use crate::guided_tour::{self, Demo, Step};
use app_ui_contracts::tour::{UiTourAction, UiTourPage};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TourIntroductionDocument {
    version: u32,
    offered: bool,
}

pub(super) fn load_introduction(s: &mut UiSession, existing_state: bool) -> AppResult<()> {
    let Some(storage) = &s.coordinator.persistence_storage else {
        return Ok(());
    };
    let offered = match storage.read_tour_introduction()? {
        Some(bytes) => {
            let doc: TourIntroductionDocument =
                serde_json::from_slice(&bytes).map_err(|e| AppError {
                    kind: AppErrorKind::InvalidManifest,
                    message: format!("Invalid tour introduction state: {e}"),
                })?;
            if doc.version != 1 {
                return Err(AppError {
                    kind: AppErrorKind::InvalidManifest,
                    message: "Unsupported tour introduction state version.".into(),
                });
            }
            doc.offered
        }
        None => existing_state,
    };
    s.coordinator.tour_introduction_offered = Some(offered);
    Ok(())
}

pub(super) fn persist_introduction(s: &UiSession) -> AppResult<()> {
    if let (Some(storage), Some(offered)) = (
        &s.coordinator.persistence_storage,
        s.coordinator.tour_introduction_offered,
    ) {
        let bytes = serde_json::to_vec(&TourIntroductionDocument {
            version: 1,
            offered,
        })
        .map_err(|e| AppError {
            kind: AppErrorKind::Internal,
            message: e.to_string(),
        })?;
        if storage.read_tour_introduction()?.as_deref() != Some(bytes.as_slice()) {
            storage.write_tour_introduction(&bytes)?;
        }
    }
    Ok(())
}

/// Only user choices are restored. Resource caches, live weather, NAVDB epochs,
/// clocks and platform capabilities continue to advance normally during a tour.
#[derive(Clone)]
pub(super) struct TourSavedState {
    flight_plan: FlightPlanModelCheckpoint,
    settings: SettingsModelCheckpoint,
    situation: SituationModelCheckpoint,
    map: MapModelCheckpoint,
    cloud: CloudModelCheckpoint,
    chart: UiChartPageState,
    wind: AltitudePlannerWindSelection,
    content_report: Option<ContentReport>,
    winds_phase: WindsAloftAcquisitionPhase,
}

impl TourSavedState {
    fn capture(s: &UiSession) -> Self {
        Self {
            flight_plan: s.flight_plan.checkpoint_model(),
            settings: s.settings.checkpoint_model(),
            situation: s.situation.checkpoint_model(),
            map: s.map.checkpoint_model(),
            cloud: s.cloud.checkpoint_model(),
            chart: s.coordinator.chart_page_state.clone(),
            wind: s.coordinator.altitude_planner_wind_selection,
            winds_phase: s.coordinator.winds_aloft_acquisition_phase,
            content_report: s.coordinator.last_content_report.clone(),
        }
    }
    fn restore(&self, s: &mut UiSession) {
        s.flight_plan.rollback_model(self.flight_plan.clone());
        s.settings.rollback_model(self.settings.clone());
        s.situation.restore_user_state(self.situation.clone());
        s.map.restore_user_choices(self.map.clone());
        s.cloud.rollback_model(self.cloud.clone());
        s.coordinator.chart_page_state = self.chart.clone();
        s.coordinator.altitude_planner_wind_selection = self.wind;
        s.coordinator.last_content_report = self.content_report.clone();
        s.coordinator.winds_aloft_acquisition_phase = self.winds_phase;
        s.projection_versions.force_flight_plan_update();
    }
}

pub fn perform_guided_tour_action_in_session(
    handle: u32,
    action: UiTourAction,
    expected_generation: Option<u64>,
) -> AppResult<HadOperationOutcome> {
    let slot = session_slot(handle)?;
    let mut guard = slot.lock_running()?;
    let s = &mut *guard;
    if action == UiTourAction::StartIntroduction
        && (s.coordinator.tour_introduction_offered != Some(false)
            || s.settings.disclaimer_required())
    {
        return unchanged_session_update_outcome(s);
    }
    let starting = matches!(
        action,
        UiTourAction::Start | UiTourAction::StartIntroduction
    );
    if !starting
        && (s.coordinator.guided_tour.is_none()
            || s.coordinator.guided_tour.as_ref().map(|t| t.generation) != expected_generation)
    {
        return Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: "That tour step has changed.".into(),
        });
    }
    if starting && s.coordinator.guided_tour.is_some() {
        return unchanged_session_update_outcome(s);
    }
    let android = s
        .coordinator
        .platform_capabilities
        .offline_packages
        .is_some();
    let all = guided_tour::steps(android);
    let index = s
        .coordinator
        .guided_tour
        .as_ref()
        .map(|t| t.position as usize - 1)
        .unwrap_or(0);
    let closing =
        action == UiTourAction::Close || (action == UiTourAction::Next && index + 1 == all.len());
    let previous_versions = s.projection_versions.versions();
    let persist_introduction = starting && s.coordinator.tour_introduction_offered == Some(false);
    let result = run_session_model_transaction_with_persistence(
        s,
        |s| {
            if closing {
                s.coordinator.tour_resume_step = if action == UiTourAction::Close {
                    s.coordinator
                        .guided_tour
                        .as_ref()
                        .map(|tour| tour.step_id.clone())
                } else {
                    None
                };
                if let Some(saved) = s.coordinator.tour_saved.clone() {
                    saved.restore(s);
                }
                s.coordinator.guided_tour = None;
                s.coordinator.tour_saved = None;
                s.coordinator.tour_demo = None;
            } else {
                let next = match action {
                    UiTourAction::StartIntroduction => 0,
                    UiTourAction::Start => s
                        .coordinator
                        .tour_resume_step
                        .as_deref()
                        .and_then(|id| all.iter().position(|step| step.id == id))
                        .unwrap_or(0),
                    UiTourAction::Restart => 0,
                    UiTourAction::Next => index + 1,
                    UiTourAction::Back => index.saturating_sub(1),
                    UiTourAction::Close => unreachable!(),
                };
                if starting {
                    if persist_introduction {
                        s.coordinator.tour_introduction_offered = Some(true);
                    }
                    s.coordinator.tour_saved = Some(Arc::new(TourSavedState::capture(s)));
                    // A separate anonymous model prevents demo edits from reaching a
                    // real sync account, even if a command records a local mutation.
                    s.cloud = CloudController::default();
                    let mut preferences = s.settings.persistent_preferences();
                    preferences.disabled_flight_data_cell_ids.clear();
                    preferences.display_dim_timeout =
                        crate::settings_controller::DisplayDimTimeout::Never;
                    preferences.inactivity_sleep_timeout =
                        crate::settings_controller::InactivitySleepTimeout::Never;
                    s.settings.restore_preferences(preferences);
                    s.coordinator.altitude_planner_wind_selection =
                        AltitudePlannerWindSelection::NoWind;
                    s.coordinator.chart_page_state.recent_airport_ids.clear();
                    s.coordinator.chart_page_state.plate_target_airport_id = None;
                }
                let generation = s
                    .coordinator
                    .guided_tour
                    .as_ref()
                    .map(|t| t.generation + 1)
                    .unwrap_or(s.coordinator.session_revision + 1);
                s.coordinator.guided_tour = Some(guided_tour::view(next, generation, android));
                prepare_step(s, &all[next])?;
            }
            Ok(vec![
                UiInvalidation::SessionSnapshot,
                UiInvalidation::FlightPlanRoute,
                UiInvalidation::MapOverlay,
                UiInvalidation::RasterTiles,
            ])
        },
        persist_introduction,
    )?;
    if !closing || !matches!(result, HadOperationOutcome::Complete { .. }) {
        return Ok(result);
    }
    let responses = std::mem::take(&mut s.runtime.tour_cloud_responses);
    drop(guard);
    // Requests already in flight at Start still belong to the saved account.
    // Land each exactly once after restoring it, using the normal transaction.
    for (request, response, time) in responses {
        complete_cloud_provider_request_in_session(handle, request, response, time)?;
    }
    let mut guard = slot.lock_running()?;
    session_update_outcome_since(
        &mut guard,
        previous_versions,
        vec![
            UiInvalidation::FlightPlanRoute,
            UiInvalidation::MapOverlay,
            UiInvalidation::RasterTiles,
        ],
    )
}

fn missing(message: impl Into<String>) -> HadReadError {
    HadReadError::Fatal(message.into())
}

fn prepare_step(s: &mut UiSession, step: &Step) -> Result<(), SessionModelTransactionError> {
    if s.coordinator.tour_demo != Some(step.demo) {
        prepare_plan(s, step.demo)?;
        s.coordinator.tour_demo = Some(step.demo);
    }
    // Repeat these assignments, rather than toggling: Back and remount must
    // reproduce the same scene regardless of the state before the tour.
    s.map.set_layer_visibility(MapLayerId::WorldBasemap, true);
    let regions = step.id == "offline-map";
    for layer in [
        MapLayerId::Vectors,
        MapLayerId::Nexrad,
        MapLayerId::TerrainWarning,
        MapLayerId::Metars,
        MapLayerId::Traffic,
        MapLayerId::OfflineRegions,
    ] {
        s.map.set_layer_visibility(
            layer,
            (layer == MapLayerId::OfflineRegions && regions)
                || (layer == MapLayerId::Vectors && !regions),
        );
    }
    if s.map.raster_catalog().is_none() {
        if let Some(store) = s.nav_data.store() {
            let catalog = crate::had_ops::raster_map_catalog_from_nav_kv(
                store,
                None,
                Some(if regions { "shaded-relief" } else { "sec" }),
            )?;
            s.map.replace_raster_catalog(Some(catalog));
        }
    }
    if let Some(catalog) = s.map.raster_catalog_mut() {
        crate::select_map_family_in_catalog(catalog, if regions { "shaded-relief" } else { "sec" });
    }
    if step.page == UiTourPage::Charts {
        prepare_plate(s, step.subject)?;
    }
    let row_uid = if step.surface == app_ui_contracts::tour::UiTourSurface::FlightPlanRow {
        let projected = crate::project_ui_state(&session_plan(s)?);
        Some(
            projected
                .display_rows
                .iter()
                .find(|row| row.label.split('*').next() == Some(step.subject))
                .ok_or_else(|| {
                    missing(format!(
                        "The tour waypoint {} is unavailable.",
                        step.subject
                    ))
                })?
                .uid
                .clone(),
        )
    } else {
        s.flight_plan
            .airway_picker()
            .view(s.nav_data.epoch())
            .map(|picker| picker.row_uid.clone())
    };
    let option_uid = if step.id == "aircraft-model" {
        Some(crate::had_ops::planner_aircraft_action_uid(
            &cardinal_selection(s)?,
        ))
    } else {
        None
    };
    if let Some(tour) = s.coordinator.guided_tour.as_mut() {
        tour.row_uid = row_uid;
        tour.option_uid = option_uid;
        let viewport = crate::MapViewport {
            center: LatLon {
                lat: tour.viewport.lat,
                lon: tour.viewport.lon,
            },
            zoom: tour.viewport.zoom,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        if tour.viewport.centered {
            s.situation.engage_map_follow(viewport);
        } else {
            s.situation.disengage_map_follow(viewport);
        }
        if matches!(step.demo, Demo::RouteDraft | Demo::RouteVia) {
            if let Some(route) = s
                .flight_plan
                .routing_editor()
                .view(s.nav_data.epoch())
                .and_then(|v| v.route)
            {
                if let Some(leg) = route.legs.iter().find(|leg| leg.highest_mea) {
                    tour.body
                        .push_str(&format!("\n\nHighest constraint: {}", leg.label));
                }
            }
        }
    }
    Ok(())
}

fn cardinal_selection(s: &UiSession) -> Result<product_contracts::AircraftSelection, HadReadError> {
    let definitions = crate::had_ops::system_aircraft_definitions(session_nav_kv_store(s)?)?;
    let (hash, model) = definitions
        .iter()
        .find(|(_, model)| {
            model.label.to_lowercase().contains("cardinal")
                || model.model.to_lowercase().contains("177")
        })
        .ok_or_else(|| {
            missing("The Cardinal airplane model is unavailable in this navigation data.")
        })?;
    Ok(product_contracts::AircraftSelection {
        definition_hash: hash.clone(),
        profile_id: model.default_profile_id.clone(),
    })
}

fn prepare_plan(s: &mut UiSession, demo: Demo) -> Result<(), SessionModelTransactionError> {
    use Demo::*;
    s.coordinator.altitude_planner_wind_selection =
        if matches!(demo, Estimated | Preview | PreviewForward | Spot) {
            AltitudePlannerWindSelection::Gfs
        } else {
            AltitudePlannerWindSelection::NoWind
        };
    if matches!(demo, Cardinal | Estimated)
        && uses_durable_live_feed_states(s)
        && s.weather.runtime().forecast_atmosphere.is_none()
    {
        s.coordinator.winds_aloft_acquisition_phase = WindsAloftAcquisitionPhase::Requested;
    }
    let input = match demo {
        Empty => "",
        Primary | Activated => "KRNT SEA OLM UBG KUAO",
        Waypoints => "KRNT BANDR ELN YKM S40",
        AirwayEndpoints | AirwayEntry | AirwayExit | AirwayAdded => "KRNT S40",
        _ => "KRNT KMWC",
    };
    let mut plan = if input.is_empty() {
        FlightPlan::empty()
    } else {
        crate::had_ops::append_flight_plan_entry(
            session_nav_kv_store(s)?,
            &FlightPlan::empty(),
            input,
        )?
    };
    s.flight_plan.set_airway_picker(Default::default());
    s.flight_plan.set_routing_editor(Default::default());
    if matches!(demo, AirwayEntry | AirwayExit | AirwayAdded) {
        plan = prepare_airway(s, plan, demo)?;
    }
    if matches!(
        demo,
        RouteDraft
            | RouteVia
            | RouteApplied
            | Cardinal
            | Estimated
            | Preview
            | PreviewForward
            | Spot
    ) {
        plan = prepare_route(s, plan, demo)?;
    }
    if matches!(demo, Cardinal | Estimated | Preview | PreviewForward | Spot) {
        plan.aircraft = Some(cardinal_selection(s)?);
        plan.cruise_altitude_ft = Some(10_000);
    }
    if matches!(demo, Activated | Preview | PreviewForward | Spot) {
        plan = crate::activate_leg(&plan, 0)?;
    }
    if demo == Spot {
        plan = crate::insert_waypoint(&plan, 0, false, NavRef::Spot(guided_tour::TOUR_SPOT))?;
    }
    let picker = s.flight_plan.airway_picker().clone();
    let router = s.flight_plan.routing_editor().clone();
    replace_session_flight_plan(s, plan)?;
    s.flight_plan.set_airway_picker(picker);
    s.flight_plan.set_routing_editor(router);
    sync_guidance_geometry_for_session(s, &crate::CoreDebugTimer::start())?;
    if !matches!(demo, Empty) {
        sync_plan_preview_to_active_leg(s)?;
        s.situation
            .select_source(crate::OwnshipSelectionCommand::Source {
                source_id: crate::OwnshipSourceId(DIRECT_SITUATION_SOURCE_ID.into()),
            });
        if demo == PreviewForward {
            for _ in 0..2 {
                apply_plan_preview_input(s, SituationControlInput::FastForward)?;
            }
        }
    }
    Ok(())
}

fn first_row(plan: &FlightPlan) -> Result<String, HadReadError> {
    crate::project_ui_state(plan)
        .display_rows
        .first()
        .map(|r| r.uid.clone())
        .ok_or_else(|| missing("The demonstration plan is empty."))
}

fn prepare_route(
    s: &mut UiSession,
    plan: FlightPlan,
    demo: Demo,
) -> Result<FlightPlan, SessionModelTransactionError> {
    let graph = s.nav_data.load_airway_graph()?;
    let store = session_nav_kv_store(s)?;
    let epoch = s.nav_data.epoch();
    let mut editor = crate::routing_editor::RoutingEditor::default().open(
        store,
        &plan,
        &first_row(&plan)?,
        epoch,
        AirwayNavigationMode::Gnss,
        graph,
    )?;
    if demo != Demo::RouteDraft {
        let ocs = nav_ref_position(store, &NavRef::Navaid("OCS".into()), None)?;
        let view = editor
            .view(epoch)
            .ok_or_else(|| missing("The route editor did not open."))?;
        editor = editor.drag(
            epoch,
            &view.edit_id,
            app_ui_contracts::session::UiAirwayRouteDragPhase::Commit,
            ocs,
            1.0,
            0,
            None,
        )?;
        if editor.view(epoch).is_none_or(|v| v.via_points.is_empty()) {
            return Err(missing("No published airway route through OCS is available.").into());
        }
    }
    if matches!(demo, Demo::RouteDraft | Demo::RouteVia) {
        s.flight_plan.set_routing_editor(editor);
        return Ok(plan);
    }
    let view = editor.view(epoch).unwrap();
    let apply = view
        .controls
        .iter()
        .find(|c| c.symbol_id == "apply_route")
        .ok_or_else(|| missing("The demonstration route cannot be applied."))?;
    match editor.advance(store, &plan, &apply.button.action_id, epoch)? {
        crate::routing_editor::Transition::Apply(plan) => Ok(plan),
        _ => Err(missing("The route was not applied.").into()),
    }
}

fn prepare_airway(
    s: &mut UiSession,
    plan: FlightPlan,
    demo: Demo,
) -> Result<FlightPlan, SessionModelTransactionError> {
    let store = session_nav_kv_store(s)?;
    let epoch = s.nav_data.epoch();
    let mut picker = crate::airway_picker::AirwayPickerController::default().open(
        store,
        first_row(&plan)?,
        NavRef::Airport("KRNT".into()),
        Some(NavRef::Airport("S40".into())),
        epoch,
    )?;
    // Resolve the actual core-issued choices; never synthesize action tokens.
    for label in ["V4", "SEA", "BRUKK"] {
        let view = picker
            .view(epoch)
            .ok_or_else(|| missing("The airway picker is unavailable."))?;
        if (demo == Demo::AirwayEntry && label == "SEA")
            || (demo == Demo::AirwayExit && label == "BRUKK")
        {
            s.flight_plan.set_airway_picker(picker);
            return Ok(plan);
        }
        let choice = view
            .sections
            .iter()
            .flat_map(|section| &section.buttons)
            .find(|button| button.label == label || button.label.starts_with(&format!("{label} ")))
            .ok_or_else(|| {
                missing(format!(
                    "The demonstration airway choice {label} is unavailable."
                ))
            })?;
        match picker.advance(store, &choice.action_id, epoch)? {
            crate::airway_picker::Transition::Picker(next) => picker = next,
            crate::airway_picker::Transition::Insert { selection, .. } => {
                let materialized = materialize_airway_presentation_selection(store, 0, selection)?;
                return Ok(crate::insert_airway_materialized(
                    &plan,
                    0,
                    Some(1),
                    materialized.airway,
                    materialized.resolved_legs,
                )?);
            }
        }
    }
    Err(missing("The demonstration airway was not inserted.").into())
}

fn prepare_plate(s: &mut UiSession, subject: &str) -> Result<(), SessionModelTransactionError> {
    let (airport, desired) = subject.split_once(':').unwrap_or((subject, ""));
    let plan = session_plan(s)?;
    let derived = crate::had_ops::chart_page_state(
        session_nav_kv_store(s)?,
        crate::had_ops::ChartPageQuery {
            plan: &plan,
            stored_recent_airport_ids: &[],
            plate_target_airport_id: Some(airport),
            candidate_airport_id: Some(airport),
            selected_reference_family_id: None,
            candidate_chart_id: None,
            suggested_chart_ids: &[],
            notam_display_index: None,
        },
    )?;
    let charts = &derived
        .airports
        .iter()
        .find(|a| a.id == airport)
        .ok_or_else(|| missing(format!("Plates for {airport} are unavailable.")))?
        .charts;
    let chart = if desired.is_empty() {
        charts.first()
    } else {
        charts.iter().find(|chart| {
            let label = chart.label.to_uppercase();
            if desired == "diagram" {
                label.contains("AIRPORT DIAGRAM")
            } else {
                label.contains("RNAV") && label.contains("15L")
            }
        })
    }
    .ok_or_else(|| missing(format!("The requested {airport} plate is unavailable.")))?;
    s.coordinator.chart_page_state =
        derive_compact_chart_page_state_with_reference(CompactChartPageInput {
            plan: &plan,
            stored_recent_airport_ids: &[airport.into()],
            plate_target_airport_id: Some(airport),
            candidate_airport_id: Some(airport),
            selected_reference_family_id: None,
            candidate_chart_id: Some(&chart.id),
            suggested_chart_ids: &[],
        });
    Ok(())
}
