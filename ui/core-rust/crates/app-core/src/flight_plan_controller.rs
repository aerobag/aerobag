// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    sync::Arc,
};

use serde::{Deserialize, Serialize};

use crate::{
    flight_plan_materialization::MaterializedFlightPlan,
    had_ops::{FlightPlanLiveData, HadReadError},
    AppError, AppErrorKind, AppResult, FlightDataComputer, FlightPlan, FlightPlanControlId,
    FlightPlanControlUiView, FlightPlanDisplayRowKind, FlightPlanRouteProjection,
    FlightPlanRouteSegment, FlightPlanRowActionExecution, FlightPlanRowActionId, FlightPlanUiState,
    GuidanceState, LatLon, LegDisplayElement, NavKvStore, NavRef, ProcedureDiscontinuity,
    RouteComponentViewKind, SequencingMode,
};

const MAX_FLIGHT_PLAN_UNDO_DEPTH: usize = 1_024;
const MAX_FLIGHT_PLAN_HISTORY_BYTES: usize = 16 * 1_024 * 1_024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuidanceLegGeometry {
    pub leg_id: String,
    pub from: LatLon,
    pub to: LatLon,
    #[serde(default)]
    pub path: Vec<LatLon>,
}

#[derive(Debug, Clone, PartialEq)]
struct FlightPlanDefinition {
    plan: Arc<FlightPlan>,
    serialized_bytes: usize,
}

