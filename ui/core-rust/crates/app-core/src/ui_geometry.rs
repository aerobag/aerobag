// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::{chart_page::PlateGeoref, geometry::LatLon, ownship::SituationRingCandidate};

pub const UI_WORLD_SIZE: f64 = 256.0;
const UI_MAX_MERCATOR_LATITUDE: f64 = 85.051_128_78;
const EARTH_RADIUS_NM: f64 = 3440.065;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiGeometryPoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiMapViewportGeometry {
    pub center_world_x: f64,
    pub center_world_y: f64,
    pub zoom: f64,
    pub rotation_deg: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiImageViewportGeometry {
    pub left: f64,
    pub top: f64,
    pub zoom: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiSituationTickMark {
    pub inner: UiGeometryPoint,
    pub outer: UiGeometryPoint,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiSituationCardinalLabel {
    pub text: &'static str,
    pub point: UiGeometryPoint,
    pub rotation_deg: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiSituationRingLayout {
    pub radius: f64,
    pub label: String,
    pub label_point: UiGeometryPoint,
    pub label_rotation_deg: f64,
    pub tick_marks: Vec<UiSituationTickMark>,
    pub cardinal_labels: Vec<UiSituationCardinalLabel>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiRouteChevronPlacement {
    pub center: UiGeometryPoint,
    pub angle_degrees: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiRouteDistancePillLayout {
    pub center: UiGeometryPoint,
    pub width: f64,
    pub rotation_degrees: f64,
}

pub fn ui_screen_to_world(
    viewport: UiMapViewportGeometry,
    point: UiGeometryPoint,
    width: f64,
    height: f64,
) -> UiGeometryPoint {
    let scale = 2.0_f64.powf(viewport.zoom);
    let offset = rotate_point(
        point.x - width / 2.0,
        point.y - height / 2.0,
        viewport.rotation_deg,
    );
    UiGeometryPoint {
        x: viewport.center_world_x + offset.x / scale,
        y: viewport.center_world_y + offset.y / scale,
    }
}

pub fn ui_world_to_screen(
    viewport: UiMapViewportGeometry,
    point: UiGeometryPoint,
    width: f64,
    height: f64,
) -> UiGeometryPoint {
    let scale = 2.0_f64.powf(viewport.zoom);
    let wrapped_x =
        point.x + ((viewport.center_world_x - point.x) / UI_WORLD_SIZE).round() * UI_WORLD_SIZE;
    let offset = rotate_point(
        (wrapped_x - viewport.center_world_x) * scale,
        (point.y - viewport.center_world_y) * scale,
        -viewport.rotation_deg,
    );
    UiGeometryPoint {
        x: offset.x + width / 2.0,
        y: offset.y + height / 2.0,
    }
}

pub fn ui_transform_screen_point(
    from: UiMapViewportGeometry,
    from_width: f64,
    from_height: f64,
    to: UiMapViewportGeometry,
    to_width: f64,
    to_height: f64,
    point: UiGeometryPoint,
) -> UiGeometryPoint {
    ui_world_to_screen(
        to,
        ui_screen_to_world(from, point, from_width, from_height),
        to_width,
        to_height,
    )
}

pub fn ui_clamp_image_viewport(
    state: UiImageViewportGeometry,
    image_width: f64,
    image_height: f64,
    viewport_width: f64,
    viewport_height: f64,
    overscroll: f64,
) -> UiImageViewportGeometry {
    let zoom = state.zoom.clamp(1.0, 8.0);
    let fit_scale = (viewport_width / image_width).min(viewport_height / image_height);
    let width = image_width * fit_scale * zoom;
    let height = image_height * fit_scale * zoom;
    UiImageViewportGeometry {
        left: clamp_between(state.left, viewport_width - overscroll - width, overscroll),
        top: clamp_between(state.top, viewport_height - overscroll - height, overscroll),
        zoom,
    }
}

pub fn ui_plate_image_point(position: LatLon, georef: &PlateGeoref) -> UiGeometryPoint {
    match georef {
        PlateGeoref::PlateTransformV1 {
            pixels_per_longitude,
            pixels_per_latitude,
            top_left_lon,
            top_left_lat,
        } => UiGeometryPoint {
            x: (position.lon - top_left_lon) * pixels_per_longitude,
            y: (position.lat - top_left_lat) * pixels_per_latitude,
        },
        PlateGeoref::AirportDiagramTransformV1 {
            pixel_x_from_lon,
            pixel_x_from_lat,
            pixel_x_offset,
            pixel_y_from_lon,
            pixel_y_from_lat,
            pixel_y_offset,
        } => UiGeometryPoint {
            x: position.lon * pixel_x_from_lon + position.lat * pixel_x_from_lat + pixel_x_offset,
            y: position.lon * pixel_y_from_lon + position.lat * pixel_y_from_lat + pixel_y_offset,
        },
    }
}

pub fn ui_project_ahead(position: LatLon, bearing_deg: f64, distance_nm: f64) -> LatLon {
    let angular_distance = distance_nm / EARTH_RADIUS_NM;
    let bearing = bearing_deg.to_radians();
    let start_lat = position.lat.to_radians();
    let start_lon = position.lon.to_radians();
    let next_lat = (start_lat.sin() * angular_distance.cos()
        + start_lat.cos() * angular_distance.sin() * bearing.cos())
    .asin();
    let next_lon = start_lon
        + (bearing.sin() * angular_distance.sin() * start_lat.cos())
            .atan2(angular_distance.cos() - start_lat.sin() * next_lat.sin());
    LatLon {
        lat: next_lat.to_degrees(),
        lon: next_lon.to_degrees(),
    }
}

pub fn ui_lat_lon_to_screen(
    position: LatLon,
    viewport: UiMapViewportGeometry,
    width: f64,
    height: f64,
) -> UiGeometryPoint {
    ui_world_to_screen(viewport, ui_lat_lon_to_world(position), width, height)
}

pub fn ui_select_situation_ring(
    position: LatLon,
    viewport: UiMapViewportGeometry,
    width: f64,
    height: f64,
    ring_candidates: &[SituationRingCandidate],
    magnetic_variation_deg: Option<f64>,
) -> Option<UiSituationRingLayout> {
    let center = ui_lat_lon_to_screen(position, viewport, width, height);
    let smaller = width.min(height);
    let min_diameter = smaller * 0.5;
    let max_diameter = smaller * 0.8;
    let target_diameter = smaller * 0.65;
    let (best, radius, _) = ring_candidates
        .iter()
        .map(|candidate| {
            let edge = ui_project_ahead(position, 90.0, candidate.radius_nm);
            let edge_point = ui_lat_lon_to_screen(edge, viewport, width, height);
            let radius = (edge_point.x - center.x).hypot(edge_point.y - center.y);
            let diameter = radius * 2.0;
            let out_of_bounds = if diameter < min_diameter {
                min_diameter - diameter
            } else if diameter > max_diameter {
                diameter - max_diameter
            } else {
                0.0
            };
            let score = if out_of_bounds > 0.0 {
                10_000.0 + out_of_bounds
            } else {
                (diameter - target_diameter).abs()
            };
            (candidate, radius, score)
        })
        .min_by(|left, right| left.2.total_cmp(&right.2))?;

    let tick_marks = magnetic_variation_deg
        .map(|variation| {
            (0..12)
                .map(|index| {
                    let angle = f64::from(index) * 30.0 + variation;
                    UiSituationTickMark {
                        inner: point_on_circle(center, radius - 14.0, angle),
                        outer: point_on_circle(center, radius, angle),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let cardinal_labels = magnetic_variation_deg
        .map(|variation| {
            let label_radius = (radius - 30.0).max(0.0);
            [
                ("N", -90.0, 0.0),
                ("E", 0.0, 90.0),
                ("S", 90.0, 0.0),
                ("W", 180.0, -90.0),
            ]
            .into_iter()
            .map(|(text, angle, rotation_deg)| UiSituationCardinalLabel {
                text,
                point: point_on_circle(center, label_radius, angle + variation),
                rotation_deg,
            })
            .collect()
        })
        .unwrap_or_default();

    Some(UiSituationRingLayout {
        radius,
        label: best.label.clone(),
        label_point: point_on_circle(center, radius + 16.0, -45.0),
        label_rotation_deg: 45.0,
        tick_marks,
        cardinal_labels,
    })
}

pub fn ui_spaced_route_chevron_placements(
    path: &[UiGeometryPoint],
    spacing: f64,
) -> Vec<UiRouteChevronPlacement> {
    if path.len() < 2 || !spacing.is_finite() || spacing <= 0.0 {
        return Vec::new();
    }
    let sections = path
        .windows(2)
        .filter_map(|points| {
            let length = (points[1].x - points[0].x).hypot(points[1].y - points[0].y);
            (length > 0.0).then_some((points[0], points[1], length))
        })
        .collect::<Vec<_>>();
    let total_length = sections.iter().map(|section| section.2).sum::<f64>();
    if total_length <= 0.0 {
        return Vec::new();
    }
    let mut distances = Vec::new();
    if total_length <= spacing {
        distances.push(total_length / 2.0);
    } else {
        let mut distance = spacing / 2.0;
        while distance <= total_length - spacing / 2.0 + 1e-6 {
            distances.push(distance);
            distance += spacing;
        }
    }

    let mut section_index = 0;
    let mut section_start_distance = 0.0;
    distances
        .into_iter()
        .map(|distance| {
            while section_index + 1 < sections.len()
                && distance > section_start_distance + sections[section_index].2
            {
                section_start_distance += sections[section_index].2;
                section_index += 1;
            }
            let (start, end, length) = sections[section_index];
            let fraction = (distance - section_start_distance) / length;
            UiRouteChevronPlacement {
                center: UiGeometryPoint {
                    x: start.x + (end.x - start.x) * fraction,
                    y: start.y + (end.y - start.y) * fraction,
                },
                angle_degrees: (end.y - start.y).atan2(end.x - start.x).to_degrees(),
            }
        })
        .collect()
}

pub fn ui_route_distance_pill_layout(
    paths: &[Vec<UiGeometryPoint>],
    segment_indexes: &[usize],
    eligible: bool,
    width: f64,
    minimum_path_to_pill_width_ratio: f64,
    map_up_deg: f64,
) -> Option<UiRouteDistancePillLayout> {
    if !eligible {
        return None;
    }
    let mut path = Vec::new();
    for (position, segment_index) in segment_indexes.iter().enumerate() {
        if let Some(points) = paths.get(*segment_index) {
            path.extend(points.iter().skip(usize::from(position > 0)).copied());
        }
    }
    if path.len() < 2 {
        return None;
    }
    let segment_lengths = path
        .windows(2)
        .map(|points| (points[1].x - points[0].x).hypot(points[1].y - points[0].y))
        .collect::<Vec<_>>();
    let path_length = segment_lengths.iter().sum::<f64>();
    if path_length < width * minimum_path_to_pill_width_ratio {
        return None;
    }
    let anchor_distance = width * minimum_path_to_pill_width_ratio / 2.0;
    let mut traversed = 0.0;
    for (index, length) in segment_lengths.into_iter().enumerate() {
        if length > 0.0 && traversed + length >= anchor_distance {
            let fraction = (anchor_distance - traversed) / length;
            let delta_x = path[index + 1].x - path[index].x;
            let delta_y = path[index + 1].y - path[index].y;
            return Some(UiRouteDistancePillLayout {
                center: UiGeometryPoint {
                    x: path[index].x + delta_x * fraction,
                    y: path[index].y + delta_y * fraction,
                },
                width,
                rotation_degrees: upright_local_rotation_degrees(delta_x, delta_y, map_up_deg),
            });
        }
        traversed += length;
    }
    None
}

fn ui_lat_lon_to_world(position: LatLon) -> UiGeometryPoint {
    let lat = position
        .lat
        .clamp(-UI_MAX_MERCATOR_LATITUDE, UI_MAX_MERCATOR_LATITUDE);
    UiGeometryPoint {
        x: ((position.lon + 180.0) / 360.0) * UI_WORLD_SIZE,
        y: ((1.0 - lat.to_radians().tan().asinh() / std::f64::consts::PI) / 2.0) * UI_WORLD_SIZE,
    }
}

fn point_on_circle(center: UiGeometryPoint, radius: f64, angle_deg: f64) -> UiGeometryPoint {
    let radians = angle_deg.to_radians();
    UiGeometryPoint {
        x: center.x + radius * radians.cos(),
        y: center.y + radius * radians.sin(),
    }
}

fn upright_local_rotation_degrees(delta_x: f64, delta_y: f64, map_up_deg: f64) -> f64 {
    let route_rotation_deg = delta_y.atan2(delta_x).to_degrees();
    let displayed_rotation_deg = normalize_signed_degrees(route_rotation_deg - map_up_deg);
    if displayed_rotation_deg <= -90.0 || displayed_rotation_deg > 90.0 {
        normalize_signed_degrees(route_rotation_deg + 180.0)
    } else {
        route_rotation_deg
    }
}

fn normalize_signed_degrees(degrees: f64) -> f64 {
    (degrees + 180.0).rem_euclid(360.0) - 180.0
}

fn rotate_point(x: f64, y: f64, degrees: f64) -> UiGeometryPoint {
    let radians = degrees.to_radians();
    UiGeometryPoint {
        x: x * radians.cos() - y * radians.sin(),
        y: x * radians.sin() + y * radians.cos(),
    }
}

fn clamp_between(value: f64, first: f64, second: f64) -> f64 {
    value.clamp(first.min(second), first.max(second))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const CONFORMANCE: &str =
        include_str!("../../app-ui-contracts/tests/goldens/ui-geometry-conformance.json");

    fn number(value: &Value, field: &str) -> f64 {
        value[field]
            .as_f64()
            .unwrap_or_else(|| panic!("missing {field}"))
    }

    fn point(value: &Value) -> UiGeometryPoint {
        UiGeometryPoint {
            x: number(value, "x"),
            y: number(value, "y"),
        }
    }

    fn viewport(value: &Value) -> UiMapViewportGeometry {
        UiMapViewportGeometry {
            center_world_x: number(value, "center_world_x"),
            center_world_y: number(value, "center_world_y"),
            zoom: number(value, "zoom"),
            rotation_deg: number(value, "rotation_deg"),
        }
    }

    fn assert_point(actual: UiGeometryPoint, expected: UiGeometryPoint) {
        assert!((actual.x - expected.x).abs() < 1e-9, "x: {actual:?}");
        assert!((actual.y - expected.y).abs() < 1e-9, "y: {actual:?}");
    }

    #[test]
    fn matches_shared_ui_geometry_vectors() {
        let vectors: Value = serde_json::from_str(CONFORMANCE).expect("geometry conformance JSON");
        let antimeridian = &vectors["map_antimeridian"];
        assert_point(
            ui_world_to_screen(
                viewport(&antimeridian["viewport"]),
                point(&antimeridian["world"]),
                number(antimeridian, "width"),
                number(antimeridian, "height"),
            ),
            point(&antimeridian["expected_screen"]),
        );

        let frame = &vectors["map_frame_transform"];
        assert_point(
            ui_transform_screen_point(
                viewport(&frame["from_viewport"]),
                number(frame, "from_width"),
                number(frame, "from_height"),
                viewport(&frame["to_viewport"]),
                number(frame, "to_width"),
                number(frame, "to_height"),
                point(&frame["point"]),
            ),
            point(&frame["expected_screen"]),
        );

        let image = &vectors["image_clamp"];
        let actual = ui_clamp_image_viewport(
            UiImageViewportGeometry {
                left: number(&image["state"], "left"),
                top: number(&image["state"], "top"),
                zoom: number(&image["state"], "zoom"),
            },
            number(image, "image_width"),
            number(image, "image_height"),
            number(image, "viewport_width"),
            number(image, "viewport_height"),
            number(image, "overscroll"),
        );
        assert!((actual.left - number(&image["expected"], "left")).abs() < 1e-9);
        assert!((actual.top - number(&image["expected"], "top")).abs() < 1e-9);
        assert!((actual.zoom - number(&image["expected"], "zoom")).abs() < 1e-9);

        let plate = &vectors["plate_affine"];
        let georef: PlateGeoref =
            serde_json::from_value(plate["georef"].clone()).expect("plate georef");
        assert_point(
            ui_plate_image_point(
                LatLon {
                    lat: number(&plate["position"], "lat"),
                    lon: number(&plate["position"], "lon"),
                },
                &georef,
            ),
            point(&plate["expected_image"]),
        );

        let situation = &vectors["situation_overlay"];
        let situation_position = LatLon {
            lat: number(&situation["position"], "lat"),
            lon: number(&situation["position"], "lon"),
        };
        let situation_viewport = viewport(&situation["viewport"]);
        let situation_width = number(situation, "width");
        let situation_height = number(situation, "height");
        assert_point(
            ui_lat_lon_to_screen(
                situation_position,
                situation_viewport,
                situation_width,
                situation_height,
            ),
            point(&situation["expected"]["center"]),
        );
        let predictor = &situation["predictor"];
        let predictor_position = ui_project_ahead(
            situation_position,
            number(predictor, "heading_deg"),
            number(predictor, "speed_kt") * number(predictor, "minutes") / 60.0,
        );
        assert_point(
            ui_lat_lon_to_screen(
                predictor_position,
                situation_viewport,
                situation_width,
                situation_height,
            ),
            point(&situation["expected"]["predictor"]),
        );
        let candidates = situation["ring_candidates"]
            .as_array()
            .expect("ring candidates")
            .iter()
            .map(|candidate| SituationRingCandidate {
                radius_nm: number(candidate, "radius_nm"),
                label: candidate["label"].as_str().expect("ring label").to_string(),
            })
            .collect::<Vec<_>>();
        let ring = ui_select_situation_ring(
            situation_position,
            situation_viewport,
            situation_width,
            situation_height,
            &candidates,
            Some(number(situation, "magnetic_variation_deg")),
        )
        .expect("situation ring");
        let expected_ring = &situation["expected"]["ring"];
        assert_eq!(ring.label, expected_ring["label"].as_str().unwrap());
        assert!((ring.radius - number(expected_ring, "radius")).abs() < 1e-9);
        assert_point(ring.label_point, point(&expected_ring["label_point"]));
        assert!(
            (ring.label_rotation_deg - number(expected_ring, "label_rotation_degrees")).abs()
                < 1e-9
        );
        assert_eq!(
            ring.tick_marks.len(),
            expected_ring["ticks"].as_array().expect("ticks").len()
        );
        for (actual, expected) in ring
            .tick_marks
            .iter()
            .zip(expected_ring["ticks"].as_array().expect("ticks"))
        {
            assert_point(actual.inner, point(&expected["inner"]));
            assert_point(actual.outer, point(&expected["outer"]));
        }
        assert_eq!(
            ring.cardinal_labels.len(),
            expected_ring["cardinals"]
                .as_array()
                .expect("cardinals")
                .len()
        );
        for (actual, expected) in ring
            .cardinal_labels
            .iter()
            .zip(expected_ring["cardinals"].as_array().expect("cardinals"))
        {
            assert_eq!(actual.text, expected["text"].as_str().unwrap());
            assert_point(actual.point, point(&expected["point"]));
            assert!((actual.rotation_deg - number(expected, "rotation_degrees")).abs() < 1e-9);
        }

        let chevrons = &vectors["route_chevrons"];
        let chevron_path = chevrons["path"]
            .as_array()
            .expect("chevron path")
            .iter()
            .map(point)
            .collect::<Vec<_>>();
        let placements =
            ui_spaced_route_chevron_placements(&chevron_path, number(chevrons, "spacing"));
        assert_eq!(
            placements.len(),
            chevrons["expected"]
                .as_array()
                .expect("chevron expected")
                .len()
        );
        for (actual, expected) in placements
            .iter()
            .zip(chevrons["expected"].as_array().expect("chevron expected"))
        {
            assert_point(actual.center, point(expected));
            assert!((actual.angle_degrees - number(expected, "angle_degrees")).abs() < 1e-9);
        }

        let pill = &vectors["route_distance_pill"];
        let paths = pill["screen_paths"]
            .as_array()
            .expect("pill paths")
            .iter()
            .map(|path| {
                path.as_array()
                    .expect("pill path")
                    .iter()
                    .map(point)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let segment_indexes = pill["segment_indexes"]
            .as_array()
            .expect("segment indexes")
            .iter()
            .map(|index| index.as_u64().expect("segment index") as usize)
            .collect::<Vec<_>>();
        let visible = pill["visible_feature_ids"]
            .as_array()
            .expect("visible feature ids")
            .iter()
            .filter_map(Value::as_str)
            .collect::<std::collections::HashSet<_>>();
        let eligible = pill["required_feature_ids"]
            .as_array()
            .expect("required feature ids")
            .iter()
            .filter_map(Value::as_str)
            .all(|required| visible.contains(required));
        let actual_pill = ui_route_distance_pill_layout(
            &paths,
            &segment_indexes,
            eligible,
            number(pill, "measured_width"),
            number(pill, "minimum_path_to_pill_width_ratio"),
            number(pill, "map_up_deg"),
        )
        .expect("route distance pill");
        assert_point(actual_pill.center, point(&pill["expected"]["center"]));
        assert!((actual_pill.width - number(&pill["expected"], "width")).abs() < 1e-9);
        assert!(
            (actual_pill.rotation_degrees - number(&pill["expected"], "rotation_degrees")).abs()
                < 1e-9
        );
    }
}
