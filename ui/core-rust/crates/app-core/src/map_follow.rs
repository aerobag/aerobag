use serde::{Deserialize, Serialize};

use crate::{
    geometry::{LatLon, MapViewport},
    ownship::OwnshipRenderState,
};

const WORLD_SIZE: f64 = 256.0;
const MAX_LATITUDE: f64 = 85.051_128_78;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MapFollowUiState {
    pub can_center_here: bool,
    pub following: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MapFollowSessionState {
    following: bool,
    anchor_offset_x_px: f64,
    anchor_offset_y_px: f64,
    current_viewport: Option<MapViewport>,
}

impl Default for MapFollowSessionState {
    fn default() -> Self {
        Self {
            following: true,
            anchor_offset_x_px: 0.0,
            anchor_offset_y_px: 0.0,
            current_viewport: None,
        }
    }
}

impl MapFollowSessionState {
    pub fn ui_state(&self, ownship: &OwnshipRenderState) -> MapFollowUiState {
        MapFollowUiState {
            can_center_here: ownship.position.is_some(),
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

    pub fn sync_for_viewport(
        &mut self,
        ownship: &OwnshipRenderState,
        viewport: MapViewport,
        width_px: f64,
        height_px: f64,
    ) {
        self.current_viewport = Some(viewport);
        if !self.following || width_px <= 0.0 || height_px <= 0.0 {
            return;
        }
        let Some(position) = ownship.position else {
            self.following = false;
            return;
        };
        let point = world_to_screen(viewport, lat_lon_to_world(position), width_px, height_px);
        if !point.x.is_finite()
            || !point.y.is_finite()
            || point.x < 0.0
            || point.x > width_px
            || point.y < 0.0
            || point.y > height_px
        {
            self.following = false;
            return;
        }
        self.anchor_offset_x_px = point.x - width_px / 2.0;
        self.anchor_offset_y_px = point.y - height_px / 2.0;
    }

    pub fn target_viewport(&self, ownship: &OwnshipRenderState) -> Option<MapViewport> {
        self.current_viewport
            .map(|viewport| self.resolve_viewport(ownship, viewport))
    }

    fn resolve_viewport(&self, ownship: &OwnshipRenderState, viewport: MapViewport) -> MapViewport {
        if !self.following {
            return viewport;
        }
        let Some(position) = ownship.position else {
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
        y: ((1.0 - clamped_lat.to_radians().tan().asinh() / std::f64::consts::PI) / 2.0)
            * WORLD_SIZE,
    }
}

fn world_to_lat_lon(x: f64, y: f64) -> LatLon {
    let lon = (x / WORLD_SIZE) * 360.0 - 180.0;
    let n = std::f64::consts::PI - (2.0 * std::f64::consts::PI * y) / WORLD_SIZE;
    let lat = n.sinh().atan().to_degrees();
    LatLon { lat, lon }
}

fn world_to_screen(
    viewport: MapViewport,
    world: WorldPoint,
    width_px: f64,
    height_px: f64,
) -> WorldPoint {
    let scale = 2.0_f64.powf(viewport.zoom);
    let center = lat_lon_to_world(viewport.center);
    WorldPoint {
        x: (world.x - center.x) * scale + width_px / 2.0,
        y: (world.y - center.y) * scale + height_px / 2.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ownship::{OwnshipBannerSeverity, OwnshipMode};

    fn ownship(lat: f64, lon: f64) -> OwnshipRenderState {
        OwnshipRenderState {
            mode: OwnshipMode::Live,
            banner_text: String::new(),
            banner_severity: OwnshipBannerSeverity::Info,
            draw_aircraft: true,
            draw_predictor: false,
            draw_cdi: true,
            position: Some(LatLon { lat, lon }),
            orientation_deg: None,
            magnetic_variation_deg: None,
            speed_kt: None,
            altitude_msl_ft: None,
            pressure_altitude_ft: None,
        }
    }

    #[test]
    fn engage_centers_on_ownship() {
        let mut state = MapFollowSessionState::default();
        let viewport = MapViewport {
            center: LatLon {
                lat: 40.0,
                lon: -120.0,
            },
            zoom: 8.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        state.engage(viewport);
        let update = state.target_viewport(&ownship(47.5, -122.3)).unwrap();
        assert!((update.center.lat - 47.5).abs() < 1e-6);
        assert!((update.center.lon + 122.3).abs() < 1e-6);
    }

    #[test]
    fn anchor_offset_keeps_ownship_off_center() {
        let mut state = MapFollowSessionState::default();
        let ownship = ownship(47.5, -122.3);
        let viewport = MapViewport {
            center: LatLon {
                lat: 47.5,
                lon: -122.3,
            },
            zoom: 8.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        state.engage(viewport);
        state.set_anchor_offset(viewport, 128.0, 64.0);
        let update = state.target_viewport(&ownship).unwrap();
        assert_ne!(update.center, viewport.center);
    }

    #[test]
    fn sync_for_viewport_updates_anchor_from_ownship_screen_position() {
        let mut state = MapFollowSessionState::default();
        let viewport = MapViewport {
            center: LatLon {
                lat: 47.5,
                lon: -122.3,
            },
            zoom: 8.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        state.engage(viewport);
        let ownship = ownship(47.45, -122.25);
        state.sync_for_viewport(&ownship, viewport, 800.0, 600.0);
        let update = state.target_viewport(&ownship).unwrap();
        assert_ne!(update.center, viewport.center);
        assert!(state.following);
    }

    #[test]
    fn sync_for_viewport_disengages_when_ownship_leaves_viewport() {
        let mut state = MapFollowSessionState::default();
        let viewport = MapViewport {
            center: LatLon {
                lat: 47.5,
                lon: -122.3,
            },
            zoom: 8.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        state.engage(viewport);
        state.sync_for_viewport(&ownship(0.0, 0.0), viewport, 800.0, 600.0);
        assert!(!state.following);
    }
}
