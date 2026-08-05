// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{collections::HashMap, sync::Arc};

use serde::{Deserialize, Serialize};

use crate::{
    flight_plan_materialization::MaterializedFlightPlan,
    had_ops::{FlightPlanLiveData, HadReadError},
    AppError, AppErrorKind, AppResult, FlightDataComputer, FlightPlan, FlightPlanDisplayRowKind,
    FlightPlanRouteProjection, FlightPlanRouteSegment, FlightPlanRowActionExecution,
    FlightPlanRowActionId, FlightPlanUiState, GuidanceState, LatLon, LegDisplayElement, NavKvStore,
    NavRef, ProcedureDiscontinuity, RouteComponentViewKind, SequencingMode,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuidanceLegGeometry {
    pub leg_id: String,
    pub from: LatLon,
    pub to: LatLon,
    #[serde(default)]
    pub path: Vec<LatLon>,
}

#[derive(Debug, Clone, PartialEq)]
struct FlightPlanModel {
    active_plan: Option<FlightPlan>,
    guidance_leg_geometry: Arc<HashMap<String, GuidanceLegGeometry>>,
    route_revision: u64,
    revision: u64,
}

impl Default for FlightPlanModel {
    fn default() -> Self {
        Self {
            active_plan: None,
            guidance_leg_geometry: Arc::new(HashMap::new()),
            route_revision: 0,
            revision: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FlightPlanProjectionInputs {
    pub ownship_position: Option<LatLon>,
    pub ownship_speed_kt: Option<f64>,
    pub ownship_altitude_ft: Option<f64>,
    pub now_epoch_ms: i64,
    pub nav_data_generation: u64,
    pub weather_revision: u64,
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
        Ok(Self {
            model: FlightPlanModel {
                active_plan: Some(active_plan),
                guidance_leg_geometry: Arc::new(geometry_map(guidance_leg_geometry)),
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
        &self.model.guidance_leg_geometry
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

    pub fn store_plan(&mut self, plan: FlightPlan) -> AppResult<&FlightPlan> {
        self.model.active_plan = Some(crate::build_flight_plan(plan)?);
        self.model.route_revision = self.model.route_revision.saturating_add(1);
        self.note_change();
        Ok(self.model.active_plan.as_ref().expect("stored active plan"))
    }

    pub fn replace_plan(&mut self, plan: FlightPlan) -> AppResult<&FlightPlan> {
        let normalized = crate::build_flight_plan(plan)?;
        let geometry =
            self_contained_guidance_leg_geometry_for_plan(&normalized)?.unwrap_or_default();
        self.model.active_plan = Some(normalized);
        self.model.guidance_leg_geometry = Arc::new(geometry_map(geometry));
        self.model.route_revision = self.model.route_revision.saturating_add(1);
        self.note_change();
        Ok(self
            .model
            .active_plan
            .as_ref()
            .expect("replaced active plan"))
    }

    pub fn install_guidance_leg_geometry(&mut self, geometries: Vec<GuidanceLegGeometry>) {
        let geometries = Arc::new(geometry_map(geometries));
        if self.model.guidance_leg_geometry == geometries {
            return;
        }
        self.model.guidance_leg_geometry = geometries;
        self.note_change();
    }

    pub fn clear_guidance_leg_geometry(&mut self) {
        if !self.model.guidance_leg_geometry.is_empty() {
            self.model.guidance_leg_geometry = Arc::new(HashMap::new());
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

    pub fn plan_after_row_action(
        &self,
        row_uid: &str,
        action_uid: &str,
        ownship_position: Option<LatLon>,
    ) -> AppResult<FlightPlan> {
        let plan = self.required_plan("perform row action")?;
        let ui = crate::project_ui_state(plan);
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
                message: format!("flight-plan row action is disabled: {action_uid}"),
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

        match &action.id {
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
            FlightPlanRowActionId::Remove
            | FlightPlanRowActionId::RemoveAirway
            | FlightPlanRowActionId::RemoveProcedure => {
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
                if row.component_kind == Some(RouteComponentViewKind::Airway) && row.depth > 0 {
                    let nav_ref = row.nav_ref.as_ref().ok_or_else(|| AppError {
                        kind: AppErrorKind::InvalidFlightPlan,
                        message: "airway child remove-all-above row has no nav reference"
                            .to_string(),
                    })?;
                    crate::remove_all_above_airway_child_waypoint(
                        plan,
                        row_component_index()?,
                        nav_ref,
                    )
                } else {
                    crate::remove_all_above(plan, row_component_index()?)
                }
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
        }
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
            let geometry =
                active_guidance_projection_geometry(plan, &self.model.guidance_leg_geometry)?;
            return Some((geometry.leg_id.clone(), geometry));
        }
        let active_detail_index = active_guidance_detail_index_for_motion(plan, guidance)?;
        active_guidance_detail_geometry_for_index(
            plan,
            active_detail_index,
            &self.model.guidance_leg_geometry,
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
            let (finish_criterion, suspended_hold) =
                if guidance.sequencing_mode == SequencingMode::DirectTo {
                    let Some(finish_criterion) =
                        direct_to_finish_criterion(plan, &self.model.guidance_leg_geometry)
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
                        &self.model.guidance_leg_geometry,
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
            self.store_plan(next_plan)?;
            sequenced = true;
        }
        Ok(sequenced)
    }

    pub fn project(
        &mut self,
        store: Option<&NavKvStore>,
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

        let projection = match self.model.active_plan.as_ref() {
            None => FlightPlanProjection {
                ui_state: None,
                materialized: None,
            },
            Some(plan) => {
                let ui_state = crate::project_ui_state(plan);
                if let Some(store) = store {
                    let projection = crate::had_ops::flight_plan_ui_projection(
                        store,
                        plan.clone(),
                        ui_state,
                        FlightDataComputer::with_clock(
                            inputs.ownship_speed_kt,
                            Some(inputs.now_epoch_ms),
                        ),
                        FlightPlanLiveData {
                            ownship_position: inputs.ownship_position,
                            ownship_altitude_ft: inputs.ownship_altitude_ft,
                            now_epoch_ms: Some(inputs.now_epoch_ms),
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
                        &self.model.guidance_leg_geometry,
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
            weather_revision: 0,
        }
    }

    fn atmosphere() -> crate::had_ops::PlannerAtmosphereSelection<'static> {
        crate::had_ops::PlannerAtmosphereSelection::no_wind(false)
    }

    #[test]
    fn projection_is_cached_until_plan_or_geometry_changes() {
        let mut controller = FlightPlanController::new(plan(), Vec::new()).expect("controller");
        assert!(
            controller
                .project(None, inputs(), atmosphere())
                .expect("projection")
                .rebuilt
        );
        assert!(
            !controller
                .project(None, inputs(), atmosphere())
                .expect("projection")
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
                .project(None, inputs(), atmosphere())
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
        controller.store_plan(changed).expect("store plan");
        assert_eq!(controller.route_revision(), 1);

        controller.rollback_model(checkpoint);
        assert_eq!(controller.route_revision(), 0);
        assert_ne!(controller.active_plan().expect("plan").name, "changed");
    }
}