impl FlightPlanDefinition {
    fn from_plan(plan: &FlightPlan) -> AppResult<Self> {
        let mut definition = plan.clone().normalized();
        definition.guidance = None;
        let serialized_bytes = serde_json::to_vec(&definition)
            .map_err(|error| AppError {
                kind: AppErrorKind::Internal,
                message: format!("could not size flight-plan history snapshot: {error}"),
            })?
            .len();
        Ok(Self {
            plan: Arc::new(definition),
            serialized_bytes,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
struct FlightPlanDefinitionController {
    snapshots: VecDeque<FlightPlanDefinition>,
    cursor: usize,
    serialized_bytes: usize,
}

impl FlightPlanDefinitionController {
    fn new(definition: FlightPlanDefinition) -> Self {
        let serialized_bytes = definition.serialized_bytes;
        Self {
            snapshots: VecDeque::from([definition]),
            cursor: 0,
            serialized_bytes,
        }
    }

    fn current(&self) -> Option<&FlightPlan> {
        self.snapshots
            .get(self.cursor)
            .map(|entry| entry.plan.as_ref())
    }

    fn can_undo(&self) -> bool {
        self.cursor > 0
    }

    fn can_redo(&self) -> bool {
        self.cursor + 1 < self.snapshots.len()
    }

    fn undo_target(&self) -> Option<&FlightPlan> {
        self.cursor
            .checked_sub(1)
            .and_then(|cursor| self.snapshots.get(cursor))
            .map(|entry| entry.plan.as_ref())
    }

    fn redo_target(&self) -> Option<&FlightPlan> {
        self.snapshots
            .get(self.cursor + 1)
            .map(|entry| entry.plan.as_ref())
    }

    fn finish_undo(&mut self) {
        debug_assert!(self.can_undo());
        self.cursor -= 1;
    }

    fn finish_redo(&mut self) {
        debug_assert!(self.can_redo());
        self.cursor += 1;
    }

    fn reset(&mut self, definition: FlightPlanDefinition) {
        *self = Self::new(definition);
    }

    fn record(&mut self, definition: FlightPlanDefinition) -> bool {
        if self.current() == Some(definition.plan.as_ref()) {
            return false;
        }
        while self.snapshots.len() > self.cursor + 1 {
            if let Some(removed) = self.snapshots.pop_back() {
                self.serialized_bytes = self
                    .serialized_bytes
                    .saturating_sub(removed.serialized_bytes);
            }
        }
        self.serialized_bytes = self
            .serialized_bytes
            .saturating_add(definition.serialized_bytes);
        self.snapshots.push_back(definition);
        self.cursor = self.snapshots.len() - 1;
        while self.snapshots.len() > MAX_FLIGHT_PLAN_UNDO_DEPTH + 1
            || (self.serialized_bytes > MAX_FLIGHT_PLAN_HISTORY_BYTES && self.snapshots.len() > 1)
        {
            let removed = self
                .snapshots
                .pop_front()
                .expect("flight-plan history is nonempty");
            self.serialized_bytes = self
                .serialized_bytes
                .saturating_sub(removed.serialized_bytes);
            self.cursor = self.cursor.saturating_sub(1);
        }
        true
    }
}

#[derive(Debug, Clone, PartialEq)]
struct FlightPlanNavigationController {
    guidance: Option<GuidanceState>,
    guidance_leg_geometry: Arc<HashMap<String, GuidanceLegGeometry>>,
}

impl Default for FlightPlanNavigationController {
    fn default() -> Self {
        Self {
            guidance: None,
            guidance_leg_geometry: Arc::new(HashMap::new()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
struct FlightPlanModel {
    definition: FlightPlanDefinitionController,
    navigation: FlightPlanNavigationController,
    // Read-only composite cache for planners and projections that consume both domains.
    active_plan: Option<FlightPlan>,
    route_revision: u64,
    revision: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FlightPlanProjectionInputs {
    pub ownship_position: Option<LatLon>,
    pub ownship_speed_kt: Option<f64>,
    pub ownship_altitude_ft: Option<f64>,
    pub now_epoch_ms: i64,
    pub nav_data_generation: u64,
    pub weather_revision: u64,
    pub aircraft_definitions_digest: [u8; 32],
    pub local_time_zone: chrono_tz::Tz,
    pub time_display_mode: crate::TimeDisplayMode,
}

#[derive(Debug, Clone)]
pub(crate) struct FlightPlanProjection {
    pub ui_state: Option<FlightPlanUiState>,
    pub materialized: Option<MaterializedFlightPlan>,
}

pub(crate) struct FlightPlanProjectionResult {
    pub projection: FlightPlanProjection,
    pub rebuilt: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct FlightPlanProjectionKey {
    revision: u64,
    inputs: FlightPlanProjectionInputs,
    had_backed: bool,
}

#[derive(Clone)]
struct FlightPlanProjectionCache {
    key: FlightPlanProjectionKey,
    projection: FlightPlanProjection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FlightPlanRouteProjectionKey {
    revision: u64,
    nav_data_generation: u64,
}

#[derive(Clone)]
struct FlightPlanRouteProjectionCache {
    key: FlightPlanRouteProjectionKey,
    projection: FlightPlanRouteProjection,
}

#[derive(Clone)]
pub(crate) struct FlightPlanModelCheckpoint {
    model: FlightPlanModel,
}

#[derive(Default)]
pub(crate) struct FlightPlanController {
    model: FlightPlanModel,
    projection_cache: Option<FlightPlanProjectionCache>,
    route_projection_cache: Option<FlightPlanRouteProjectionCache>,
}

impl FlightPlanController {
    pub fn new(
        plan: FlightPlan,
        guidance_leg_geometry: Vec<GuidanceLegGeometry>,
    ) -> AppResult<Self> {
        let active_plan = crate::build_flight_plan(plan)?;
        let definition = FlightPlanDefinition::from_plan(&active_plan)?;
        let guidance = active_plan.guidance.clone();
        Ok(Self {
            model: FlightPlanModel {
                definition: FlightPlanDefinitionController::new(definition),
                navigation: FlightPlanNavigationController {
                    guidance,
                    guidance_leg_geometry: Arc::new(geometry_map(guidance_leg_geometry)),
                },
                active_plan: Some(active_plan),
                route_revision: 0,
                revision: 0,
            },
            projection_cache: None,
            route_projection_cache: None,
        })
    }

    pub fn revision(&self) -> u64 {
        self.model.revision
    }

    pub fn route_revision(&self) -> u64 {
        self.model.route_revision
    }

    pub fn active_plan(&self) -> Option<&FlightPlan> {
        self.model.active_plan.as_ref()
    }

    pub fn guidance_leg_geometry(&self) -> &HashMap<String, GuidanceLegGeometry> {
        &self.model.navigation.guidance_leg_geometry
    }

    pub fn checkpoint_model(&self) -> FlightPlanModelCheckpoint {
        FlightPlanModelCheckpoint {
            model: self.model.clone(),
        }
    }

    pub fn rollback_model(&mut self, checkpoint: FlightPlanModelCheckpoint) {
        self.model = checkpoint.model;
        self.clear_projection_caches();
    }

    /// Accepts a user-authored definition edit and records it in undo history.
    pub fn apply_definition_edit(&mut self, plan: FlightPlan) -> AppResult<&FlightPlan> {
        let normalized = crate::build_flight_plan(plan)?;
        let definition = FlightPlanDefinition::from_plan(&normalized)?;
        self.model.definition.record(definition);
        self.install_normalized_plan(normalized)?;
        self.model.route_revision = self.model.route_revision.saturating_add(1);
        self.note_change();
        Ok(self.model.active_plan.as_ref().expect("edited active plan"))
    }

    /// Accepts an operational navigation update. The definition is checked but
    /// never replaced, so guidance code cannot enter edit history by accident.
    pub fn apply_navigation_update(&mut self, plan: FlightPlan) -> AppResult<&FlightPlan> {
        let normalized = crate::build_flight_plan(plan)?;
        let incoming_definition = FlightPlanDefinition::from_plan(&normalized)?;
        if self.model.definition.current() != Some(incoming_definition.plan.as_ref()) {
            return Err(AppError {
                kind: AppErrorKind::Internal,
                message: "navigation update attempted to mutate the flight-plan definition"
                    .to_string(),
            });
        }
        let mut combined = self
            .model
            .definition
            .current()
            .cloned()
            .ok_or_else(|| AppError {
                kind: AppErrorKind::Internal,
                message: "navigation update has no flight-plan definition".to_string(),
            })?;
        combined.guidance = normalized.guidance;
        let combined = crate::build_flight_plan(combined)?;
        self.model.navigation.guidance = combined.guidance.clone();
        self.model.active_plan = Some(combined);
        self.model.route_revision = self.model.route_revision.saturating_add(1);
        self.note_change();
        Ok(self
            .model
            .active_plan
            .as_ref()
            .expect("navigation-updated active plan"))
    }

    /// Replaces the plan from persistence or sync and establishes a new local
    /// history baseline.
    pub fn replace_plan(&mut self, plan: FlightPlan) -> AppResult<&FlightPlan> {
        let normalized = crate::build_flight_plan(plan)?;
        self.model
            .definition
            .reset(FlightPlanDefinition::from_plan(&normalized)?);
        self.install_normalized_plan(normalized)?;
        self.model.route_revision = self.model.route_revision.saturating_add(1);
        self.note_change();
        Ok(self
            .model
            .active_plan
            .as_ref()
            .expect("replaced active plan"))
    }

    pub fn can_undo(&self) -> bool {
        self.model.definition.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.model.definition.can_redo()
    }

    pub fn undo_definition_edit(&mut self) -> AppResult<&FlightPlan> {
        let historical_definition =
            self.model
                .definition
                .undo_target()
                .cloned()
                .ok_or_else(|| AppError {
                    kind: AppErrorKind::UnsupportedOperation,
                    message: "No flight-plan edit is available to undo.".to_string(),
                })?;
        let current = self.required_plan("undo flight-plan edit")?.clone();
        let restored =
            crate::planning::restore_flight_plan_definition(&current, &historical_definition)?;
        let restored = crate::build_flight_plan(restored)?;
        self.model.definition.finish_undo();
        self.install_normalized_plan(restored)?;
        self.model.route_revision = self.model.route_revision.saturating_add(1);
        self.note_change();
        Ok(self.model.active_plan.as_ref().expect("undo active plan"))
    }

    pub fn redo_definition_edit(&mut self) -> AppResult<&FlightPlan> {
        let historical_definition =
            self.model
                .definition
                .redo_target()
                .cloned()
                .ok_or_else(|| AppError {
                    kind: AppErrorKind::UnsupportedOperation,
                    message: "No flight-plan edit is available to redo.".to_string(),
                })?;
        let current = self.required_plan("redo flight-plan edit")?.clone();
        let restored =
            crate::planning::restore_flight_plan_definition(&current, &historical_definition)?;
        let restored = crate::build_flight_plan(restored)?;
        self.model.definition.finish_redo();
        self.install_normalized_plan(restored)?;
        self.model.route_revision = self.model.route_revision.saturating_add(1);
        self.note_change();
        Ok(self.model.active_plan.as_ref().expect("redo active plan"))
    }

    pub fn install_guidance_leg_geometry(&mut self, geometries: Vec<GuidanceLegGeometry>) {
        let geometries = Arc::new(geometry_map(geometries));
        if self.model.navigation.guidance_leg_geometry == geometries {
            return;
        }
        self.model.navigation.guidance_leg_geometry = geometries;
        self.note_change();
    }

    pub fn clear_guidance_leg_geometry(&mut self) {
        if !self.model.navigation.guidance_leg_geometry.is_empty() {
            self.model.navigation.guidance_leg_geometry = Arc::new(HashMap::new());
            self.note_change();
        }
    }

    pub fn invalidate_nav_data(&mut self) {
        self.clear_projection_caches();
    }

    pub fn plan_after_activate_next_leg(&self) -> AppResult<FlightPlan> {
        crate::activate_next_leg(self.required_plan("activate next leg")?)
    }

    pub fn plan_after_stop_navigation(&self) -> AppResult<FlightPlan> {
        crate::stop_navigation(self.required_plan("stop navigation")?)
    }

    pub fn plan_after_suspend_sequencing(&self) -> AppResult<FlightPlan> {
        crate::suspend_sequencing(self.required_plan("suspend sequencing")?)
    }

    pub fn plan_after_unsuspend_sequencing(&self) -> AppResult<FlightPlan> {
        crate::unsuspend_sequencing(self.required_plan("unsuspend sequencing")?)
    }

    pub fn plan_after_manual_sequence(&self) -> AppResult<FlightPlan> {
        crate::sequence_active_leg(self.required_plan("sequence active leg")?)
    }

    pub fn plan_after_set_cruise_altitude(&self, altitude_ft: i32) -> AppResult<FlightPlan> {
        Ok(FlightPlan {
            cruise_altitude_ft: Some(altitude_ft),
            ..self.required_plan("set cruise altitude")?.clone()
        })
    }

    pub fn plan_after_set_aircraft(
        &self,
        selection: product_contracts::AircraftSelection,
        cruise_altitude_ft: i32,
    ) -> AppResult<FlightPlan> {
        Ok(FlightPlan {
            aircraft: Some(selection),
            cruise_altitude_ft: Some(cruise_altitude_ft),
            ..self.required_plan("set aircraft")?.clone()
        })
    }

    pub fn plan_after_row_action(
        &self,
        row_uid: &str,
        action_uid: &str,
        ownship_position: Option<LatLon>,
    ) -> AppResult<(FlightPlan, FlightPlanMutationDomain)> {
        let plan = self.required_plan("perform row action")?;
        let mut ui = crate::project_ui_state(plan);
        crate::planning::apply_flight_plan_live_action_availability(
            &mut ui,
            ownship_position.is_some(),
        );
        let row = ui
            .display_rows
            .iter()
            .find(|row| row.uid == row_uid)
            .ok_or_else(|| AppError {
                kind: AppErrorKind::InvalidFlightPlan,
                message: format!("flight-plan row action target is stale: {row_uid}"),
            })?;
        let action = crate::planning::flight_plan_row_actions(row)
            .find(|action| action.uid == action_uid)
            .ok_or_else(|| AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: format!("flight-plan row action is unavailable: {action_uid}"),
            })?;
        if !action.enabled {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: action
                    .disabled_reason
                    .clone()
                    .unwrap_or_else(|| format!("flight-plan row action is disabled: {action_uid}")),
            });
        }
        if action.execution != FlightPlanRowActionExecution::CoreSession {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: format!("flight-plan row action is UI-controller owned: {action_uid}"),
            });
        }
        let row_component_index = || -> AppResult<usize> {
            let component_uid = row.component_uid.as_deref().ok_or_else(|| AppError {
                kind: AppErrorKind::InvalidFlightPlan,
                message: "flight-plan row has no route component uid".to_string(),
            })?;
            plan.route_component_uids
                .iter()
                .position(|uid| uid == component_uid)
                .ok_or_else(|| AppError {
                    kind: AppErrorKind::InvalidFlightPlan,
                    message: format!("flight-plan row component is stale: {component_uid}"),
                })
        };

        let domain = match &action.id {
            FlightPlanRowActionId::ActivateLeg | FlightPlanRowActionId::DirectTo => {
                FlightPlanMutationDomain::Navigation
            }
            _ => FlightPlanMutationDomain::Definition,
        };
        let plan = match &action.id {
            FlightPlanRowActionId::ActivateLeg => {
                let leg_index = row.leg_index.ok_or_else(|| AppError {
                    kind: AppErrorKind::InvalidFlightPlan,
                    message: "activate-leg row has no leg index".to_string(),
                })?;
                if row.row_kind == FlightPlanDisplayRowKind::Discontinuity
                    && row.label == ProcedureDiscontinuity::Hold.display_label()
                {
                    match crate::terminal_hold_start_detail_index_for_leg(plan, leg_index) {
                        Some(detail_index) => {
                            crate::activate_leg_at_detail_index(plan, leg_index, detail_index)
                        }
                        None => crate::activate_leg(plan, leg_index),
                    }
                } else {
                    crate::activate_leg(plan, leg_index)
                }
            }
            FlightPlanRowActionId::DirectTo => crate::activate_direct_to_row(
                plan,
                ownship_position.ok_or_else(|| AppError {
                    kind: AppErrorKind::UnsupportedOperation,
                    message: "cannot activate direct-to without ownship position".to_string(),
                })?,
                &crate::FlightPlanRowId(row.uid.clone()),
            ),
            FlightPlanRowActionId::Remove | FlightPlanRowActionId::RemoveProcedure => {
                if row.component_kind == Some(RouteComponentViewKind::Airway) && row.depth > 0 {
                    let nav_ref = row.nav_ref.as_ref().ok_or_else(|| AppError {
                        kind: AppErrorKind::InvalidFlightPlan,
                        message: "airway child remove row has no nav reference".to_string(),
                    })?;
                    crate::remove_airway_child_waypoint(plan, row_component_index()?, nav_ref)
                } else {
                    crate::delete_component(plan, row_component_index()?)
                }
            }
            FlightPlanRowActionId::RemoveAllAbove => {
                crate::remove_all_above(plan, row_component_index()?)
            }
            FlightPlanRowActionId::MoveUp => {
                crate::move_component(plan, row_component_index()?, -1)
            }
            FlightPlanRowActionId::MoveDown => {
                crate::move_component(plan, row_component_index()?, 1)
            }
            _ => Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: format!("unsupported core flight-plan row action: {action_uid}"),
            }),
        }?;
        Ok((plan, domain))
    }

    pub fn plan_after_direct_to(
        &self,
        from_position: LatLon,
        target: NavRef,
    ) -> AppResult<FlightPlan> {
        crate::activate_direct_to(
            self.required_plan("activate direct-to")?,
            from_position,
            target,
        )
    }

    pub fn plan_after_restore_direct_to(&self) -> AppResult<FlightPlan> {
        crate::restore_direct_to(self.required_plan("restore direct-to")?)
    }

    pub fn active_guidance_detail_geometry(&self) -> Option<(String, GuidanceLegGeometry)> {
        let plan = self.model.active_plan.as_ref()?;
        let guidance = plan.guidance.as_ref()?;
        if guidance.sequencing_mode == SequencingMode::DirectTo {
            let geometry = active_guidance_projection_geometry(
                plan,
                &self.model.navigation.guidance_leg_geometry,
            )?;
            return Some((geometry.leg_id.clone(), geometry));
        }
        let active_detail_index = active_guidance_detail_index_for_motion(plan, guidance)?;
        active_guidance_detail_geometry_for_index(
            plan,
            active_detail_index,
            &self.model.navigation.guidance_leg_geometry,
        )
    }

    pub fn sequence_by_ownship_motion(
        &mut self,
        previous_position: LatLon,
        current_position: LatLon,
    ) -> AppResult<bool> {
        let mut sequenced = false;
        for _ in 0..16 {
            let Some(plan) = self.model.active_plan.as_ref() else {
                return Ok(sequenced);
            };
            let Some(guidance) = plan.guidance.as_ref() else {
                return Ok(sequenced);
            };
            let (finish_criterion, suspended_hold) = if guidance.sequencing_mode
                == SequencingMode::DirectTo
            {
                let Some(finish_criterion) =
                    direct_to_finish_criterion(plan, &self.model.navigation.guidance_leg_geometry)
                else {
                    return Ok(sequenced);
                };
                (finish_criterion, false)
            } else {
                let Some(active_detail_index) =
                    active_guidance_detail_index_for_motion(plan, guidance)
                else {
                    return Ok(sequenced);
                };
                let suspended_hold = guidance.sequencing_mode == SequencingMode::Suspended;
                let Some(finish_criterion) = active_detail_finish_criterion(
                    plan,
                    active_detail_index,
                    &self.model.navigation.guidance_leg_geometry,
                    suspended_hold,
                ) else {
                    return Ok(sequenced);
                };
                (finish_criterion, suspended_hold)
            };
            if !finish_criterion.crossed_by(previous_position, current_position) {
                return Ok(sequenced);
            }
            let next_plan = if suspended_hold {
                sequence_suspended_terminal_hold_detail(plan)?
            } else {
                crate::sequence_active_detail(plan)?
            };
            self.apply_navigation_update(next_plan)?;
            sequenced = true;
        }
        Ok(sequenced)
    }

    pub fn project(
        &mut self,
        store: Option<&NavKvStore>,
        private_aircraft_definitions: &BTreeMap<String, product_contracts::AircraftDefinition>,
        inputs: FlightPlanProjectionInputs,
        atmosphere: crate::had_ops::PlannerAtmosphereSelection<'_>,
    ) -> Result<FlightPlanProjectionResult, HadReadError> {
        let key = FlightPlanProjectionKey {
            revision: self.model.revision,
            inputs,
            had_backed: store.is_some(),
        };
        if let Some(cache) = self.projection_cache.as_ref() {
            if cache.key == key {
                return Ok(FlightPlanProjectionResult {
                    projection: cache.projection.clone(),
                    rebuilt: false,
                });
            }
        }

        let mut projection = match self.model.active_plan.as_ref() {
            None => FlightPlanProjection {
                ui_state: None,
                materialized: None,
            },
            Some(plan) => {
                let ui_state = crate::project_ui_state(plan);
                if let Some(store) = store {
                    let projection = crate::had_ops::flight_plan_ui_projection(
                        store,
                        private_aircraft_definitions,
                        plan.clone(),
                        ui_state,
                        FlightDataComputer::with_fuel_flow_clock_and_time_display(
                            inputs.ownship_speed_kt,
                            None,
                            Some(inputs.now_epoch_ms),
                            inputs.time_display_mode,
                            inputs.local_time_zone,
                        ),
                        FlightPlanLiveData {
                            ownship_position: inputs.ownship_position,
                            ownship_altitude_ft: inputs.ownship_altitude_ft,
                            now_epoch_ms: Some(inputs.now_epoch_ms),
                            local_time_zone: inputs.local_time_zone,
                            time_display_mode: inputs.time_display_mode,
                        },
                        atmosphere,
                    )?;
                    FlightPlanProjection {
                        ui_state: Some(projection.ui_state),
                        materialized: Some(projection.materialized),
                    }
                } else {
                    let materialized = MaterializedFlightPlan::build(
                        plan,
                        &self.model.navigation.guidance_leg_geometry,
                        inputs.ownship_position,
                    )
                    .map_err(|error| HadReadError::Fatal(error.message))?;
                    FlightPlanProjection {
                        ui_state: Some(ui_state),
                        materialized: Some(materialized),
                    }
                }
            }
        };
        if let Some(ui_state) = projection.ui_state.as_mut() {
            crate::planning::apply_flight_plan_live_action_availability(
                ui_state,
                inputs.ownship_position.is_some(),
            );
            ui_state.controls.splice(
                0..0,
                [
                    history_control(
                        FlightPlanControlId::Undo,
                        "UNDO",
                        self.can_undo(),
                        "No flight-plan edit is available to undo.",
                    ),
                    history_control(
                        FlightPlanControlId::Redo,
                        "REDO",
                        self.can_redo(),
                        "No flight-plan edit is available to redo.",
                    ),
                ],
            );
        }
        self.projection_cache = Some(FlightPlanProjectionCache {
            key,
            projection: projection.clone(),
        });
        Ok(FlightPlanProjectionResult {
            projection,
            rebuilt: true,
        })
    }

    pub fn project_route(
        &mut self,
        store: &NavKvStore,
        nav_data_generation: u64,
    ) -> Result<(FlightPlanRouteProjection, bool), HadReadError> {
        let key = FlightPlanRouteProjectionKey {
            revision: self.model.revision,
            nav_data_generation,
        };
        if let Some(cache) = self.route_projection_cache.as_ref() {
            if cache.key == key {
                return Ok((cache.projection.clone(), false));
            }
        }
        let (segments, distance_annotations) = match self.model.active_plan.as_ref() {
            Some(plan) => {
                let segments = crate::had_ops::project_flight_plan_route(store, plan)?;
                let distance_annotations =
                    crate::project_flight_plan_route_distance_annotations(plan, &segments)?;
                (segments, distance_annotations)
            }
            None => (Vec::new(), Vec::new()),
        };
        let projection = FlightPlanRouteProjection {
            flight_plan_route_revision: self.model.route_revision,
            segments,
            distance_annotations,
        };
        self.route_projection_cache = Some(FlightPlanRouteProjectionCache {
            key,
            projection: projection.clone(),
        });
        Ok((projection, true))
    }

    fn note_change(&mut self) {
        self.model.revision = self.model.revision.saturating_add(1);
        self.clear_projection_caches();
    }

    fn clear_projection_caches(&mut self) {
        self.projection_cache = None;
        self.route_projection_cache = None;
    }

    fn required_plan(&self, context: &str) -> AppResult<&FlightPlan> {
        self.model.active_plan.as_ref().ok_or_else(|| AppError {
            kind: AppErrorKind::Internal,
            message: format!("cannot {context}: no active flight plan"),
        })
    }

    fn install_normalized_plan(&mut self, plan: FlightPlan) -> AppResult<()> {
        let geometry = self_contained_guidance_leg_geometry_for_plan(&plan)?.unwrap_or_default();
        self.model.navigation.guidance = plan.guidance.clone();
        self.model.navigation.guidance_leg_geometry = Arc::new(geometry_map(geometry));
        self.model.active_plan = Some(plan);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FlightPlanMutationDomain {
    Definition,
    Navigation,
}

fn history_control(
    id: FlightPlanControlId,
    label: &str,
    enabled: bool,
    disabled_reason: &str,
) -> FlightPlanControlUiView {
    FlightPlanControlUiView {
        id,
        label: label.to_string(),
        enabled,
        disabled_reason: (!enabled).then(|| disabled_reason.to_string()),
    }
}

fn geometry_map(geometries: Vec<GuidanceLegGeometry>) -> HashMap<String, GuidanceLegGeometry> {
    geometries
        .into_iter()
        .map(|geometry| (geometry.leg_id.clone(), geometry))
        .collect()
}

pub(crate) fn guidance_leg_geometry_from_route(
    route: Vec<FlightPlanRouteSegment>,
) -> Vec<GuidanceLegGeometry> {
    route
        .into_iter()
        .map(|segment| GuidanceLegGeometry {
            leg_id: segment.id,
            from: segment.from,
            to: segment.to,
            path: segment.path,
        })
        .collect()
}

pub(crate) fn self_contained_guidance_leg_geometry_for_plan(
    plan: &FlightPlan,
) -> AppResult<Option<Vec<GuidanceLegGeometry>>> {
    let mut resolve_position = |nav_ref: &NavRef, _procedure_airport_id: Option<&str>| match nav_ref
    {
        NavRef::LatLon(position) | NavRef::Spot(position) => Ok(*position),
        _ => Err(()),
    };
    if let Ok(route) = crate::project_flight_plan_route_with_resolver(plan, &mut resolve_position) {
        return Ok(Some(guidance_leg_geometry_from_route(route)));
    }

    let mut geometries = Vec::new();
    for (leg_index, leg) in plan.resolved_legs.iter().enumerate() {
        if let Ok(route) = crate::project_flight_plan_leg_route_with_resolver(
            plan,
            leg_index,
            leg,
            &mut resolve_position,
        ) {
            geometries.extend(guidance_leg_geometry_from_route(route));
        }
    }
    Ok((!geometries.is_empty()).then_some(geometries))
}

fn active_guidance_projection_geometry(
    plan: &FlightPlan,
    geometry_by_leg_id: &HashMap<String, GuidanceLegGeometry>,
) -> Option<GuidanceLegGeometry> {
    let guidance = plan.guidance.as_ref()?;
    crate::flight_plan_materialization::active_geometry_for_guidance(
        plan,
        guidance,
        geometry_by_leg_id,
    )
}

fn active_guidance_detail_index_for_motion(
    plan: &FlightPlan,
    guidance: &GuidanceState,
) -> Option<usize> {
    match guidance.sequencing_mode {
        SequencingMode::FollowPlan => guidance.active_detail_index.or_else(|| {
            crate::planning::first_guidance_detail_index_for_leg(plan, guidance.active_leg_index)
        }),
        SequencingMode::Suspended => {
            let active_detail_index = guidance.active_detail_index?;
            terminal_hold_detail_range(plan, guidance.active_leg_index)
                .filter(|(hold_start, hold_end)| {
                    active_detail_index >= *hold_start && active_detail_index <= *hold_end
                })
                .map(|_| active_detail_index)
        }
        SequencingMode::DirectTo => None,
    }
}

fn active_guidance_detail_geometry_for_index(
    plan: &FlightPlan,
    active_detail_index: usize,
    geometry_by_leg_id: &HashMap<String, GuidanceLegGeometry>,
) -> Option<(String, GuidanceLegGeometry)> {
    let mut current_index = 0usize;
    for (leg_index, leg) in plan.resolved_legs.iter().enumerate() {
        let detail_count = crate::guidance_detail_count_for_leg(leg);
        if active_detail_index < current_index + detail_count {
            let element_index = active_detail_index - current_index;
            let detail_id =
                crate::guidance_detail_id_for_leg_element(leg_index, leg, element_index);
            let geometry = geometry_by_leg_id.get(&detail_id).cloned()?;
            return Some((detail_id, geometry));
        }
        current_index += detail_count;
    }
    None
}

fn direct_to_finish_criterion(
    plan: &FlightPlan,
    geometry_by_leg_id: &HashMap<String, GuidanceLegGeometry>,
) -> Option<crate::sequencing::SequencingFinishCriterion> {
    let guidance = plan.guidance.as_ref()?;
    if guidance.sequencing_mode != SequencingMode::DirectTo {
        return None;
    }
    let current = active_guidance_projection_geometry(plan, geometry_by_leg_id)?;
    let current_course = terminal_course_for_guidance_geometry(&current)?;
    let next_course = guidance
        .direct_to
        .as_ref()
        .and_then(|direct_to| crate::planning::direct_to_resume_leg_index(plan, direct_to))
        .and_then(|resume_leg_index| {
            crate::planning::first_guidance_detail_index_for_leg(plan, resume_leg_index)
        })
        .and_then(|detail_index| {
            active_guidance_detail_geometry_for_index(plan, detail_index, geometry_by_leg_id)
        })
        .and_then(|(_, geometry)| initial_course_for_guidance_geometry(&geometry))
        .unwrap_or(current_course);
    Some(crate::sequencing::plane_finish_criterion(
        current.to,
        current_course,
        next_course,
    ))
}

fn active_detail_finish_criterion(
    plan: &FlightPlan,
    active_detail_index: usize,
    geometry_by_leg_id: &HashMap<String, GuidanceLegGeometry>,
    wrap_terminal_hold: bool,
) -> Option<crate::sequencing::SequencingFinishCriterion> {
    if crate::planning::guidance_detail_is_manual_sequence(plan, active_detail_index) {
        return None;
    }
    let (_, current) =
        active_guidance_detail_geometry_for_index(plan, active_detail_index, geometry_by_leg_id)?;
    if let Some(arc_criterion) = active_detail_arc_finish_criterion(plan, active_detail_index) {
        return Some(arc_criterion);
    }
    let current_course = terminal_course_for_guidance_geometry(&current)?;
    let next_detail_index = if wrap_terminal_hold {
        next_terminal_hold_detail_index(plan, active_detail_index)
    } else {
        active_detail_index.checked_add(1)
    };
    let next_course = next_detail_index
        .and_then(|detail_index| {
            active_guidance_detail_geometry_for_index(plan, detail_index, geometry_by_leg_id)
        })
        .and_then(|(_, geometry)| initial_course_for_guidance_geometry(&geometry))
        .unwrap_or(current_course);
    Some(crate::sequencing::plane_finish_criterion(
        current.to,
        current_course,
        next_course,
    ))
}

#[cfg(test)]
pub(crate) fn active_detail_finish_criterion_for_test(
    plan: &FlightPlan,
    active_detail_index: usize,
    geometry_by_leg_id: &HashMap<String, GuidanceLegGeometry>,
    wrap_terminal_hold: bool,
) -> Option<crate::sequencing::SequencingFinishCriterion> {
    active_detail_finish_criterion(
        plan,
        active_detail_index,
        geometry_by_leg_id,
        wrap_terminal_hold,
    )
}

fn active_detail_arc_finish_criterion(
    plan: &FlightPlan,
    active_detail_index: usize,
) -> Option<crate::sequencing::SequencingFinishCriterion> {
    let detail = crate::planning::guidance_detail_ref_by_index(plan, active_detail_index)?;
    let leg = plan.resolved_legs.get(detail.leg_index)?;
    let element = leg
        .procedure_provenance
        .as_ref()?
        .display_path
        .as_ref()?
        .elements
        .get(detail.element_index)?;
    let LegDisplayElement::Arc {
        center,
        start: _,
        end,
        clockwise,
        sweep_degrees,
        ..
    } = element
    else {
        return None;
    };
    crate::sequencing::arc_finish_criterion(*center, *end, *clockwise, *sweep_degrees)
}

fn sequence_suspended_terminal_hold_detail(plan: &FlightPlan) -> AppResult<FlightPlan> {
    let guidance = plan.guidance.clone().ok_or_else(|| AppError {
        kind: AppErrorKind::UnsupportedOperation,
        message: "cannot sequence suspended hold without guidance state".to_string(),
    })?;
    let active_detail_index = guidance.active_detail_index.ok_or_else(|| AppError {
        kind: AppErrorKind::InvalidFlightPlan,
        message: "suspended hold requires an active guidance detail".to_string(),
    })?;
    let (hold_start, hold_end) = terminal_hold_detail_range(plan, guidance.active_leg_index)
        .ok_or_else(|| AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "suspended guidance detail is not in a terminal hold".to_string(),
        })?;
    if active_detail_index < hold_start || active_detail_index > hold_end {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "suspended guidance detail is outside the terminal hold".to_string(),
        });
    }
    let next_detail_index = if active_detail_index >= hold_end {
        hold_start
    } else {
        active_detail_index + 1
    };

    Ok(FlightPlan {
        guidance: Some(GuidanceState {
            active_leg_index: guidance.active_leg_index,
            active_detail_index: Some(next_detail_index),
            display_split_leg_id: guidance.display_split_leg_id.clone(),
            sequencing_mode: SequencingMode::Suspended,
            direct_to: None,
            suspend_reason: guidance.suspend_reason,
        }),
        ..plan.clone()
    })
}

fn terminal_hold_detail_range(plan: &FlightPlan, leg_index: usize) -> Option<(usize, usize)> {
    let hold_start = crate::terminal_hold_start_detail_index_for_leg(plan, leg_index)?;
    let leg = plan.resolved_legs.get(leg_index)?;
    let first_detail = crate::planning::first_guidance_detail_index_for_leg(plan, leg_index)?;
    let detail_count = crate::guidance_detail_count_for_leg(leg);
    detail_count
        .checked_sub(1)
        .map(|last_offset| (hold_start, first_detail + last_offset))
}

fn next_terminal_hold_detail_index(plan: &FlightPlan, active_detail_index: usize) -> Option<usize> {
    let active_detail = crate::planning::guidance_detail_ref_by_index(plan, active_detail_index)?;
    let (hold_start, hold_end) = terminal_hold_detail_range(plan, active_detail.leg_index)?;
    if active_detail_index < hold_start || active_detail_index > hold_end {
        return active_detail_index.checked_add(1);
    }
    Some(if active_detail_index >= hold_end {
        hold_start
    } else {
        active_detail_index + 1
    })
}

fn initial_course_for_guidance_geometry(geometry: &GuidanceLegGeometry) -> Option<f64> {
    crate::flight_plan_materialization::geometry_points(geometry)
        .windows(2)
        .find(|segment| crate::great_circle_distance_nm(segment[0], segment[1]) > f64::EPSILON)
        .map(|segment| crate::initial_course_deg(segment[0], segment[1]))
}

fn terminal_course_for_guidance_geometry(geometry: &GuidanceLegGeometry) -> Option<f64> {
    crate::flight_plan_materialization::geometry_points(geometry)
        .windows(2)
        .rev()
        .find(|segment| crate::great_circle_distance_nm(segment[0], segment[1]) > f64::EPSILON)
        .map(|segment| crate::initial_course_deg(segment[0], segment[1]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> FlightPlan {
        FlightPlan::empty()
    }

    fn inputs() -> FlightPlanProjectionInputs {
        FlightPlanProjectionInputs {
            ownship_position: None,
            ownship_speed_kt: None,
            ownship_altitude_ft: None,
            now_epoch_ms: 0,
            nav_data_generation: 0,
            aircraft_definitions_digest: [0; 32],
            weather_revision: 0,
            local_time_zone: chrono_tz::UTC,
            time_display_mode: crate::TimeDisplayMode::Local,
        }
    }

    fn atmosphere() -> crate::had_ops::PlannerAtmosphereSelection<'static> {
        crate::had_ops::PlannerAtmosphereSelection::no_wind(false)
    }

    fn direct_to_plan() -> FlightPlan {
        FlightPlan {
            route_components: vec![
                crate::RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KPAE".to_string()),
                },
                crate::RouteComponent::Waypoint {
                    waypoint: NavRef::Navaid("YKM".to_string()),
                },
            ],
            ..FlightPlan::empty()
        }
    }

    fn direct_to_action(
        projection: &FlightPlanProjection,
    ) -> (
        &crate::planning::FlightPlanDisplayRowUiView,
        &crate::planning::FlightPlanRowActionUiView,
    ) {
        let row = projection
            .ui_state
            .as_ref()
            .expect("flight-plan UI")
            .display_rows
            .iter()
            .find(|row| row.nav_ref == Some(NavRef::Navaid("YKM".to_string())))
            .expect("YKM row");
        let action = crate::planning::flight_plan_row_actions(row)
            .find(|action| action.id == FlightPlanRowActionId::DirectTo)
            .expect("Direct-To action");
        (row, action)
    }

    #[test]
    fn direct_to_menu_tracks_ownship_availability() {
        let mut controller =
            FlightPlanController::new(direct_to_plan(), Vec::new()).expect("controller");

        let unavailable = controller
            .project(None, &BTreeMap::new(), inputs(), atmosphere())
            .expect("projection without ownship")
            .projection;
        let (row, action) = direct_to_action(&unavailable);
        assert!(!action.enabled);
        assert_eq!(
            action.disabled_reason.as_deref(),
            Some(crate::planning::DIRECT_TO_OWNSHIP_POSITION_DISABLED_REASON)
        );
        let error = controller
            .plan_after_row_action(&row.uid, &action.uid, None)
            .expect_err("disabled Direct-To must not execute");
        assert_eq!(
            error.message,
            crate::planning::DIRECT_TO_OWNSHIP_POSITION_DISABLED_REASON
        );

        let mut positioned_inputs = inputs();
        positioned_inputs.ownship_position = Some(LatLon {
            lat: 47.5,
            lon: -122.3,
        });
        let available = controller
            .project(None, &BTreeMap::new(), positioned_inputs, atmosphere())
            .expect("projection with ownship")
            .projection;
        let (_, action) = direct_to_action(&available);
        assert!(action.enabled);
        assert_eq!(action.disabled_reason, None);
    }

    #[test]
    fn projection_is_cached_until_plan_or_geometry_changes() {
        let mut controller = FlightPlanController::new(plan(), Vec::new()).expect("controller");
        assert!(
            controller
                .project(None, &BTreeMap::new(), inputs(), atmosphere())
                .expect("projection")
                .rebuilt
        );
        assert!(
            !controller
                .project(None, &BTreeMap::new(), inputs(), atmosphere())
                .expect("projection")
                .rebuilt
        );

        let mut definitions_changed = inputs();
        definitions_changed.aircraft_definitions_digest = [1; 32];
        assert!(
            controller
                .project(None, &BTreeMap::new(), definitions_changed, atmosphere(),)
                .expect("aircraft definition invalidation")
                .rebuilt
        );

        controller.install_guidance_leg_geometry(vec![GuidanceLegGeometry {
            leg_id: "leg-1".to_string(),
            from: LatLon { lat: 0.0, lon: 0.0 },
            to: LatLon { lat: 0.0, lon: 1.0 },
            path: Vec::new(),
        }]);
        assert!(
            controller
                .project(None, &BTreeMap::new(), inputs(), atmosphere())
                .expect("projection")
                .rebuilt
        );
    }

    #[test]
    fn checkpoint_restores_plan_and_route_revision() {
        let mut controller = FlightPlanController::new(plan(), Vec::new()).expect("controller");
        let checkpoint = controller.checkpoint_model();
        let mut changed = plan();
        changed.name = "changed".to_string();
        controller
            .apply_definition_edit(changed)
            .expect("store plan edit");
        assert_eq!(controller.route_revision(), 1);

        controller.rollback_model(checkpoint);
        assert_eq!(controller.route_revision(), 0);
        assert_ne!(controller.active_plan().expect("plan").name, "changed");
    }

    #[test]
    fn definition_history_is_deep_branchable_and_excludes_navigation() {
        let mut controller =
            FlightPlanController::new(direct_to_plan(), Vec::new()).expect("controller");

        for name in ["first edit", "second edit", "third edit"] {
            let mut edited = controller.active_plan().expect("plan").clone();
            edited.name = name.to_string();
            controller
                .apply_definition_edit(edited)
                .expect("definition edit");
        }
        assert_eq!(controller.active_plan().expect("plan").name, "third edit");

        controller.undo_definition_edit().expect("first undo");
        controller.undo_definition_edit().expect("second undo");
        assert_eq!(controller.active_plan().expect("plan").name, "first edit");
        assert!(controller.can_redo());

        let navigated = crate::activate_leg(controller.active_plan().expect("plan"), 0)
            .expect("activate guidance");
        controller
            .apply_navigation_update(navigated)
            .expect("navigation update");
        assert!(controller.can_redo(), "navigation must not truncate redo");

        controller.redo_definition_edit().expect("redo");
        assert_eq!(controller.active_plan().expect("plan").name, "second edit");
        assert!(controller.active_plan().expect("plan").guidance.is_some());

        controller
            .undo_definition_edit()
            .expect("undo before branch");
        let mut branched = controller.active_plan().expect("plan").clone();
        branched.name = "branched edit".to_string();
        controller
            .apply_definition_edit(branched)
            .expect("branched edit");
        assert!(!controller.can_redo());
    }

    #[test]
    fn undo_reconciles_current_guidance_instead_of_rewinding_it() {
        let original = crate::build_flight_plan(FlightPlan {
            route_components: vec![
                crate::RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KPAE".to_string()),
                },
                crate::RouteComponent::Waypoint {
                    waypoint: NavRef::Navaid("YKM".to_string()),
                },
                crate::RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KBOI".to_string()),
                },
            ],
            ..FlightPlan::empty()
        })
        .expect("plan");
        let mut controller = FlightPlanController::new(original, Vec::new()).expect("controller");
        let mut edited = controller.active_plan().expect("plan").clone();
        edited.name = "edited".to_string();
        controller
            .apply_definition_edit(edited)
            .expect("definition edit");

        let navigated = crate::activate_leg(controller.active_plan().expect("plan"), 1)
            .expect("activate second leg");
        controller
            .apply_navigation_update(navigated)
            .expect("navigation update");
        let active_to_before = crate::project_ui_state(controller.active_plan().expect("plan"))
            .guidance
            .and_then(|guidance| guidance.active_to_row_uid)
            .expect("active to row");

        controller.undo_definition_edit().expect("undo");
        let restored = controller.active_plan().expect("plan");
        assert_eq!(restored.name, "Flight Plan");
        let active_to_after = crate::project_ui_state(restored)
            .guidance
            .and_then(|guidance| guidance.active_to_row_uid)
            .expect("restored active to row");
        assert_eq!(active_to_after, active_to_before);
    }

    #[test]
    fn undo_restores_the_exact_definition_after_remove_all_above() {
        let original = crate::build_flight_plan(FlightPlan {
            route_components: vec![
                crate::RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KPAE".to_string()),
                },
                crate::RouteComponent::Waypoint {
                    waypoint: NavRef::Navaid("YKM".to_string()),
                },
                crate::RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KBOI".to_string()),
                },
            ],
            ..FlightPlan::empty()
        })
        .expect("plan");
        let trimmed = crate::remove_all_above(&original, 1).expect("remove all above");
        let mut controller =
            FlightPlanController::new(original.clone(), Vec::new()).expect("controller");
        controller
            .apply_definition_edit(trimmed.clone())
            .expect("destructive edit");

