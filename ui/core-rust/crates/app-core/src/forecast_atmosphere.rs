// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    cell::RefCell,
    collections::{BTreeSet, HashMap},
    sync::Arc,
};

use product_contracts::{
    atmosphere_tile_key, unpack_i16_le, AtmosphereManifest, AtmosphereTileV1,
    ATMOSPHERE_HEIGHT_UNITS_PER_M, ATMOSPHERE_TEMPERATURE_UNITS_PER_C,
    ATMOSPHERE_WIND_UNITS_PER_MPS,
};

use crate::{AtmosphereModel, AtmosphereSample, LatLon, NavKvLookup, NavKvStore};

const METERS_PER_FOOT: f64 = 0.3048;
const KNOTS_PER_MPS: f64 = 1.943_844_492_440_6;

pub(crate) struct InstalledForecastAtmosphere {
    manifest: AtmosphereManifest,
    store: NavKvStore,
    decoded_tiles: RefCell<HashMap<(u32, u32), Arc<AtmosphereTileV1>>>,
}

impl InstalledForecastAtmosphere {
    pub(crate) fn new(manifest: AtmosphereManifest, store: NavKvStore) -> Result<Self, String> {
        manifest.validate(&format!("had-nav-kv-v{}", had_nav_kv::VERSION))?;
        Ok(Self {
            manifest,
            store,
            decoded_tiles: RefCell::new(HashMap::new()),
        })
    }

    pub(crate) fn version_label(&self) -> &str {
        &self.manifest.version_label
    }

    pub(crate) fn manifest(&self) -> &AtmosphereManifest {
        &self.manifest
    }

    pub(crate) fn insert_page(&mut self, page_index: u32, bytes: Vec<u8>) {
        self.store.insert_page(page_index, bytes);
    }

    pub(crate) fn missing_pages_for_paths(
        &self,
        paths: &[Vec<LatLon>],
    ) -> Result<Vec<u32>, String> {
        const PREFETCH_STEP_NM: f64 = 25.0;

        let mut keys = BTreeSet::new();
        for path in paths {
            for edge in path.windows(2) {
                let step_count = (crate::geodesy::great_circle_distance_nm(edge[0], edge[1])
                    / PREFETCH_STEP_NM)
                    .ceil()
                    .max(1.0) as usize;
                for step in 0..=step_count {
                    let position = crate::geodesy::great_circle_intermediate(
                        edge[0],
                        edge[1],
                        step as f64 / step_count as f64,
                    );
                    keys.extend(self.tile_keys_for_position(position)?);
                }
            }
        }
        self.store
            .missing_pages_for_keys(&keys.into_iter().collect::<Vec<_>>())
    }

    fn tile_keys_for_position(&self, position: LatLon) -> Result<[String; 4], String> {
        let grid = &self.manifest.grid;
        let row_coordinate = (position.lat - grid.latitude_origin_deg) / grid.latitude_step_deg;
        let column_coordinate =
            (position.lon - grid.longitude_origin_deg) / grid.longitude_step_deg;
        let (row_lower, row_upper, _) =
            interpolation_indices(row_coordinate, grid.row_count, "latitude")?;
        let (column_lower, column_upper, _) =
            interpolation_indices(column_coordinate, grid.column_count, "longitude")?;
        let key = |row: u32, column: u32| {
            atmosphere_tile_key(row / grid.tile_row_count, column / grid.tile_column_count)
        };
        Ok([
            key(row_lower, column_lower),
            key(row_lower, column_upper),
            key(row_upper, column_lower),
            key(row_upper, column_upper),
        ])
    }

    fn tile(&self, tile_row: u32, tile_column: u32) -> Result<Arc<AtmosphereTileV1>, String> {
        if let Some(tile) = self.decoded_tiles.borrow().get(&(tile_row, tile_column)) {
            return Ok(Arc::clone(tile));
        }
        let key = atmosphere_tile_key(tile_row, tile_column);
        let bytes = match self
            .store
            .get_bytes(&key)
            .map_err(|error| error.to_string())?
        {
            NavKvLookup::Hit(bytes) => bytes,
            NavKvLookup::MissingKey => return Err(format!("forecast has no tile {key}")),
            NavKvLookup::MissingPages(pages) => {
                return Err(format!(
                    "forecast tile {key} is missing installed NavKv pages {pages:?}"
                ))
            }
        };
        let tile = AtmosphereTileV1::decode_wire(&bytes)?;
        tile.validate(&self.manifest)?;
        let expected_row_start = tile_row * self.manifest.grid.tile_row_count;
        let expected_column_start = tile_column * self.manifest.grid.tile_column_count;
        if tile.grid_row_start != expected_row_start
            || tile.grid_column_start != expected_column_start
        {
            return Err(format!(
                "forecast tile {key} contains the wrong grid origin"
            ));
        }
        let tile = Arc::new(tile);
        self.decoded_tiles
            .borrow_mut()
            .insert((tile_row, tile_column), Arc::clone(&tile));
        Ok(tile)
    }

