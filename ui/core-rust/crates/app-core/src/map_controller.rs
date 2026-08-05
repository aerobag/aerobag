// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use app_ui_contracts::session::{
    MapLayerId, UiMapLayerOption, UiMapLayerState, UiMapLayerToggleState,
};

use crate::{
    map_overlay::{ObstacleLayerConfig, PointTileLayerConfig},
    AirspaceFeaturePayload, MapOverlayConfig, RasterMapCatalog, RasterMapUiState,
    VectorAggregateTilePayload,
};

#[derive(Clone)]
struct MapModel {
    layer_state: UiMapLayerState,
    overlay_config: Arc<MapOverlayConfig>,
    vector_manifest_loaded: bool,
    raster_catalog: Option<Arc<RasterMapCatalog>>,
    revision: u64,
}

impl Default for MapModel {
    fn default() -> Self {
        Self {
            layer_state: default_map_layer_state(),
            overlay_config: Arc::new(uninitialized_map_overlay_config()),
            vector_manifest_loaded: false,
            raster_catalog: None,
            revision: 0,
        }
    }
}

#[derive(Default)]
pub(crate) struct MapRuntime {
    pub vector_tile_cache: HashMap<String, VectorAggregateTilePayload>,
    pub airspace_feature_cache: HashMap<String, AirspaceFeaturePayload>,
    pub terrain_source_tile_cache: HashMap<String, Vec<u8>>,
    pub agl_terrain_resource_ids_in_flight: HashSet<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MapProjection {
    pub layer_state: UiMapLayerState,
    pub raster_map: Option<RasterMapUiState>,
}

pub(crate) struct MapProjectionResult {
    pub projection: MapProjection,
    pub rebuilt: bool,
}

#[derive(Clone)]
struct MapProjectionCache {
    revision: u64,
    projection: MapProjection,
}

#[derive(Clone)]
pub(crate) struct MapModelCheckpoint {
    model: MapModel,
}

#[derive(Default)]
pub(crate) struct MapController {
    model: MapModel,
    runtime: MapRuntime,
    projection_cache: Option<MapProjectionCache>,
}

impl MapController {
    pub fn revision(&self) -> u64 {
        self.model.revision
    }

    pub fn checkpoint_model(&self) -> MapModelCheckpoint {
        MapModelCheckpoint {
            model: self.model.clone(),
        }
    }

    pub fn rollback_model(&mut self, checkpoint: MapModelCheckpoint) {
        self.model = checkpoint.model;
        self.projection_cache = None;
    }

    pub fn layer_state(&self) -> &UiMapLayerState {
        &self.model.layer_state
    }

    #[cfg(test)]
    pub fn layer_visible(&self, layer: MapLayerId) -> bool {
        map_layer_toggle(&self.model.layer_state, layer).visible
    }

    pub fn set_layer_visibility(&mut self, layer: MapLayerId, visible: bool) {
        let toggle = map_layer_toggle_mut(&mut self.model.layer_state, layer);
        if toggle.visible != visible {
            toggle.visible = visible;
            self.note_change();
        }
    }

    pub fn set_layer_enabled(&mut self, layer: MapLayerId, enabled: bool) {
        let disabled_reason = (!enabled).then(|| map_layer_disabled_reason(layer).to_string());
        let toggle = map_layer_toggle_mut(&mut self.model.layer_state, layer);
        let visible = toggle.visible && enabled;
        if toggle.enabled != enabled
            || toggle.disabled_reason != disabled_reason
            || toggle.visible != visible
        {
            toggle.enabled = enabled;
            toggle.disabled_reason = disabled_reason;
            toggle.visible = visible;
            self.note_change();
        }
    }

    pub fn overlay_config(&self) -> &MapOverlayConfig {
        &self.model.overlay_config
    }

    #[cfg(test)]
    pub fn replace_overlay_config(
        &mut self,
        overlay_config: MapOverlayConfig,
        vector_manifest_loaded: bool,
    ) {
        self.model.overlay_config = Arc::new(overlay_config);
        self.model.vector_manifest_loaded = vector_manifest_loaded;
        self.note_change();
    }

