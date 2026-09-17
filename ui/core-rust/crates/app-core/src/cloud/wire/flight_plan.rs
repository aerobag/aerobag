// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Flight-plan payload schema 4. No runtime planning types occur in the wire
//! graph. The resolved route is retained for offline crossfill; runtime-only
//! guidance and debug geometry are deliberately not synchronized.

use super::wire_enum;
use crate::planning as runtime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct StoredFlightPlan {
    pub id: String,
    pub name: String,
    pub route_components: Vec<RouteComponent>,
    pub route_component_uids: Vec<String>,
    pub route_component_uid_counter: u64,
    pub resolved_legs: Vec<ResolvedLeg>,
    pub departure: Option<String>,
    pub destination: Option<String>,
    pub alternate: Option<String>,
    pub aircraft: Option<product_contracts::AircraftSelection>,
    pub cruise_altitude_ft: Option<i32>,
    pub planned_departure_time_epoch_ms: Option<i64>,
    pub notes: Option<String>,
    pub updated_at_epoch_ms: i64,
    pub version: u64,
}

impl From<runtime::FlightPlan> for StoredFlightPlan {
    fn from(value: runtime::FlightPlan) -> Self {
        let runtime::FlightPlan {
            id,
            name,
            route_components,
            route_component_uids,
            route_component_uid_counter,
            resolved_legs,
            guidance: _,
            departure,
            destination,
            alternate,
            aircraft,
            cruise_altitude_ft,
            planned_departure_time_epoch_ms,
            notes,
            updated_at_epoch_ms,
            version,
        } = value;
        Self {
            id,
            name,
            route_components: map(route_components),
            route_component_uids,
            route_component_uid_counter,
            resolved_legs: map(resolved_legs),
            departure: departure.map(|v| v.0),
            destination: destination.map(|v| v.0),
            alternate: alternate.map(|v| v.0),
            aircraft,
            cruise_altitude_ft,
            planned_departure_time_epoch_ms,
            notes,
            updated_at_epoch_ms,
            version,
        }
    }
}
impl TryFrom<StoredFlightPlan> for runtime::FlightPlan {
    type Error = crate::AppError;
    fn try_from(value: StoredFlightPlan) -> Result<Self, Self::Error> {
        let StoredFlightPlan {
            id,
            name,
            route_components,
            route_component_uids,
            route_component_uid_counter,
            resolved_legs,
            departure,
            destination,
            alternate,
            aircraft,
            cruise_altitude_ft,
            planned_departure_time_epoch_ms,
            notes,
            updated_at_epoch_ms,
            version,
        } = value;
        crate::build_flight_plan(Self {
            id,
            name,
            route_components: map(route_components),
            route_component_uids,
            route_component_uid_counter,
            resolved_legs: map(resolved_legs),
            guidance: None,
            departure: departure.map(crate::AirportId),
            destination: destination.map(crate::AirportId),
            alternate: alternate.map(crate::AirportId),
            aircraft,
            cruise_altitude_ft,
            planned_departure_time_epoch_ms,
            notes,
            updated_at_epoch_ms,
            version,
        })
    }
}