    fn sample_grid_point(
        &self,
        row: u32,
        column: u32,
        valid_time_index: usize,
        altitude_m: f64,
    ) -> Result<AtmosphereSample, String> {
        let grid = &self.manifest.grid;
        let tile_row = row / grid.tile_row_count;
        let tile_column = column / grid.tile_column_count;
        let tile = self.tile(tile_row, tile_column)?;
        let local_row = row - tile.grid_row_start;
        let local_column = column - tile.grid_column_start;
        let sample_at_level = |level_index: usize| -> Result<Option<GridSample>, String> {
            let index = (((valid_time_index * self.manifest.pressure_levels_mb.len()
                + level_index)
                * tile.row_count as usize
                + local_row as usize)
                * tile.column_count as usize)
                + local_column as usize;
            if !tile.sample_is_valid(index) {
                return Ok(None);
            }
            Ok(Some(GridSample {
                east_wind_kt: unpack_scaled(
                    &tile.east_wind_tenth_mps_i16_le,
                    index,
                    ATMOSPHERE_WIND_UNITS_PER_MPS,
                )? * KNOTS_PER_MPS,
                north_wind_kt: unpack_scaled(
                    &tile.north_wind_tenth_mps_i16_le,
                    index,
                    ATMOSPHERE_WIND_UNITS_PER_MPS,
                )? * KNOTS_PER_MPS,
                temperature_c: unpack_scaled(
                    &tile.temperature_centi_c_i16_le,
                    index,
                    ATMOSPHERE_TEMPERATURE_UNITS_PER_C,
                )?,
                height_m: unpack_scaled(
                    &tile.geopotential_height_m_i16_le,
                    index,
                    ATMOSPHERE_HEIGHT_UNITS_PER_M,
                )?,
            }))
        };

        let mut levels = Vec::with_capacity(self.manifest.pressure_levels_mb.len());
        for level_index in 0..self.manifest.pressure_levels_mb.len() {
            if let Some(sample) = sample_at_level(level_index)? {
                if levels
                    .last()
                    .is_some_and(|previous: &GridSample| sample.height_m <= previous.height_m)
                {
                    return Err("forecast pressure surfaces are not ordered by height".to_string());
                }
                levels.push(sample);
            }
        }
        if levels.len() < 2 {
            return Err(format!(
                "forecast has fewer than two valid pressure surfaces at row {row}, column {column}, time {valid_time_index}"
            ));
        }

        // GFS masks pressure surfaces below terrain. Interpolate only between
        // valid surfaces; outside their range, retain the nearest edge sample.
        if altitude_m <= levels[0].height_m {
            return Ok(levels[0].into());
        }
        if altitude_m >= levels[levels.len() - 1].height_m {
            return Ok(levels[levels.len() - 1].into());
        }
        let pair_index = levels
            .windows(2)
            .position(|pair| altitude_m <= pair[1].height_m)
            .expect("altitude inside valid pressure-surface range has a bracket");
        let lower = levels[pair_index];
        let upper = levels[pair_index + 1];
        let fraction = (altitude_m - lower.height_m) / (upper.height_m - lower.height_m);
        Ok(interpolate_sample(lower.into(), upper.into(), fraction))
    }
}

