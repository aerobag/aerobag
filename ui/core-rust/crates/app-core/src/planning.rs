use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedLeg {
    pub id: String,
    pub from: NavRef,
    pub to: NavRef,
    pub source: ResolvedLegSource,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResolvedLegSource {
    LegacyPlanLeg { leg_index: usize },
    RouteComponent { component_index: usize },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuidanceState {
    pub active_leg_index: usize,
    pub sequencing_mode: SequencingMode,
    pub direct_to: Option<DirectToState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SequencingMode {
    FollowPlan,
    Suspended,
    DirectTo,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DirectToState {
    pub target: NavRef,
    pub resume_leg_index: Option<usize>,
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
        });
    }

    legs
}

fn resolved_legs_from_legacy_legs(legs: &[PlanLeg]) -> Vec<ResolvedLeg> {
    legs.iter()
        .enumerate()
        .map(|(index, leg)| ResolvedLeg {
            id: format!("legacy-{index}"),
            from: leg.from.clone(),
            to: leg.to.clone(),
            source: ResolvedLegSource::LegacyPlanLeg { leg_index: index },
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

    let mut resolved = resolved_legs_from_waypoint_components(&plan.route_components);
    resolved.extend(kept_non_waypoint_legs);
    resolved.sort_by(|left, right| left.id.cmp(&right.id));

    plan.resolved_legs = resolved;
    plan.legs = legacy_legs_from_resolved_legs(&plan.resolved_legs);

    if plan.route_components.is_empty() || plan.resolved_legs.is_empty() {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "flight plan must contain at least one flyable leg after edit".to_string(),
        });
    }

    Ok(plan)
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
                },
                ResolvedLeg {
                    id: "airway-1".to_string(),
                    from: NavRef::Fix("LAHAB".to_string()),
                    to: NavRef::Fix("PDZ".to_string()),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
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
}