        controller.undo_definition_edit().expect("undo remove all");
        assert_eq!(controller.active_plan(), Some(&original));
        controller.redo_definition_edit().expect("redo remove all");
        assert_eq!(controller.active_plan(), Some(&trimmed));
    }

    #[test]
    fn projected_history_controls_track_cursor_availability() {
        let mut controller = FlightPlanController::new(plan(), Vec::new()).expect("controller");
        let initial = controller
            .project(None, &BTreeMap::new(), inputs(), atmosphere())
            .expect("initial projection")
            .projection
            .ui_state
            .expect("UI state");
        assert!(!initial.controls[0].enabled);
        assert_eq!(initial.controls[0].id, FlightPlanControlId::Undo);
        assert!(!initial.controls[1].enabled);
        assert_eq!(initial.controls[1].id, FlightPlanControlId::Redo);

        let mut edited = controller.active_plan().expect("plan").clone();
        edited.notes = Some("remember this".to_string());
        controller
            .apply_definition_edit(edited)
            .expect("definition edit");
        let edited = controller
            .project(None, &BTreeMap::new(), inputs(), atmosphere())
            .expect("edited projection")
            .projection
            .ui_state
            .expect("UI state");
        assert!(edited.controls[0].enabled);
        assert!(!edited.controls[1].enabled);

        controller.undo_definition_edit().expect("undo");
        let undone = controller
            .project(None, &BTreeMap::new(), inputs(), atmosphere())
            .expect("undone projection")
            .projection
            .ui_state
            .expect("UI state");
        assert!(!undone.controls[0].enabled);
        assert!(undone.controls[1].enabled);
    }

    #[test]
    fn history_retains_up_to_the_configured_deep_undo_budget() {
        let mut controller = FlightPlanController::new(plan(), Vec::new()).expect("controller");
        for index in 0..MAX_FLIGHT_PLAN_UNDO_DEPTH + 8 {
            let mut edited = controller.active_plan().expect("plan").clone();
            edited.name = format!("edit {index}");
            controller
                .apply_definition_edit(edited)
                .expect("definition edit");
        }
        let mut undo_count = 0;
        while controller.can_undo() {
            controller.undo_definition_edit().expect("deep undo");
            undo_count += 1;
        }
        assert_eq!(undo_count, MAX_FLIGHT_PLAN_UNDO_DEPTH);
    }
}