impl AtmosphereModel for InstalledForecastAtmosphere {
    fn sample(
        &self,
        position: LatLon,
        pressure_altitude_ft: f64,
        epoch_ms: i64,
    ) -> Result<AtmosphereSample, String> {
        let grid = &self.manifest.grid;
        let row_coordinate = (position.lat - grid.latitude_origin_deg) / grid.latitude_step_deg;
        let column_coordinate =
            (position.lon - grid.longitude_origin_deg) / grid.longitude_step_deg;
        let (row_lower, row_upper, row_fraction) =
            interpolation_indices(row_coordinate, grid.row_count, "latitude")?;
        let (column_lower, column_upper, column_fraction) =
            interpolation_indices(column_coordinate, grid.column_count, "longitude")?;
        let (time_lower, time_upper, time_fraction) =
            time_interpolation_indices(&self.manifest.valid_times_epoch_ms, epoch_ms)?;
        let altitude_m = pressure_altitude_ft * METERS_PER_FOOT;

        let sample_at_time = |time_index| -> Result<AtmosphereSample, String> {
            let northwest =
                self.sample_grid_point(row_lower, column_lower, time_index, altitude_m)?;
            let northeast =
                self.sample_grid_point(row_lower, column_upper, time_index, altitude_m)?;
            let southwest =
                self.sample_grid_point(row_upper, column_lower, time_index, altitude_m)?;
            let southeast =
                self.sample_grid_point(row_upper, column_upper, time_index, altitude_m)?;
            let north = interpolate_sample(northwest, northeast, column_fraction);
            let south = interpolate_sample(southwest, southeast, column_fraction);
            Ok(interpolate_sample(north, south, row_fraction))
        };
        Ok(interpolate_sample(
            sample_at_time(time_lower)?,
            sample_at_time(time_upper)?,
            time_fraction,
        ))
    }
}

#[derive(Debug, Clone, Copy)]
struct GridSample {
    east_wind_kt: f64,
    north_wind_kt: f64,
    temperature_c: f64,
    height_m: f64,
}

impl From<GridSample> for AtmosphereSample {
    fn from(value: GridSample) -> Self {
        Self {
            wind_east_kt: value.east_wind_kt,
            wind_north_kt: value.north_wind_kt,
            temperature_c: value.temperature_c,
        }
    }
}

fn unpack_scaled(bytes: &[u8], index: usize, units_per_value: f64) -> Result<f64, String> {
    unpack_i16_le(bytes, index)
        .map(|value| f64::from(value) / units_per_value)
        .ok_or_else(|| format!("forecast packed array has no sample {index}"))
}

fn interpolation_indices(
    coordinate: f64,
    count: u32,
    axis: &str,
) -> Result<(u32, u32, f64), String> {
    if !coordinate.is_finite() || coordinate < 0.0 || coordinate > f64::from(count - 1) {
        return Err(format!("position falls outside forecast {axis} coverage"));
    }
    let lower = coordinate.floor() as u32;
    let upper = (lower + 1).min(count - 1);
    Ok((lower, upper, coordinate - f64::from(lower)))
}

fn time_interpolation_indices(times: &[i64], epoch_ms: i64) -> Result<(usize, usize, f64), String> {
    if epoch_ms < times[0] || epoch_ms > *times.last().unwrap() {
        return Err("time falls outside forecast coverage".to_string());
    }
    for (lower, pair) in times.windows(2).enumerate() {
        if epoch_ms <= pair[1] {
            let fraction = (epoch_ms - pair[0]) as f64 / (pair[1] - pair[0]) as f64;
            return Ok((lower, lower + 1, fraction));
        }
    }
    let last = times.len() - 1;
    Ok((last, last, 0.0))
}

fn interpolate_sample(
    lower: AtmosphereSample,
    upper: AtmosphereSample,
    fraction: f64,
) -> AtmosphereSample {
    AtmosphereSample {
        wind_east_kt: lerp(lower.wind_east_kt, upper.wind_east_kt, fraction),
        wind_north_kt: lerp(lower.wind_north_kt, upper.wind_north_kt, fraction),
        temperature_c: lerp(lower.temperature_c, upper.temperature_c, fraction),
    }
}

fn lerp(lower: f64, upper: f64, fraction: f64) -> f64 {
    lower + (upper - lower) * fraction
}

#[cfg(test)]
pub(crate) mod tests {
    use had_nav_kv::{build_nav_kv_strict, NavKvPair, NavKvRoot};
    use product_contracts::{
        pack_i16_le, AtmosphereGridContract, ATMOSPHERE_ARRAY_ORDER,
        ATMOSPHERE_MANIFEST_SCHEMA_VERSION, ATMOSPHERE_PRODUCT_ID, ATMOSPHERE_TILE_ENCODING,
        ATMOSPHERE_TILE_SCHEMA_VERSION,
    };

