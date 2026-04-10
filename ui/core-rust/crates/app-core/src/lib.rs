pub mod catalog;
pub mod chart_page;
pub mod content;
pub mod errors;
pub mod geometry;
pub mod ids;
pub mod planning;
pub mod session;
pub mod state;

pub use catalog::{
    CatalogBundle, CatalogFamily, CatalogHandle, CatalogPackage, CatalogRegion, ChartCoverage,
    ChartRecord, PlateRecord, SupplementRecord,
};
pub use chart_page::{
    build_chart_catalog, derive_chart_page, derive_chart_page_from_catalog,
    derive_chart_page_state, derive_chart_page_state_from_catalog, DerivedChartAirport,
    DerivedChartAsset, DerivedChartCatalog, DerivedChartPage, DerivedChartPageState,
    ResourceAirportResources, ResourceCsup, ResourceIndexChartPageInput, ResourcePlate,
};
pub use content::{
    AvailabilityDetail, CachedPlate, CachedTileset, ContentAvailability, ContentInventory,
    ContentPolicy, ContentReport, ContentReportItem, ContentRequirement, InstalledPackage,
};
pub use errors::{AppError, AppErrorKind, AppResult};
pub use geometry::{GeoBounds, GeometryBundle, LatLon, MapViewport, PolygonRecord};
pub use ids::{AirportId, ChartFamilyId, ChartId, PackageId, PlateId, RegionId};
pub use planning::{FlightPlan, NavRef, PlanLeg};
pub use session::{
    create_ui_session, destroy_session, get_session_snapshot, move_waypoint_in_session,
    remove_leg_in_session, restore_chart_page_state_in_session, select_airport_in_session,
    select_chart_in_session,
    UiChartPageState, UiSessionInitResult, UiSessionSnapshot,
};
pub use state::{AppEvent, AppState};

use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    sync::{Arc, Mutex, OnceLock},
};

pub fn load_catalog(catalog_json: &str) -> AppResult<CatalogHandle> {
    let bundle: CatalogBundle = serde_json::from_str(catalog_json).map_err(|err| AppError {
        kind: AppErrorKind::InvalidCatalog,
        message: format!("failed to parse catalog json: {err}"),
    })?;
    Ok(CatalogHandle { bundle })
}

pub fn load_geometry(geometry_json: &str) -> AppResult<GeometryBundle> {
    serde_json::from_str(geometry_json).map_err(|err| AppError {
        kind: AppErrorKind::InvalidCatalog,
        message: format!("failed to parse geometry json: {err}"),
    })
}

pub fn load_resource_index_chart_page_input(
    resource_index_json: &str,
) -> AppResult<Arc<ResourceIndexChartPageInput>> {
    static CACHE: OnceLock<Mutex<HashMap<u64, Arc<ResourceIndexChartPageInput>>>> = OnceLock::new();

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    resource_index_json.hash(&mut hasher);
    let key = hasher.finish();

    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(cached) = cache
        .lock()
        .expect("resource index cache poisoned")
        .get(&key)
        .cloned()
    {
        return Ok(cached);
    }

    let parsed: ResourceIndexChartPageInput =
        serde_json::from_str(resource_index_json).map_err(|err| AppError {
            kind: AppErrorKind::InvalidCatalog,
            message: format!("failed to parse resource index json: {err}"),
        })?;
    let parsed = Arc::new(parsed);
    cache
        .lock()
        .expect("resource index cache poisoned")
        .insert(key, parsed.clone());
    Ok(parsed)
}

pub fn chart_for_position(
    catalog: &CatalogHandle,
    geometry: &GeometryBundle,
    family: ChartFamilyId,
    lat: f64,
    lon: f64,
) -> AppResult<Option<ChartRecord>> {
    let point = LatLon { lat, lon };
    for chart in &catalog.bundle.charts {
        if chart.family_id != family {
            continue;
        }
        if geometry.chart_contains(chart, point) {
            return Ok(Some(chart.clone()));
        }
    }
    Ok(None)
}

pub fn build_flight_plan(plan: FlightPlan) -> AppResult<FlightPlan> {
    if plan.legs.is_empty() {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "flight plan must contain at least one leg".to_string(),
        });
    }
    Ok(plan)
}

