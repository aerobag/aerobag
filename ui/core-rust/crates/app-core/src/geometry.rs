use serde::{Deserialize, Serialize};

use crate::catalog::ChartRecord;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LatLon {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GeoBounds {
    pub south: f64,
    pub west: f64,
    pub north: f64,
    pub east: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MapViewport {
    pub center: LatLon,
    pub zoom: f64,
    pub rotation_deg: f64,
    pub pitch_deg: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolygonRecord {
    pub id: String,
    pub points: Vec<[f64; 2]>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeometryBundle {
    pub schema_version: u32,
    pub polygons: Vec<PolygonRecord>,
}

impl GeometryBundle {
    pub fn chart_contains(&self, chart: &ChartRecord, point: LatLon) -> bool {
        match &chart.coverage {
            crate::catalog::ChartCoverage::BBox(bounds) => point_in_bounds(point, *bounds),
            crate::catalog::ChartCoverage::PolygonRef { polygon_id } => self
                .polygons
                .iter()
                .find(|poly| poly.id == *polygon_id)
                .map(|poly| point_in_polygon(point, poly))
                .unwrap_or(false),
        }
    }
}

fn point_in_bounds(point: LatLon, bounds: GeoBounds) -> bool {
    point.lat >= bounds.south
        && point.lat <= bounds.north
        && point.lon >= bounds.west
        && point.lon <= bounds.east
}

fn point_in_polygon(point: LatLon, polygon: &PolygonRecord) -> bool {
    let mut inside = false;
    let mut previous_index = polygon.points.len().saturating_sub(1);

    for current_index in 0..polygon.points.len() {
        let [current_lon, current_lat] = polygon.points[current_index];
        let [previous_lon, previous_lat] = polygon.points[previous_index];

        let crosses_latitude =
            (current_lat > point.lat) != (previous_lat > point.lat);
        if crosses_latitude {
            let interpolated_lon = previous_lon
                + (current_lon - previous_lon) * (point.lat - previous_lat)
                    / (current_lat - previous_lat);
            if point.lon < interpolated_lon {
                inside = !inside;
            }
        }

        previous_index = current_index;
    }

    inside
}