    use super::*;

    pub(crate) fn test_forecast_payload() -> (AtmosphereManifest, Vec<u8>, Vec<Vec<u8>>, String) {
        test_forecast_payload_with_masked_level(None)
    }

    fn test_forecast_payload_with_masked_level(
        masked_level: Option<usize>,
    ) -> (AtmosphereManifest, Vec<u8>, Vec<Vec<u8>>, String) {
        let grid = AtmosphereGridContract {
            latitude_origin_deg: 1.0,
            latitude_step_deg: -1.0,
            longitude_origin_deg: 10.0,
            longitude_step_deg: 1.0,
            row_count: 2,
            column_count: 2,
            tile_row_count: 8,
            tile_column_count: 8,
        };
        let time_count = 2;
        let level_count = 3;
        let spatial_count = 2 * 2;
        let sample_count: usize = time_count * level_count * spatial_count;
        let mut east = Vec::new();
        let mut north = Vec::new();
        let mut temperature = Vec::new();
        let mut height = Vec::new();
        for time in 0..time_count {
            for level in 0..level_count {
                for row in 0..2 {
                    for column in 0..2 {
                        let spatial = row * 10 + column;
                        east.push(((time * 100 + level * 20 + spatial) * 10) as i16);
                        north.push(0);
                        temperature.push(((10 + time * 10 + level * 20 + spatial) * 100) as i16);
                        height.push((level * 1_000) as i16);
                    }
                }
            }
        }
        let mut valid_mask = vec![0xff; sample_count.div_ceil(8)];
        if let Some(level) = masked_level {
            for time in 0..time_count {
                for spatial in 0..spatial_count {
                    let index = (time * level_count + level) * spatial_count + spatial;
                    valid_mask[index / 8] &= !(1 << (index % 8));
                }
            }
        }
        let tile = AtmosphereTileV1 {
            schema_version: ATMOSPHERE_TILE_SCHEMA_VERSION,
            grid_row_start: 0,
            grid_column_start: 0,
            row_count: 2,
            column_count: 2,
            valid_mask,
            east_wind_tenth_mps_i16_le: pack_i16_le(east),
            north_wind_tenth_mps_i16_le: pack_i16_le(north),
            temperature_centi_c_i16_le: pack_i16_le(temperature),
            geopotential_height_m_i16_le: pack_i16_le(height),
        };
        let pairs = vec![NavKvPair {
            key: atmosphere_tile_key(0, 0),
            value: tile.encode_wire(),
        }];
        let state_sha256 = had_nav_kv::nav_kv_canonical_sha256_from_pairs(&pairs);
        let built = build_nav_kv_strict(pairs, 4096).unwrap();
        let manifest = AtmosphereManifest {
            schema_version: ATMOSPHERE_MANIFEST_SCHEMA_VERSION,
            product_id: ATMOSPHERE_PRODUCT_ID.to_string(),
            version_label: "test".to_string(),
            generated_at_utc: "2026-08-04T00:00:00Z".to_string(),
            model_id: "test".to_string(),
            cycle_time_epoch_ms: 0,
            valid_times_epoch_ms: vec![0, 1_000],
            pressure_levels_mb: vec![1000, 850, 700],
            grid,
            encoding: format!("had-nav-kv-v{}", had_nav_kv::VERSION),
            tile_encoding: ATMOSPHERE_TILE_ENCODING.to_string(),
            array_order: ATMOSPHERE_ARRAY_ORDER.to_string(),
            root: "root".to_string(),
            page_path_template: "page_{page:04}".to_string(),
            page_count: built.pages.len() as u32,
            page_size: built.page_size,
            logical_bytes_len: built.logical_bytes_len,
            value_bytes_len: built.value_bytes_len,
            state_sha256: state_sha256.clone(),
            tile_count: 1,
        };
        (manifest, built.root_bytes, built.pages, state_sha256)
    }

    fn test_forecast() -> InstalledForecastAtmosphere {
        let (manifest, root, pages, _) = test_forecast_payload();
        test_forecast_from_payload(manifest, root, pages)
    }

