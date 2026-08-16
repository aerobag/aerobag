// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use anyhow::{bail, Context};

pub const PACKAGE_ASSET_MANIFEST_NAME: &str = "package-assets.json";
pub const CHART_REFERENCE_MANIFEST_DIR: &str = "chart-references";

pub fn is_enroute_navaid_type(kind: &str) -> bool {
    matches!(
        kind.trim().to_ascii_uppercase().as_str(),
        "VOR" | "VOR/DME" | "VORTAC"
    )
}

pub fn airport_location_label(city: &str, state: &str) -> Option<String> {
    let city = titlecase_words(city.trim());
    let state = state.trim().to_ascii_uppercase();
    match (city.is_empty(), state.is_empty()) {
        (true, true) => None,
        (false, true) => Some(city),
        (true, false) => Some(state),
        (false, false) => Some(format!("{city}, {state}")),
    }
}

fn titlecase_words(value: &str) -> String {
    value
        .split_whitespace()
        .map(|word| {
            let mut capitalize = true;
            word.chars()
                .flat_map(|ch| {
                    let rendered = if capitalize {
                        ch.to_uppercase().collect::<String>()
                    } else {
                        ch.to_lowercase().collect::<String>()
                    };
                    capitalize = ch == '-' || ch == '\'';
                    rendered.chars().collect::<Vec<_>>()
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub use had_nav_kv as nav_kv;
pub mod runway;

static XZ_TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn xz_compress_file_with_system_xz(source: &Path) -> anyhow::Result<Vec<u8>> {
    let output = Command::new("xz")
        .arg("--format=xz")
        .arg("--check=crc64")
        .arg("-6")
        .arg("--stdout")
        .arg("--threads=1")
        .arg(source)
        .output()
        .with_context(|| format!("failed to run xz for {}", source.display()))?;
    if !output.status.success() {
        bail!(
            "xz failed for {}: {}",
            source.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output.stdout)
}

pub fn xz_compress_bytes_with_system_xz(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    let counter = XZ_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("aerobag-xz-input-{}-{counter}", std::process::id()));
    fs::write(&path, bytes).with_context(|| format!("failed to write {}", path.display()))?;
    let result = xz_compress_file_with_system_xz(&path);
    let remove_result = fs::remove_file(&path);
    if let Err(err) = remove_result {
        if result.is_ok() {
            return Err(err).with_context(|| format!("failed to remove {}", path.display()));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn airport_locations_are_friendly_and_optional() {
        assert_eq!(
            airport_location_label("MOAB", "ut").as_deref(),
            Some("Moab, UT")
        );
        assert_eq!(
            airport_location_label("O'NEILL", "NE").as_deref(),
            Some("O'Neill, NE")
        );
        assert_eq!(airport_location_label("", "").as_deref(), None);
    }

    #[test]
    fn system_xz_bytes_materially_compress_repetitive_payload() {
        let raw = b"ABCD".repeat(16 * 1024);
        let encoded = xz_compress_bytes_with_system_xz(&raw).unwrap();
        assert!(
            encoded.len() < raw.len() / 4,
            "system xz did not materially compress fixture: raw={} encoded={}",
            raw.len(),
            encoded.len()
        );
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PackageAssetManifest {
    pub schema_version: u32,
    pub family_id: String,
    pub package_id: String,
    pub assets: Vec<PackageAssetRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PackageAssetRecord {
    pub id: String,
    pub airport_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icao_airport_id: Option<String>,
    pub label: String,
    pub asset_kind: String,
    pub document_type: String,
    pub asset_path: String,
    pub thumbnail_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub procedure_uid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cifp_procedure_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub georef: Option<PlateGeoref>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChartReferenceManifest {
    pub schema_version: u32,
    pub family_id: String,
    pub package_id: String,
    pub assets: Vec<ChartReferenceAssetRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChartReferenceAssetRecord {
    pub id: String,
    pub family_id: String,
    pub source_chart_id: String,
    pub label: String,
    pub kind: String,
    pub asset_path: String,
    pub thumbnail_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_coverage: Option<ChartReferenceCoverage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChartPackageCollection {
    pub family_id: String,
    pub chart_index: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct ChartReferenceCoverage {
    pub lat_min: f64,
    pub lat_max: f64,
    pub lon_min: f64,
    pub lon_max: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlateGeoref {
    PlateTransformV1 {
        pixels_per_longitude: f64,
        pixels_per_latitude: f64,
        top_left_lon: f64,
        top_left_lat: f64,
    },
    AirportDiagramTransformV1 {
        pixel_x_from_lon: f64,
        pixel_x_from_lat: f64,
        pixel_x_offset: f64,
        pixel_y_from_lon: f64,
        pixel_y_from_lat: f64,
        pixel_y_offset: f64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureManifest {
    pub schema_version: u32,
    pub run_id: String,
    pub created_at_utc: String,
    pub captures: Vec<CaptureEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureEntry {
    pub label: String,
    pub repo: String,
    pub command: Vec<String>,
    pub stdout_log: String,
    pub stderr_log: String,
    #[serde(default)]
    pub tile_paths: Option<String>,
    pub outputs_hashes: String,
    #[serde(default)]
    pub source_urls: Option<String>,
    #[serde(default)]
    pub downloads: Option<String>,
    #[serde(default)]
    pub package_outputs: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpectedTileCounts {
    pub sec: u64,
    pub tac: u64,
    pub enr_l: u64,
}

impl ExpectedTileCounts {
    pub const CURRENT_BASELINE: Self = Self {
        sec: 35_494,
        tac: 7_174,
        enr_l: 27_428,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartFamily {
    Sec,
    Tac,
    Flyway,
    EnrL,
    EnrH,
}

impl ChartFamily {
    pub fn capture_label(self) -> &'static str {
        match self {
            Self::Sec => "charts-sec",
            Self::Tac => "charts-tac",
            Self::Flyway => "charts-flyway",
            Self::EnrL => "charts-enr-l",
            Self::EnrH => "charts-enr-h",
        }
    }

    pub fn baseline_tile_count(self) -> Option<u64> {
        match self {
            Self::Sec => Some(ExpectedTileCounts::CURRENT_BASELINE.sec),
            Self::Tac => Some(ExpectedTileCounts::CURRENT_BASELINE.tac),
            Self::Flyway => None,
            Self::EnrL => Some(ExpectedTileCounts::CURRENT_BASELINE.enr_l),
            Self::EnrH => None,
        }
    }

    pub fn from_capture_label(label: &str) -> Option<Self> {
        match label {
            "charts-sec" => Some(Self::Sec),
            "charts-tac" => Some(Self::Tac),
            "charts-flyway" => Some(Self::Flyway),
            "charts-enr-l" => Some(Self::EnrL),
            "charts-enr-h" => Some(Self::EnrH),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RegionBounds {
    pub lon_min: f64,
    pub lat_max: f64,
    pub lon_max: f64,
    pub lat_min: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Region {
    Ak,
    Pac,
    Nw,
    Sw,
    Nc,
    Ec,
    Sc,
    Ne,
    Se,
}

impl Region {
    pub const ALL: [Self; 9] = [
        Self::Ak,
        Self::Pac,
        Self::Nw,
        Self::Sw,
        Self::Nc,
        Self::Ec,
        Self::Sc,
        Self::Ne,
        Self::Se,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Self::Ak => "AK",
            Self::Pac => "PAC",
            Self::Nw => "NW",
            Self::Sw => "SW",
            Self::Nc => "NC",
            Self::Ec => "EC",
            Self::Sc => "SC",
            Self::Ne => "NE",
            Self::Se => "SE",
        }
    }

    pub fn from_code(value: &str) -> Option<Self> {
        match value {
            "AK" => Some(Self::Ak),
            "PAC" => Some(Self::Pac),
            "NW" => Some(Self::Nw),
            "SW" => Some(Self::Sw),
            "NC" => Some(Self::Nc),
            "EC" => Some(Self::Ec),
            "SC" => Some(Self::Sc),
            "NE" => Some(Self::Ne),
            "SE" => Some(Self::Se),
            _ => None,
        }
    }

    pub fn state_codes(self) -> &'static [&'static str] {
        match self {
            Self::Ak => &["AK"],
            Self::Pac => &["HI", "XX"],
            Self::Nw => &["WA", "MT", "WY", "ID", "OR"],
            Self::Sw => &["CA", "NV", "UT", "CO", "NM", "AZ"],
            Self::Nc => &["ND", "MN", "IA", "MO", "KS", "NE", "SD"],
            Self::Ec => &["WI", "MI", "OH", "IN", "IL"],
            Self::Sc => &["OK", "AR", "MS", "LA", "TX"],
            Self::Ne => &[
                "NY", "ME", "VT", "NH", "MA", "RI", "CT", "NJ", "DE", "MD", "DC", "VA", "WV", "PA",
            ],
            Self::Se => &["KY", "NC", "SC", "GA", "FL", "AL", "TN", "PR", "VI"],
        }
    }

    pub fn bounds(self) -> RegionBounds {
        self.bounds_list()[0]
    }

    pub fn bounds_list(self) -> &'static [RegionBounds] {
        match self {
            Self::Ak => {
                // The Alaska chart region crosses the antimeridian. Keep it as two
                // ordinary lon ranges so tile and app code never has to reason about a
                // single box that wraps around +/-180.
                &[
                    RegionBounds {
                        lon_min: -180.0,
                        lat_max: 72.0,
                        lon_max: -126.0,
                        lat_min: 51.0,
                    },
                    RegionBounds {
                        lon_min: 170.0,
                        lat_max: 56.0,
                        lon_max: 180.0,
                        lat_min: 51.0,
                    },
                ]
            }
            Self::Pac => {
                // PAC is not just Hawaii. The FAA Pacific SEC source includes
                // detailed island coverage for Hawaii, Samoa, and Guam/NMI; packaging
                // must admit all of those rectangles or the high-zoom source tiles get
                // thrown away before clients can probe them.
                &[
                    RegionBounds {
                        lon_min: -162.0,
                        lat_max: 24.0,
                        lon_max: -152.0,
                        lat_min: 18.0,
                    },
                    RegionBounds {
                        lon_min: -174.0,
                        lat_max: -11.0,
                        lon_max: -168.0,
                        lat_min: -16.0,
                    },
                    RegionBounds {
                        lon_min: 140.0,
                        lat_max: 22.0,
                        lon_max: 147.0,
                        lat_min: 10.0,
                    },
                ]
            }
            Self::Nw => &[RegionBounds {
                lon_min: -125.0,
                lat_max: 50.0,
                lon_max: -103.0,
                lat_min: 40.0,
            }],
            Self::Sw => &[RegionBounds {
                lon_min: -125.0,
                lat_max: 40.0,
                lon_max: -103.0,
                lat_min: 15.0,
            }],
            Self::Nc => &[RegionBounds {
                lon_min: -105.0,
                lat_max: 50.0,
                lon_max: -90.0,
                lat_min: 37.0,
            }],
            Self::Ec => &[RegionBounds {
                lon_min: -95.0,
                lat_max: 50.0,
                lon_max: -80.0,
                lat_min: 37.0,
            }],
            Self::Sc => &[RegionBounds {
                lon_min: -110.0,
                lat_max: 37.0,
                lon_max: -90.0,
                lat_min: 15.0,
            }],
            Self::Ne => &[RegionBounds {
                lon_min: -80.0,
                lat_max: 50.0,
                lon_max: -60.0,
                lat_min: 37.0,
            }],
            Self::Se => &[RegionBounds {
                lon_min: -90.0,
                lat_max: 37.0,
                lon_max: -60.0,
                lat_min: 14.0,
            }],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkKind {
    Network,
    Extract,
    Cpu,
    Io,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parallelism {
    Serial,
    Bounded,
    Wide,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConcurrencyConfig {
    pub fetch_jobs: usize,
    pub extract_jobs: usize,
    pub cpu_jobs: usize,
    pub zip_jobs: usize,
}

impl ConcurrencyConfig {
    pub fn recommended_for_machine(cpus: usize) -> Self {
        let fetch_jobs = cpus.clamp(4, 12);
        let extract_jobs = (cpus / 2).clamp(2, 8);
        let cpu_jobs = cpus.saturating_sub(2).max(1);
        let zip_jobs = (cpus / 4).clamp(1, 4);
        Self {
            fetch_jobs,
            extract_jobs,
            cpu_jobs,
            zip_jobs,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhasePlan {
    pub name: &'static str,
    pub work_kind: WorkKind,
    pub legacy_parallelism: Parallelism,
    pub rust_parallelism: Parallelism,
    pub recommended_jobs: usize,
    pub expected_bottleneck: &'static str,
    pub note: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheMode {
    Online,
    Refresh,
    Offline,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunPaths {
    pub root: PathBuf,
    pub logs: PathBuf,
    pub artifacts: PathBuf,
    pub meta: PathBuf,
}

impl RunPaths {
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();
        Self {
            logs: root.join("logs"),
            artifacts: root.join("artifacts"),
            meta: root.join("meta"),
            root,
        }
    }
}
