use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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
    EnrL,
    EnrH,
}

impl ChartFamily {
    pub fn capture_label(self) -> &'static str {
        match self {
            Self::Sec => "charts-sec",
            Self::Tac => "charts-tac",
            Self::EnrL => "charts-enr-l",
            Self::EnrH => "charts-enr-h",
        }
    }

    pub fn baseline_tile_count(self) -> Option<u64> {
        match self {
            Self::Sec => Some(ExpectedTileCounts::CURRENT_BASELINE.sec),
            Self::Tac => Some(ExpectedTileCounts::CURRENT_BASELINE.tac),
            Self::EnrL => Some(ExpectedTileCounts::CURRENT_BASELINE.enr_l),
            Self::EnrH => None,
        }
    }

    pub fn from_capture_label(label: &str) -> Option<Self> {
        match label {
            "charts-sec" => Some(Self::Sec),
            "charts-tac" => Some(Self::Tac),
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
            Self::Ne => &["NY", "ME", "VT", "NH", "MA", "RI", "CT", "NJ", "DE", "MD", "DC", "VA", "WV", "PA"],
            Self::Se => &["KY", "NC", "SC", "GA", "FL", "AL", "TN", "PR", "VI"],
        }
    }

    pub fn bounds(self) -> RegionBounds {
        match self {
            Self::Ak => RegionBounds {
                lon_min: -180.0,
                lat_max: 71.0,
                lon_max: -126.0,
                lat_min: 51.0,
            },
            Self::Pac => RegionBounds {
                lon_min: -162.0,
                lat_max: 24.0,
                lon_max: -152.0,
                lat_min: 18.0,
            },
            Self::Nw => RegionBounds {
                lon_min: -125.0,
                lat_max: 50.0,
                lon_max: -103.0,
                lat_min: 40.0,
            },
            Self::Sw => RegionBounds {
                lon_min: -125.0,
                lat_max: 40.0,
                lon_max: -103.0,
                lat_min: 15.0,
            },
            Self::Nc => RegionBounds {
                lon_min: -105.0,
                lat_max: 50.0,
                lon_max: -90.0,
                lat_min: 37.0,
            },
            Self::Ec => RegionBounds {
                lon_min: -95.0,
                lat_max: 50.0,
                lon_max: -80.0,
                lat_min: 37.0,
            },
            Self::Sc => RegionBounds {
                lon_min: -110.0,
                lat_max: 37.0,
                lon_max: -90.0,
                lat_min: 15.0,
            },
            Self::Ne => RegionBounds {
                lon_min: -80.0,
                lat_max: 50.0,
                lon_max: -60.0,
                lat_min: 37.0,
            },
            Self::Se => RegionBounds {
                lon_min: -90.0,
                lat_max: 37.0,
                lon_max: -60.0,
                lat_min: 15.0,
            },
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
