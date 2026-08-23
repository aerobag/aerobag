// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{collections::BTreeMap, fs, path::PathBuf, process::Command};

use anyhow::{bail, Context};
use chrono::{DateTime, SecondsFormat, Utc};
use had_nav_kv::{build_nav_kv_strict, nav_kv_canonical_sha256_from_pairs, NavKvPair};
use product_contracts::{
    atmosphere_tile_key, pack_i16_le, AtmosphereGridContract, AtmosphereManifest, AtmosphereTileV1,
    ATMOSPHERE_ARRAY_ORDER, ATMOSPHERE_HEIGHT_UNITS_PER_M, ATMOSPHERE_MANIFEST_SCHEMA_VERSION,
    ATMOSPHERE_PRODUCT_ID, ATMOSPHERE_TEMPERATURE_UNITS_PER_C, ATMOSPHERE_TILE_ENCODING,
    ATMOSPHERE_TILE_SCHEMA_VERSION, ATMOSPHERE_WIND_UNITS_PER_MPS,
};
use serde::Deserialize;

const NAV_KV_PAGE_BYTES: u32 = 64 * 1024;
const GRID_TILE_ROWS: u32 = 8;
const GRID_TILE_COLUMNS: u32 = 8;

#[derive(Debug)]
pub(crate) struct BuildAtmosphereDatasetRequest {
    pub input_dir: PathBuf,
    pub output_dir: PathBuf,
    pub decode_dir: PathBuf,
    pub version_label: String,
    pub cycle_time_utc: DateTime<Utc>,
    pub forecast_hours: Vec<u32>,
    pub pressure_levels_mb: Vec<u32>,
}

