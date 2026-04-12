use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::errors::{AppError, AppErrorKind, AppResult};
use crate::geometry::LatLon;
use crate::ids::AirportId;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlightPlan {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub legs: Vec<PlanLeg>,
    #[serde(default)]
    pub route_components: Vec<RouteComponent>,
    #[serde(default)]
    pub resolved_legs: Vec<ResolvedLeg>,
    #[serde(default)]
    pub guidance: Option<GuidanceState>,
    pub departure: Option<AirportId>,
    pub destination: Option<AirportId>,
    pub alternate: Option<AirportId>,
    pub cruise_altitude_ft: Option<i32>,
    pub notes: Option<String>,
    pub updated_at_epoch_ms: i64,
    pub version: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanLeg {
    pub from: NavRef,
    pub to: NavRef,
    pub airway: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RouteComponent {
    Waypoint { waypoint: NavRef },
    Airway { airway: AirwaySegment },
    Procedure { procedure: ProcedureSegment },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirwaySegment {
    pub name: String,
    #[serde(default)]
    pub branch_key: Option<String>,
    pub entry: NavRef,
    pub exit: NavRef,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcedureSegment {
    pub airport_id: AirportId,
    pub procedure_id: String,
    pub kind: ProcedureKind,
    pub runway_transition: Option<String>,
    pub enroute_transition: Option<String>,
    #[serde(default)]
    pub terminal_discontinuity: Option<ProcedureDiscontinuity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathTermination {
    InitialFix,
    TrackToFix,
    CourseToFix,
    DirectToFix,
    HeadingToManual,
    HeadingToAltitude,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcedureKind {
    Sid,
    Star,
    Approach,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcedureDiscontinuity {
    Vectors,
    Hold,
    Other(String),
}

impl ProcedureDiscontinuity {
    pub fn display_label(&self) -> &str {
        match self {
            ProcedureDiscontinuity::Vectors => "VECTORS",
            ProcedureDiscontinuity::Hold => "HOLD",
            ProcedureDiscontinuity::Other(label) => label.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConcretizedNavItem {
    Waypoint { nav_ref: NavRef },
    Discontinuity {
        discontinuity: ProcedureDiscontinuity,
        label: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedLeg {
    pub id: String,
    pub from: NavRef,
    pub to: NavRef,
    pub source: ResolvedLegSource,
    #[serde(default)]
    pub procedure_provenance: Option<ProcedureLegProvenance>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResolvedLegSource {
    LegacyPlanLeg { leg_index: usize },
    RouteComponent { component_index: usize },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcedureSegmentRole {
    EnrouteTransition,
    Common,
    RunwayTransition,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcedureLegProvenance {
    pub airport_id: String,
    pub procedure_id: String,
    pub kind: ProcedureKind,
    pub role: ProcedureSegmentRole,
    pub path_termination: PathTermination,
    pub leg_sequence: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuidanceState {
    pub active_leg_index: usize,
    pub sequencing_mode: SequencingMode,
    pub direct_to: Option<DirectToState>,
    #[serde(default)]
    pub suspend_reason: Option<SuspendReason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SequencingMode {
    FollowPlan,
    Suspended,
    DirectTo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuspendReason {
    Manual,
    Boundary,
    RouteEnd,
    DirectToComplete,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DirectToState {
    pub start: NavRef,
    pub target: NavRef,
    pub target_leg_id: Option<String>,
    pub resume_leg_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlightPlanUiState {
    pub components: Vec<RouteComponentUiView>,
    pub resolved_legs: Vec<ResolvedLegUiView>,
    pub guidance: Option<GuidanceUiView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteComponentViewKind {
    Waypoint,
    Airway,
    Procedure,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteComponentUiView {
    pub component_index: usize,
    pub kind: RouteComponentViewKind,
    pub summary: String,
    pub items: Vec<ConcretizedNavItem>,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedLegUiView {
    pub leg_index: usize,
    pub leg_id: String,
    pub component_index: Option<usize>,
    pub from: NavRef,
    pub to: NavRef,
    pub active: bool,
    pub suspend_boundary_after: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuidanceUiView {
    pub sequencing_mode: SequencingMode,
    pub active_leg_index: Option<usize>,
    pub active_component_index: Option<usize>,
    pub active_leg: Option<PlanLeg>,
    pub direct_to: Option<DirectToUiView>,
    pub can_activate_next_leg: bool,
    pub can_suspend: bool,
    pub can_unsuspend: bool,
    pub suspend_boundary_after_active_leg: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DirectToUiView {
    pub start: NavRef,
    pub target: NavRef,
    pub target_leg_id: Option<String>,
    pub resume_leg_id: Option<String>,
    pub on_plan_target: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NavRef {
    Airport(String),
    Navaid(String),
    Fix(String),
    LatLon(LatLon),
}

impl NavRef {
    pub fn airport_code(&self) -> Option<&str> {
        match self {
            NavRef::Airport(code) => Some(code.as_str()),
            _ => None,
        }
    }
}

impl FlightPlan {
    pub fn normalized(mut self) -> Self {
        if self.route_components.is_empty() {
            self.route_components = legacy_components_from_legs(&self.legs);
        }

        if self.resolved_legs.is_empty() {
            self.resolved_legs = if self.route_components.is_empty() {
                resolved_legs_from_legacy_legs(&self.legs)
            } else {
                resolved_legs_from_waypoint_components(&self.route_components)
            };
        }

        if self.legs.is_empty() && !self.resolved_legs.is_empty() {
            self.legs = legacy_legs_from_resolved_legs(&self.resolved_legs);
        }

        self
    }
}

impl RouteComponent {
    fn is_waypoint(&self) -> bool {
        matches!(self, RouteComponent::Waypoint { .. })
    }
}

pub fn activate_direct_to(
    plan: &FlightPlan,
    from_position: LatLon,
    target: NavRef,
) -> AppResult<FlightPlan> {
    let plan = plan.clone().normalized();
    if plan.resolved_legs.is_empty() {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "cannot activate direct-to without a resolved route".to_string(),
        });
    }

    let target_leg_index = plan
        .resolved_legs
        .iter()
        .position(|leg| leg.to == target);
    let resume_leg_index = plan
        .resolved_legs
        .iter()
        .position(|leg| leg.from == target);
    let target_leg_id = target_leg_index.map(|index| plan.resolved_legs[index].id.clone());
    let resume_leg_id = resume_leg_index.map(|index| plan.resolved_legs[index].id.clone());
    let active_leg_index = target_leg_index
        .or_else(|| plan.guidance.as_ref().map(|guidance| guidance.active_leg_index))
        .unwrap_or(0);

    Ok(FlightPlan {
        guidance: Some(GuidanceState {
            active_leg_index,
            sequencing_mode: SequencingMode::DirectTo,
            direct_to: Some(DirectToState {
                start: NavRef::LatLon(from_position),
                target,
                target_leg_id,
                resume_leg_id,
            }),
            suspend_reason: None,
        }),
        ..plan
    })
}

pub fn activate_direct_to_leg(
    plan: &FlightPlan,
    from_position: LatLon,
    target_leg_id: &str,
) -> AppResult<FlightPlan> {
    let plan = plan.clone().normalized();
    let target_leg_index = plan
        .resolved_legs
        .iter()
        .position(|leg| leg.id == target_leg_id)
        .ok_or_else(|| AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: format!("direct-to target leg not found: {target_leg_id}"),
        })?;
    let target_leg = &plan.resolved_legs[target_leg_index];
    let resume_leg_id = resume_leg_id_after_leg(&plan, target_leg_index);

    Ok(FlightPlan {
        guidance: Some(GuidanceState {
            active_leg_index: target_leg_index,
            sequencing_mode: SequencingMode::DirectTo,
            direct_to: Some(DirectToState {
                start: NavRef::LatLon(from_position),
                target: target_leg.to.clone(),
                target_leg_id: Some(target_leg.id.clone()),
                resume_leg_id,
            }),
            suspend_reason: None,
        }),
        ..plan
    })
}

pub fn activate_leg(plan: &FlightPlan, leg_index: usize) -> AppResult<FlightPlan> {
    let plan = plan.clone().normalized();
    if leg_index >= plan.resolved_legs.len() {
        return Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: format!("leg index out of bounds: {leg_index}"),
        });
    }

    Ok(FlightPlan {
        guidance: Some(GuidanceState {
            active_leg_index: leg_index,
            sequencing_mode: SequencingMode::FollowPlan,
            direct_to: None,
            suspend_reason: None,
        }),
        ..plan
    })
}

pub fn activate_next_leg(plan: &FlightPlan) -> AppResult<FlightPlan> {
    let plan = plan.clone().normalized();
    let guidance = plan.guidance.clone().ok_or_else(|| AppError {
        kind: AppErrorKind::UnsupportedOperation,
        message: "cannot activate next leg without guidance state".to_string(),
    })?;
    let next_leg_index = guidance
        .active_leg_index
        .checked_add(1)
        .filter(|index| *index < plan.resolved_legs.len())
        .ok_or_else(|| AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: "no next leg is available".to_string(),
        })?;

    Ok(FlightPlan {
        guidance: Some(GuidanceState {
            active_leg_index: next_leg_index,
            sequencing_mode: SequencingMode::FollowPlan,
            direct_to: None,
            suspend_reason: None,
        }),
        ..plan
    })
}

pub fn suspend_sequencing(plan: &FlightPlan) -> AppResult<FlightPlan> {
    let plan = plan.clone().normalized();
    let guidance = plan.guidance.clone().ok_or_else(|| AppError {
        kind: AppErrorKind::UnsupportedOperation,
        message: "cannot suspend sequencing without guidance state".to_string(),
    })?;

    Ok(FlightPlan {
        guidance: Some(GuidanceState {
            active_leg_index: guidance.active_leg_index,
            sequencing_mode: SequencingMode::Suspended,
            direct_to: None,
            suspend_reason: Some(SuspendReason::Manual),
        }),
        ..plan
    })
}

pub fn unsuspend_sequencing(plan: &FlightPlan) -> AppResult<FlightPlan> {
    let plan = plan.clone().normalized();
    let guidance = plan.guidance.clone().ok_or_else(|| AppError {
        kind: AppErrorKind::UnsupportedOperation,
        message: "cannot unsuspend sequencing without guidance state".to_string(),
    })?;

    if guidance.sequencing_mode != SequencingMode::Suspended {
        return Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: "sequencing is not suspended".to_string(),
        });
    }

    if should_suspend_after_active_leg(&plan, guidance.active_leg_index) {
        return activate_next_leg(&plan);
    }

    Ok(FlightPlan {
        guidance: Some(GuidanceState {
            active_leg_index: guidance.active_leg_index,
            sequencing_mode: SequencingMode::FollowPlan,
            direct_to: None,
            suspend_reason: None,
        }),
        ..plan
    })
}

pub fn sequence_active_leg(plan: &FlightPlan) -> AppResult<FlightPlan> {
    let plan = plan.clone().normalized();
    let guidance = plan.guidance.clone().ok_or_else(|| AppError {
        kind: AppErrorKind::UnsupportedOperation,
        message: "cannot sequence a plan without guidance state".to_string(),
    })?;

    let next_guidance = match guidance.sequencing_mode {
        SequencingMode::DirectTo => {
            let direct_to = guidance.direct_to.ok_or_else(|| AppError {
                kind: AppErrorKind::InvalidFlightPlan,
                message: "direct-to sequencing mode requires direct-to state".to_string(),
            })?;

            match direct_to
                .resume_leg_id
                .as_deref()
                .and_then(|resume_leg_id| leg_index_by_id(&plan.resolved_legs, resume_leg_id))
            {
                Some(resume_leg_index) => GuidanceState {
                    active_leg_index: resume_leg_index,
                    sequencing_mode: SequencingMode::FollowPlan,
                    direct_to: None,
                    suspend_reason: None,
                },
                None => GuidanceState {
                    active_leg_index: direct_to
                        .target_leg_id
                        .as_deref()
                        .and_then(|target_leg_id| leg_index_by_id(&plan.resolved_legs, target_leg_id))
                        .unwrap_or(guidance.active_leg_index),
                    sequencing_mode: SequencingMode::Suspended,
                    direct_to: None,
                    suspend_reason: Some(SuspendReason::DirectToComplete),
                },
            }
        }
        SequencingMode::FollowPlan => {
            if plan.resolved_legs.is_empty() {
                return Err(AppError {
                    kind: AppErrorKind::InvalidFlightPlan,
                    message: "cannot sequence an empty resolved route".to_string(),
                });
            }

            if should_suspend_after_active_leg(&plan, guidance.active_leg_index) {
                GuidanceState {
                    active_leg_index: guidance.active_leg_index,
                    sequencing_mode: SequencingMode::Suspended,
                    direct_to: None,
                    suspend_reason: Some(SuspendReason::Boundary),
                }
            } else if guidance.active_leg_index + 1 < plan.resolved_legs.len() {
                GuidanceState {
                    active_leg_index: guidance.active_leg_index + 1,
                    sequencing_mode: SequencingMode::FollowPlan,
                    direct_to: None,
                    suspend_reason: None,
                }
            } else {
                GuidanceState {
                    active_leg_index: guidance.active_leg_index,
                    sequencing_mode: SequencingMode::Suspended,
                    direct_to: None,
                    suspend_reason: Some(SuspendReason::RouteEnd),
                }
            }
        }
        SequencingMode::Suspended => guidance,
    };

    Ok(FlightPlan {
        guidance: Some(next_guidance),
        ..plan
    })
}

pub fn active_guidance_leg(plan: &FlightPlan) -> Option<PlanLeg> {
    let plan = plan.clone().normalized();
    let guidance = plan.guidance.clone()?;

    match guidance.sequencing_mode {
        SequencingMode::DirectTo => {
            let direct_to = guidance.direct_to?;
            Some(PlanLeg {
                from: direct_to.start,
                to: direct_to.target,
                airway: None,
            })
        }
        SequencingMode::FollowPlan => plan.resolved_legs.get(guidance.active_leg_index).map(|leg| PlanLeg {
            from: leg.from.clone(),
            to: leg.to.clone(),
            airway: None,
        }),
        SequencingMode::Suspended => {
            let preserve_active_leg = match guidance.suspend_reason {
                Some(SuspendReason::Manual) => true,
                Some(SuspendReason::Boundary | SuspendReason::RouteEnd | SuspendReason::DirectToComplete) => false,
                None => {
                    guidance.direct_to.is_none()
                        && !should_suspend_after_active_leg(&plan, guidance.active_leg_index)
                        && guidance.active_leg_index + 1 < plan.resolved_legs.len()
                }
            };
            if !preserve_active_leg {
                None
            } else {
                plan.resolved_legs.get(guidance.active_leg_index).map(|leg| PlanLeg {
                    from: leg.from.clone(),
                    to: leg.to.clone(),
                    airway: None,
                })
            }
        }
    }
}

pub fn project_ui_state(plan: &FlightPlan) -> FlightPlanUiState {
    let plan = plan.clone().normalized();
    let grouped_legs = grouped_component_legs(&plan);
    let active_component_index = plan
        .guidance
        .as_ref()
        .and_then(|guidance| active_component_index_for_guidance(&plan, guidance));

    let components = plan
        .route_components
        .iter()
        .enumerate()
        .map(|(component_index, component)| RouteComponentUiView {
            component_index,
            kind: component_view_kind(component),
            summary: component_summary(component),
            items: component_ui_items(
                component,
                intrinsic_component_legs(&plan, &grouped_legs, component_index),
            ),
            active: active_component_index == Some(component_index),
        })
        .collect();

    let resolved_legs = plan
        .resolved_legs
        .iter()
        .enumerate()
        .map(|(leg_index, leg)| ResolvedLegUiView {
            leg_index,
            leg_id: leg.id.clone(),
            component_index: match leg.source {
                ResolvedLegSource::RouteComponent { component_index } => Some(component_index),
                ResolvedLegSource::LegacyPlanLeg { .. } => None,
            },
            from: leg.from.clone(),
            to: leg.to.clone(),
            active: plan
                .guidance
                .as_ref()
                .is_some_and(|guidance| guidance.sequencing_mode != SequencingMode::DirectTo && guidance.active_leg_index == leg_index),
            suspend_boundary_after: should_suspend_after_active_leg(&plan, leg_index),
        })
        .collect();

    let guidance = plan.guidance.as_ref().map(|guidance| GuidanceUiView {
        sequencing_mode: guidance.sequencing_mode.clone(),
        active_leg_index: if plan.resolved_legs.is_empty() {
            None
        } else {
            Some(guidance.active_leg_index)
        },
        active_component_index,
        active_leg: active_guidance_leg(&plan),
        direct_to: guidance.direct_to.as_ref().map(|direct_to| DirectToUiView {
            start: direct_to.start.clone(),
            target: direct_to.target.clone(),
            target_leg_id: direct_to.target_leg_id.clone(),
            resume_leg_id: direct_to.resume_leg_id.clone(),
            on_plan_target: direct_to.target_leg_id.is_some(),
        }),
        can_activate_next_leg: guidance.active_leg_index + 1 < plan.resolved_legs.len(),
        can_suspend: guidance.sequencing_mode != SequencingMode::Suspended,
        can_unsuspend: guidance.sequencing_mode == SequencingMode::Suspended,
        suspend_boundary_after_active_leg: should_suspend_after_active_leg(&plan, guidance.active_leg_index),
    });

    FlightPlanUiState {
        components,
        resolved_legs,
        guidance,
    }
}

pub fn delete_component(plan: &FlightPlan, component_index: usize) -> AppResult<FlightPlan> {
    if component_index >= plan.route_components.len() {
        return Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: format!("component index out of bounds: {component_index}"),
        });
    }

    let mut next = plan.clone();
    next.route_components.remove(component_index);
    rebuild_after_component_edit(next)
}

pub fn delete_waypoint_component(plan: &FlightPlan, component_index: usize) -> AppResult<FlightPlan> {
    match plan.route_components.get(component_index) {
        Some(RouteComponent::Waypoint { .. }) => delete_component(plan, component_index),
        Some(RouteComponent::Airway { .. }) | Some(RouteComponent::Procedure { .. }) => Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: "cannot delete inside a grouped route component directly; flatten or remove the whole component".to_string(),
        }),
        None => Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: format!("component index out of bounds: {component_index}"),
        }),
    }
}

pub fn flatten_component_to_waypoints(
    plan: &FlightPlan,
    component_index: usize,
    waypoints: Vec<NavRef>,
) -> AppResult<FlightPlan> {
    if waypoints.len() < 2 {
        return Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: "flattening requires at least two waypoints".to_string(),
        });
    }

    match plan.route_components.get(component_index) {
        Some(RouteComponent::Waypoint { .. }) => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: "waypoint components are already explicit".to_string(),
            })
        }
        Some(RouteComponent::Airway { .. }) | Some(RouteComponent::Procedure { .. }) => {}
        None => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: format!("component index out of bounds: {component_index}"),
            })
        }
    }

    let mut next = plan.clone();
    let replacements = waypoints
        .into_iter()
        .map(|waypoint| RouteComponent::Waypoint { waypoint })
        .collect::<Vec<_>>();
    next.route_components
        .splice(component_index..=component_index, replacements);

    rebuild_after_component_edit(next)
}

pub fn insert_airway_between_waypoints(
    plan: &FlightPlan,
    start_component_index: usize,
    end_component_index: usize,
    airway: AirwaySegment,
    airway_legs: Vec<ResolvedLeg>,
) -> AppResult<FlightPlan> {
    if start_component_index >= end_component_index {
        return Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: format!(
                "airway insertion requires an increasing waypoint span, got start={start_component_index} end={end_component_index}"
            ),
        });
    }

    let plan = plan.clone().normalized();
    match plan.route_components.get(start_component_index) {
        Some(RouteComponent::Waypoint { .. }) => {}
        Some(_) => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: "airway insertion start must be a waypoint component".to_string(),
            })
        }
        None => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: format!("component index out of bounds: {start_component_index}"),
            })
        }
    }
    match plan.route_components.get(end_component_index) {
        Some(RouteComponent::Waypoint { .. }) => {}
        Some(_) => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: "airway insertion end must be a waypoint component".to_string(),
            })
        }
        None => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: format!("component index out of bounds: {end_component_index}"),
            })
        }
    }

    if plan.route_components[start_component_index + 1..end_component_index]
        .iter()
        .any(|component| !component.is_waypoint())
    {
        return Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message:
                "airway insertion cannot replace a span containing grouped components; flatten or remove them first"
                    .to_string(),
        });
    }

    let mut new_components = Vec::<RouteComponent>::new();
    let mut preserved_grouped_legs = BTreeMap::<usize, Vec<ResolvedLeg>>::new();
    let old_grouped_legs = grouped_component_legs(&plan);

    for old_index in 0..=start_component_index {
        let new_index = new_components.len();
        new_components.push(plan.route_components[old_index].clone());
        if let Some(legs) = old_grouped_legs.get(&old_index) {
            preserved_grouped_legs.insert(new_index, rewrite_grouped_legs_source(legs, new_index));
        }
    }

    let airway_component_index = new_components.len();
    new_components.push(RouteComponent::Airway {
        airway: airway.clone(),
    });

    for old_index in end_component_index..plan.route_components.len() {
        let new_index = new_components.len();
        new_components.push(plan.route_components[old_index].clone());
        if let Some(legs) = old_grouped_legs.get(&old_index) {
            preserved_grouped_legs.insert(new_index, rewrite_grouped_legs_source(legs, new_index));
        }
    }

    preserved_grouped_legs.insert(
        airway_component_index,
        rewrite_grouped_legs_source(&airway_legs, airway_component_index),
    );

    let resolved_legs = rebuild_resolved_legs_with_grouped_components(
        &new_components,
        &preserved_grouped_legs,
    );

    if resolved_legs.is_empty() {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "flight plan must contain at least one flyable leg after airway insertion"
                .to_string(),
        });
    }

    Ok(FlightPlan {
        route_components: new_components,
        resolved_legs: resolved_legs.clone(),
        legs: legacy_legs_from_resolved_legs(&resolved_legs),
        guidance: None,
        ..plan
    })
}