pub fn remove_flight_plan_leg(plan: &FlightPlan, index: usize) -> AppResult<FlightPlan> {
    if index >= plan.legs.len() {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!("flight plan leg index out of range: {index}"),
        });
    }

    let mut next = plan.clone();
    next.legs.remove(index);

    if next.legs.is_empty() {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "flight plan must contain at least one leg".to_string(),
        });
    }

    next.departure = next
        .legs
        .first()
        .and_then(|leg| leg.from.airport_code())
        .map(|code| AirportId(code.to_string()));
    next.destination = next
        .legs
        .last()
        .and_then(|leg| leg.to.airport_code())
        .map(|code| AirportId(code.to_string()));
    next.updated_at_epoch_ms += 1;
    next.version += 1;
    Ok(next)
}

pub fn move_flight_plan_waypoint(
    plan: &FlightPlan,
    waypoint_index: usize,
    delta: isize,
) -> AppResult<FlightPlan> {
    if delta == 0 {
        return Ok(plan.clone());
    }

    if plan.legs.is_empty() {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "flight plan must contain at least one leg".to_string(),
        });
    }

    let mut waypoints = Vec::with_capacity(plan.legs.len() + 1);
    waypoints.push(
        plan.legs
            .first()
            .map(|leg| leg.from.clone())
            .ok_or_else(|| AppError {
                kind: AppErrorKind::InvalidFlightPlan,
                message: "flight plan must contain at least one leg".to_string(),
            })?,
    );
    waypoints.extend(plan.legs.iter().map(|leg| leg.to.clone()));

    if waypoint_index >= waypoints.len() {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!("flight plan waypoint index out of range: {waypoint_index}"),
        });
    }

    let next_index = waypoint_index as isize + delta;
    if next_index < 0 || next_index >= waypoints.len() as isize {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!(
                "flight plan waypoint move out of range: {waypoint_index} -> {next_index}"
            ),
        });
    }

    waypoints.swap(waypoint_index, next_index as usize);

    let legs = waypoints
        .windows(2)
        .map(|pair| PlanLeg {
            from: pair[0].clone(),
            to: pair[1].clone(),
            airway: None,
        })
        .collect::<Vec<_>>();

    let mut next = plan.clone();
    next.legs = legs;
    next.departure = waypoints
        .first()
        .and_then(|waypoint| waypoint.airport_code())
        .map(|code| AirportId(code.to_string()));
    next.destination = waypoints
        .last()
        .and_then(|waypoint| waypoint.airport_code())
        .map(|code| AirportId(code.to_string()));
    next.updated_at_epoch_ms += 1;
    next.version += 1;
    Ok(next)
}

pub fn plan_content_requirements(
    catalog: &CatalogHandle,
    plan: &FlightPlan,
) -> AppResult<Vec<ContentRequirement>> {
    let mut package_ids = Vec::new();

    for leg in &plan.legs {
        let airport_codes = [leg.from.airport_code(), leg.to.airport_code()];
        for airport_code in airport_codes.into_iter().flatten() {
            for plate in &catalog.bundle.plates {
                if plate.airport_id.0.eq_ignore_ascii_case(airport_code) {
                    if let Some(pkg) = catalog
                        .bundle
                        .packages
                        .iter()
                        .find(|pkg| pkg.region_id == plate.region_id)
                    {
                        package_ids.push(pkg.id.clone());
                    }
                }
            }
        }
    }

    package_ids.sort();
    package_ids.dedup();

    Ok(vec![ContentRequirement {
        package_ids,
        chart_ids: Vec::new(),
        plate_ids: Vec::new(),
    }])
}