    pub fn install_vector_manifest_config(&mut self, mut overlay_config: MapOverlayConfig) {
        overlay_config.obstacle_layer = self.model.overlay_config.obstacle_layer.clone();
        self.model.overlay_config = Arc::new(overlay_config);
        self.model.vector_manifest_loaded = true;
        self.note_change();
    }

    pub fn set_obstacle_layer(&mut self, obstacle_layer: Option<ObstacleLayerConfig>) {
        if self.model.overlay_config.obstacle_layer != obstacle_layer {
            Arc::make_mut(&mut self.model.overlay_config).obstacle_layer = obstacle_layer;
            self.note_change();
        }
    }

    pub fn vector_manifest_loaded(&self) -> bool {
        self.model.vector_manifest_loaded
    }

    pub fn raster_catalog(&self) -> Option<&RasterMapCatalog> {
        self.model.raster_catalog.as_deref()
    }

    pub fn replace_raster_catalog(&mut self, catalog: Option<RasterMapCatalog>) {
        self.model.raster_catalog = catalog.map(Arc::new);
        self.note_change();
    }

    pub fn raster_catalog_mut(&mut self) -> Option<&mut RasterMapCatalog> {
        self.model.raster_catalog.as_ref()?;
        self.note_change();
        self.model.raster_catalog.as_mut().map(Arc::make_mut)
    }

    pub fn runtime(&self) -> &MapRuntime {
        &self.runtime
    }

    pub fn runtime_mut(&mut self) -> &mut MapRuntime {
        self.note_change();
        &mut self.runtime
    }

    pub fn invalidate_nav_data(&mut self) {
        self.clear_runtime_caches();
        self.model.vector_manifest_loaded = false;
    }

    pub fn clear_runtime_caches(&mut self) {
        self.runtime.vector_tile_cache.clear();
        self.runtime.airspace_feature_cache.clear();
        self.runtime.terrain_source_tile_cache.clear();
        self.runtime.agl_terrain_resource_ids_in_flight.clear();
        self.note_change();
    }

    pub fn project(&mut self) -> MapProjectionResult {
        if let Some(cache) = self.projection_cache.as_ref() {
            if cache.revision == self.model.revision {
                return MapProjectionResult {
                    projection: cache.projection.clone(),
                    rebuilt: false,
                };
            }
        }
        let projection = MapProjection {
            layer_state: self.model.layer_state.clone(),
            raster_map: self
                .model
                .raster_catalog
                .as_deref()
                .and_then(crate::raster_map_ui_state),
        };
        self.projection_cache = Some(MapProjectionCache {
            revision: self.model.revision,
            projection: projection.clone(),
        });
        MapProjectionResult {
            projection,
            rebuilt: true,
        }
    }