fn map<A, B: From<A>>(values: Vec<A>) -> Vec<B> {
    values.into_iter().map(Into::into).collect()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum RouteComponent {
    Waypoint { waypoint: NavRef },
    Airway { airway: Airway },
    Procedure { procedure: Procedure },
}
impl From<runtime::RouteComponent> for RouteComponent {
    fn from(value: runtime::RouteComponent) -> Self {
        match value {
            runtime::RouteComponent::Waypoint { waypoint } => Self::Waypoint {
                waypoint: waypoint.into(),
            },
            runtime::RouteComponent::Airway { airway } => Self::Airway {
                airway: airway.into(),
            },
            runtime::RouteComponent::Procedure { procedure } => Self::Procedure {
                procedure: procedure.into(),
            },
        }
    }
}
impl From<RouteComponent> for runtime::RouteComponent {
    fn from(value: RouteComponent) -> Self {
        match value {
            RouteComponent::Waypoint { waypoint } => Self::Waypoint {
                waypoint: waypoint.into(),
            },
            RouteComponent::Airway { airway } => Self::Airway {
                airway: airway.into(),
            },
            RouteComponent::Procedure { procedure } => Self::Procedure {
                procedure: procedure.into(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct Airway {
    pub name: String,
    pub branch_key: Option<String>,
    pub entry: String,
    pub exit: String,
}
impl From<runtime::AirwaySegment<runtime::FlightPlanWaypointId>> for Airway {
    fn from(value: runtime::AirwaySegment<runtime::FlightPlanWaypointId>) -> Self {
        let runtime::AirwaySegment {
            name,
            branch_key,
            entry,
            exit,
        } = value;
        Self {
            name,
            branch_key,
            entry: entry.0,
            exit: exit.0,
        }
    }
}
impl From<Airway> for runtime::AirwaySegment<runtime::FlightPlanWaypointId> {
    fn from(value: Airway) -> Self {
        let Airway {
            name,
            branch_key,
            entry,
            exit,
        } = value;
        Self {
            name,
            branch_key,
            entry: runtime::FlightPlanWaypointId(entry),
            exit: runtime::FlightPlanWaypointId(exit),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct Position {
    pub lat: f64,
    pub lon: f64,
}
impl From<crate::LatLon> for Position {
    fn from(value: crate::LatLon) -> Self {
        let crate::LatLon { lat, lon } = value;
        Self { lat, lon }
    }
}
impl From<Position> for crate::LatLon {
    fn from(value: Position) -> Self {
        Self {
            lat: value.lat,
            lon: value.lon,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) enum NavRef {
    Airport(String),
    Navaid(String),
    Fix(String),
    LatLon(Position),
    Spot(Position),
    ArincNavaid {
        identifier: String,
        icao_code: String,
        section_code: String,
        subsection_code: String,
    },
    TerminalNavaid {
        airport_id: String,
        identifier: String,
        icao_code: String,
        section_code: String,
        subsection_code: String,
    },
}
impl From<runtime::NavRef> for NavRef {
    fn from(value: runtime::NavRef) -> Self {
        use runtime::NavRef as R;
        match value {
            R::Airport(v) => Self::Airport(v),
            R::Navaid(v) => Self::Navaid(v),
            R::Fix(v) => Self::Fix(v),
            R::LatLon(v) => Self::LatLon(v.into()),
            R::Spot(v) => Self::Spot(v.into()),
            R::ArincNavaid {
                identifier,
                icao_code,
                section_code,
                subsection_code,
            } => Self::ArincNavaid {
                identifier,
                icao_code,
                section_code,
                subsection_code,
            },
            R::TerminalNavaid {
                airport_id,
                identifier,
                icao_code,
                section_code,
                subsection_code,
            } => Self::TerminalNavaid {
                airport_id,
                identifier,
                icao_code,
                section_code,
                subsection_code,
            },
        }
    }
}
impl From<NavRef> for runtime::NavRef {
    fn from(value: NavRef) -> Self {
        match value {
            NavRef::Airport(v) => Self::Airport(v),
            NavRef::Navaid(v) => Self::Navaid(v),
            NavRef::Fix(v) => Self::Fix(v),
            NavRef::LatLon(v) => Self::LatLon(v.into()),
            NavRef::Spot(v) => Self::Spot(v.into()),
            NavRef::ArincNavaid {
                identifier,
                icao_code,
                section_code,
                subsection_code,
            } => Self::ArincNavaid {
                identifier,
                icao_code,
                section_code,
                subsection_code,
            },
            NavRef::TerminalNavaid {
                airport_id,
                identifier,
                icao_code,
                section_code,
                subsection_code,
            } => Self::TerminalNavaid {
                airport_id,
                identifier,
                icao_code,
                section_code,
                subsection_code,
            },
        }
    }
}

wire_enum! { #[serde(rename_all = "snake_case")] ProcedureKind => runtime::ProcedureKind { Sid, Star, Approach } }
wire_enum! { #[serde(rename_all = "snake_case")] ProcedureRole => runtime::ProcedureSegmentRole { EnrouteTransition, Common, RunwayTransition } }
wire_enum! { #[serde(rename_all = "snake_case")] PathStyle => runtime::LegDisplayPathStyle { Solid, Dashed, Vectors } }

// Enums with an explicit opaque source-code variant. Conversion is exhaustive
// in both directions, just like the unit-variant wire enums above.
macro_rules! source_enum {
    ($name:ident => $runtime:path { $($variant:ident),+ }) => {
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        #[cfg_attr(test, derive(schemars::JsonSchema))]
        #[serde(rename_all = "snake_case")]
        pub(crate) enum $name { $($variant),+, Other(String) }
        impl From<$runtime> for $name {
            fn from(value: $runtime) -> Self {
                use $runtime as R;
                match value { $(R::$variant => Self::$variant),+, R::Other(v) => Self::Other(v) }
            }
        }
        impl From<$name> for $runtime {
            fn from(value: $name) -> Self {
                match value { $($name::$variant => Self::$variant),+, $name::Other(v) => Self::Other(v) }
            }
        }
    };
}
source_enum! { Discontinuity => runtime::ProcedureDiscontinuity { Vectors, Hold } }
source_enum! { Termination => runtime::PathTermination { InitialFix, TrackToFix, CourseToFix, DirectToFix, HeadingToManual, HeadingToAltitude } }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct Procedure {
    pub airport_id: String,
    pub procedure_id: String,
    pub display_label: Option<String>,
    pub kind: ProcedureKind,
    pub runway_transition: Option<String>,
    pub enroute_transition: Option<String>,
    pub terminal_discontinuity: Option<Discontinuity>,
    pub data_quality: Vec<String>,
}
impl From<runtime::ProcedureSegment> for Procedure {
    fn from(value: runtime::ProcedureSegment) -> Self {
        let runtime::ProcedureSegment {
            airport_id,
            procedure_id,
            display_label,
            kind,
            runway_transition,
            enroute_transition,
            terminal_discontinuity,
            data_quality,
        } = value;
        Self {
            airport_id: airport_id.0,
            procedure_id,
            display_label,
            kind: kind.into(),
            runway_transition,
            enroute_transition,
            terminal_discontinuity: terminal_discontinuity.map(Into::into),
            data_quality,
        }
    }
}
impl From<Procedure> for runtime::ProcedureSegment {
    fn from(value: Procedure) -> Self {
        let Procedure {
            airport_id,
            procedure_id,
            display_label,
            kind,
            runway_transition,
            enroute_transition,
            terminal_discontinuity,
            data_quality,
        } = value;
        Self {
            airport_id: crate::AirportId(airport_id),
            procedure_id,
            display_label,
            kind: kind.into(),
            runway_transition,
            enroute_transition,
            terminal_discontinuity: terminal_discontinuity.map(Into::into),
            data_quality,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct ResolvedLeg {
    pub id: String,
    pub from: NavRef,
    pub to: NavRef,
    pub source: LegSource,
    pub procedure_provenance: Option<Provenance>,
}
impl From<runtime::ResolvedLeg> for ResolvedLeg {
    fn from(value: runtime::ResolvedLeg) -> Self {
        let runtime::ResolvedLeg {
            id,
            from,
            to,
            source,
            procedure_provenance,
        } = value;
        Self {
            id,
            from: from.into(),
            to: to.into(),
            source: source.into(),
            procedure_provenance: procedure_provenance.map(Into::into),
        }
    }
}
impl From<ResolvedLeg> for runtime::ResolvedLeg {
    fn from(value: ResolvedLeg) -> Self {
        let ResolvedLeg {
            id,
            from,
            to,
            source,
            procedure_provenance,
        } = value;
        Self {
            id,
            from: from.into(),
            to: to.into(),
            source: source.into(),
            procedure_provenance: procedure_provenance.map(Into::into),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum LegSource {
    RouteComponent {
        component_index: usize,
    },
    SyntheticBridge {
        from_component_index: usize,
        to_component_index: usize,
    },
}
impl From<runtime::ResolvedLegSource> for LegSource {
    fn from(value: runtime::ResolvedLegSource) -> Self {
        match value {
            runtime::ResolvedLegSource::RouteComponent { component_index } => {
                Self::RouteComponent { component_index }
            }
            runtime::ResolvedLegSource::SyntheticBridge {
                from_component_index,
                to_component_index,
            } => Self::SyntheticBridge {
                from_component_index,
                to_component_index,
            },
        }
    }
}
impl From<LegSource> for runtime::ResolvedLegSource {
    fn from(value: LegSource) -> Self {
        match value {
            LegSource::RouteComponent { component_index } => {
                Self::RouteComponent { component_index }
            }
            LegSource::SyntheticBridge {
                from_component_index,
                to_component_index,
            } => Self::SyntheticBridge {
                from_component_index,
                to_component_index,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct Provenance {
    pub airport_id: String,
    pub procedure_id: String,
    pub kind: ProcedureKind,
    pub role: ProcedureRole,
    pub path_termination: Termination,
    pub leg_sequence: i32,
    pub discontinuity_after: Option<Discontinuity>,
    pub display_path: Option<DisplayPath>,
}
impl From<runtime::ProcedureLegProvenance> for Provenance {
    fn from(value: runtime::ProcedureLegProvenance) -> Self {
        let runtime::ProcedureLegProvenance {
            airport_id,
            procedure_id,
            kind,
            role,
            path_termination,
            leg_sequence,
            discontinuity_after,
            display_path,
        } = value;
        Self {
            airport_id,
            procedure_id,
            kind: kind.into(),
            role: role.into(),
            path_termination: path_termination.into(),
            leg_sequence,
            discontinuity_after: discontinuity_after.map(Into::into),
            display_path: display_path.map(Into::into),
        }
    }
}
impl From<Provenance> for runtime::ProcedureLegProvenance {
    fn from(value: Provenance) -> Self {
        let Provenance {
            airport_id,
            procedure_id,
            kind,
            role,
            path_termination,
            leg_sequence,
            discontinuity_after,
            display_path,
        } = value;
        Self {
            airport_id,
            procedure_id,
            kind: kind.into(),
            role: role.into(),
            path_termination: path_termination.into(),
            leg_sequence,
            discontinuity_after: discontinuity_after.map(Into::into),
            display_path: display_path.map(Into::into),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct DisplayPath {
    pub style: PathStyle,
    pub elements: Vec<DisplayElement>,
    pub effective_terminal_course_deg: Option<f64>,
}
impl From<runtime::LegDisplayPath> for DisplayPath {
    fn from(value: runtime::LegDisplayPath) -> Self {
        let runtime::LegDisplayPath {
            style,
            elements,
            effective_terminal_course_deg,
            debug_element_sources: _,
            debug_element_roles: _,
        } = value;
        Self {
            style: style.into(),
            elements: map(elements),
            effective_terminal_course_deg,
        }
    }
}
impl From<DisplayPath> for runtime::LegDisplayPath {
    fn from(value: DisplayPath) -> Self {
        let DisplayPath {
            style,
            elements,
            effective_terminal_course_deg,
        } = value;
        Self {
            style: style.into(),
            elements: map(elements),
            effective_terminal_course_deg,
            debug_element_sources: Vec::new(),
            debug_element_roles: Vec::new(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum DisplayElement {
    Segment {
        start: Position,
        end: Position,
    },
    Arc {
        center: Position,
        radius_nm: f64,
        start: Position,
        end: Position,
        clockwise: bool,
        sweep_degrees: f64,
    },
}
impl From<runtime::LegDisplayElement> for DisplayElement {
    fn from(value: runtime::LegDisplayElement) -> Self {
        match value {
            runtime::LegDisplayElement::Segment { start, end } => Self::Segment {
                start: start.into(),
                end: end.into(),
            },
            runtime::LegDisplayElement::Arc {
                center,
                radius_nm,
                start,
                end,
                clockwise,
                sweep_degrees,
            } => Self::Arc {
                center: center.into(),
                radius_nm,
                start: start.into(),
                end: end.into(),
                clockwise,
                sweep_degrees,
            },
        }
    }
}
impl From<DisplayElement> for runtime::LegDisplayElement {
    fn from(value: DisplayElement) -> Self {
        match value {
            DisplayElement::Segment { start, end } => Self::Segment {
                start: start.into(),
                end: end.into(),
            },
            DisplayElement::Arc {
                center,
                radius_nm,
                start,
                end,
                clockwise,
                sweep_degrees,
            } => Self::Arc {
                center: center.into(),
                radius_nm,
                start: start.into(),
                end: end.into(),
                clockwise,
                sweep_degrees,
            },
        }
    }
}
