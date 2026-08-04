// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use prost::Message as _;
use serde::{Deserialize, Serialize};

pub const ATMOSPHERE_PRODUCT_ID: &str = "winds-aloft";
pub const ATMOSPHERE_MANIFEST_SCHEMA_VERSION: u32 = 1;
pub const ATMOSPHERE_TILE_SCHEMA_VERSION: u32 = 1;
pub const ATMOSPHERE_TILE_ENCODING: &str = "aerobag-atmosphere-tile-v1-protobuf";
pub const ATMOSPHERE_ARRAY_ORDER: &str = "valid_time,pressure_level,row,column";
pub const ATMOSPHERE_WIND_UNITS_PER_MPS: f64 = 10.0;
pub const ATMOSPHERE_TEMPERATURE_UNITS_PER_C: f64 = 100.0;
pub const ATMOSPHERE_HEIGHT_UNITS_PER_M: f64 = 1.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AtmosphereGridContract {
    /// Latitude at the center of row zero.
    pub latitude_origin_deg: f64,
    /// Signed latitude increment between adjacent rows.
    pub latitude_step_deg: f64,
    /// Longitude at the center of column zero.
    pub longitude_origin_deg: f64,
    /// Signed longitude increment between adjacent columns.
    pub longitude_step_deg: f64,
    pub row_count: u32,
    pub column_count: u32,
    pub tile_row_count: u32,
    pub tile_column_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AtmosphereManifest {
    pub schema_version: u32,
    pub product_id: String,
    pub version_label: String,
    pub generated_at_utc: String,
    pub model_id: String,
    pub cycle_time_epoch_ms: i64,
    pub valid_times_epoch_ms: Vec<i64>,
    pub pressure_levels_mb: Vec<u32>,
    pub grid: AtmosphereGridContract,
    pub encoding: String,
    pub tile_encoding: String,
    pub array_order: String,
    pub root: String,
    pub page_path_template: String,
    pub page_count: u32,
    pub page_size: u32,
    pub logical_bytes_len: u32,
    pub value_bytes_len: u32,
    pub state_sha256: String,
    pub tile_count: u32,
}

impl AtmosphereManifest {
    pub fn validate(&self, nav_kv_encoding: &str) -> Result<(), String> {
        if self.schema_version != ATMOSPHERE_MANIFEST_SCHEMA_VERSION {
            return Err(format!(
                "unsupported atmosphere manifest schema_version {}",
                self.schema_version
            ));
        }
        if self.product_id != ATMOSPHERE_PRODUCT_ID {
            return Err(format!(
                "atmosphere manifest product_id is {}, expected {ATMOSPHERE_PRODUCT_ID}",
                self.product_id
            ));
        }
        if self.encoding != nav_kv_encoding {
            return Err(format!(
                "atmosphere manifest encoding is {}, expected {nav_kv_encoding}",
                self.encoding
            ));
        }
        if self.tile_encoding != ATMOSPHERE_TILE_ENCODING {
            return Err(format!(
                "atmosphere tile encoding is {}, expected {ATMOSPHERE_TILE_ENCODING}",
                self.tile_encoding
            ));
        }
        if self.array_order != ATMOSPHERE_ARRAY_ORDER {
            return Err(format!(
                "atmosphere array order is {}, expected {ATMOSPHERE_ARRAY_ORDER}",
                self.array_order
            ));
        }
        if self.valid_times_epoch_ms.is_empty() {
            return Err("atmosphere manifest has no valid times".to_string());
        }
        if self
            .valid_times_epoch_ms
            .windows(2)
            .any(|times| times[0] >= times[1])
        {
            return Err("atmosphere valid times are not strictly increasing".to_string());
        }
        if self.pressure_levels_mb.len() < 2 {
            return Err("atmosphere manifest needs at least two pressure levels".to_string());
        }
        if self
            .pressure_levels_mb
            .windows(2)
            .any(|levels| levels[0] <= levels[1])
        {
            return Err("atmosphere pressure levels must decrease with altitude".to_string());
        }
        let grid = &self.grid;
        if !grid.latitude_origin_deg.is_finite()
            || !grid.longitude_origin_deg.is_finite()
            || !grid.latitude_step_deg.is_finite()
            || !grid.longitude_step_deg.is_finite()
            || grid.latitude_step_deg == 0.0
            || grid.longitude_step_deg == 0.0
        {
            return Err("atmosphere grid has invalid coordinates or increments".to_string());
        }
        if grid.row_count < 2
            || grid.column_count < 2
            || grid.tile_row_count == 0
            || grid.tile_column_count == 0
        {
            return Err("atmosphere grid has invalid dimensions".to_string());
        }
        let expected_tiles = grid.row_count.div_ceil(grid.tile_row_count)
            * grid.column_count.div_ceil(grid.tile_column_count);
        if self.tile_count != expected_tiles {
            return Err(format!(
                "atmosphere manifest tile_count {} does not match grid tile count {expected_tiles}",
                self.tile_count
            ));
        }
        if self.root.is_empty() || self.page_path_template.is_empty() || self.page_count == 0 {
            return Err("atmosphere manifest has incomplete NavKv members".to_string());
        }
        Ok(())
    }
}

/// Dense atmospheric samples for one source-grid tile.
///
/// Every packed array uses [`ATMOSPHERE_ARRAY_ORDER`]. Signed values are
/// two's-complement little-endian i16s. A validity bit covers all four fields
/// at the corresponding sample index.
#[derive(Clone, PartialEq, prost::Message)]
pub struct AtmosphereTileV1 {
    #[prost(uint32, tag = "1")]
    pub schema_version: u32,
    #[prost(uint32, tag = "2")]
    pub grid_row_start: u32,
    #[prost(uint32, tag = "3")]
    pub grid_column_start: u32,
    #[prost(uint32, tag = "4")]
    pub row_count: u32,
    #[prost(uint32, tag = "5")]
    pub column_count: u32,
    #[prost(bytes = "vec", tag = "6")]
    pub valid_mask: Vec<u8>,
    #[prost(bytes = "vec", tag = "7")]
    pub east_wind_tenth_mps_i16_le: Vec<u8>,
    #[prost(bytes = "vec", tag = "8")]
    pub north_wind_tenth_mps_i16_le: Vec<u8>,
    #[prost(bytes = "vec", tag = "9")]
    pub temperature_centi_c_i16_le: Vec<u8>,
    #[prost(bytes = "vec", tag = "10")]
    pub geopotential_height_m_i16_le: Vec<u8>,
}

impl AtmosphereTileV1 {
    pub fn encode_wire(&self) -> Vec<u8> {
        self.encode_to_vec()
    }

    pub fn decode_wire(bytes: &[u8]) -> Result<Self, String> {
        Self::decode(bytes).map_err(|error| format!("invalid atmosphere tile protobuf: {error}"))
    }

    pub fn sample_count(&self, manifest: &AtmosphereManifest) -> Result<usize, String> {
        if self.schema_version != ATMOSPHERE_TILE_SCHEMA_VERSION {
            return Err(format!(
                "unsupported atmosphere tile schema_version {}",
                self.schema_version
            ));
        }
        if self.row_count == 0
            || self.column_count == 0
            || self.row_count > manifest.grid.tile_row_count
            || self.column_count > manifest.grid.tile_column_count
            || self.grid_row_start + self.row_count > manifest.grid.row_count
            || self.grid_column_start + self.column_count > manifest.grid.column_count
        {
            return Err("atmosphere tile dimensions fall outside its manifest grid".to_string());
        }
        manifest
            .valid_times_epoch_ms
            .len()
            .checked_mul(manifest.pressure_levels_mb.len())
            .and_then(|count| count.checked_mul(self.row_count as usize))
            .and_then(|count| count.checked_mul(self.column_count as usize))
            .ok_or_else(|| "atmosphere tile sample count overflow".to_string())
    }

    pub fn validate(&self, manifest: &AtmosphereManifest) -> Result<(), String> {
        let sample_count = self.sample_count(manifest)?;
        let packed_bytes = sample_count
            .checked_mul(2)
            .ok_or_else(|| "atmosphere tile byte count overflow".to_string())?;
        for (name, bytes) in [
            ("east wind", &self.east_wind_tenth_mps_i16_le),
            ("north wind", &self.north_wind_tenth_mps_i16_le),
            ("temperature", &self.temperature_centi_c_i16_le),
            ("geopotential height", &self.geopotential_height_m_i16_le),
        ] {
            if bytes.len() != packed_bytes {
                return Err(format!(
                    "atmosphere tile {name} array has {} bytes, expected {packed_bytes}",
                    bytes.len()
                ));
            }
        }
        let valid_bytes = sample_count.div_ceil(8);
        if self.valid_mask.len() != valid_bytes {
            return Err(format!(
                "atmosphere tile validity mask has {} bytes, expected {valid_bytes}",
                self.valid_mask.len()
            ));
        }
        Ok(())
    }

    pub fn sample_is_valid(&self, index: usize) -> bool {
        self.valid_mask
            .get(index / 8)
            .is_some_and(|byte| byte & (1 << (index % 8)) != 0)
    }
}

pub fn atmosphere_tile_key(tile_row: u32, tile_column: u32) -> String {
    format!("atmosphere/tile/r{tile_row:05}/c{tile_column:05}")
}

pub fn pack_i16_le(values: impl IntoIterator<Item = i16>) -> Vec<u8> {
    values
        .into_iter()
        .flat_map(i16::to_le_bytes)
        .collect::<Vec<_>>()
}

pub fn unpack_i16_le(bytes: &[u8], index: usize) -> Option<i16> {
    let offset = index.checked_mul(2)?;
    Some(i16::from_le_bytes([
        *bytes.get(offset)?,
        *bytes.get(offset + 1)?,
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_keys_are_lexically_spatial_and_zero_padded() {
        assert_eq!(atmosphere_tile_key(2, 11), "atmosphere/tile/r00002/c00011");
        assert!(atmosphere_tile_key(2, 11) < atmosphere_tile_key(10, 0));
    }

    #[test]
    fn signed_arrays_round_trip_little_endian() {
        let bytes = pack_i16_le([-32768, -1, 0, 1, 32767]);
        assert_eq!(unpack_i16_le(&bytes, 0), Some(-32768));
        assert_eq!(unpack_i16_le(&bytes, 1), Some(-1));
        assert_eq!(unpack_i16_le(&bytes, 4), Some(32767));
        assert_eq!(unpack_i16_le(&bytes, 5), None);
    }
}