pub fn insert_procedure_between_waypoints(
    plan: &FlightPlan,
    start_component_index: usize,
    end_component_index: usize,
    procedure: ProcedureSegment,
    procedure_legs: Vec<ResolvedLeg>,
) -> AppResult<FlightPlan> {
    if start_component_index >= end_component_index {
        return Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: format!(
                "procedure insertion requires an increasing waypoint span, got start={start_component_index} end={end_component_index}"
            ),
        });
    }

    let plan = plan.clone().normalized();
    match plan.route_components.get(start_component_index) {
        Some(RouteComponent::Waypoint { .. }) => {}
        Some(_) => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: "procedure insertion start must be a waypoint component".to_string(),
            })
        }
        None => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: format!("component index out of bounds: {start_component_index}"),
            })
        }
    }
    match plan.route_components.get(end_component_index) {
        Some(RouteComponent::Waypoint { .. }) => {}
        Some(_) => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: "procedure insertion end must be a waypoint component".to_string(),
            })
        }
        None => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: format!("component index out of bounds: {end_component_index}"),
            })
        }
    }

    if plan.route_components[start_component_index + 1..end_component_index]
        .iter()
        .any(|component| !component.is_waypoint())
    {
        return Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message:
                "procedure insertion cannot replace a span containing grouped components; flatten or remove them first"
                    .to_string(),
        });
    }

    let mut new_components = Vec::<RouteComponent>::new();
    let mut preserved_grouped_legs = BTreeMap::<usize, Vec<ResolvedLeg>>::new();
    let old_grouped_legs = grouped_component_legs(&plan);

    for old_index in 0..=start_component_index {
        let new_index = new_components.len();
        new_components.push(plan.route_components[old_index].clone());
        if let Some(legs) = old_grouped_legs.get(&old_index) {
            preserved_grouped_legs.insert(new_index, rewrite_grouped_legs_source(legs, new_index));
        }
    }

    let procedure_component_index = new_components.len();
    new_components.push(RouteComponent::Procedure {
        procedure: procedure.clone(),
    });

    for old_index in end_component_index..plan.route_components.len() {
        let new_index = new_components.len();
        new_components.push(plan.route_components[old_index].clone());
        if let Some(legs) = old_grouped_legs.get(&old_index) {
            preserved_grouped_legs.insert(new_index, rewrite_grouped_legs_source(legs, new_index));
        }
    }

    preserved_grouped_legs.insert(
        procedure_component_index,
        rewrite_grouped_legs_source(&procedure_legs, procedure_component_index),
    );

    let resolved_legs = rebuild_resolved_legs_with_grouped_components(
        &new_components,
        &preserved_grouped_legs,
    );

    if resolved_legs.is_empty() {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "flight plan must contain at least one flyable leg after procedure insertion"
                .to_string(),
        });
    }

    Ok(FlightPlan {
        route_components: new_components,
        resolved_legs: resolved_legs.clone(),
        legs: legacy_legs_from_resolved_legs(&resolved_legs),
        guidance: None,
        ..plan
    })
}