    fn note_change(&mut self) {
        self.model.revision = self.model.revision.saturating_add(1);
        self.projection_cache = None;
    }
}

fn default_map_layer_state() -> UiMapLayerState {
    UiMapLayerState {
        options: vec![
            UiMapLayerOption {
                layer_id: MapLayerId::Metars,
                label: "Observations".to_string(),
            },
            UiMapLayerOption {
                layer_id: MapLayerId::Vectors,
                label: "Vectors".to_string(),
            },
            UiMapLayerOption {
                layer_id: MapLayerId::Nexrad,
                label: "NEXRAD".to_string(),
            },
            UiMapLayerOption {
                layer_id: MapLayerId::Traffic,
                label: "ADS-B Traffic".to_string(),
            },
            UiMapLayerOption {
                layer_id: MapLayerId::TerrainWarning,
                label: "Terrain Warning".to_string(),
            },
            UiMapLayerOption {
                layer_id: MapLayerId::WorldBasemap,
                label: "World Map".to_string(),
            },
            UiMapLayerOption {
                layer_id: MapLayerId::OfflineRegions,
                label: "Offline Regions".to_string(),
            },
        ],
        world_basemap: enabled_layer(true),
        vectors: enabled_layer(true),
        metars: enabled_layer(true),
        nexrad: enabled_layer(false),
        traffic: enabled_layer(false),
        terrain_warning: enabled_layer(true),
        offline_regions: enabled_layer(false),
    }
}

fn enabled_layer(visible: bool) -> UiMapLayerToggleState {
    UiMapLayerToggleState {
        visible,
        enabled: true,
        disabled_reason: None,
    }
}

fn uninitialized_map_overlay_config() -> MapOverlayConfig {
    let empty_point_layer = PointTileLayerConfig {
        min_zoom: 0,
        max_zoom: 0,
        available_zooms: Vec::new(),
        tile_path_template: None,
    };
    MapOverlayConfig {
        airspace_reference_tile_min_zoom: 0,
        airspace_reference_tile_max_zoom: 0,
        airspace_label_tile_min_zoom: 0,
        airspace_label_tile_max_zoom: 0,
        airport_layer: empty_point_layer.clone(),
        fix_layer: empty_point_layer.clone(),
        nav_layer: empty_point_layer,
        obstacle_layer: None,
        metar_layer: None,
    }
}

#[cfg(test)]
fn map_layer_toggle(state: &UiMapLayerState, layer: MapLayerId) -> &UiMapLayerToggleState {
    match layer {
        MapLayerId::WorldBasemap => &state.world_basemap,
        MapLayerId::Vectors => &state.vectors,
        MapLayerId::Metars => &state.metars,
        MapLayerId::Nexrad => &state.nexrad,
        MapLayerId::Traffic => &state.traffic,
        MapLayerId::TerrainWarning => &state.terrain_warning,
        MapLayerId::OfflineRegions => &state.offline_regions,
    }
}

fn map_layer_toggle_mut(
    state: &mut UiMapLayerState,
    layer: MapLayerId,
) -> &mut UiMapLayerToggleState {
    match layer {
        MapLayerId::WorldBasemap => &mut state.world_basemap,
        MapLayerId::Vectors => &mut state.vectors,
        MapLayerId::Metars => &mut state.metars,
        MapLayerId::Nexrad => &mut state.nexrad,
        MapLayerId::Traffic => &mut state.traffic,
        MapLayerId::TerrainWarning => &mut state.terrain_warning,
        MapLayerId::OfflineRegions => &mut state.offline_regions,
    }
}

fn map_layer_disabled_reason(layer: MapLayerId) -> &'static str {
    match layer {
        MapLayerId::WorldBasemap => "The world map layer is unavailable.",
        MapLayerId::Vectors => "The vector layer is unavailable.",
        MapLayerId::Metars => "Weather observations are unavailable.",
        MapLayerId::Nexrad => "NEXRAD is unavailable.",
        MapLayerId::Traffic => "ADS-B traffic is unavailable.",
        MapLayerId::TerrainWarning => "Terrain warning is unavailable.",
        MapLayerId::OfflineRegions => "Offline package regions are unavailable.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_checkpoint_preserves_runtime_allocations() {
        let mut controller = MapController::default();
        controller
            .runtime_mut()
            .terrain_source_tile_cache
            .insert("seed".to_string(), vec![0x5a; 1024]);
        let runtime_address = std::ptr::addr_of!(*controller.runtime());
        let bytes_address = controller.runtime().terrain_source_tile_cache["seed"].as_ptr();
        let checkpoint = controller.checkpoint_model();

        controller.set_layer_visibility(MapLayerId::Nexrad, true);
        controller.rollback_model(checkpoint);

        assert!(!controller.layer_visible(MapLayerId::Nexrad));
        assert_eq!(std::ptr::addr_of!(*controller.runtime()), runtime_address);
        assert_eq!(
            controller.runtime().terrain_source_tile_cache["seed"].as_ptr(),
            bytes_address
        );
    }

    #[test]
    fn projection_is_cached_until_map_state_changes() {
        let mut controller = MapController::default();
        let first = controller.project();
        assert!(first.rebuilt);
        assert!(!controller.project().rebuilt);

        controller.set_layer_visibility(MapLayerId::Nexrad, true);
        let changed = controller.project();

        assert!(changed.rebuilt);
        assert!(changed.projection.layer_state.nexrad.visible);
        assert!(!controller.project().rebuilt);
    }
}
