use serde::{Deserialize, Serialize};

use crate::{
    geometry::{LatLon, MapViewport},
    situation::Situation,
};

const WORLD_SIZE: f64 = 256.0;
const MAX_LATITUDE: f64 = 85.051_128_78;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MapFollowUiState {
    pub can_center_here: bool,
    pub following: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct MapFollowSessionState {
    following: bool,
    anchor_offset_x_px: f64,
    anchor_offset_y_px: f64,
    current_viewport: Option<MapViewport>,
}

impl MapFollowSessionState {
    pub fn ui_state(&self, situation: &Situation) -> MapFollowUiState {
        MapFollowUiState {
            can_center_here: situation.position.lat_lon().is_some(),
            following: self.following,
        }
    }

    pub fn engage(&mut self, viewport: MapViewport) {
        self.following = true;
        self.anchor_offset_x_px = 0.0;
        self.anchor_offset_y_px = 0.0;
        self.current_viewport = Some(viewport);
    }

    pub fn disengage(&mut self, viewport: MapViewport) {
        self.following = false;
        self.current_viewport = Some(viewport);
    }

    pub fn set_anchor_offset(&mut self, viewport: MapViewport, offset_x_px: f64, offset_y_px: f64) {
        self.current_viewport = Some(viewport);
        if self.following {
            self.anchor_offset_x_px = offset_x_px;
            self.anchor_offset_y_px = offset_y_px;
        }
    }

    pub fn target_viewport(&self, situation: &Situation) -> Option<MapViewport> {
        self.current_viewport
            .map(|viewport| self.resolve_viewport(situation, viewport))
    }

    fn resolve_viewport(&self, situation: &Situation, viewport: MapViewport) -> MapViewport {
        if !self.following {
            return viewport;
        }
        let Some(position) = situation.position.lat_lon() else {
            return viewport;
        };
        let scale = 2.0_f64.powf(viewport.zoom);
        let world = lat_lon_to_world(position);
        MapViewport {
            center: world_to_lat_lon(
                world.x - self.anchor_offset_x_px / scale,
                world.y - self.anchor_offset_y_px / scale,
            ),
            zoom: viewport.zoom,
            rotation_deg: viewport.rotation_deg,
            pitch_deg: viewport.pitch_deg,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct WorldPoint {
    x: f64,
    y: f64,
}

fn lat_lon_to_world(position: LatLon) -> WorldPoint {
    let clamped_lat = position.lat.clamp(-MAX_LATITUDE, MAX_LATITUDE);
    WorldPoint {
        x: ((position.lon + 180.0) / 360.0) * WORLD_SIZE,
        y: ((1.0 - clamped_lat.to_radians().tan().asinh() / std::f64::consts::PI) / 2.0) * WORLD_SIZE,
    }
}

fn world_to_lat_lon(x: f64, y: f64) -> LatLon {
    let lon = (x / WORLD_SIZE) * 360.0 - 180.0;
    let n = std::f64::consts::PI - (2.0 * std::f64::consts::PI * y) / WORLD_SIZE;
    let lat = n.sinh().atan().to_degrees();
    LatLon { lat, lon }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::situation::SituationPosition;

    fn situation(lat: f64, lon: f64) -> Situation {
        Situation {
            position: SituationPosition::LatLon { lat, lon },
            orientation_deg: None,
            speed_kt: None,
        }
    }

    #[test]
    fn engage_centers_on_ownship() {
        let mut state = MapFollowSessionState::default();
        let viewport = MapViewport {
            center: LatLon { lat: 40.0, lon: -120.0 },
            zoom: 8.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        state.engage(viewport);
        let update = state.target_viewport(&situation(47.5, -122.3)).unwrap();
        assert!((update.center.lat - 47.5).abs() < 1e-6);
        assert!((update.center.lon + 122.3).abs() < 1e-6);
    }

    #[test]
    fn anchor_offset_keeps_ownship_off_center() {
        let mut state = MapFollowSessionState::default();
        let situation = situation(47.5, -122.3);
        let viewport = MapViewport {
            center: LatLon { lat: 47.5, lon: -122.3 },
            zoom: 8.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        state.engage(viewport);
        state.set_anchor_offset(viewport, 128.0, 64.0);
        let update = state.target_viewport(&situation).unwrap();
        assert_ne!(update.center, viewport.center);
    }
}