pub fn replace_airway_component(
    plan: &FlightPlan,
    component_index: usize,
    airway: AirwaySegment,
    airway_legs: Vec<ResolvedLeg>,
) -> AppResult<FlightPlan> {
    let plan = plan.clone().normalized();
    match plan.route_components.get(component_index) {
        Some(RouteComponent::Airway { .. }) => {}
        Some(_) => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: "target component is not an airway".to_string(),
            })
        }
        None => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: format!("component index out of bounds: {component_index}"),
            })
        }
    }

    if airway_legs.is_empty() {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "replacement airway must contain at least one resolved leg".to_string(),
        });
    }

    let mut new_components = plan.route_components.clone();
    new_components[component_index] = RouteComponent::Airway {
        airway: airway.clone(),
    };

    let mut grouped_legs = grouped_component_legs(&plan);
    grouped_legs.insert(
        component_index,
        rewrite_grouped_legs_source(&airway_legs, component_index),
    );

    let resolved_legs = rebuild_resolved_legs_with_grouped_components(&new_components, &grouped_legs);
    if resolved_legs.is_empty() {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "flight plan must contain at least one flyable leg after airway replacement"
                .to_string(),
        });
    }

    Ok(FlightPlan {
        route_components: new_components,
        resolved_legs: resolved_legs.clone(),
        legs: legacy_legs_from_resolved_legs(&resolved_legs),
        guidance: None,
        ..plan
    })
}

pub fn replace_procedure_component(
    plan: &FlightPlan,
    component_index: usize,
    procedure: ProcedureSegment,
    procedure_legs: Vec<ResolvedLeg>,
) -> AppResult<FlightPlan> {
    let plan = plan.clone().normalized();
    match plan.route_components.get(component_index) {
        Some(RouteComponent::Procedure { .. }) => {}
        Some(_) => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: "target component is not a procedure".to_string(),
            })
        }
        None => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: format!("component index out of bounds: {component_index}"),
            })
        }
    }

    if procedure_legs.is_empty() {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "replacement procedure must contain at least one resolved leg".to_string(),
        });
    }

    let mut new_components = plan.route_components.clone();
    new_components[component_index] = RouteComponent::Procedure {
        procedure: procedure.clone(),
    };

    let mut grouped_legs = grouped_component_legs(&plan);
    grouped_legs.insert(
        component_index,
        rewrite_grouped_legs_source(&procedure_legs, component_index),
    );

    let resolved_legs = rebuild_resolved_legs_with_grouped_components(&new_components, &grouped_legs);
    if resolved_legs.is_empty() {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "flight plan must contain at least one flyable leg after procedure replacement"
                .to_string(),
        });
    }

    Ok(FlightPlan {
        route_components: new_components,
        resolved_legs: resolved_legs.clone(),
        legs: legacy_legs_from_resolved_legs(&resolved_legs),
        guidance: None,
        ..plan
    })
}

pub fn change_airway_entry(
    plan: &FlightPlan,
    component_index: usize,
    entry: NavRef,
    airway_legs: Vec<ResolvedLeg>,
) -> AppResult<FlightPlan> {
    let existing = match plan
        .clone()
        .normalized()
        .route_components
        .get(component_index)
    {
        Some(RouteComponent::Airway { airway }) => airway.clone(),
        Some(_) => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: "target component is not an airway".to_string(),
            })
        }
        None => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: format!("component index out of bounds: {component_index}"),
            })
        }
    };

    replace_airway_component(
        plan,
        component_index,
        AirwaySegment {
            entry,
            ..existing
        },
        airway_legs,
    )
}

pub fn change_airway_exit(
    plan: &FlightPlan,
    component_index: usize,
    exit: NavRef,
    airway_legs: Vec<ResolvedLeg>,
) -> AppResult<FlightPlan> {
    let existing = match plan
        .clone()
        .normalized()
        .route_components
        .get(component_index)
    {
        Some(RouteComponent::Airway { airway }) => airway.clone(),
        Some(_) => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: "target component is not an airway".to_string(),
            })
        }
        None => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: format!("component index out of bounds: {component_index}"),
            })
        }
    };

    replace_airway_component(
        plan,
        component_index,
        AirwaySegment {
            exit,
            ..existing
        },
        airway_legs,
    )
}

pub fn change_procedure_enroute_transition(
    plan: &FlightPlan,
    component_index: usize,
    enroute_transition: Option<String>,
    procedure_legs: Vec<ResolvedLeg>,
) -> AppResult<FlightPlan> {
    let existing = match plan.clone().normalized().route_components.get(component_index) {
        Some(RouteComponent::Procedure { procedure }) => procedure.clone(),
        Some(_) => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: "target component is not a procedure".to_string(),
            })
        }
        None => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: format!("component index out of bounds: {component_index}"),
            })
        }
    };

    replace_procedure_component(
        plan,
        component_index,
        ProcedureSegment {
            enroute_transition,
            ..existing
        },
        procedure_legs,
    )
}

pub fn change_procedure_runway_transition(
    plan: &FlightPlan,
    component_index: usize,
    runway_transition: Option<String>,
    procedure_legs: Vec<ResolvedLeg>,
) -> AppResult<FlightPlan> {
    let existing = match plan.clone().normalized().route_components.get(component_index) {
        Some(RouteComponent::Procedure { procedure }) => procedure.clone(),
        Some(_) => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: "target component is not a procedure".to_string(),
            })
        }
        None => {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: format!("component index out of bounds: {component_index}"),
            })
        }
    };

    replace_procedure_component(
        plan,
        component_index,
        ProcedureSegment {
            runway_transition,
            ..existing
        },
        procedure_legs,
    )
}

pub fn interpret_path_termination(code: &str) -> PathTermination {
    match code.trim() {
        "IF" => PathTermination::InitialFix,
        "TF" => PathTermination::TrackToFix,
        "CF" => PathTermination::CourseToFix,
        "DF" => PathTermination::DirectToFix,
        "FM" | "HM" => PathTermination::HeadingToManual,
        "VA" | "VI" => PathTermination::HeadingToAltitude,
        other => PathTermination::Other(other.to_string()),
    }
}

fn legacy_components_from_legs(legs: &[PlanLeg]) -> Vec<RouteComponent> {
    let mut components = Vec::new();

    for leg in legs {
        push_unique_waypoint(&mut components, &leg.from);
        push_unique_waypoint(&mut components, &leg.to);
    }

    components
}

fn push_unique_waypoint(components: &mut Vec<RouteComponent>, waypoint: &NavRef) {
    let should_push = components
        .last()
        .and_then(|component| match component {
            RouteComponent::Waypoint { waypoint: existing } => Some(existing != waypoint),
            _ => Some(true),
        })
        .unwrap_or(true);

    if should_push {
        components.push(RouteComponent::Waypoint {
            waypoint: waypoint.clone(),
        });
    }
}

fn resolved_legs_from_waypoint_components(components: &[RouteComponent]) -> Vec<ResolvedLeg> {
    let mut legs = Vec::new();

    for (index, pair) in components.windows(2).enumerate() {
        let [left, right] = pair else {
            continue;
        };

        let (RouteComponent::Waypoint { waypoint: from }, RouteComponent::Waypoint { waypoint: to }) =
            (left, right)
        else {
            continue;
        };

        legs.push(ResolvedLeg {
            id: format!("component-{index}-{}", index + 1),
            from: from.clone(),
            to: to.clone(),
            source: ResolvedLegSource::RouteComponent {
                component_index: index,
            },
            procedure_provenance: None,
        });
    }

    legs
}

fn rebuild_resolved_legs_with_grouped_components(
    components: &[RouteComponent],
    grouped_component_legs: &BTreeMap<usize, Vec<ResolvedLeg>>,
) -> Vec<ResolvedLeg> {
    let mut resolved = Vec::new();

    for (index, component) in components.iter().enumerate() {
        if index > 0 {
            let previous_index = index - 1;
            if let Some((from, to)) = bridge_between_components(
                components.get(previous_index),
                grouped_component_legs.get(&previous_index),
                Some(component),
                grouped_component_legs.get(&index),
            ) {
                if from != to {
                    resolved.push(ResolvedLeg {
                        id: format!("component-{previous_index}-{index}"),
                        from,
                        to,
                        source: ResolvedLegSource::RouteComponent {
                            component_index: previous_index,
                        },
                        procedure_provenance: None,
                    });
                }
            }
        }

        if let Some(legs) = grouped_component_legs.get(&index) {
            resolved.extend(legs.iter().cloned());
        }
    }

    resolved
}

fn resolved_legs_from_legacy_legs(legs: &[PlanLeg]) -> Vec<ResolvedLeg> {
    legs.iter()
        .enumerate()
        .map(|(index, leg)| ResolvedLeg {
            id: format!("legacy-{index}"),
            from: leg.from.clone(),
            to: leg.to.clone(),
            source: ResolvedLegSource::LegacyPlanLeg { leg_index: index },
            procedure_provenance: None,
        })
        .collect()
}

fn legacy_legs_from_resolved_legs(resolved_legs: &[ResolvedLeg]) -> Vec<PlanLeg> {
    resolved_legs
        .iter()
        .map(|leg| PlanLeg {
            from: leg.from.clone(),
            to: leg.to.clone(),
            airway: None,
        })
        .collect()
}

fn grouped_component_legs(plan: &FlightPlan) -> BTreeMap<usize, Vec<ResolvedLeg>> {
    let mut by_component = BTreeMap::<usize, Vec<ResolvedLeg>>::new();

    for leg in &plan.resolved_legs {
        let ResolvedLegSource::RouteComponent { component_index } = leg.source else {
            continue;
        };
        let Some(component) = plan.route_components.get(component_index) else {
            continue;
        };
        if component.is_waypoint() {
            continue;
        }
        by_component
            .entry(component_index)
            .or_default()
            .push(leg.clone());
    }

    by_component
}