    fn test_forecast_from_payload(
        manifest: AtmosphereManifest,
        root: Vec<u8>,
        pages: Vec<Vec<u8>>,
    ) -> InstalledForecastAtmosphere {
        let mut store = NavKvStore::new(NavKvRoot::parse(&root).unwrap());
        for (page, bytes) in pages.into_iter().enumerate() {
            store.insert_page(page as u32, bytes);
        }
        InstalledForecastAtmosphere::new(manifest, store).unwrap()
    }

    #[test]
    fn samples_space_height_and_time_without_exposing_tile_boundaries() {
        let forecast = test_forecast();
        let sample = forecast
            .sample(
                LatLon {
                    lat: 0.5,
                    lon: 10.5,
                },
                500.0 / METERS_PER_FOOT,
                500,
            )
            .unwrap();
        assert!((sample.wind_east_kt - 65.5 * KNOTS_PER_MPS).abs() < 1.0e-9);
        assert!((sample.temperature_c - 30.5).abs() < 1.0e-9);
    }

    #[test]
    fn route_prefetch_identifies_and_accepts_only_needed_nav_kv_pages() {
        let (manifest, root, pages, _) = test_forecast_payload();
        let store = NavKvStore::new(NavKvRoot::parse(&root).unwrap());
        let mut forecast = InstalledForecastAtmosphere::new(manifest, store).unwrap();
        let paths = vec![vec![
            LatLon {
                lat: 1.0,
                lon: 10.0,
            },
            LatLon {
                lat: 0.0,
                lon: 11.0,
            },
        ]];

        let mut fault_rounds = 0;
        loop {
            let needed = forecast.missing_pages_for_paths(&paths).unwrap();
            if needed.is_empty() {
                break;
            }
            fault_rounds += 1;
            assert!(fault_rounds <= 4, "route page faults must converge");
            for page in needed {
                forecast.insert_page(page, pages[page as usize].clone());
            }
        }
        assert!(fault_rounds > 0);
        forecast
            .sample(
                LatLon {
                    lat: 0.5,
                    lon: 10.5,
                },
                500.0 / METERS_PER_FOOT,
                500,
            )
            .expect("route pages make forecast sampleable");
    }

    #[test]
    fn skips_pressure_surfaces_masked_below_terrain() {
        let (manifest, root, pages, _) = test_forecast_payload_with_masked_level(Some(0));
        let forecast = test_forecast_from_payload(manifest, root, pages);
        let sample = forecast
            .sample(
                LatLon {
                    lat: 1.0,
                    lon: 10.0,
                },
                1_500.0 / METERS_PER_FOOT,
                0,
            )
            .unwrap();
        assert!((sample.wind_east_kt - 30.0 * KNOTS_PER_MPS).abs() < 1.0e-9);
        assert!((sample.temperature_c - 40.0).abs() < 1.0e-9);
    }

    #[test]
    fn clamps_to_edge_surfaces_instead_of_extrapolating() {
        let (manifest, root, pages, _) = test_forecast_payload_with_masked_level(Some(0));
        let forecast = test_forecast_from_payload(manifest, root, pages);
        let low = forecast
            .sample(
                LatLon {
                    lat: 1.0,
                    lon: 10.0,
                },
                500.0 / METERS_PER_FOOT,
                0,
            )
            .unwrap();
        let high = forecast
            .sample(
                LatLon {
                    lat: 1.0,
                    lon: 10.0,
                },
                2_500.0 / METERS_PER_FOOT,
                0,
            )
            .unwrap();
        assert!((low.wind_east_kt - 20.0 * KNOTS_PER_MPS).abs() < 1.0e-9);
        assert!((low.temperature_c - 30.0).abs() < 1.0e-9);
        assert!((high.wind_east_kt - 40.0 * KNOTS_PER_MPS).abs() < 1.0e-9);
        assert!((high.temperature_c - 50.0).abs() < 1.0e-9);
    }

    #[test]
    fn rejects_positions_and_times_outside_the_forecast_cube() {
        let forecast = test_forecast();
        assert!(forecast
            .sample(
                LatLon {
                    lat: 5.0,
                    lon: 10.0
                },
                1_000.0,
                500
            )
            .is_err());
        assert!(forecast
            .sample(
                LatLon {
                    lat: 0.5,
                    lon: 10.5
                },
                1_000.0,
                2_000
            )
            .is_err());
    }
}