pub fn resolve_content_status(
    requirements: &[ContentRequirement],
    inventory: &ContentInventory,
    policy: ContentPolicy,
) -> AppResult<ContentReport> {
    let mut items = Vec::new();

    for requirement in requirements {
        for package_id in &requirement.package_ids {
            let installed = inventory
                .installed_packages
                .iter()
                .any(|pkg| &pkg.package_id == package_id && pkg.integrity_ok);

            let availability = match (installed, policy) {
                (true, ContentPolicy::StreamAllowed) => ContentAvailability::LocalAndRemote,
                (true, _) => ContentAvailability::LocalOnly,
                (false, ContentPolicy::StreamAllowed) => ContentAvailability::RemoteOnly,
                (false, _) => ContentAvailability::Unavailable,
            };

            items.push(ContentReportItem {
                label: package_id.package_name(),
                availability: AvailabilityDetail {
                    availability,
                    cycle_current: true,
                    integrity_ok: installed,
                    cached: installed,
                    offline_usable: installed,
                },
            });
        }
    }

    let fully_satisfied = items.iter().all(|item| match policy {
        ContentPolicy::StreamAllowed => !matches!(
            item.availability.availability,
            ContentAvailability::Unavailable
        ),
        _ => matches!(
            item.availability.availability,
            ContentAvailability::LocalOnly | ContentAvailability::LocalAndRemote
        ),
    });

    Ok(ContentReport {
        fully_satisfied,
        items,
    })
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

    fn sample_geometry_json() -> String {
        serde_json::json!({
            "schema_version": 1,
            "polygons": [
                {
                    "id": "sectional:boston",
                    "points": [
                        [-72.0, 43.0],
                        [-72.0, 41.0],
                        [-69.0, 41.0],
                        [-69.0, 43.0]
                    ]
                }
            ]
        })
        .to_string()
    }

    fn sample_plan() -> FlightPlan {
        FlightPlan {
            id: "plan-1".to_string(),
            name: "KBOS local".to_string(),
            legs: vec![PlanLeg {
                from: NavRef::Airport("KBOS".to_string()),
                to: NavRef::Airport("KBOS".to_string()),
                airway: None,
            }],
            departure: Some(AirportId("KBOS".to_string())),
            destination: Some(AirportId("KBOS".to_string())),
            alternate: None,
            cruise_altitude_ft: Some(3000),
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        }
    }

    #[test]
    fn remove_flight_plan_leg_updates_endpoints_and_version() {
        let plan = FlightPlan {
            id: "plan-1".to_string(),
            name: "NW sample".to_string(),
            legs: vec![
                PlanLeg {
                    from: NavRef::Airport("KRNT".to_string()),
                    to: NavRef::Airport("KSEA".to_string()),
                    airway: None,
                },
                PlanLeg {
                    from: NavRef::Airport("KSEA".to_string()),
                    to: NavRef::Airport("KPAE".to_string()),
                    airway: None,
                },
            ],
            departure: Some(AirportId("KRNT".to_string())),
            destination: Some(AirportId("KPAE".to_string())),
            alternate: None,
            cruise_altitude_ft: Some(3000),
            notes: None,
            updated_at_epoch_ms: 10,
            version: 1,
        };

        let next = remove_flight_plan_leg(&plan, 0).unwrap();

        assert_eq!(next.legs.len(), 1);
        assert_eq!(next.departure, Some(AirportId("KSEA".to_string())));
        assert_eq!(next.destination, Some(AirportId("KPAE".to_string())));
        assert_eq!(next.updated_at_epoch_ms, 11);
        assert_eq!(next.version, 2);
    }

    #[test]
    fn loads_catalog_with_structured_ids() {
        let handle = load_catalog(&sample_catalog_json()).unwrap();
        assert_eq!(handle.bundle.schema_version, 1);
        assert_eq!(handle.bundle.families[0].id, ChartFamilyId::Sectional);
        assert_eq!(handle.bundle.regions[0].id, RegionId::Ne);
    }

    #[test]
    fn finds_chart_for_point_inside_polygon() {
        let catalog = load_catalog(&sample_catalog_json()).unwrap();
        let geometry = load_geometry(&sample_geometry_json()).unwrap();
        let chart =
            chart_for_position(&catalog, &geometry, ChartFamilyId::Sectional, 42.0, -71.0)
                .unwrap();
        assert_eq!(chart.unwrap().name, "Boston");
    }

    #[test]
    fn does_not_find_chart_for_point_outside_polygon() {
        let catalog = load_catalog(&sample_catalog_json()).unwrap();
        let geometry = load_geometry(&sample_geometry_json()).unwrap();
        let chart =
            chart_for_position(&catalog, &geometry, ChartFamilyId::Sectional, 35.0, -71.0)
                .unwrap();
        assert!(chart.is_none());
    }

    #[test]
    fn rejects_empty_flight_plan() {
        let result = build_flight_plan(FlightPlan {
            id: "plan-1".to_string(),
            name: "Empty".to_string(),
            legs: Vec::new(),
            departure: None,
            destination: None,
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        });
        assert_eq!(result.unwrap_err().kind, AppErrorKind::InvalidFlightPlan);
    }

    #[test]
    fn deduplicates_required_packages_across_matching_legs() {
        let catalog = load_catalog(&sample_catalog_json()).unwrap();
        let requirements = plan_content_requirements(&catalog, &sample_plan()).unwrap();
        assert_eq!(requirements.len(), 1);
        assert_eq!(requirements[0].package_ids.len(), 1);
        assert_eq!(requirements[0].package_ids[0].region, RegionId::Ne);
    }

    #[test]
    fn stream_allowed_reports_remote_content_as_satisfied() {
        let requirements = vec![ContentRequirement {
            package_ids: vec![PackageId {
                region: RegionId::Ne,
                family: ChartFamilyId::Sectional,
                cycle: "2026-04-16".to_string(),
            }],
            chart_ids: Vec::new(),
            plate_ids: Vec::new(),
        }];

        let report = resolve_content_status(
            &requirements,
            &ContentInventory {
                installed_packages: Vec::new(),
                cached_tilesets: Vec::new(),
                cached_plates: Vec::new(),
            },
            ContentPolicy::StreamAllowed,
        )
        .unwrap();

        assert!(report.fully_satisfied);
        assert_eq!(
            report.items[0].availability.availability,
            ContentAvailability::RemoteOnly
        );
    }

    #[test]
    fn offline_required_marks_missing_content_unsatisfied() {
        let requirements = vec![ContentRequirement {
            package_ids: vec![PackageId {
                region: RegionId::Ne,
                family: ChartFamilyId::Sectional,
                cycle: "2026-04-16".to_string(),
            }],
            chart_ids: Vec::new(),
            plate_ids: Vec::new(),
        }];

        let report = resolve_content_status(
            &requirements,
            &ContentInventory {
                installed_packages: Vec::new(),
                cached_tilesets: Vec::new(),
                cached_plates: Vec::new(),
            },
            ContentPolicy::OfflineRequired,
        )
        .unwrap();

        assert!(!report.fully_satisfied);
        assert_eq!(
            report.items[0].availability.availability,
            ContentAvailability::Unavailable
        );
    }

    #[test]
    fn move_flight_plan_waypoint_rebuilds_waypoint_sequence() {
        let plan = FlightPlan {
            id: "plan-1".to_string(),
            name: "NW sample".to_string(),
            legs: vec![
                PlanLeg {
                    from: NavRef::Airport("KRNT".to_string()),
                    to: NavRef::Navaid("SEA".to_string()),
                    airway: Some("V27".to_string()),
                },
                PlanLeg {
                    from: NavRef::Navaid("SEA".to_string()),
                    to: NavRef::Navaid("PAE".to_string()),
                    airway: Some("V27".to_string()),
                },
                PlanLeg {
                    from: NavRef::Navaid("PAE".to_string()),
                    to: NavRef::Airport("KAWO".to_string()),
                    airway: None,
                },
            ],
            departure: Some(AirportId("KRNT".to_string())),
            destination: Some(AirportId("KAWO".to_string())),
            alternate: None,
            cruise_altitude_ft: Some(3000),
            notes: None,
            updated_at_epoch_ms: 10,
            version: 1,
        };

        let next = move_flight_plan_waypoint(&plan, 2, -1).unwrap();

        assert_eq!(
            next.legs,
            vec![
                PlanLeg {
                    from: NavRef::Airport("KRNT".to_string()),
                    to: NavRef::Navaid("PAE".to_string()),
                    airway: None,
                },
                PlanLeg {
                    from: NavRef::Navaid("PAE".to_string()),
                    to: NavRef::Navaid("SEA".to_string()),
                    airway: None,
                },
                PlanLeg {
                    from: NavRef::Navaid("SEA".to_string()),
                    to: NavRef::Airport("KAWO".to_string()),
                    airway: None,
                },
            ]
        );
        assert_eq!(next.departure, Some(AirportId("KRNT".to_string())));
        assert_eq!(next.destination, Some(AirportId("KAWO".to_string())));
        assert_eq!(next.updated_at_epoch_ms, 11);
        assert_eq!(next.version, 2);
    }
}
