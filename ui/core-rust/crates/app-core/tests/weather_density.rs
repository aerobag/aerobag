// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;

use app_core::map_overlay::{MetarTileRecord, PointTileLayerConfig};
use app_core::{
    query_map_overlay, tile_key, AirspaceFeaturePayload, LatLon, MapOverlayConfig, MapOverlayQuery,
    MapOverlayQueryResult, MapViewport, MetarProductPayload, MetarRecord, MetarTilePayload,
    PirepProductPayload, PirepRecord, PointTilePayload, VectorAggregateTilePayload,
};

const DISPLAY_LIMIT: usize = 1_000;
const WORLD_SIZE: f64 = 256.0;

fn overlay_config() -> MapOverlayConfig {
    let point_layer = || PointTileLayerConfig {
        min_zoom: 9,
        max_zoom: 9,
        available_zooms: vec![9],
        tile_path_template: None,
    };
    MapOverlayConfig {
        airspace_reference_tile_min_zoom: 0,
        airspace_reference_tile_max_zoom: 0,
        airspace_label_tile_min_zoom: 0,
        airspace_label_tile_max_zoom: 0,
        airport_layer: point_layer(),
        fix_layer: point_layer(),
        nav_layer: point_layer(),
        obstacle_layer: None,
        metar_layer: Some(PointTileLayerConfig {
            min_zoom: 5,
            max_zoom: 7,
            available_zooms: vec![5, 6, 7],
            tile_path_template: None,
        }),
    }
}

fn weather_tile_xy(position: LatLon, zoom: u32) -> (u32, u32) {
    let sin_lat = position.lat.to_radians().sin();
    let world_x = (position.lon + 180.0) / 360.0 * WORLD_SIZE;
    let world_y = (0.5 - ((1.0 + sin_lat) / (1.0 - sin_lat)).ln() / (4.0 * std::f64::consts::PI))
        * WORLD_SIZE;
    let tiles_at_zoom = 2_u32.pow(zoom);
    let tile_world_size = WORLD_SIZE / f64::from(tiles_at_zoom);
    let tile = |world: f64| {
        (world / tile_world_size)
            .floor()
            .clamp(0.0, f64::from(tiles_at_zoom - 1)) as u32
    };
    (tile(world_x), tile(world_y))
}

fn add_tile_record(
    cache: &mut HashMap<String, MetarTilePayload>,
    zoom: u32,
    position: LatLon,
    kind: &str,
    id: &str,
) {
    let (x, y) = weather_tile_xy(position, zoom);
    cache
        .entry(tile_key("metars", zoom, x, y))
        .or_insert_with(|| MetarTilePayload {
            schema_version: 1,
            layer: "metars".to_string(),
            z: zoom,
            x,
            y,
            records: Vec::new(),
        })
        .records
        .push(MetarTileRecord {
            kind: kind.to_string(),
            id: id.to_string(),
        });
}

struct WeatherDensityFixture {
    tiles: HashMap<String, MetarTilePayload>,
    metars: MetarProductPayload,
    pireps: PirepProductPayload,
}