fn component_view_kind(component: &RouteComponent) -> RouteComponentViewKind {
    match component {
        RouteComponent::Waypoint { .. } => RouteComponentViewKind::Waypoint,
        RouteComponent::Airway { .. } => RouteComponentViewKind::Airway,
        RouteComponent::Procedure { .. } => RouteComponentViewKind::Procedure,
    }
}

fn component_summary(component: &RouteComponent) -> String {
    match component {
        RouteComponent::Waypoint { waypoint } => nav_ref_label(waypoint),
        RouteComponent::Airway { airway } => airway.name.clone(),
        RouteComponent::Procedure { procedure } => {
            let mut summary = procedure.procedure_id.clone();
            if let Some(transition) = procedure.enroute_transition.as_deref() {
                if !transition.is_empty() {
                    summary.push(' ');
                    summary.push_str(transition);
                }
            }
            if let Some(runway) = procedure.runway_transition.as_deref() {
                if !runway.is_empty() {
                    summary.push(' ');
                    summary.push_str(runway);
                }
            }
            summary
        }
    }
}

fn component_ui_items(component: &RouteComponent, grouped_legs: Vec<ResolvedLeg>) -> Vec<ConcretizedNavItem> {
    match component {
        RouteComponent::Waypoint { waypoint } => {
            vec![ConcretizedNavItem::Waypoint { nav_ref: waypoint.clone() }]
        }
        RouteComponent::Airway { airway } => {
            let mut items = Vec::new();
            let mut push_waypoint = |nav_ref: NavRef| {
                let duplicate = matches!(
                    items.last(),
                    Some(ConcretizedNavItem::Waypoint { nav_ref: existing }) if *existing == nav_ref
                );
                if !duplicate {
                    items.push(ConcretizedNavItem::Waypoint { nav_ref });
                }
            };

            push_waypoint(airway.entry.clone());
            if grouped_legs.is_empty() {
                push_waypoint(airway.exit.clone());
            } else {
                for leg in grouped_legs {
                    push_waypoint(leg.to.clone());
                }
            }
            items
        }
        RouteComponent::Procedure { procedure } => {
            let mut items = Vec::new();
            let mut push_waypoint = |nav_ref: NavRef| {
                let duplicate = matches!(
                    items.last(),
                    Some(ConcretizedNavItem::Waypoint { nav_ref: existing }) if *existing == nav_ref
                );
                if !duplicate {
                    items.push(ConcretizedNavItem::Waypoint { nav_ref });
                }
            };

            if let Some(first) = grouped_legs.first() {
                    push_waypoint(first.from.clone());
                for leg in grouped_legs {
                    push_waypoint(leg.to.clone());
                }
            }

            if let Some(discontinuity) = procedure.terminal_discontinuity.clone() {
                items.push(ConcretizedNavItem::Discontinuity {
                    label: discontinuity.display_label().to_string(),
                    discontinuity,
                });
            }

            items
        }
    }
}

fn intrinsic_component_legs(
    plan: &FlightPlan,
    grouped_by_component: &BTreeMap<usize, Vec<ResolvedLeg>>,
    component_index: usize,
) -> Vec<ResolvedLeg> {
    let Some(component) = plan.route_components.get(component_index) else {
        return Vec::new();
    };
    let mut legs = grouped_by_component
        .get(&component_index)
        .cloned()
        .unwrap_or_default();

    if matches!(component, RouteComponent::Waypoint { .. }) {
        return Vec::new();
    }

    let next_component_entry = plan
        .route_components
        .get(component_index + 1)
        .and_then(|next| component_entry(next, grouped_by_component.get(&(component_index + 1))));
    if next_component_entry
        .as_ref()
        .zip(legs.last())
        .is_some_and(|(next_entry, last_leg)| &last_leg.to == next_entry)
    {
        legs.pop();
    }

    legs
}

fn active_component_index_for_guidance(plan: &FlightPlan, guidance: &GuidanceState) -> Option<usize> {
    match guidance.sequencing_mode {
        SequencingMode::DirectTo => guidance
            .direct_to
            .as_ref()
            .and_then(|direct_to| direct_to.target_leg_id.as_deref())
            .and_then(|leg_id| leg_index_by_id(&plan.resolved_legs, leg_id))
            .and_then(|leg_index| match plan.resolved_legs.get(leg_index)?.source {
                ResolvedLegSource::RouteComponent { component_index } => Some(component_index),
                ResolvedLegSource::LegacyPlanLeg { .. } => None,
            }),
        SequencingMode::FollowPlan | SequencingMode::Suspended => match plan.resolved_legs.get(guidance.active_leg_index)?.source {
            ResolvedLegSource::RouteComponent { component_index } => Some(component_index),
            ResolvedLegSource::LegacyPlanLeg { .. } => None,
        },
    }
}

fn nav_ref_label(nav_ref: &NavRef) -> String {
    match nav_ref {
        NavRef::Airport(code) | NavRef::Navaid(code) | NavRef::Fix(code) => code.clone(),
        NavRef::LatLon(position) => format!("{:.4},{:.4}", position.lat, position.lon),
    }
}

fn rewrite_grouped_legs_source(legs: &[ResolvedLeg], component_index: usize) -> Vec<ResolvedLeg> {
    legs.iter()
        .cloned()
        .map(|mut leg| {
            leg.source = ResolvedLegSource::RouteComponent { component_index };
            leg
        })
        .collect()
}

fn bridge_between_components(
    left: Option<&RouteComponent>,
    left_grouped_legs: Option<&Vec<ResolvedLeg>>,
    right: Option<&RouteComponent>,
    right_grouped_legs: Option<&Vec<ResolvedLeg>>,
) -> Option<(NavRef, NavRef)> {
    let left_exit = component_exit(left?, left_grouped_legs)?;
    let right_entry = component_entry(right?, right_grouped_legs)?;
    Some((left_exit, right_entry))
}

fn component_entry(
    component: &RouteComponent,
    grouped_legs: Option<&Vec<ResolvedLeg>>,
) -> Option<NavRef> {
    match component {
        RouteComponent::Waypoint { waypoint } => Some(waypoint.clone()),
        RouteComponent::Airway { airway } => Some(airway.entry.clone()),
        RouteComponent::Procedure { .. } => {
            let legs = grouped_legs?;
            let first = legs.first()?;
            Some(first.from.clone())
        }
    }
}

fn component_exit(
    component: &RouteComponent,
    grouped_legs: Option<&Vec<ResolvedLeg>>,
) -> Option<NavRef> {
    match component {
        RouteComponent::Waypoint { waypoint } => Some(waypoint.clone()),
        RouteComponent::Airway { airway } => Some(airway.exit.clone()),
        RouteComponent::Procedure { procedure } => {
            if procedure.terminal_discontinuity.is_some() {
                return None;
            }

            let legs = grouped_legs?;
            let last = legs.last()?;
            Some(last.to.clone())
        }
    }
}

fn rebuild_after_component_edit(mut plan: FlightPlan) -> AppResult<FlightPlan> {
    let old_resolved = plan.resolved_legs.clone();
    let kept_non_waypoint_legs = old_resolved
        .into_iter()
        .filter_map(|mut leg| match leg.source {
            ResolvedLegSource::RouteComponent { component_index } => {
                if component_index >= plan.route_components.len() {
                    return None;
                }

                if plan.route_components[component_index].is_waypoint() {
                    return None;
                }

                leg.source = ResolvedLegSource::RouteComponent { component_index };
                Some(leg)
            }
            ResolvedLegSource::LegacyPlanLeg { .. } => None,
        })
        .collect::<Vec<_>>();

    let grouped_by_component = kept_non_waypoint_legs.into_iter().fold(
        BTreeMap::<usize, Vec<ResolvedLeg>>::new(),
        |mut by_component, leg| {
            let ResolvedLegSource::RouteComponent { component_index } = leg.source else {
                return by_component;
            };
            by_component.entry(component_index).or_default().push(leg);
            by_component
        },
    );
    let resolved = rebuild_resolved_legs_with_grouped_components(&plan.route_components, &grouped_by_component);

    plan.resolved_legs = resolved;
    plan.legs = legacy_legs_from_resolved_legs(&plan.resolved_legs);
    plan.guidance = revalidate_guidance_after_plan_edit(plan.guidance, &plan.resolved_legs);

    if plan.route_components.is_empty() || plan.resolved_legs.is_empty() {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "flight plan must contain at least one flyable leg after edit".to_string(),
        });
    }

    Ok(plan)
}

fn leg_index_by_id(resolved_legs: &[ResolvedLeg], leg_id: &str) -> Option<usize> {
    resolved_legs.iter().position(|leg| leg.id == leg_id)
}

fn revalidate_guidance_after_plan_edit(
    guidance: Option<GuidanceState>,
    resolved_legs: &[ResolvedLeg],
) -> Option<GuidanceState> {
    let mut guidance = guidance?;

    if guidance.active_leg_index >= resolved_legs.len() {
        guidance.active_leg_index = resolved_legs.len().saturating_sub(1);
    }

    if let Some(direct_to) = guidance.direct_to.as_mut() {
        let target_leg_index = direct_to
            .target_leg_id
            .as_deref()
            .and_then(|target_leg_id| leg_index_by_id(resolved_legs, target_leg_id))
            .filter(|index| resolved_legs[*index].to == direct_to.target);
        let resume_leg_index = direct_to
            .resume_leg_id
            .as_deref()
            .and_then(|resume_leg_id| leg_index_by_id(resolved_legs, resume_leg_id));

        if let Some(target_leg_index) = target_leg_index {
            guidance.active_leg_index = target_leg_index;
        } else {
            direct_to.target_leg_id = None;
            direct_to.resume_leg_id = None;
        }

        if resume_leg_index.is_none() {
            direct_to.resume_leg_id = None;
        }
    }

    Some(guidance)
}

fn should_suspend_after_active_leg(plan: &FlightPlan, active_leg_index: usize) -> bool {
    let Some(active_leg) = plan.resolved_legs.get(active_leg_index) else {
        return false;
    };
    let ResolvedLegSource::RouteComponent { component_index } = active_leg.source else {
        return false;
    };
    let Some(RouteComponent::Procedure { procedure }) = plan.route_components.get(component_index) else {
        return false;
    };
    if procedure.terminal_discontinuity.is_none() {
        return false;
    }

    let last_leg_for_component = plan
        .resolved_legs
        .iter()
        .enumerate()
        .filter_map(|(index, leg)| match leg.source {
            ResolvedLegSource::RouteComponent { component_index: leg_component_index }
                if leg_component_index == component_index =>
            {
                Some(index)
            }
            _ => None,
        })
        .max();

    last_leg_for_component == Some(active_leg_index)
}