#[derive(Debug)]
pub(crate) struct BuildAtmosphereDatasetResult {
    pub state_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub manifest: AtmosphereManifest,
    pub state_sha256: String,
    pub tile_count: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GdalInfo {
    size: [usize; 2],
    geo_transform: [f64; 6],
    bands: Vec<GdalBand>,
}

#[derive(Debug, Deserialize)]
struct GdalBand {
    band: usize,
    metadata: BTreeMap<String, BTreeMap<String, String>>,
}

#[derive(Debug)]
struct DecodedForecast {
    width: usize,
    height: usize,
    latitude_origin_deg: f64,
    latitude_step_deg: f64,
    longitude_origin_deg: f64,
    longitude_step_deg: f64,
    valid_time_epoch_ms: i64,
    fields: BTreeMap<(u32, String), Vec<f32>>,
}

pub(crate) fn build_atmosphere_dataset(
    request: &BuildAtmosphereDatasetRequest,
) -> anyhow::Result<BuildAtmosphereDatasetResult> {
    if request.forecast_hours.is_empty() || request.pressure_levels_mb.len() < 2 {
        bail!("atmosphere build requires forecast times and at least two pressure levels");
    }
    fs::create_dir_all(&request.output_dir)
        .with_context(|| format!("failed to create {}", request.output_dir.display()))?;
    fs::create_dir_all(&request.decode_dir)
        .with_context(|| format!("failed to create {}", request.decode_dir.display()))?;

    let mut forecasts = Vec::with_capacity(request.forecast_hours.len());
    for forecast_hour in &request.forecast_hours {
        let file_name = format!(
            "gfs_{}_{}_f{forecast_hour:03}.grib2",
            request.cycle_time_utc.format("%Y%m%d"),
            request.cycle_time_utc.format("%H")
        );
        let forecast = decode_grib_forecast(
            &request.input_dir.join(file_name),
            &request.decode_dir,
            *forecast_hour,
            &request.pressure_levels_mb,
        )?;
        let expected_time =
            request.cycle_time_utc.timestamp_millis() + i64::from(*forecast_hour) * 60 * 60 * 1_000;
        if forecast.valid_time_epoch_ms != expected_time {
            bail!(
                "forecast hour {forecast_hour} has valid time {}, expected {expected_time}",
                forecast.valid_time_epoch_ms
            );
        }
        if let Some(first) = forecasts.first() {
            validate_same_grid(first, &forecast)?;
        }
        forecasts.push(forecast);
    }

    let first = forecasts
        .first()
        .context("atmosphere build decoded no forecasts")?;
    let grid = AtmosphereGridContract {
        latitude_origin_deg: first.latitude_origin_deg,
        latitude_step_deg: first.latitude_step_deg,
        longitude_origin_deg: first.longitude_origin_deg,
        longitude_step_deg: first.longitude_step_deg,
        row_count: first.height as u32,
        column_count: first.width as u32,
        tile_row_count: GRID_TILE_ROWS,
        tile_column_count: GRID_TILE_COLUMNS,
    };
    let mut pairs = build_tile_pairs(&forecasts, &request.pressure_levels_mb, &grid)?;
    pairs.sort_by(|left, right| left.key.cmp(&right.key));
    let state_sha256 = nav_kv_canonical_sha256_from_pairs(&pairs);
    let built = build_nav_kv_strict(pairs.clone(), NAV_KV_PAGE_BYTES)
        .map_err(|error| anyhow::anyhow!("failed to build atmosphere NavKv: {error}"))?;
    fs::write(request.output_dir.join("root"), &built.root_bytes)
        .context("failed to write atmosphere NavKv root")?;
    for (page, bytes) in built.pages.iter().enumerate() {
        fs::write(request.output_dir.join(format!("page_{page:04}")), bytes)
            .with_context(|| format!("failed to write atmosphere NavKv page {page}"))?;
    }

    let manifest = AtmosphereManifest {
        schema_version: ATMOSPHERE_MANIFEST_SCHEMA_VERSION,
        product_id: ATMOSPHERE_PRODUCT_ID.to_string(),
        version_label: request.version_label.clone(),
        generated_at_utc: request
            .cycle_time_utc
            .to_rfc3339_opts(SecondsFormat::Secs, true),
        model_id: "gfs-0p25".to_string(),
        cycle_time_epoch_ms: request.cycle_time_utc.timestamp_millis(),
        valid_times_epoch_ms: forecasts
            .iter()
            .map(|forecast| forecast.valid_time_epoch_ms)
            .collect(),
        pressure_levels_mb: request.pressure_levels_mb.clone(),
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
        tile_count: pairs.len() as u32,
    };
    manifest
        .validate(&format!("had-nav-kv-v{}", had_nav_kv::VERSION))
        .map_err(anyhow::Error::msg)?;
    let manifest_path = request.output_dir.join("manifest.json");
    fs::write(&manifest_path, serde_json::to_vec(&manifest)?)
        .context("failed to write atmosphere manifest")?;
    Ok(BuildAtmosphereDatasetResult {
        state_dir: request.output_dir.clone(),
        manifest_path,
        manifest,
        state_sha256,
        tile_count: pairs.len(),
    })
}

fn decode_grib_forecast(
    grib_path: &std::path::Path,
    decode_dir: &std::path::Path,
    forecast_hour: u32,
    pressure_levels_mb: &[u32],
) -> anyhow::Result<DecodedForecast> {
    let info_output = Command::new("gdalinfo")
        .arg("-json")
        .arg(grib_path)
        .output()
        .with_context(|| format!("failed to run gdalinfo on {}", grib_path.display()))?;
    if !info_output.status.success() {
        bail!(
            "gdalinfo failed for {}: {}",
            grib_path.display(),
            String::from_utf8_lossy(&info_output.stderr).trim()
        );
    }
    let info: GdalInfo = serde_json::from_slice(&info_output.stdout)
        .with_context(|| format!("invalid gdalinfo JSON for {}", grib_path.display()))?;
    if info.geo_transform[2].abs() > f64::EPSILON || info.geo_transform[4].abs() > f64::EPSILON {
        bail!("rotated GFS grids are not supported");
    }

    let raster_path = decode_dir.join(format!("forecast-{forecast_hour:03}.bin"));
    let translate = Command::new("gdal_translate")
        .args(["-of", "ENVI", "-ot", "Float32", "-co", "INTERLEAVE=BSQ"])
        .arg(grib_path)
        .arg(&raster_path)
        .output()
        .with_context(|| format!("failed to run gdal_translate on {}", grib_path.display()))?;
    if !translate.status.success() {
        bail!(
            "gdal_translate failed for {}: {}",
            grib_path.display(),
            String::from_utf8_lossy(&translate.stderr).trim()
        );
    }
    let header_path = raster_path.with_extension("hdr");
    let header = fs::read_to_string(&header_path)
        .with_context(|| format!("failed to read {}", header_path.display()))?;
    for required in ["data type = 4", "interleave = bsq", "byte order = 0"] {
        if !header.lines().any(|line| line.trim() == required) {
            bail!(
                "decoded GFS ENVI header does not declare required {required:?}: {}",
                header_path.display()
            );
        }
    }
    let bytes = fs::read(&raster_path)
        .with_context(|| format!("failed to read {}", raster_path.display()))?;
    let cell_count = info.size[0]
        .checked_mul(info.size[1])
        .context("GFS grid cell count overflow")?;
    let expected_bytes = cell_count
        .checked_mul(info.bands.len())
        .and_then(|count| count.checked_mul(4))
        .context("GFS decoded raster byte count overflow")?;
    if bytes.len() != expected_bytes {
        bail!(
            "decoded GFS raster has {} bytes, expected {expected_bytes}",
            bytes.len()
        );
    }

    let expected_time_seconds = info
        .bands
        .first()
        .and_then(band_metadata)
        .and_then(|metadata| metadata.get("GRIB_VALID_TIME"))
        .context("GFS band has no GRIB_VALID_TIME")?
        .parse::<i64>()
        .context("invalid GRIB_VALID_TIME")?;
    let mut fields = BTreeMap::new();
    for band in &info.bands {
        let metadata = band_metadata(band).context("GFS band has no default metadata")?;
        let element = metadata
            .get("GRIB_ELEMENT")
            .context("GFS band has no GRIB_ELEMENT")?;
        if !matches!(element.as_str(), "UGRD" | "VGRD" | "HGT" | "TMP") {
            continue;
        }
        let level_mb = parse_pressure_level_mb(
            metadata
                .get("GRIB_SHORT_NAME")
                .context("GFS band has no GRIB_SHORT_NAME")?,
        )?;
        if !pressure_levels_mb.contains(&level_mb) {
            continue;
        }
        let valid_time_seconds = metadata
            .get("GRIB_VALID_TIME")
            .context("GFS band has no GRIB_VALID_TIME")?
            .parse::<i64>()
            .context("invalid GRIB_VALID_TIME")?;
        if valid_time_seconds != expected_time_seconds {
            bail!("GFS bands contain inconsistent valid times");
        }
        let units = metadata.get("GRIB_UNIT").map(String::as_str).unwrap_or("");
        validate_units(element, units)?;
        let band_offset = band
            .band
            .checked_sub(1)
            .and_then(|index| index.checked_mul(cell_count))
            .and_then(|index| index.checked_mul(4))
            .context("GFS band offset overflow")?;
        let mut values = Vec::with_capacity(cell_count);
        for offset in (band_offset..band_offset + cell_count * 4).step_by(4) {
            let raw = f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
            values.push(if element == "TMP" && units == "[K]" {
                raw - 273.15
            } else {
                raw
            });
        }
        if fields.insert((level_mb, element.clone()), values).is_some() {
            bail!("duplicate GFS field {element} at {level_mb} mb");
        }
    }
    for level_mb in pressure_levels_mb {
        for element in ["UGRD", "VGRD", "HGT", "TMP"] {
            if !fields.contains_key(&(*level_mb, element.to_string())) {
                bail!(
                    "{} is missing {element} at {level_mb} mb; filtered GRIB is incomplete",
                    grib_path.display()
                );
            }
        }
    }
    Ok(DecodedForecast {
        width: info.size[0],
        height: info.size[1],
        longitude_origin_deg: info.geo_transform[0] + info.geo_transform[1] * 0.5,
        longitude_step_deg: info.geo_transform[1],
        latitude_origin_deg: info.geo_transform[3] + info.geo_transform[5] * 0.5,
        latitude_step_deg: info.geo_transform[5],
        valid_time_epoch_ms: expected_time_seconds * 1_000,
        fields,
    })
}

fn band_metadata(band: &GdalBand) -> Option<&BTreeMap<String, String>> {
    band.metadata
        .get("")
        .or_else(|| band.metadata.values().next())
}

fn parse_pressure_level_mb(short_name: &str) -> anyhow::Result<u32> {
    let pascals = short_name
        .strip_suffix("-ISBL")
        .context("GFS field is not on an isobaric surface")?
        .parse::<u32>()
        .context("invalid GFS isobaric pressure")?;
    if pascals % 100 != 0 {
        bail!("GFS pressure {pascals} Pa is not an integral millibar value");
    }
    Ok(pascals / 100)
}

fn validate_units(element: &str, units: &str) -> anyhow::Result<()> {
    let valid = match element {
        "UGRD" | "VGRD" => units == "[m/s]",
        "HGT" => units == "[gpm]" || units == "[m]",
        "TMP" => units == "[C]" || units == "[K]",
        _ => false,
    };
    if !valid {
        bail!("GFS field {element} has unsupported units {units}");
    }
    Ok(())
}

fn validate_same_grid(first: &DecodedForecast, next: &DecodedForecast) -> anyhow::Result<()> {
    let same = first.width == next.width
        && first.height == next.height
        && first.latitude_origin_deg == next.latitude_origin_deg
        && first.latitude_step_deg == next.latitude_step_deg
        && first.longitude_origin_deg == next.longitude_origin_deg
        && first.longitude_step_deg == next.longitude_step_deg;
    if !same {
        bail!("GFS forecasts do not share one source grid");
    }
    Ok(())
}

fn build_tile_pairs(
    forecasts: &[DecodedForecast],
    pressure_levels_mb: &[u32],
    grid: &AtmosphereGridContract,
) -> anyhow::Result<Vec<NavKvPair>> {
    let tile_rows = grid.row_count.div_ceil(grid.tile_row_count);
    let tile_columns = grid.column_count.div_ceil(grid.tile_column_count);
    let mut pairs = Vec::with_capacity((tile_rows * tile_columns) as usize);
    for tile_row in 0..tile_rows {
        for tile_column in 0..tile_columns {
            let row_start = tile_row * grid.tile_row_count;
            let column_start = tile_column * grid.tile_column_count;
            let row_count = grid.tile_row_count.min(grid.row_count - row_start);
            let column_count = grid.tile_column_count.min(grid.column_count - column_start);
            let sample_count = forecasts.len()
                * pressure_levels_mb.len()
                * row_count as usize
                * column_count as usize;
            let mut valid_mask = vec![0_u8; sample_count.div_ceil(8)];
            let mut east = Vec::with_capacity(sample_count);
            let mut north = Vec::with_capacity(sample_count);
            let mut temperature = Vec::with_capacity(sample_count);
            let mut height = Vec::with_capacity(sample_count);
            let mut sample_index = 0;
            for forecast in forecasts {
                for level_mb in pressure_levels_mb {
                    let east_grid = field(forecast, *level_mb, "UGRD")?;
                    let north_grid = field(forecast, *level_mb, "VGRD")?;
                    let temperature_grid = field(forecast, *level_mb, "TMP")?;
                    let height_grid = field(forecast, *level_mb, "HGT")?;
                    for local_row in 0..row_count {
                        for local_column in 0..column_count {
                            let source_index = (row_start + local_row) as usize * forecast.width
                                + (column_start + local_column) as usize;
                            let values = [
                                quantize(east_grid[source_index], ATMOSPHERE_WIND_UNITS_PER_MPS),
                                quantize(north_grid[source_index], ATMOSPHERE_WIND_UNITS_PER_MPS),
                                quantize(
                                    temperature_grid[source_index],
                                    ATMOSPHERE_TEMPERATURE_UNITS_PER_C,
                                ),
                                quantize(height_grid[source_index], ATMOSPHERE_HEIGHT_UNITS_PER_M),
                            ];
                            if values.iter().all(Option::is_some) {
                                valid_mask[sample_index / 8] |= 1 << (sample_index % 8);
                            }
                            east.push(values[0].unwrap_or_default());
                            north.push(values[1].unwrap_or_default());
                            temperature.push(values[2].unwrap_or_default());
                            height.push(values[3].unwrap_or_default());
                            sample_index += 1;
                        }
                    }
                }
            }
            let tile = AtmosphereTileV1 {
                schema_version: ATMOSPHERE_TILE_SCHEMA_VERSION,
                grid_row_start: row_start,
                grid_column_start: column_start,
                row_count,
                column_count,
                valid_mask,
                east_wind_tenth_mps_i16_le: pack_i16_le(east),
                north_wind_tenth_mps_i16_le: pack_i16_le(north),
                temperature_centi_c_i16_le: pack_i16_le(temperature),
                geopotential_height_m_i16_le: pack_i16_le(height),
            };
            let provisional_manifest = AtmosphereManifest {
                schema_version: ATMOSPHERE_MANIFEST_SCHEMA_VERSION,
                product_id: ATMOSPHERE_PRODUCT_ID.to_string(),
                version_label: String::new(),
                generated_at_utc: String::new(),
                model_id: String::new(),
                cycle_time_epoch_ms: 0,
                valid_times_epoch_ms: forecasts
                    .iter()
                    .map(|forecast| forecast.valid_time_epoch_ms)
                    .collect(),
                pressure_levels_mb: pressure_levels_mb.to_vec(),
                grid: grid.clone(),
                encoding: String::new(),
                tile_encoding: ATMOSPHERE_TILE_ENCODING.to_string(),
                array_order: ATMOSPHERE_ARRAY_ORDER.to_string(),
                root: String::new(),
                page_path_template: String::new(),
                page_count: 0,
                page_size: 0,
                logical_bytes_len: 0,
                value_bytes_len: 0,
                state_sha256: String::new(),
                tile_count: tile_rows * tile_columns,
            };
            tile.validate(&provisional_manifest)
                .map_err(anyhow::Error::msg)?;
            pairs.push(NavKvPair {
                key: atmosphere_tile_key(tile_row, tile_column),
                value: tile.encode_wire(),
            });
        }
    }
    Ok(pairs)
}

fn field<'a>(
    forecast: &'a DecodedForecast,
    level_mb: u32,
    element: &str,
) -> anyhow::Result<&'a [f32]> {
    forecast
        .fields
        .get(&(level_mb, element.to_string()))
        .map(Vec::as_slice)
        .with_context(|| format!("missing decoded {element} at {level_mb} mb"))
}

