// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::{chart_page::PlateGeoref, geometry::LatLon};

pub const UI_WORLD_SIZE: f64 = 256.0;

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
    }
}