fn resume_leg_id_after_leg(plan: &FlightPlan, leg_index: usize) -> Option<String> {
    if should_suspend_after_active_leg(plan, leg_index) {
        return None;
    }

    plan.resolved_legs.get(leg_index + 1).map(|leg| leg.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_airway_component_plan() -> FlightPlan {
        FlightPlan {
            id: "plan-1".to_string(),
            name: "Airway".to_string(),
            legs: Vec::new(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KBOS".to_string()),
                },
                RouteComponent::Airway {
                    airway: AirwaySegment {
                        name: "V16".to_string(),
                        branch_key: Some("V16-A".to_string()),
                        entry: NavRef::Fix("DODGR".to_string()),
                        exit: NavRef::Fix("PDZ".to_string()),
                    },
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KJFK".to_string()),
                },
            ],
            resolved_legs: vec![
                ResolvedLeg {
                    id: "airway-0".to_string(),
                    from: NavRef::Fix("DODGR".to_string()),
                    to: NavRef::Fix("LAHAB".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "airway-1".to_string(),
                    from: NavRef::Fix("LAHAB".to_string()),
                    to: NavRef::Fix("PDZ".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
            ],
            guidance: None,
            departure: Some(AirportId("KBOS".to_string())),
            destination: Some(AirportId("KJFK".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        }
    }

    #[test]
    fn interpret_path_termination_maps_known_codes() {
        assert_eq!(interpret_path_termination("IF"), PathTermination::InitialFix);
        assert_eq!(interpret_path_termination("TF"), PathTermination::TrackToFix);
        assert_eq!(interpret_path_termination("DF"), PathTermination::DirectToFix);
    }

    #[test]
    fn delete_waypoint_component_rejects_grouped_components() {
        let plan = sample_airway_component_plan();
        let err = delete_waypoint_component(&plan, 1).unwrap_err();
        assert_eq!(err.kind, AppErrorKind::UnsupportedOperation);
    }

    #[test]
    fn flatten_component_replaces_grouped_component_with_explicit_waypoints() {
        let plan = sample_airway_component_plan();
        let flattened = flatten_component_to_waypoints(
            &plan,
            1,
            vec![
                NavRef::Fix("DODGR".to_string()),
                NavRef::Fix("LAHAB".to_string()),
                NavRef::Fix("PDZ".to_string()),
            ],
        )
        .unwrap();

        assert_eq!(flattened.route_components.len(), 5);
        assert!(matches!(
            flattened.route_components[1],
            RouteComponent::Waypoint { .. }
        ));
        assert_eq!(flattened.legs.len(), 4);
    }

    fn sample_waypoint_only_plan() -> FlightPlan {
        FlightPlan {
            id: "plan-2".to_string(),
            name: "Waypoint only".to_string(),
            legs: Vec::new(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KRNT".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KUAO".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KHIO".to_string()),
                },
            ],
            resolved_legs: vec![
                ResolvedLeg {
                    id: "component-0-1".to_string(),
                    from: NavRef::Airport("KRNT".to_string()),
                    to: NavRef::Airport("KUAO".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 0 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "component-1-2".to_string(),
                    from: NavRef::Airport("KUAO".to_string()),
                    to: NavRef::Airport("KHIO".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
            ],
            guidance: None,
            departure: Some(AirportId("KRNT".to_string())),
            destination: Some(AirportId("KHIO".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        }
    }

    fn sample_guided_waypoint_plan() -> FlightPlan {
        FlightPlan {
            id: "plan-3".to_string(),
            name: "Guided".to_string(),
            legs: Vec::new(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KRNT".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Navaid("SEA".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Fix("OLM".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KUAO".to_string()),
                },
            ],
            resolved_legs: vec![
                ResolvedLeg {
                    id: "component-0-1".to_string(),
                    from: NavRef::Airport("KRNT".to_string()),
                    to: NavRef::Navaid("SEA".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 0 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "component-1-2".to_string(),
                    from: NavRef::Navaid("SEA".to_string()),
                    to: NavRef::Fix("OLM".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "component-2-3".to_string(),
                    from: NavRef::Fix("OLM".to_string()),
                    to: NavRef::Airport("KUAO".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 2 },
                    procedure_provenance: None,
                },
            ],
            guidance: Some(GuidanceState {
                active_leg_index: 0,
                sequencing_mode: SequencingMode::FollowPlan,
                direct_to: None,
                suspend_reason: None,
            }),
            departure: Some(AirportId("KRNT".to_string())),
            destination: Some(AirportId("KUAO".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        }
    }

    fn sample_duplicate_waypoint_plan() -> FlightPlan {
        FlightPlan {
            id: "plan-dup".to_string(),
            name: "Duplicate waypoint".to_string(),
            legs: Vec::new(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KAAA".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Fix("IAF".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Fix("PTURN".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Fix("IAF".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KBBB".to_string()),
                },
            ],
            resolved_legs: vec![
                ResolvedLeg {
                    id: "component-0-1".to_string(),
                    from: NavRef::Airport("KAAA".to_string()),
                    to: NavRef::Fix("IAF".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 0 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "component-1-2".to_string(),
                    from: NavRef::Fix("IAF".to_string()),
                    to: NavRef::Fix("PTURN".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "component-2-3".to_string(),
                    from: NavRef::Fix("PTURN".to_string()),
                    to: NavRef::Fix("IAF".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 2 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "component-3-4".to_string(),
                    from: NavRef::Fix("IAF".to_string()),
                    to: NavRef::Airport("KBBB".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 3 },
                    procedure_provenance: None,
                },
            ],
            guidance: Some(GuidanceState {
                active_leg_index: 0,
                sequencing_mode: SequencingMode::FollowPlan,
                direct_to: None,
                suspend_reason: None,
            }),
            departure: Some(AirportId("KAAA".to_string())),
            destination: Some(AirportId("KBBB".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        }
    }

    #[test]
    fn procedure_without_terminal_discontinuity_bridges_to_following_waypoint() {
        let components = vec![
            RouteComponent::Waypoint {
                waypoint: NavRef::Airport("KAAA".to_string()),
            },
            RouteComponent::Procedure {
                procedure: ProcedureSegment {
                    airport_id: AirportId("KAAA".to_string()),
                    procedure_id: "STAR1".to_string(),
                    kind: ProcedureKind::Star,
                    runway_transition: Some("RW10".to_string()),
                    enroute_transition: Some("FOO".to_string()),
                    terminal_discontinuity: None,
                },
            },
            RouteComponent::Waypoint {
                waypoint: NavRef::Airport("KBBB".to_string()),
            },
        ];
        let grouped = BTreeMap::from([(
            1usize,
            vec![ResolvedLeg {
                id: "proc-0".to_string(),
                from: NavRef::Fix("FOO".to_string()),
                to: NavRef::Fix("BAR".to_string()),
                source: ResolvedLegSource::RouteComponent { component_index: 1 },
                procedure_provenance: None,
            }],
        )]);

        let resolved = rebuild_resolved_legs_with_grouped_components(&components, &grouped);

        assert_eq!(resolved.len(), 3);
        assert_eq!(resolved[0].from, NavRef::Airport("KAAA".to_string()));
        assert_eq!(resolved[0].to, NavRef::Fix("FOO".to_string()));
        assert_eq!(resolved[2].from, NavRef::Fix("BAR".to_string()));
        assert_eq!(resolved[2].to, NavRef::Airport("KBBB".to_string()));
    }

    #[test]
    fn procedure_with_terminal_discontinuity_does_not_bridge_to_following_waypoint() {
        let components = vec![
            RouteComponent::Waypoint {
                waypoint: NavRef::Airport("KAAA".to_string()),
            },
            RouteComponent::Procedure {
                procedure: ProcedureSegment {
                    airport_id: AirportId("KAAA".to_string()),
                    procedure_id: "STAR1".to_string(),
                    kind: ProcedureKind::Star,
                    runway_transition: Some("RW10".to_string()),
                    enroute_transition: Some("FOO".to_string()),
                    terminal_discontinuity: Some(ProcedureDiscontinuity::Vectors),
                },
            },
            RouteComponent::Waypoint {
                waypoint: NavRef::Airport("KBBB".to_string()),
            },
        ];
        let grouped = BTreeMap::from([(
            1usize,
            vec![ResolvedLeg {
                id: "proc-0".to_string(),
                from: NavRef::Fix("FOO".to_string()),
                to: NavRef::Fix("BAR".to_string()),
                source: ResolvedLegSource::RouteComponent { component_index: 1 },
                procedure_provenance: None,
            }],
        )]);

        let resolved = rebuild_resolved_legs_with_grouped_components(&components, &grouped);

        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].from, NavRef::Airport("KAAA".to_string()));
        assert_eq!(resolved[0].to, NavRef::Fix("FOO".to_string()));
        assert_eq!(resolved[1].from, NavRef::Fix("FOO".to_string()));
        assert_eq!(resolved[1].to, NavRef::Fix("BAR".to_string()));
    }

    fn sample_inserted_airway() -> (AirwaySegment, Vec<ResolvedLeg>) {
        (
            AirwaySegment {
                name: "V2".to_string(),
                branch_key: Some("V2-A".to_string()),
                entry: NavRef::Navaid("SEA".to_string()),
                exit: NavRef::Fix("VAMPS".to_string()),
            },
            vec![
                ResolvedLeg {
                    id: "airway-V2-A-0".to_string(),
                    from: NavRef::Navaid("SEA".to_string()),
                    to: NavRef::Fix("SUMMA".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 99 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "airway-V2-A-1".to_string(),
                    from: NavRef::Fix("SUMMA".to_string()),
                    to: NavRef::Fix("VAMPS".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 99 },
                    procedure_provenance: None,
                },
            ],
        )
    }

    fn sample_inserted_procedure() -> (ProcedureSegment, Vec<ResolvedLeg>) {
        (
            ProcedureSegment {
                airport_id: AirportId("CYQG".to_string()),
                procedure_id: "AUTTO1".to_string(),
                kind: ProcedureKind::Sid,
                runway_transition: None,
                enroute_transition: Some("COLTS".to_string()),
                terminal_discontinuity: Some(ProcedureDiscontinuity::Vectors),
            },
            vec![
                ResolvedLeg {
                    id: "proc-autto1-0".to_string(),
                    from: NavRef::Fix("COLTS".to_string()),
                    to: NavRef::Fix("BOREK".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 99 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "proc-autto1-1".to_string(),
                    from: NavRef::Fix("BOREK".to_string()),
                    to: NavRef::Fix("AXXIS".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 99 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "proc-autto1-2".to_string(),
                    from: NavRef::Fix("AXXIS".to_string()),
                    to: NavRef::Fix("GIGGY".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 99 },
                    procedure_provenance: None,
                },
            ],
        )
    }

    fn sample_replaced_procedure() -> (ProcedureSegment, Vec<ResolvedLeg>) {
        (
            ProcedureSegment {
                airport_id: AirportId("CYQG".to_string()),
                procedure_id: "AUTTO1".to_string(),
                kind: ProcedureKind::Sid,
                runway_transition: None,
                enroute_transition: Some("PICUP".to_string()),
                terminal_discontinuity: Some(ProcedureDiscontinuity::Vectors),
            },
            vec![
                ResolvedLeg {
                    id: "proc-autto1-p0".to_string(),
                    from: NavRef::Fix("PICUP".to_string()),
                    to: NavRef::Fix("AXXIS".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 99 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "proc-autto1-p1".to_string(),
                    from: NavRef::Fix("AXXIS".to_string()),
                    to: NavRef::Fix("GIGGY".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 99 },
                    procedure_provenance: None,
                },
            ],
        )
    }

    fn sample_retargeted_airway() -> (AirwaySegment, Vec<ResolvedLeg>) {
        (
            AirwaySegment {
                name: "V2".to_string(),
                branch_key: Some("V2-A".to_string()),
                entry: NavRef::Fix("OLM".to_string()),
                exit: NavRef::Fix("BTG".to_string()),
            },
            vec![
                ResolvedLeg {
                    id: "airway-V2-A-r0".to_string(),
                    from: NavRef::Fix("OLM".to_string()),
                    to: NavRef::Fix("SUMMA".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 99 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "airway-V2-A-r1".to_string(),
                    from: NavRef::Fix("SUMMA".to_string()),
                    to: NavRef::Fix("BTG".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 99 },
                    procedure_provenance: None,
                },
            ],
        )
    }

    #[test]
    fn insert_airway_between_waypoints_keeps_airway_atomic_and_bridges_resolved_route() {
        let plan = sample_waypoint_only_plan();
        let (airway, airway_legs) = sample_inserted_airway();

        let inserted = insert_airway_between_waypoints(&plan, 0, 1, airway, airway_legs).unwrap();

        assert_eq!(inserted.route_components.len(), 4);
        assert!(matches!(
            inserted.route_components[1],
            RouteComponent::Airway { .. }
        ));
        assert_eq!(inserted.legs.len(), 5);
        assert_eq!(inserted.legs[0].from, NavRef::Airport("KRNT".to_string()));
        assert_eq!(inserted.legs[0].to, NavRef::Navaid("SEA".to_string()));
        assert_eq!(inserted.legs[3].from, NavRef::Fix("VAMPS".to_string()));
        assert_eq!(inserted.legs[3].to, NavRef::Airport("KUAO".to_string()));
        assert_eq!(inserted.legs[4].from, NavRef::Airport("KUAO".to_string()));
        assert_eq!(inserted.legs[4].to, NavRef::Airport("KHIO".to_string()));
    }

    #[test]
    fn insert_airway_between_waypoints_does_not_duplicate_matching_boundaries() {
        let mut plan = sample_waypoint_only_plan();
        plan.route_components[1] = RouteComponent::Waypoint {
            waypoint: NavRef::Fix("VAMPS".to_string()),
        };
        plan.resolved_legs[0].to = NavRef::Fix("VAMPS".to_string());
        let airway = AirwaySegment {
            name: "V2".to_string(),
            branch_key: Some("V2-A".to_string()),
            entry: NavRef::Airport("KRNT".to_string()),
            exit: NavRef::Fix("VAMPS".to_string()),
        };
        let airway_legs = vec![ResolvedLeg {
            id: "airway-V2-A-0".to_string(),
            from: NavRef::Airport("KRNT".to_string()),
            to: NavRef::Fix("VAMPS".to_string()),
            source: ResolvedLegSource::RouteComponent { component_index: 88 },
            procedure_provenance: None,
        }];

        let inserted = insert_airway_between_waypoints(&plan, 0, 1, airway, airway_legs).unwrap();

        assert_eq!(inserted.route_components.len(), 4);
        assert!(matches!(inserted.route_components[1], RouteComponent::Airway { .. }));
    }

    #[test]
    fn insert_airway_between_waypoints_rejects_grouped_components_inside_replaced_span() {
        let mut plan = sample_waypoint_only_plan();
        plan.route_components.insert(
            1,
            RouteComponent::Airway {
                airway: AirwaySegment {
                    name: "J1".to_string(),
                    branch_key: Some("J1-A".to_string()),
                    entry: NavRef::Fix("A".to_string()),
                    exit: NavRef::Fix("B".to_string()),
                },
            },
        );
        let (airway, airway_legs) = sample_inserted_airway();

        let err = insert_airway_between_waypoints(&plan, 0, 2, airway, airway_legs).unwrap_err();
        assert_eq!(err.kind, AppErrorKind::UnsupportedOperation);
    }

    #[test]
    fn insert_airway_between_waypoints_rejects_non_waypoint_boundaries() {
        let plan = sample_airway_component_plan();
        let (airway, airway_legs) = sample_inserted_airway();

        let err = insert_airway_between_waypoints(&plan, 0, 1, airway, airway_legs).unwrap_err();
        assert_eq!(err.kind, AppErrorKind::UnsupportedOperation);
    }

    #[test]
    fn insert_airway_between_waypoints_preserves_grouped_components_outside_span() {
        let mut plan = sample_waypoint_only_plan();
        plan.route_components.push(RouteComponent::Airway {
            airway: AirwaySegment {
                name: "J1".to_string(),
                branch_key: Some("J1-A".to_string()),
                entry: NavRef::Fix("FOO".to_string()),
                exit: NavRef::Fix("BAR".to_string()),
            },
        });
        plan.route_components.push(RouteComponent::Waypoint {
            waypoint: NavRef::Airport("KPDX".to_string()),
        });
        plan.resolved_legs.push(ResolvedLeg {
            id: "j1-0".to_string(),
            from: NavRef::Fix("FOO".to_string()),
            to: NavRef::Fix("BAR".to_string()),
            source: ResolvedLegSource::RouteComponent { component_index: 3 },
            procedure_provenance: None,
        });
        let (airway, airway_legs) = sample_inserted_airway();

        let inserted = insert_airway_between_waypoints(&plan, 0, 1, airway, airway_legs).unwrap();

        assert!(inserted.route_components.iter().any(|component| matches!(
            component,
            RouteComponent::Airway { airway } if airway.name == "J1"
        )));
        assert!(inserted.resolved_legs.iter().any(|leg| leg.id == "j1-0"));
    }

    #[test]
    fn insert_airway_between_waypoints_requires_increasing_span() {
        let plan = sample_waypoint_only_plan();
        let (airway, airway_legs) = sample_inserted_airway();

        let err = insert_airway_between_waypoints(&plan, 1, 1, airway, airway_legs).unwrap_err();
        assert_eq!(err.kind, AppErrorKind::UnsupportedOperation);
    }

    #[test]
    fn replace_airway_component_keeps_atomic_slot_and_updates_bridges() {
        let plan = insert_airway_between_waypoints(
            &sample_waypoint_only_plan(),
            0,
            1,
            sample_inserted_airway().0,
            sample_inserted_airway().1,
        )
        .unwrap();
        let (airway, airway_legs) = sample_retargeted_airway();

        let replaced = replace_airway_component(&plan, 1, airway, airway_legs).unwrap();

        assert_eq!(replaced.route_components.len(), 4);
        assert!(matches!(replaced.route_components[1], RouteComponent::Airway { .. }));
        assert_eq!(replaced.legs[0].from, NavRef::Airport("KRNT".to_string()));
        assert_eq!(replaced.legs[0].to, NavRef::Fix("OLM".to_string()));
        assert_eq!(replaced.legs[3].from, NavRef::Fix("BTG".to_string()));
        assert_eq!(replaced.legs[3].to, NavRef::Airport("KUAO".to_string()));
        assert!(replaced.guidance.is_none());
    }

    #[test]
    fn change_airway_entry_preserves_airway_identity_and_updates_entry_bridge() {
        let plan = insert_airway_between_waypoints(
            &sample_waypoint_only_plan(),
            0,
            1,
            sample_inserted_airway().0,
            sample_inserted_airway().1,
        )
        .unwrap();
        let (_, retargeted_legs) = sample_retargeted_airway();

        let changed = change_airway_entry(&plan, 1, NavRef::Fix("OLM".to_string()), retargeted_legs).unwrap();

        let RouteComponent::Airway { airway } = &changed.route_components[1] else {
            panic!("expected airway");
        };
        assert_eq!(airway.name, "V2");
        assert_eq!(airway.branch_key.as_deref(), Some("V2-A"));
        assert_eq!(airway.entry, NavRef::Fix("OLM".to_string()));
        assert_eq!(airway.exit, NavRef::Fix("VAMPS".to_string()));
        assert_eq!(changed.legs[0].to, NavRef::Fix("OLM".to_string()));
    }

    #[test]
    fn change_airway_exit_preserves_airway_identity_and_updates_exit_bridge() {
        let plan = insert_airway_between_waypoints(
            &sample_waypoint_only_plan(),
            0,
            1,
            sample_inserted_airway().0,
            sample_inserted_airway().1,
        )
        .unwrap();
        let airway_legs = vec![
            ResolvedLeg {
                id: "airway-V2-A-e0".to_string(),
                from: NavRef::Navaid("SEA".to_string()),
                to: NavRef::Fix("SUMMA".to_string()),
                source: ResolvedLegSource::RouteComponent { component_index: 99 },
                procedure_provenance: None,
            },
            ResolvedLeg {
                id: "airway-V2-A-e1".to_string(),
                from: NavRef::Fix("SUMMA".to_string()),
                to: NavRef::Fix("BTG".to_string()),
                source: ResolvedLegSource::RouteComponent { component_index: 99 },
                procedure_provenance: None,
            },
        ];

        let changed = change_airway_exit(&plan, 1, NavRef::Fix("BTG".to_string()), airway_legs).unwrap();

        let RouteComponent::Airway { airway } = &changed.route_components[1] else {
            panic!("expected airway");
        };
        assert_eq!(airway.name, "V2");
        assert_eq!(airway.branch_key.as_deref(), Some("V2-A"));
        assert_eq!(airway.entry, NavRef::Navaid("SEA".to_string()));
        assert_eq!(airway.exit, NavRef::Fix("BTG".to_string()));
        assert_eq!(changed.legs[3].from, NavRef::Fix("BTG".to_string()));
        assert_eq!(changed.legs[3].to, NavRef::Airport("KUAO".to_string()));
    }

    #[test]
    fn replace_airway_component_can_swap_airway_identity() {
        let plan = insert_airway_between_waypoints(
            &sample_waypoint_only_plan(),
            0,
            1,
            sample_inserted_airway().0,
            sample_inserted_airway().1,
        )
        .unwrap();
        let replacement = AirwaySegment {
            name: "J1".to_string(),
            branch_key: Some("J1-A".to_string()),
            entry: NavRef::Fix("SEA".to_string()),
            exit: NavRef::Fix("BTG".to_string()),
        };
        let replacement_legs = vec![ResolvedLeg {
            id: "j1-r0".to_string(),
            from: NavRef::Fix("SEA".to_string()),
            to: NavRef::Fix("BTG".to_string()),
            source: ResolvedLegSource::RouteComponent { component_index: 99 },
            procedure_provenance: None,
        }];

        let replaced = replace_airway_component(&plan, 1, replacement, replacement_legs).unwrap();

        let RouteComponent::Airway { airway } = &replaced.route_components[1] else {
            panic!("expected airway");
        };
        assert_eq!(airway.name, "J1");
        assert_eq!(airway.branch_key.as_deref(), Some("J1-A"));
    }

    #[test]
    fn replace_airway_component_rejects_non_airway_slot() {
        let (airway, airway_legs) = sample_inserted_airway();
        let err = replace_airway_component(&sample_waypoint_only_plan(), 0, airway, airway_legs).unwrap_err();
        assert_eq!(err.kind, AppErrorKind::UnsupportedOperation);
    }

    #[test]
    fn replace_airway_component_rejects_empty_leg_set() {
        let plan = insert_airway_between_waypoints(
            &sample_waypoint_only_plan(),
            0,
            1,
            sample_inserted_airway().0,
            sample_inserted_airway().1,
        )
        .unwrap();
        let err = replace_airway_component(
            &plan,
            1,
            AirwaySegment {
                name: "V2".to_string(),
                branch_key: Some("V2-A".to_string()),
                entry: NavRef::Fix("OLM".to_string()),
                exit: NavRef::Fix("BTG".to_string()),
            },
            Vec::new(),
        )
        .unwrap_err();
        assert_eq!(err.kind, AppErrorKind::InvalidFlightPlan);
    }

    #[test]
    fn insert_procedure_between_waypoints_keeps_procedure_atomic_and_blocks_outbound_bridge_on_gap() {
        let plan = sample_waypoint_only_plan();
        let (procedure, procedure_legs) = sample_inserted_procedure();

        let inserted =
            insert_procedure_between_waypoints(&plan, 0, 1, procedure, procedure_legs).unwrap();

        assert_eq!(inserted.route_components.len(), 4);
        assert!(matches!(
            inserted.route_components[1],
            RouteComponent::Procedure { .. }
        ));
        assert_eq!(inserted.legs.len(), 5);
        assert_eq!(inserted.legs[0].from, NavRef::Airport("KRNT".to_string()));
        assert_eq!(inserted.legs[0].to, NavRef::Fix("COLTS".to_string()));
        assert_eq!(inserted.legs[3].to, NavRef::Fix("GIGGY".to_string()));
        assert!(
            inserted
                .legs
                .iter()
                .all(|leg| leg.to != NavRef::Airport("KUAO".to_string()))
        );
    }

    #[test]
    fn insert_procedure_between_waypoints_rejects_grouped_components_inside_replaced_span() {
        let mut plan = sample_waypoint_only_plan();
        plan.route_components.insert(
            1,
            RouteComponent::Airway {
                airway: AirwaySegment {
                    name: "J1".to_string(),
                    branch_key: Some("J1-A".to_string()),
                    entry: NavRef::Fix("A".to_string()),
                    exit: NavRef::Fix("B".to_string()),
                },
            },
        );
        let (procedure, procedure_legs) = sample_inserted_procedure();

        let err =
            insert_procedure_between_waypoints(&plan, 0, 2, procedure, procedure_legs).unwrap_err();
        assert_eq!(err.kind, AppErrorKind::UnsupportedOperation);
    }

    #[test]
    fn replace_procedure_component_updates_entry_bridge_and_keeps_atomic_slot() {
        let inserted = insert_procedure_between_waypoints(
            &sample_waypoint_only_plan(),
            0,
            1,
            sample_inserted_procedure().0,
            sample_inserted_procedure().1,
        )
        .unwrap();
        let (replacement, replacement_legs) = sample_replaced_procedure();

        let replaced =
            replace_procedure_component(&inserted, 1, replacement, replacement_legs).unwrap();

        assert!(matches!(
            replaced.route_components[1],
            RouteComponent::Procedure { .. }
        ));
        assert_eq!(replaced.legs[0].to, NavRef::Fix("PICUP".to_string()));
        assert_eq!(replaced.legs[2].to, NavRef::Fix("GIGGY".to_string()));
        assert!(
            replaced
                .legs
                .iter()
                .all(|leg| leg.to != NavRef::Airport("KUAO".to_string()))
        );
        assert!(replaced.guidance.is_none());
    }

    #[test]
    fn change_procedure_enroute_transition_preserves_identity_and_updates_spec() {
        let inserted = insert_procedure_between_waypoints(
            &sample_waypoint_only_plan(),
            0,
            1,
            sample_inserted_procedure().0,
            sample_inserted_procedure().1,
        )
        .unwrap();
        let (_, replacement_legs) = sample_replaced_procedure();

        let changed = change_procedure_enroute_transition(
            &inserted,
            1,
            Some("PICUP".to_string()),
            replacement_legs,
        )
        .unwrap();

        let RouteComponent::Procedure { procedure } = &changed.route_components[1] else {
            panic!("expected procedure");
        };
        assert_eq!(procedure.procedure_id, "AUTTO1");
        assert_eq!(procedure.enroute_transition.as_deref(), Some("PICUP"));
        assert_eq!(changed.legs[0].to, NavRef::Fix("PICUP".to_string()));
    }

    #[test]
    fn change_procedure_runway_transition_preserves_identity_and_updates_spec() {
        let plan = FlightPlan {
            id: "plan-star".to_string(),
            name: "STAR".to_string(),
            legs: Vec::new(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Fix("ARD".to_string()),
                },
                RouteComponent::Procedure {
                    procedure: ProcedureSegment {
                        airport_id: AirportId("47N".to_string()),
                        procedure_id: "CENTR1".to_string(),
                        kind: ProcedureKind::Star,
                        runway_transition: Some("RW07".to_string()),
                        enroute_transition: None,
                        terminal_discontinuity: Some(ProcedureDiscontinuity::Vectors),
                    },
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("47N".to_string()),
                },
            ],
            resolved_legs: vec![
                ResolvedLeg {
                    id: "star-0".to_string(),
                    from: NavRef::Fix("ARD".to_string()),
                    to: NavRef::Fix("DYLIN".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "star-1".to_string(),
                    from: NavRef::Fix("DYLIN".to_string()),
                    to: NavRef::Fix("METRO".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
            ],
            guidance: None,
            departure: None,
            destination: Some(AirportId("47N".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };
        let replacement_legs = vec![
            ResolvedLeg {
                id: "star-rw25-0".to_string(),
                from: NavRef::Fix("ARD".to_string()),
                to: NavRef::Fix("DYLIN".to_string()),
                source: ResolvedLegSource::RouteComponent { component_index: 99 },
                procedure_provenance: None,
            },
            ResolvedLeg {
                id: "star-rw25-1".to_string(),
                from: NavRef::Fix("DYLIN".to_string()),
                to: NavRef::Fix("METRO".to_string()),
                source: ResolvedLegSource::RouteComponent { component_index: 99 },
                procedure_provenance: None,
            },
        ];

        let changed = change_procedure_runway_transition(
            &plan,
            1,
            Some("RW25".to_string()),
            replacement_legs,
        )
        .unwrap();

        let RouteComponent::Procedure { procedure } = &changed.route_components[1] else {
            panic!("expected procedure");
        };
        assert_eq!(procedure.procedure_id, "CENTR1");
        assert_eq!(procedure.runway_transition.as_deref(), Some("RW25"));
    }

    #[test]
    fn replace_procedure_component_rejects_non_procedure_slot() {
        let (procedure, procedure_legs) = sample_inserted_procedure();
        let err =
            replace_procedure_component(&sample_waypoint_only_plan(), 0, procedure, procedure_legs)
                .unwrap_err();
        assert_eq!(err.kind, AppErrorKind::UnsupportedOperation);
    }

    #[test]
    fn activate_direct_to_stashes_hidden_start_and_resume_leg() {
        let plan = sample_guided_waypoint_plan();

        let activated = activate_direct_to(
            &plan,
            LatLon {
                lat: 47.490,
                lon: -122.216,
            },
            NavRef::Fix("OLM".to_string()),
        )
        .unwrap();

        let guidance = activated.guidance.as_ref().unwrap();
        assert_eq!(guidance.sequencing_mode, SequencingMode::DirectTo);
        assert_eq!(guidance.active_leg_index, 1);
        let direct_to = guidance.direct_to.as_ref().unwrap();
        assert_eq!(
            direct_to.start,
            NavRef::LatLon(LatLon {
                lat: 47.490,
                lon: -122.216
            })
        );
        assert_eq!(direct_to.target, NavRef::Fix("OLM".to_string()));
        assert_eq!(direct_to.target_leg_id.as_deref(), Some("component-1-2"));
        assert_eq!(direct_to.resume_leg_id.as_deref(), Some("component-2-3"));

        let active_leg = active_guidance_leg(&activated).unwrap();
        assert_eq!(
            active_leg.from,
            NavRef::LatLon(LatLon {
                lat: 47.490,
                lon: -122.216
            })
        );
        assert_eq!(active_leg.to, NavRef::Fix("OLM".to_string()));
    }

    #[test]
    fn sequence_direct_to_resumes_underlying_plan_at_next_leg() {
        let activated = activate_direct_to(
            &sample_guided_waypoint_plan(),
            LatLon {
                lat: 47.490,
                lon: -122.216,
            },
            NavRef::Fix("OLM".to_string()),
        )
        .unwrap();

        let sequenced = sequence_active_leg(&activated).unwrap();

        let guidance = sequenced.guidance.as_ref().unwrap();
        assert_eq!(guidance.sequencing_mode, SequencingMode::FollowPlan);
        assert_eq!(guidance.active_leg_index, 2);
        assert!(guidance.direct_to.is_none());

        let active_leg = active_guidance_leg(&sequenced).unwrap();
        assert_eq!(active_leg.from, NavRef::Fix("OLM".to_string()));
        assert_eq!(active_leg.to, NavRef::Airport("KUAO".to_string()));
    }

    #[test]
    fn activate_leg_clears_direct_to_and_retargets_underlying_plan() {
        let activated = activate_direct_to(
            &sample_guided_waypoint_plan(),
            LatLon {
                lat: 47.490,
                lon: -122.216,
            },
            NavRef::Fix("OLM".to_string()),
        )
        .unwrap();

        let resumed = activate_leg(&activated, 2).unwrap();

        let guidance = resumed.guidance.as_ref().unwrap();
        assert_eq!(guidance.sequencing_mode, SequencingMode::FollowPlan);
        assert_eq!(guidance.active_leg_index, 2);
        assert!(guidance.direct_to.is_none());
    }

    #[test]
    fn off_plan_direct_to_suspends_after_hidden_leg_completes() {
        let activated = activate_direct_to(
            &sample_guided_waypoint_plan(),
            LatLon {
                lat: 47.490,
                lon: -122.216,
            },
            NavRef::Fix("BTG".to_string()),
        )
        .unwrap();

        let guidance = activated.guidance.as_ref().unwrap();
        assert_eq!(guidance.sequencing_mode, SequencingMode::DirectTo);
        assert_eq!(guidance.direct_to.as_ref().unwrap().target_leg_id, None);
        assert_eq!(guidance.direct_to.as_ref().unwrap().resume_leg_id, None);

        let sequenced = sequence_active_leg(&activated).unwrap();
        let guidance = sequenced.guidance.as_ref().unwrap();
        assert_eq!(guidance.sequencing_mode, SequencingMode::Suspended);
        assert!(guidance.direct_to.is_none());
        assert!(active_guidance_leg(&sequenced).is_none());
    }

    #[test]
    fn follow_plan_sequencing_advances_until_route_end_then_suspends() {
        let once = sequence_active_leg(&sample_guided_waypoint_plan()).unwrap();
        assert_eq!(once.guidance.as_ref().unwrap().active_leg_index, 1);
        assert_eq!(
            once.guidance.as_ref().unwrap().sequencing_mode,
            SequencingMode::FollowPlan
        );

        let twice = sequence_active_leg(&once).unwrap();
        assert_eq!(twice.guidance.as_ref().unwrap().active_leg_index, 2);
        assert_eq!(
            twice.guidance.as_ref().unwrap().sequencing_mode,
            SequencingMode::FollowPlan
        );

        let done = sequence_active_leg(&twice).unwrap();
        assert_eq!(done.guidance.as_ref().unwrap().active_leg_index, 2);
        assert_eq!(
            done.guidance.as_ref().unwrap().sequencing_mode,
            SequencingMode::Suspended
        );
    }

    #[test]
    fn sequencing_suspends_at_terminal_discontinuous_procedure_boundary() {
        let inserted = insert_procedure_between_waypoints(
            &sample_waypoint_only_plan(),
            0,
            1,
            sample_inserted_procedure().0,
            sample_inserted_procedure().1,
        )
        .unwrap();
        let plan = FlightPlan {
            guidance: Some(GuidanceState {
                active_leg_index: 3,
                sequencing_mode: SequencingMode::FollowPlan,
                direct_to: None,
                suspend_reason: None,
            }),
            ..inserted
        };

        let sequenced = sequence_active_leg(&plan).unwrap();

        let guidance = sequenced.guidance.as_ref().unwrap();
        assert_eq!(guidance.active_leg_index, 3);
        assert_eq!(guidance.sequencing_mode, SequencingMode::Suspended);
    }

    #[test]
    fn sequencing_continues_through_non_discontinuous_grouped_procedure() {
        let plan = FlightPlan {
            id: "plan-proc-seq".to_string(),
            name: "Procedure sequencing".to_string(),
            legs: Vec::new(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KAAA".to_string()),
                },
                RouteComponent::Procedure {
                    procedure: ProcedureSegment {
                        airport_id: AirportId("KAAA".to_string()),
                        procedure_id: "STAR1".to_string(),
                        kind: ProcedureKind::Star,
                        runway_transition: Some("RW10".to_string()),
                        enroute_transition: Some("FOO".to_string()),
                        terminal_discontinuity: None,
                    },
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KBBB".to_string()),
                },
            ],
            resolved_legs: vec![
                ResolvedLeg {
                    id: "component-0-1".to_string(),
                    from: NavRef::Airport("KAAA".to_string()),
                    to: NavRef::Fix("FOO".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 0 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "proc-0".to_string(),
                    from: NavRef::Fix("FOO".to_string()),
                    to: NavRef::Fix("BAR".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "component-1-2".to_string(),
                    from: NavRef::Fix("BAR".to_string()),
                    to: NavRef::Airport("KBBB".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
            ],
            guidance: Some(GuidanceState {
                active_leg_index: 1,
                sequencing_mode: SequencingMode::FollowPlan,
                direct_to: None,
                suspend_reason: None,
            }),
            departure: Some(AirportId("KAAA".to_string())),
            destination: Some(AirportId("KBBB".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };

        let sequenced = sequence_active_leg(&plan).unwrap();

        let guidance = sequenced.guidance.as_ref().unwrap();
        assert_eq!(guidance.active_leg_index, 2);
        assert_eq!(guidance.sequencing_mode, SequencingMode::FollowPlan);
    }

    #[test]
    fn suspend_sequencing_forces_suspended_mode_and_clears_direct_to() {
        let activated = activate_direct_to(
            &sample_guided_waypoint_plan(),
            LatLon {
                lat: 47.49,
                lon: -122.216,
            },
            NavRef::Fix("OLM".to_string()),
        )
        .unwrap();

        let suspended = suspend_sequencing(&activated).unwrap();

        let guidance = suspended.guidance.as_ref().unwrap();
        assert_eq!(guidance.active_leg_index, 1);
        assert_eq!(guidance.sequencing_mode, SequencingMode::Suspended);
        assert!(guidance.direct_to.is_none());
        let active_leg = active_guidance_leg(&suspended).unwrap();
        assert_eq!(active_leg.from, NavRef::Navaid("SEA".to_string()));
        assert_eq!(active_leg.to, NavRef::Fix("OLM".to_string()));
    }

    #[test]
    fn unsuspend_sequencing_restores_follow_plan_when_not_at_boundary() {
        let suspended = suspend_sequencing(&sample_guided_waypoint_plan()).unwrap();

        let unsuspended = unsuspend_sequencing(&suspended).unwrap();

        let guidance = unsuspended.guidance.as_ref().unwrap();
        assert_eq!(guidance.active_leg_index, 0);
        assert_eq!(guidance.sequencing_mode, SequencingMode::FollowPlan);
    }

    #[test]
    fn unsuspend_sequencing_at_terminal_discontinuous_boundary_activates_next_leg() {
        let inserted = insert_procedure_between_waypoints(
            &sample_waypoint_only_plan(),
            0,
            1,
            sample_inserted_procedure().0,
            sample_inserted_procedure().1,
        )
        .unwrap();
        let suspended = FlightPlan {
            guidance: Some(GuidanceState {
                active_leg_index: 3,
                sequencing_mode: SequencingMode::Suspended,
                direct_to: None,
                suspend_reason: Some(SuspendReason::Boundary),
            }),
            ..inserted
        };

        let unsuspended = unsuspend_sequencing(&suspended).unwrap();

        let guidance = unsuspended.guidance.as_ref().unwrap();
        assert_eq!(guidance.active_leg_index, 4);
        assert_eq!(guidance.sequencing_mode, SequencingMode::FollowPlan);
    }

    #[test]
    fn activate_next_leg_crosses_suspend_boundary_explicitly() {
        let inserted = insert_procedure_between_waypoints(
            &sample_waypoint_only_plan(),
            0,
            1,
            sample_inserted_procedure().0,
            sample_inserted_procedure().1,
        )
        .unwrap();
        let suspended = FlightPlan {
            guidance: Some(GuidanceState {
                active_leg_index: 3,
                sequencing_mode: SequencingMode::Suspended,
                direct_to: None,
                suspend_reason: Some(SuspendReason::Boundary),
            }),
            ..inserted
        };

        let next = activate_next_leg(&suspended).unwrap();

        let guidance = next.guidance.as_ref().unwrap();
        assert_eq!(guidance.active_leg_index, 4);
        assert_eq!(guidance.sequencing_mode, SequencingMode::FollowPlan);
    }

    #[test]
    fn unsuspend_sequencing_rejects_non_suspended_mode() {
        let err = unsuspend_sequencing(&sample_guided_waypoint_plan()).unwrap_err();
        assert_eq!(err.kind, AppErrorKind::UnsupportedOperation);
    }

    #[test]
    fn activate_next_leg_rejects_end_of_route() {
        let plan = FlightPlan {
            guidance: Some(GuidanceState {
                active_leg_index: 2,
                sequencing_mode: SequencingMode::FollowPlan,
                direct_to: None,
                suspend_reason: None,
            }),
            ..sample_guided_waypoint_plan()
        };

        let err = activate_next_leg(&plan).unwrap_err();
        assert_eq!(err.kind, AppErrorKind::UnsupportedOperation);
    }

    #[test]
    fn activate_direct_to_leg_can_target_specific_duplicate_waypoint_occurrence() {
        let activated = activate_direct_to_leg(
            &sample_duplicate_waypoint_plan(),
            LatLon {
                lat: 42.0,
                lon: -71.0,
            },
            "component-2-3",
        )
        .unwrap();

        let guidance = activated.guidance.as_ref().unwrap();
        let direct_to = guidance.direct_to.as_ref().unwrap();
        assert_eq!(guidance.active_leg_index, 2);
        assert_eq!(direct_to.target, NavRef::Fix("IAF".to_string()));
        assert_eq!(direct_to.target_leg_id.as_deref(), Some("component-2-3"));
        assert_eq!(direct_to.resume_leg_id.as_deref(), Some("component-3-4"));
    }

    #[test]
    fn plan_edit_that_removes_targeted_occurrence_degrades_direct_to_to_off_plan() {
        let activated = activate_direct_to_leg(
            &sample_duplicate_waypoint_plan(),
            LatLon {
                lat: 42.0,
                lon: -71.0,
            },
            "component-2-3",
        )
        .unwrap();

        let edited = delete_waypoint_component(&activated, 2).unwrap();

        let guidance = edited.guidance.as_ref().unwrap();
        let direct_to = guidance.direct_to.as_ref().unwrap();
        assert_eq!(guidance.sequencing_mode, SequencingMode::DirectTo);
        assert_eq!(direct_to.target, NavRef::Fix("IAF".to_string()));
        assert!(direct_to.target_leg_id.is_none());
        assert!(direct_to.resume_leg_id.is_none());
    }

    #[test]
    fn project_ui_state_expands_atomic_airway_component_without_exposing_bridge_legs() {
        let inserted = insert_airway_between_waypoints(
            &sample_waypoint_only_plan(),
            0,
            1,
            sample_inserted_airway().0,
            sample_inserted_airway().1,
        )
        .unwrap();

        let ui = project_ui_state(&inserted);

        assert_eq!(ui.components.len(), 4);
        assert_eq!(ui.components[1].kind, RouteComponentViewKind::Airway);
        assert_eq!(ui.components[1].summary, "V2");
        assert_eq!(
            ui.components[1].items,
            vec![
                ConcretizedNavItem::Waypoint {
                    nav_ref: NavRef::Navaid("SEA".to_string())
                },
                ConcretizedNavItem::Waypoint {
                    nav_ref: NavRef::Fix("SUMMA".to_string())
                },
                ConcretizedNavItem::Waypoint {
                    nav_ref: NavRef::Fix("VAMPS".to_string())
                },
            ]
        );
        assert_eq!(ui.resolved_legs[0].from, NavRef::Airport("KRNT".to_string()));
        assert_eq!(ui.resolved_legs[0].to, NavRef::Navaid("SEA".to_string()));
    }

    #[test]
    fn project_ui_state_marks_active_procedure_component_and_discontinuity() {
        let inserted = insert_procedure_between_waypoints(
            &sample_waypoint_only_plan(),
            0,
            1,
            sample_inserted_procedure().0,
            sample_inserted_procedure().1,
        )
        .unwrap();
        let guided = FlightPlan {
            guidance: Some(GuidanceState {
                active_leg_index: 2,
                sequencing_mode: SequencingMode::FollowPlan,
                direct_to: None,
                suspend_reason: None,
            }),
            ..inserted
        };

        let ui = project_ui_state(&guided);

        assert!(ui.components[1].active);
        assert_eq!(ui.components[1].kind, RouteComponentViewKind::Procedure);
        assert!(matches!(
            ui.components[1].items.last(),
            Some(ConcretizedNavItem::Discontinuity {
                discontinuity: ProcedureDiscontinuity::Vectors,
                ..
            })
        ));
        assert_eq!(ui.guidance.as_ref().unwrap().active_component_index, Some(1));
        assert_eq!(ui.guidance.as_ref().unwrap().active_leg_index, Some(2));
    }

    #[test]
    fn project_ui_state_exposes_direct_to_without_ui_recomputing_on_plan_status() {
        let activated = activate_direct_to_leg(
            &sample_duplicate_waypoint_plan(),
            LatLon {
                lat: 42.0,
                lon: -71.0,
            },
            "component-2-3",
        )
        .unwrap();

        let ui = project_ui_state(&activated);

        let guidance = ui.guidance.as_ref().unwrap();
        assert_eq!(guidance.sequencing_mode, SequencingMode::DirectTo);
        assert_eq!(guidance.active_component_index, Some(2));
        let direct_to = guidance.direct_to.as_ref().unwrap();
        assert!(direct_to.on_plan_target);
        assert_eq!(direct_to.target, NavRef::Fix("IAF".to_string()));
        assert_eq!(direct_to.target_leg_id.as_deref(), Some("component-2-3"));
    }

    #[test]
    fn direct_to_last_leg_of_terminal_discontinuous_procedure_has_no_resume_leg() {
        let inserted = insert_procedure_between_waypoints(
            &sample_waypoint_only_plan(),
            0,
            1,
            sample_inserted_procedure().0,
            sample_inserted_procedure().1,
        )
        .unwrap();

        let activated = activate_direct_to_leg(
            &inserted,
            LatLon {
                lat: 42.0,
                lon: -71.0,
            },
            "proc-autto1-2",
        )
        .unwrap();

        let guidance = activated.guidance.as_ref().unwrap();
        let direct_to = guidance.direct_to.as_ref().unwrap();
        assert_eq!(direct_to.target_leg_id.as_deref(), Some("proc-autto1-2"));
        assert!(direct_to.resume_leg_id.is_none());

        let sequenced = sequence_active_leg(&activated).unwrap();
        let guidance = sequenced.guidance.as_ref().unwrap();
        assert_eq!(guidance.sequencing_mode, SequencingMode::Suspended);
        assert!(guidance.direct_to.is_none());
    }

    #[test]
    fn direct_to_interior_leg_of_terminal_discontinuous_procedure_resumes_within_procedure() {
        let inserted = insert_procedure_between_waypoints(
            &sample_waypoint_only_plan(),
            0,
            1,
            sample_inserted_procedure().0,
            sample_inserted_procedure().1,
        )
        .unwrap();

        let activated = activate_direct_to_leg(
            &inserted,
            LatLon {
                lat: 42.0,
                lon: -71.0,
            },
            "proc-autto1-1",
        )
        .unwrap();

        let guidance = activated.guidance.as_ref().unwrap();
        let direct_to = guidance.direct_to.as_ref().unwrap();
        assert_eq!(direct_to.target_leg_id.as_deref(), Some("proc-autto1-1"));
        assert_eq!(direct_to.resume_leg_id.as_deref(), Some("proc-autto1-2"));

        let sequenced = sequence_active_leg(&activated).unwrap();
        let guidance = sequenced.guidance.as_ref().unwrap();
        assert_eq!(guidance.sequencing_mode, SequencingMode::FollowPlan);
        assert_eq!(guidance.active_leg_index, 3);
    }
}