fn weather_density_fixture() -> WeatherDensityFixture {
    let mut tiles = HashMap::new();
    let mut metars_by_station = HashMap::new();
    let mut pireps_by_id = HashMap::new();

    // Preserve the two production density shapes that exposed the regression:
    // clustered CONUS stations and a busy North Atlantic report corridor.
    // Every 24th station models the nav-db importance filter in the z5 tiles.
    for row in 0..36 {
        for column in 0..72 {
            let index = row * 72 + column;
            let id = format!("K{index:04}");
            let position = LatLon {
                lat: 25.0 + f64::from(row) * 0.7,
                lon: -124.0 + f64::from(column) * 0.8,
            };
            metars_by_station.insert(
                id.clone(),
                MetarRecord {
                    raw_text: format!("METAR {id} 010000Z 00000KT 10SM CLR 10/08 A3000"),
                    observed_at_utc: None,
                    station_id: id.clone(),
                    flight_category: Some("VFR".to_string()),
                    clouds: None,
                    longitude: position.lon,
                    latitude: position.lat,
                },
            );
            if index % 24 == 0 {
                add_tile_record(&mut tiles, 5, position, "metar", &id);
            }
            add_tile_record(&mut tiles, 6, position, "metar", &id);
            add_tile_record(&mut tiles, 7, position, "metar", &id);
        }
    }
    for row in 0..36 {
        for column in 0..60 {
            let index = row * 60 + column;
            let id = format!("pirep:{index:04}");
            let position = LatLon {
                lat: 45.0 + f64::from(row) * 0.7,
                lon: -65.0 + f64::from(column),
            };
            pireps_by_id.insert(
                id.clone(),
                PirepRecord {
                    id: id.clone(),
                    raw_text: format!("TEST AIREP {index}"),
                    observed_at_utc: None,
                    report_type: Some("AIREP".to_string()),
                    longitude: position.lon,
                    latitude: position.lat,
                    symbol: "generic".to_string(),
                    icing: "none".to_string(),
                    turbulence: "none".to_string(),
                },
            );
            for zoom in [5, 6, 7] {
                add_tile_record(&mut tiles, zoom, position, "pirep", &id);
            }
        }
    }

    WeatherDensityFixture {
        tiles,
        metars: MetarProductPayload {
            schema_version: 3,
            version_label: "density-fixture".to_string(),
            generated_at_utc: None,
            observed_at_utc: None,
            metar_count: Some(metars_by_station.len() as u32),
            metars_by_station,
        },
        pireps: PirepProductPayload {
            schema_version: 3,
            version_label: "density-fixture".to_string(),
            generated_at_utc: None,
            observed_at_utc: None,
            pirep_count: Some(pireps_by_id.len() as u32),
            pireps_by_id,
        },
    }
}

fn query_fixture(
    fixture: &WeatherDensityFixture,
    center: LatLon,
    zoom: f64,
    width_px: f64,
    height_px: f64,
) -> MapOverlayQueryResult {
    let config = overlay_config();
    let vectors = HashMap::<String, VectorAggregateTilePayload>::new();
    let obstacles = HashMap::<String, PointTilePayload>::new();
    let airspaces = HashMap::<String, AirspaceFeaturePayload>::new();
    query_map_overlay(
        &MapViewport {
            center,
            zoom,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        },
        width_px,
        height_px,
        MapOverlayQuery {
            display_metars: true,
            metar_payload: Some(&fixture.metars),
            pirep_payload: Some(&fixture.pireps),
            ..MapOverlayQuery::new(&config, &vectors, &obstacles, &fixture.tiles, &airspaces)
        },
    )
}

fn assert_uncapped(result: &MapOverlayQueryResult) {
    assert!(result
        .data_status_records
        .iter()
        .all(|record| record.id != "map_overlay:metar_display_feature_limit"));
}

#[test]
fn production_shape_weather_density_stays_within_display_envelope() {
    let fixture = weather_density_fixture();

    let continental = query_fixture(
        &fixture,
        LatLon {
            lat: 52.0,
            lon: -35.0,
        },
        2.2,
        1440.0,
        1100.0,
    );
    assert!(continental.visible_metars.is_empty());
    assert!(continental.visible_pireps.is_empty());
    assert_uncapped(&continental);

    for zoom in [5.0, 6.9] {
        let sparse = query_fixture(
            &fixture,
            LatLon {
                lat: 38.0,
                lon: -92.0,
            },
            zoom,
            1920.0,
            1080.0,
        );
        assert!(sparse.visible_metars.len() <= 110);
        assert!(sparse.visible_pireps.is_empty());
        assert_uncapped(&sparse);
    }

    let conus = query_fixture(
        &fixture,
        LatLon {
            lat: 38.0,
            lon: -92.0,
        },
        7.0,
        1920.0,
        1080.0,
    );
    let atlantic = query_fixture(
        &fixture,
        LatLon {
            lat: 57.0,
            lon: -35.0,
        },
        7.0,
        1920.0,
        1080.0,
    );
    let phone = query_fixture(
        &fixture,
        LatLon {
            lat: 38.0,
            lon: -92.0,
        },
        7.0,
        430.0,
        900.0,
    );
    let conus_close = query_fixture(
        &fixture,
        LatLon {
            lat: 38.0,
            lon: -92.0,
        },
        8.0,
        1920.0,
        1080.0,
    );

    assert!((200..DISPLAY_LIMIT).contains(&conus.visible_metars.len()));
    assert!((100..DISPLAY_LIMIT).contains(&atlantic.visible_pireps.len()));
    assert!((20..500).contains(&phone.visible_metars.len()));
    assert!(conus_close.visible_metars.len() < conus.visible_metars.len());
    for result in [&conus, &atlantic, &phone, &conus_close] {
        assert_uncapped(result);
    }
}