fn quantize(value: f32, scale: f64) -> Option<i16> {
    let value = f64::from(value) * scale;
    (value.is_finite() && value >= f64::from(i16::MIN) && value <= f64::from(i16::MAX))
        .then(|| value.round() as i16)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;
    use product_contracts::{unpack_i16_le, ATMOSPHERE_TILE_SCHEMA_VERSION};
    use tempfile::tempdir;

    fn forecast(valid_time_epoch_ms: i64, offset: f32) -> DecodedForecast {
        let mut fields = BTreeMap::new();
        for level in [1000_u32, 850] {
            for (element, base) in [("UGRD", 1.0), ("VGRD", 2.0), ("TMP", 3.0), ("HGT", 4.0)] {
                fields.insert(
                    (level, element.to_string()),
                    (0..12).map(|index| index as f32 + base + offset).collect(),
                );
            }
        }
        DecodedForecast {
            width: 4,
            height: 3,
            latitude_origin_deg: 50.0,
            latitude_step_deg: -0.25,
            longitude_origin_deg: -120.0,
            longitude_step_deg: 0.25,
            valid_time_epoch_ms,
            fields,
        }
    }

    #[test]
    fn tile_values_are_time_level_row_column_ordered() {
        let forecasts = vec![forecast(1_000, 0.0), forecast(2_000, 100.0)];
        let grid = AtmosphereGridContract {
            latitude_origin_deg: 50.0,
            latitude_step_deg: -0.25,
            longitude_origin_deg: -120.0,
            longitude_step_deg: 0.25,
            row_count: 3,
            column_count: 4,
            tile_row_count: 8,
            tile_column_count: 8,
        };
        let pairs = build_tile_pairs(&forecasts, &[1000, 850], &grid).unwrap();
        assert_eq!(pairs.len(), 1);
        let tile = AtmosphereTileV1::decode_wire(&pairs[0].value).unwrap();
        assert_eq!(tile.schema_version, ATMOSPHERE_TILE_SCHEMA_VERSION);
        assert_eq!(unpack_i16_le(&tile.east_wind_tenth_mps_i16_le, 0), Some(10));
        assert_eq!(
            unpack_i16_le(&tile.east_wind_tenth_mps_i16_le, 12),
            Some(10)
        );
        assert_eq!(
            unpack_i16_le(&tile.east_wind_tenth_mps_i16_le, 24),
            Some(1010)
        );
        assert!(tile.sample_is_valid(47));
    }

    #[test]
    fn pressure_parser_rejects_non_isobaric_and_fractional_mb_levels() {
        assert_eq!(parse_pressure_level_mb("85000-ISBL").unwrap(), 850);
        assert!(parse_pressure_level_mb("85000-SFC").is_err());
        assert!(parse_pressure_level_mb("85001-ISBL").is_err());
    }

    #[test]
    #[ignore = "requires AEROBAG_GFS_FIXTURE_DIR and AEROBAG_GFS_CYCLE_UTC"]
    fn external_filtered_grib_fixture_builds_only_nav_kv_members() {
        let input_dir = PathBuf::from(std::env::var("AEROBAG_GFS_FIXTURE_DIR").unwrap());
        let cycle_time_utc =
            DateTime::parse_from_rfc3339(&std::env::var("AEROBAG_GFS_CYCLE_UTC").unwrap())
                .unwrap()
                .with_timezone(&Utc);
        let temp = tempdir().unwrap();
        let result = build_atmosphere_dataset(&BuildAtmosphereDatasetRequest {
            input_dir,
            output_dir: temp.path().join("output"),
            decode_dir: temp.path().join("decoded"),
            version_label: "fixture".to_string(),
            cycle_time_utc,
            forecast_hours: vec![0, 3, 6, 9, 12],
            pressure_levels_mb: vec![1000, 925, 850, 700, 600, 500, 400, 300],
        })
        .unwrap();
        let members = fs::read_dir(&result.state_dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(members.contains(&"manifest.json".to_string()));
        assert!(members.contains(&"root".to_string()));
        assert!(members.iter().any(|member| member.starts_with("page_")));
        assert!(!members.iter().any(|member| {
            member.ends_with(".zip") || member.ends_with(".grib2") || member == "grib2"
        }));
        eprintln!(
            "atmosphere fixture: {} tiles, {} pages, {} value bytes",
            result.tile_count, result.manifest.page_count, result.manifest.value_bytes_len
        );
    }
}
