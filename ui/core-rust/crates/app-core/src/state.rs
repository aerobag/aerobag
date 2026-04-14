use serde::{Deserialize, Serialize};

use crate::catalog::CatalogHandle;
use crate::content::{ContentInventory, ContentPolicy, ContentReport, ContentRequirement};
use crate::errors::AppResult;
use crate::planning::{project_ui_state, FlightPlan, FlightPlanUiState};
use crate::situation::Situation;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppState {
    pub active_plan: Option<FlightPlan>,
    pub situation: Situation,
    pub content_policy: ContentPolicy,
    pub last_content_requirements: Vec<ContentRequirement>,
    pub last_content_report: Option<ContentReport>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppUiState {
    pub active_plan: Option<FlightPlanUiState>,
    pub content_policy: ContentPolicy,
    pub last_content_requirements: Vec<ContentRequirement>,
    pub last_content_report: Option<ContentReport>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            active_plan: None,
            situation: Situation::default(),
            content_policy: ContentPolicy::PreferLocal,
            last_content_requirements: Vec::new(),
            last_content_report: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AppEvent {
    SetContentPolicy(ContentPolicy),
    SetSituation(Situation),
    ReplaceFlightPlan(FlightPlan),
    RefreshContent {
        inventory: ContentInventory,
    },
    ClearFlightPlan,
}

pub fn reduce(
    state: &AppState,
    event: AppEvent,
    catalog: &CatalogHandle,
) -> AppResult<AppState> {
    let mut next = state.clone();

    match event {
        AppEvent::SetContentPolicy(policy) => {
            next.content_policy = policy;
            if let Some(report) = refresh_report_if_possible(&next, catalog)? {
                next.last_content_report = Some(report);
            }
        }
        AppEvent::SetSituation(situation) => {
            next.situation = situation;
        }
        AppEvent::ReplaceFlightPlan(plan) => {
            let plan = crate::build_flight_plan(plan)?;
            next.active_plan = Some(plan.clone());
            next.last_content_requirements = crate::plan_content_requirements(catalog, &plan)?;
            next.last_content_report = None;
        }
        AppEvent::RefreshContent { inventory } => {
            if !next.last_content_requirements.is_empty() {
                next.last_content_report = Some(crate::resolve_content_status(
                    &next.last_content_requirements,
                    &inventory,
                    next.content_policy,
                )?);
            }
        }
        AppEvent::ClearFlightPlan => {
            next.active_plan = None;
            next.last_content_requirements.clear();
            next.last_content_report = None;
        }
    }

    Ok(next)
}

fn refresh_report_if_possible(
    state: &AppState,
    _catalog: &CatalogHandle,
) -> AppResult<Option<ContentReport>> {
    if state.last_content_requirements.is_empty() {
        return Ok(None);
    }

    // A policy change alone should not fabricate a new report without inventory.
    // The report is recomputed once the platform provides current inventory.
    Ok(state.last_content_report.clone())
}

pub fn project_app_ui_state(state: &AppState) -> AppUiState {
    AppUiState {
        active_plan: state.active_plan.as_ref().map(project_ui_state),
        content_policy: state.content_policy,
        last_content_requirements: state.last_content_requirements.clone(),
        last_content_report: state.last_content_report.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        load_catalog, AirportId, AppErrorKind, ChartFamilyId, ContentAvailability,
        InstalledPackage, NavRef, PackageId, RegionId, RouteComponent,
    };

    fn sample_catalog() -> CatalogHandle {
        load_catalog(
            &serde_json::json!({
                "schema_version": 1,
                "cycle": "2026-04-16",
                "catalog_revision": "2026-04-05T22:00:00Z",
                "families": [
                    {
                        "id": "sec",
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
                            "family": "sec",
                            "cycle": "2026-04-16"
                        },
                        "package_name": "NE_SEC",
                        "family_id": "sec",
                        "region_id": "ne",
                        "cycle": "2026-04-16",
                        "artifact_kind": "zip",
                        "relative_url": "/2026-04-16/NE_SEC.zip",
                        "manifest_name": "NE_SEC",
                        "size_bytes": null,
                        "checksum_sha256": null
                    }
                ],
                "charts": [],
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
            .to_string(),
        )
        .unwrap()
    }

    fn sample_plan() -> FlightPlan {
        FlightPlan {
            id: "plan-1".to_string(),
            name: "KBOS to KJFK".to_string(),
            legs: Vec::new(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KBOS".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KJFK".to_string()),
                },
            ],
            resolved_legs: Vec::new(),
            guidance: None,
            departure: Some(AirportId("KBOS".to_string())),
            destination: Some(AirportId("KJFK".to_string())),
            alternate: None,
            cruise_altitude_ft: Some(3000),
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        }
    }

    #[test]
    fn replace_plan_populates_requirements_and_clears_stale_report() {
        let catalog = sample_catalog();
        let initial = AppState {
            last_content_report: Some(ContentReport {
                fully_satisfied: true,
                items: Vec::new(),
            }),
            ..AppState::default()
        };

        let next = reduce(&initial, AppEvent::ReplaceFlightPlan(sample_plan()), &catalog).unwrap();

        assert!(next.active_plan.is_some());
        assert_eq!(next.last_content_requirements.len(), 1);
        assert!(next.last_content_report.is_none());
    }

    #[test]
    fn set_situation_updates_state() {
        let catalog = sample_catalog();
        let next = reduce(
            &AppState::default(),
            AppEvent::SetSituation(Situation {
                position: crate::SituationPosition::LatLon {
                    lat: 47.5,
                    lon: -122.3,
                },
                orientation_deg: Some(90.0),
                speed_kt: Some(120.0),
            }),
            &catalog,
        )
        .unwrap();

        assert_eq!(
            next.situation.position,
            crate::SituationPosition::LatLon {
                lat: 47.5,
                lon: -122.3,
            }
        );
        assert_eq!(next.situation.orientation_deg, Some(90.0));
        assert_eq!(next.situation.speed_kt, Some(120.0));
    }

    #[test]
    fn refresh_content_uses_current_policy() {
        let catalog = sample_catalog();
        let with_plan = reduce(
            &AppState::default(),
            AppEvent::ReplaceFlightPlan(sample_plan()),
            &catalog,
        )
        .unwrap();

        let streamed = reduce(
            &with_plan,
            AppEvent::RefreshContent {
                inventory: ContentInventory {
                    installed_packages: Vec::new(),
                    cached_tilesets: Vec::new(),
                    cached_plates: Vec::new(),
                },
            },
            &catalog,
        )
        .unwrap();

        assert_eq!(
            streamed
                .last_content_report
                .as_ref()
                .unwrap()
                .items[0]
                .availability
                .availability,
            ContentAvailability::Unavailable
        );

        let web_policy = reduce(
            &with_plan,
            AppEvent::SetContentPolicy(ContentPolicy::StreamAllowed),
            &catalog,
        )
        .unwrap();

        let streamed = reduce(
            &web_policy,
            AppEvent::RefreshContent {
                inventory: ContentInventory {
                    installed_packages: Vec::new(),
                    cached_tilesets: Vec::new(),
                    cached_plates: Vec::new(),
                },
            },
            &catalog,
        )
        .unwrap();

        assert!(streamed.last_content_report.as_ref().unwrap().fully_satisfied);
        assert_eq!(
            streamed
                .last_content_report
                .as_ref()
                .unwrap()
                .items[0]
                .availability
                .availability,
            ContentAvailability::RemoteOnly
        );
    }

    #[test]
    fn clear_flight_plan_drops_requirements_and_report() {
        let catalog = sample_catalog();
        let with_plan = reduce(
            &AppState::default(),
            AppEvent::ReplaceFlightPlan(sample_plan()),
            &catalog,
        )
        .unwrap();

        let with_report = reduce(
            &with_plan,
            AppEvent::RefreshContent {
                inventory: ContentInventory {
                    installed_packages: vec![InstalledPackage {
                        package_id: PackageId {
                            region: RegionId::Ne,
                            family: ChartFamilyId::Sectional,
                            cycle: "2026-04-16".to_string(),
                        },
                        integrity_ok: true,
                    }],
                    cached_tilesets: Vec::new(),
                    cached_plates: Vec::new(),
                },
            },
            &catalog,
        )
        .unwrap();

        let cleared = reduce(&with_report, AppEvent::ClearFlightPlan, &catalog).unwrap();

        assert!(cleared.active_plan.is_none());
        assert!(cleared.last_content_requirements.is_empty());
        assert!(cleared.last_content_report.is_none());
    }

    #[test]
    fn reducer_reuses_plan_validation() {
        let catalog = sample_catalog();
        let result = reduce(
            &AppState::default(),
            AppEvent::ReplaceFlightPlan(FlightPlan {
                id: "bad".to_string(),
                name: "bad".to_string(),
                legs: Vec::new(),
                route_components: Vec::new(),
                resolved_legs: Vec::new(),
                guidance: None,
                departure: None,
                destination: None,
                alternate: None,
                cruise_altitude_ft: None,
                notes: None,
                updated_at_epoch_ms: 0,
                version: 1,
            }),
            &catalog,
        );

        assert_eq!(result.unwrap_err().kind, AppErrorKind::InvalidFlightPlan);
    }

    #[test]
    fn project_app_ui_state_projects_active_plan() {
        let catalog = sample_catalog();
        let with_plan = reduce(
            &AppState::default(),
            AppEvent::ReplaceFlightPlan(sample_plan()),
            &catalog,
        )
        .unwrap();

        let ui = project_app_ui_state(&with_plan);

        assert!(ui.active_plan.is_some());
        assert_eq!(ui.content_policy, with_plan.content_policy);
        assert_eq!(
            ui.active_plan.as_ref().unwrap().components.len(),
            with_plan
                .active_plan
                .as_ref()
                .unwrap()
                .clone()
                .normalized()
                .route_components
                .len()
        );
    }
}
