use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use anyhow::{bail, Context};
use quick_xml::{events::Event, Reader};
use rusqlite::{params, Connection};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

mod tpp_cifp_matching;

pub use tpp_cifp_matching::{
    audit_tpp_cifp_matching, build_data_package_with_tpp_matches,
    choose_bundle as choose_matching_bundle, load_bundle as load_matching_bundle,
    publish_tpp_cifp_matches, resolve_db_path as resolve_matching_db_path,
    tpp_zip_paths_from_bundle, DataTppMatchRequest, DataTppMatchResult,
    PublishedMatchSummary, TppCifpAuditReport,
};

pub const INTERMEDIATE_SQLITE_BASENAME: &str = "intermediate-sqlite.db";

const TABLES: &[&str] = &[
    "airports",
    "airportfreq",
    "airportrunways",
    "nav",
    "fix",
    "awos",
    "saa",
    "airways",
    "cifp_sid_star_app",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataBuildMode {
    Production,
    LegacyAvare,
}

impl DataBuildMode {
    pub fn parse(value: &str) -> anyhow::Result<Self> {
        match value {
            "production" => Ok(Self::Production),
            "legacy_avare" => Ok(Self::LegacyAvare),
            other => bail!("unsupported data build mode: {other}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DataBuildRequest {
    pub input_dir: PathBuf,
    pub output_dir: PathBuf,
    pub manifest_version: String,
    pub mode: DataBuildMode,
    pub artifact_stem: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DataBuildResult {
    pub main_db: PathBuf,
    pub manifest_path: PathBuf,
    pub zip_path: PathBuf,
    pub row_counts: BTreeMap<String, usize>,
}

fn field(line: &str, start: usize, len: usize) -> &str {
    let bytes = line.as_bytes();
    if start >= bytes.len() {
        return "";
    }
    let end = (start + len).min(bytes.len());
    std::str::from_utf8(&bytes[start..end]).unwrap_or("")
}

fn trim(s: &str) -> &str {
    s.trim()
}

fn airport_faa_id(line: &str) -> String {
    trim(field(line, 27, 4)).to_string()
}

fn airport_icao_id(line: &str) -> String {
    trim(field(line, 1210, 7)).to_string()
}

fn canonical_airport_id_from_apt_line(line: &str) -> String {
    let faa = airport_faa_id(line);
    let icao = airport_icao_id(line);
    if icao.is_empty() {
        faa
    } else {
        icao
    }
}

fn load_airport_id_map(input_dir: &Path) -> anyhow::Result<BTreeMap<String, String>> {
    let path = input_dir.join("APT.txt");
    let text = read_text_lossy(&path)?;
    let mut ids = BTreeMap::new();
    for raw in text.lines() {
        if !raw.starts_with("APT") {
            continue;
        }
        let faa = airport_faa_id(raw);
        let canonical = canonical_airport_id_from_apt_line(raw);
        if !faa.is_empty() && !canonical.is_empty() {
            ids.insert(faa, canonical);
        }
    }
    Ok(ids)
}

fn canonicalize_airport_id(raw_id: &str, airport_ids: &BTreeMap<String, String>) -> String {
    let raw_id = raw_id.trim();
    airport_ids
        .get(raw_id)
        .cloned()
        .unwrap_or_else(|| raw_id.to_string())
}

fn read_text_lossy(path: &Path) -> anyhow::Result<String> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn perl_num(s: &str) -> f64 {
    let s = s.trim_start();
    let mut end = 0;
    let bytes = s.as_bytes();
    if matches!(bytes.first(), Some(b'+') | Some(b'-')) {
        end = 1;
    }
    let mut seen_digit = false;
    let mut seen_dot = false;
    while end < bytes.len() {
        let b = bytes[end];
        if b.is_ascii_digit() {
            seen_digit = true;
            end += 1;
        } else if b == b'.' && !seen_dot {
            seen_dot = true;
            end += 1;
        } else {
            break;
        }
    }
    if !seen_digit {
        0.0
    } else {
        s[..end].parse::<f64>().unwrap_or(0.0)
    }
}

fn sanitize_commas_quotes(mut s: String) -> String {
    s = s.replace(',', " ");
    s.replace('"', " ")
}

fn apt_coord_lat(value: &str) -> f64 {
    let deg = perl_num(trim(field(value, 0, 2)));
    let min = perl_num(trim(field(value, 3, 2))) / 60.0;
    let sec = perl_num(trim(field(value, 6, 7))) / 3600.0;
    let hemi = field(value, 13, 1);
    let coord = deg + min + sec;
    if hemi == "N" {
        coord
    } else {
        -coord
    }
}

fn apt_coord_lon(value: &str) -> f64 {
    let deg = perl_num(trim(field(value, 0, 3)));
    let min = perl_num(trim(field(value, 4, 2))) / 60.0;
    let sec = perl_num(trim(field(value, 7, 7))) / 3600.0;
    let hemi = field(value, 14, 1);
    let coord = deg + min + sec;
    if hemi == "W" {
        -coord
    } else {
        coord
    }
}

fn nav_fix_lat(value: &str) -> f64 {
    let deg = perl_num(trim(field(value, 0, 2)));
    let min = perl_num(trim(field(value, 3, 2))) / 60.0;
    let sec = perl_num(trim(field(value, 6, 7))) / 3600.0;
    let hemi = field(value, 12, 1);
    let coord = deg + min + sec;
    if hemi == "N" {
        coord
    } else {
        -coord
    }
}

fn nav_fix_lon(value: &str) -> f64 {
    let deg = perl_num(trim(field(value, 0, 3)));
    let min = perl_num(trim(field(value, 4, 2))) / 60.0;
    let sec = perl_num(trim(field(value, 7, 6))) / 3600.0;
    let hemi = field(value, 13, 1);
    let coord = deg + min + sec;
    if hemi == "W" {
        -coord
    } else {
        coord
    }
}

fn awy_coord(value: &str) -> f64 {
    let mut chars = value.chars().collect::<Vec<_>>();
    let hemi = chars.pop().unwrap_or(' ');
    let text = chars.into_iter().collect::<String>();
    let parts = text.split('-').map(trim).collect::<Vec<_>>();
    if parts.len() != 3 {
        return 0.0;
    }
    let coord = perl_num(parts[0]) + perl_num(parts[1]) / 60.0 + perl_num(parts[2]) / 3600.0;
    match hemi {
        'S' | 'W' => -coord,
        _ => coord,
    }
}

fn awos_coord_lat_bug(value: &str) -> Option<f64> {
    if value.len() != 14 {
        return None;
    }
    let deg = perl_num(trim(field(value, 0, 3)));
    let min = perl_num(trim(field(value, 4, 2))) / 60.0;
    let sec = perl_num(trim(field(value, 7, 7))) / 3600.0;
    let hemi = field(value, 0, 1);
    let coord = deg + min + sec;
    Some(if hemi == "N" { coord } else { -coord })
}

fn awos_coord_lon_bug(value: &str) -> Option<f64> {
    if value.len() != 14 {
        return None;
    }
    let deg = perl_num(trim(field(value, 0, 3)));
    let min = perl_num(trim(field(value, 4, 2))) / 60.0;
    let sec = perl_num(trim(field(value, 7, 7))) / 3600.0;
    let hemi = field(value, 14, 1);
    let coord = deg + min + sec;
    Some(if hemi == "W" { -coord } else { coord })
}

fn setup_schema(conn: &Connection, mode: DataBuildMode) -> anyhow::Result<()> {
    let base = "
CREATE TABLE airports(LocationID Text,ARPLatitude float,ARPLongitude float,Type Text,FacilityName Text,Use Text,FSSPhone Text,Manager Text,ManagerPhone Text,ARPElevation Text,MagneticVariation Text,TrafficPatternAltitude Text,FuelTypes Text,Customs Text,Beacon Text,LightSchedule Text,SegCircle Text,ATCT Text,UNICOMFrequencies Text,CTAFFrequency Text,NonCommercialLandingFee Text,State Text, City Text, UNIQUE(LocationID));
CREATE TABLE airport_aliases(alias_id Text, airport_id Text, UNIQUE(alias_id));
CREATE TABLE airportfreq(LocationID Text,Type Text, Freq Text);
CREATE TABLE airportrunways(LocationID Text,Length Text,Width Text,Surface Text,LEIdent Text,HEIdent Text,LELatitude Text,HELatitude Text,LELongitude Text,HELongitude Text,LEElevation Text,HEElevation Text,LEHeadingT Text,HEHeading Text,LEDT Text,HEDT Text,LELights Text,HELights Text,LEILS Text,HEILS Text,LEVGSI Text,HEVGSI Text,LEPattern Text, HEPattern Text);
CREATE TABLE nav(LocationID Text,ARPLatitude float,ARPLongitude float,Type Text,FacilityName Text,Variation TinyInt,Class Text,Hiwas Text,Elevation Text);
CREATE TABLE fix(LocationID Text,ARPLatitude float,ARPLongitude float,Type Text,FacilityName Text);
CREATE TABLE awos(LocationID Text, Type Text, Status Text, Latitude float,Longitude float, Elevation Text, Frequency1 Text, Frequency2 Text, Telephone1 Text, Telephone2 Text, Remark Text);
CREATE TABLE saa(designator TEXT,name TEXT,upperlimit TEXT,lowerlimit TEXT,begintime TEXT,endtime TEXT,timeref TEXT,beginday TEXT,endday TEXT,day TEXT,FreqTx TEXT,FreqRx TEXT,lat FLOAT,lon FLOAT);
-- `recommended_navaid` / `recd_nav_*` are inherited ARINC-style names from the CIFP
-- fixed-width SID/STAR/approach record layout. FAA says CIFP follows ARINC 424, and
-- public ARINC 424 field-list mirrors use the term `Recommended Navaid`, so we keep
-- that naming here instead of renaming it to something local and less traceable.
CREATE TABLE cifp_sid_star_app(record_type Text,customer_area_code Text,section_code Text,airport_identifier Text,icao_code_1 Text,subsection_code Text,sid_star_approach_identifier Text,route_type Text,transition_identifier Text,sequence_number Text,fix_identifier Text,icao_code_2 Text,section_code_2 Text,subsection_code_2 Text,continuation_record_number Text,waypoint_description_code Text,turn_direction Text,rnp Text,path_and_termination Text,turn_direction_valid Text,recommended_navaid Text,icao_code_3 Text,arc_radius Text,theta Text,rho Text,magnetic_course Text,route_distance_holding_distance_or_time Text,recd_nav_section Text,recd_nav_subsection Text,reserved Text,altitude_description Text,atc_indicator Text,altitude_1 Text,altitude_2 Text,transition_altitude Text,speed_limit Text,vertical_angle Text,center_fix_or_taa_procedure_turn_indicator Text,multiple_code_or_taa_sector_identifier Text,icao_code_4 Text,section_code_3 Text,subsection_code_3 Text,gps_fms_indication Text,speed_limit_description Text,apch_route_qualifier_1 Text,apch_route_qualifier_2 Text,file_record_number Text,cycle_date Text);
CREATE TABLE geo(Latitude float, Longitude float, height float, declination float);
";
    let airway_schema = match mode {
        DataBuildMode::Production => {
            "CREATE TABLE airways_branch(name Text, branch_key Text, sequence_number Integer, sequence_token Text, point_name Text, Latitude float, Longitude float);
CREATE INDEX idx_airways_branch_name_branch_sequence ON airways_branch(name, branch_key, sequence_number);
CREATE INDEX idx_airways_branch_lat_lon ON airways_branch(Latitude, Longitude);"
        }
        DataBuildMode::LegacyAvare => {
            "CREATE TABLE obs(ARPLatitude float,ARPLongitude float,Height float);
CREATE TABLE airways(name Text, sequence Text, Latitude float, Longitude float);"
        }
    };
    conn.execute_batch(&format!("{base}\n{airway_schema}\n"))
    .context("failed to create data schema")?;
    Ok(())
}

fn insert_airports(conn: &Connection, input_dir: &Path) -> anyhow::Result<usize> {
    let path = input_dir.join("APT.txt");
    let text = read_text_lossy(&path)?;
    let mut stmt = conn.prepare("INSERT INTO airports VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)")?;
    let mut count = 0;
    for raw in text.lines() {
        if !raw.starts_with("APT") {
            continue;
        }
        let line = sanitize_commas_quotes(raw.to_string());
        let id = canonical_airport_id_from_apt_line(&line);
        let kind = trim(field(&line, 14, 12)).to_string();
        let name = trim(field(&line, 133, 50)).to_string();
        let state = trim(field(&line, 91, 2)).to_string();
        let city = trim(field(&line, 93, 40)).to_string();
        let manager = trim(field(&line, 355, 35)).to_string();
        let manager_phone = trim(field(&line, 507, 16)).to_string();
        let lat = apt_coord_lat(trim(field(&line, 523, 14)));
        let lon = apt_coord_lon(trim(field(&line, 550, 15)));
        let variation = trim(field(&line, 586, 3)).to_string();
        let fuel_chunk = trim(field(&line, 900, 40)).to_string();
        let mut fuel_parts = Vec::new();
        let mut idx = 0;
        while idx < fuel_chunk.len() {
            let end = (idx + 5).min(fuel_chunk.len());
            let mut fuel = trim(&fuel_chunk[idx..end]).to_string();
            fuel = match fuel.as_str() {
                s if s.starts_with('A') => fuel.replacen('A', "JET-A", 1),
                s if s.starts_with('B') => fuel.replacen('B', "JET-B", 1),
                "80" => "80(RED)".to_string(),
                "100" => "100(GREEN)".to_string(),
                "100LL" => "100LL(BLUE)".to_string(),
                _ => fuel,
            };
            if !fuel.is_empty() {
                fuel_parts.push(fuel);
            }
            idx += 5;
        }
        let fuel = fuel_parts.join(" ");
        let use_type = trim(field(&line, 185, 2)).to_string();
        let elevation = trim(field(&line, 578, 7)).to_string();
        let pattern = trim(field(&line, 593, 4)).to_string();
        let ctaf = trim(field(&line, 988, 7)).to_string();
        let unicom = trim(field(&line, 981, 7)).to_string();
        let atct = trim(field(&line, 980, 1)).to_string();
        let fee = trim(field(&line, 1002, 1)).to_string();
        let lightsched = trim(field(&line, 966, 7)).to_string();
        let segcircle = trim(field(&line, 995, 4)).to_string();
        let customs = format!(
            "{}{}",
            trim(field(&line, 877, 1)),
            trim(field(&line, 878, 1))
        );
        let beacon = trim(field(&line, 999, 3)).to_string();
        let tel = trim(field(&line, 762, 16)).to_string();
        stmt.execute(params![
            id,
            lat,
            lon,
            kind,
            name,
            use_type,
            tel,
            manager,
            manager_phone,
            elevation,
            variation,
            pattern,
            fuel,
            customs,
            beacon,
            lightsched,
            segcircle,
            atct,
            unicom,
            ctaf,
            fee,
            state,
            city
        ])?;
        count += 1;
    }
    Ok(count)
}

fn insert_airport_aliases(conn: &Connection, input_dir: &Path) -> anyhow::Result<usize> {
    let path = input_dir.join("APT.txt");
    let text = read_text_lossy(&path)?;
    let mut stmt = conn.prepare("INSERT OR IGNORE INTO airport_aliases VALUES (?1, ?2)")?;
    let mut count = 0;
    for raw in text.lines() {
        if !raw.starts_with("APT") {
            continue;
        }
        let faa = airport_faa_id(raw);
        let canonical = canonical_airport_id_from_apt_line(raw);
        if canonical.is_empty() {
            continue;
        }
        for alias in [faa, canonical.clone()] {
            if alias.is_empty() {
                continue;
            }
            count += stmt.execute(params![alias, canonical])?;
        }
    }
    Ok(count)
}

fn insert_airport_freq_with_ids(
    conn: &Connection,
    input_dir: &Path,
    airport_ids: &BTreeMap<String, String>,
) -> anyhow::Result<usize> {
    let path = input_dir.join("TWR.txt");
    let text = read_text_lossy(&path)?;
    let mut stmt = conn.prepare("INSERT INTO airportfreq VALUES (?1, ?2, ?3)")?;
    let mut id = String::new();
    let mut count = 0;
    for raw in text.lines() {
        if raw.starts_with("TWR1") {
            id = canonicalize_airport_id(field(raw, 4, 4), airport_ids);
        } else if raw.starts_with("TWR3") {
            let mut rest = field(raw, 8, raw.len().saturating_sub(8)).to_string();
            while rest.len() > 93 {
                let freq = trim(field(&rest, 0, 44)).replace(',', ";");
                let kind = trim(field(&rest, 44, 50)).replace(',', ";");
                rest = field(&rest, 94, rest.len().saturating_sub(94)).to_string();
                if ["ATIS", "GND", "LCL", "EMERG", "GATE", "CD"]
                    .iter()
                    .any(|needle| kind.contains(needle))
                {
                    stmt.execute(params![id, kind, freq])?;
                    count += 1;
                }
            }
        } else if raw.starts_with("TWR6") {
            let remark = trim(field(raw, 13, raw.len().saturating_sub(13))).replace(',', " ");
            stmt.execute(params![id, "Remark", remark])?;
            count += 1;
        }
    }
    Ok(count)
}

fn insert_runways(conn: &Connection, input_dir: &Path) -> anyhow::Result<usize> {
    let path = input_dir.join("APT.txt");
    let text = read_text_lossy(&path)?;
    let mut stmt = conn.prepare("INSERT INTO airportrunways VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)")?;
    let mut id = String::new();
    let mut count = 0;
    for raw in text.lines() {
        if raw.starts_with("APT") {
            id = canonical_airport_id_from_apt_line(raw);
            continue;
        }
        if !raw.starts_with("RWY") {
            continue;
        }
        let len = trim(field(raw, 23, 5)).to_string();
        let width = trim(field(raw, 28, 4)).to_string();
        let surface = trim(field(raw, 32, 12)).to_string();
        let run0 = trim(field(raw, 65, 3)).to_string();
        let run0lat_s = trim(field(raw, 88, 14));
        let run0lat = if run0lat_s.len() == 14 {
            apt_coord_lat(run0lat_s).to_string()
        } else {
            String::new()
        };
        let run0lon_s = trim(field(raw, 115, 15));
        let run0lon = if run0lon_s.len() == 15 {
            apt_coord_lon(run0lon_s).to_string()
        } else {
            String::new()
        };
        let run0elev = trim(field(raw, 142, 7)).to_string();
        let run0true = trim(field(raw, 68, 3)).to_string();
        let run0dt = trim(field(raw, 217, 4)).to_string();
        let run0light = trim(field(raw, 237, 8)).to_string();
        let run0ils = trim(field(raw, 71, 10)).to_string();
        let run0vgsi = trim(field(raw, 228, 5)).to_string();
        let run0pattern = trim(field(raw, 81, 1)).to_string();
        let run1 = trim(field(raw, 287, 3)).to_string();
        let run1lat_s = trim(field(raw, 310, 14));
        let run1lat = if run1lat_s.len() == 14 {
            apt_coord_lat(run1lat_s).to_string()
        } else {
            String::new()
        };
        let run1lon_s = trim(field(raw, 337, 15));
        let run1lon = if run1lon_s.len() == 15 {
            apt_coord_lon(run1lon_s).to_string()
        } else {
            String::new()
        };
        let run1elev = trim(field(raw, 364, 7)).to_string();
        let run1true = trim(field(raw, 290, 3)).to_string();
        let run1dt = trim(field(raw, 439, 4)).to_string();
        let run1light = trim(field(raw, 459, 8)).to_string();
        let run1ils = trim(field(raw, 293, 10)).to_string();
        let run1vgsi = trim(field(raw, 450, 5)).to_string();
        let run1pattern = trim(field(raw, 303, 1)).to_string();
        stmt.execute(params![
            id,
            len,
            width,
            surface,
            run0,
            run1,
            run0lat,
            run1lat,
            run0lon,
            run1lon,
            run0elev,
            run1elev,
            run0true,
            run1true,
            run0dt,
            run1dt,
            run0light,
            run1light,
            run0ils,
            run1ils,
            run0vgsi,
            run1vgsi,
            run0pattern,
            run1pattern
        ])?;
        count += 1;
    }
    Ok(count)
}

fn insert_nav(conn: &Connection, input_dir: &Path) -> anyhow::Result<usize> {
    let path = input_dir.join("NAV.txt");
    let text = read_text_lossy(&path)?;
    let mut stmt = conn.prepare("INSERT INTO nav VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)")?;
    let mut count = 0;
    for raw in text.lines() {
        if !raw.starts_with("NAV1") {
            continue;
        }
        let id = trim(field(raw, 4, 4)).to_string();
        let kind = trim(field(raw, 8, 10)).to_string();
        let name =
            format!("{} {}", trim(field(raw, 42, 30)), trim(field(raw, 533, 7))).replace(',', ";");
        let lat = nav_fix_lat(trim(field(raw, 371, 13)));
        let lon = nav_fix_lon(trim(field(raw, 396, 14)));
        let var = trim(field(raw, 479, 5));
        let var_last = var.chars().last().unwrap_or(' ');
        let variation = perl_num(var) * if var_last == 'E' { 1.0 } else { -1.0 };
        let class = trim(field(raw, 281, 1)).to_string();
        let hiwas = trim(field(raw, 800, 1)).to_string();
        let elevation = trim(field(raw, 472, 7)).to_string();
        stmt.execute(params![
            id, lat, lon, kind, name, variation, class, hiwas, elevation
        ])?;
        count += 1;
    }
    Ok(count)
}

fn insert_fix(conn: &Connection, input_dir: &Path) -> anyhow::Result<usize> {
    let path = input_dir.join("FIX.txt");
    let text = read_text_lossy(&path)?;
    let mut stmt = conn.prepare("INSERT INTO fix VALUES (?1, ?2, ?3, ?4, ?5)")?;
    let mut count = 0;
    for raw in text.lines() {
        if !raw.starts_with("FIX1") {
            continue;
        }
        let id = trim(field(raw, 4, 6)).to_string();
        let kind = trim(field(raw, 212, 15)).to_string();
        let name =
            format!("{} {}", trim(field(raw, 4, 7)), trim(field(raw, 141, 10))).replace(',', ";");
        let lat = nav_fix_lat(trim(field(raw, 66, 13)));
        let lon = nav_fix_lon(trim(field(raw, 80, 14)));
        stmt.execute(params![id, lat, lon, kind, name])?;
        count += 1;
    }
    Ok(count)
}

fn insert_obs(conn: &Connection, input_dir: &Path) -> anyhow::Result<usize> {
    let path = input_dir.join("DOF.DAT");
    let text = read_text_lossy(&path)?;
    let mut stmt = conn.prepare("INSERT INTO obs VALUES (?1, ?2, ?3)")?;
    let mut count = 0;
    for raw in text.lines() {
        if raw.len() < 95 {
            continue;
        }
        if !raw.as_bytes()[0].is_ascii_alphanumeric() || raw.as_bytes().get(2) != Some(&b'-') {
            continue;
        }
        let lat_deg = perl_num(trim(field(raw, 35, 2)));
        let lat_min = perl_num(trim(field(raw, 38, 2))) / 60.0;
        let lat_sec = perl_num(trim(field(raw, 41, 5))) / 3600.0;
        let lat_hemi = trim(field(raw, 46, 1));
        let lat = if lat_hemi == "N" {
            lat_deg + lat_min + lat_sec
        } else {
            -(lat_deg + lat_min + lat_sec)
        };
        let lon_deg = perl_num(trim(field(raw, 48, 3)));
        let lon_min = perl_num(trim(field(raw, 52, 2))) / 60.0;
        let lon_sec = perl_num(trim(field(raw, 55, 5))) / 3600.0;
        let lon_hemi = trim(field(raw, 60, 1));
        let lon = if lon_hemi == "W" {
            -(lon_deg + lon_min + lon_sec)
        } else {
            lon_deg + lon_min + lon_sec
        };
        let ht = perl_num(trim(field(raw, 90, 5)));
        let htagl = perl_num(trim(field(raw, 84, 5)));
        if htagl >= 400.0 {
            stmt.execute(params![lat, lon, ht])?;
            count += 1;
        }
    }
    Ok(count)
}

fn insert_awos_with_ids(
    conn: &Connection,
    input_dir: &Path,
    airport_ids: &BTreeMap<String, String>,
) -> anyhow::Result<usize> {
    let path = input_dir.join("AWOS.txt");
    let text = read_text_lossy(&path)?;
    let mut stmt =
        conn.prepare("INSERT INTO awos VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)")?;
    let mut ready_to_print = 0;
    let mut ident = String::new();
    let mut kind = String::new();
    let mut status = String::new();
    let mut lat = String::new();
    let mut lon = String::new();
    let mut elevation = String::new();
    let mut freq1 = String::new();
    let mut freq2 = String::new();
    let mut tel1 = String::new();
    let mut tel2 = String::new();
    let mut remark = String::new();
    let mut count = 0;
    for raw in text.lines() {
        let line = raw.replace(',', " ");
        if line.starts_with("AWOS1") {
            ready_to_print += 1;
            if ready_to_print == 2 && status.to_uppercase() == "Y" {
                stmt.execute(params![
                    ident, kind, status, lat, lon, elevation, freq1, freq2, tel1, tel2, remark
                ])?;
                count += 1;
            }
            ready_to_print = 1;
            remark.clear();
            ident = canonicalize_airport_id(field(&line, 5, 4), airport_ids);
            kind = trim(field(&line, 9, 10)).to_string();
            status = trim(field(&line, 19, 1)).to_string();
            let lat_s = trim(field(&line, 31, 14));
            lat = awos_coord_lat_bug(lat_s)
                .map(|value| value.to_string())
                .unwrap_or_default();
            let lon_s = trim(field(&line, 45, 14));
            lon = awos_coord_lon_bug(lon_s)
                .map(|value| value.to_string())
                .unwrap_or_default();
            elevation = trim(field(&line, 60, 7)).to_string();
            freq1 = trim(field(&line, 68, 7)).to_string();
            freq2 = trim(field(&line, 75, 7)).to_string();
            tel1 = trim(field(&line, 82, 14)).to_string();
            tel2 = trim(field(&line, 96, 14)).to_string();
        } else if line.starts_with("AWOS2") {
            let piece = trim(field(&line, 19, 236));
            remark = format!("{piece}...{remark}");
        }
    }
    if status.to_uppercase() == "Y" {
        stmt.execute(params![
            ident, kind, status, lat, lon, elevation, freq1, freq2, tel1, tel2, remark
        ])?;
        count += 1;
    }
    Ok(count)
}

fn insert_airways(conn: &Connection, input_dir: &Path) -> anyhow::Result<usize> {
    let path = input_dir.join("AWY.txt");
    let text = read_text_lossy(&path)?;
    let mut branch_stmt = conn.prepare(
        "INSERT INTO airways_branch VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    let mut count = 0;
    for raw in text.lines() {
        if !raw.starts_with("AWY2") {
            continue;
        }
        let name = trim(field(raw, 4, 5)).to_string();
        let sequence_token = trim(field(raw, 9, 6)).to_string();
        let lat_s = trim(field(raw, 83, 14));
        let lon_s = trim(field(raw, 97, 14));
        if lat_s.is_empty() || lon_s.is_empty() {
            continue;
        }
        let lat = awy_coord(lat_s);
        let lon = awy_coord(lon_s);
        branch_stmt.execute(params![
            name,
            airway_branch_key(&sequence_token),
            airway_sequence_number(&sequence_token),
            sequence_token,
            trim(field(raw, 15, 25)).to_string(),
            lat,
            lon
        ])?;
        count += 1;
    }
    Ok(count)
}

fn insert_airways_legacy(conn: &Connection, input_dir: &Path) -> anyhow::Result<usize> {
    let path = input_dir.join("AWY.txt");
    let text = read_text_lossy(&path)?;
    let mut stmt = conn.prepare("INSERT INTO airways VALUES (?1, ?2, ?3, ?4)")?;
    let mut count = 0;
    for raw in text.lines() {
        if !raw.starts_with("AWY2") {
            continue;
        }
        let name = trim(field(raw, 4, 5)).to_string();
        let seq = trim(field(raw, 10, 5)).to_string();
        let lat_s = trim(field(raw, 83, 14));
        let lon_s = trim(field(raw, 97, 14));
        if lat_s.is_empty() || lon_s.is_empty() {
            continue;
        }
        let lat = awy_coord(lat_s);
        let lon = awy_coord(lon_s);
        stmt.execute(params![name, seq, lat, lon])?;
        count += 1;
    }
    Ok(count)
}

fn airway_branch_key(sequence_token: &str) -> String {
    sequence_token
        .chars()
        .find(|ch| !ch.is_ascii_digit() && !ch.is_whitespace())
        .map(|ch| ch.to_string())
        .unwrap_or_default()
}

fn airway_sequence_number(sequence_token: &str) -> i32 {
    trim(sequence_token)
        .chars()
        .filter(|ch| ch.is_ascii_digit())
        .collect::<String>()
        .parse::<i32>()
        .unwrap_or_default()
}

#[derive(Debug, Default)]
struct SaaParseResult {
    designator: String,
    name: String,
    upperlimit: String,
    upperlimitref: String,
    lowerlimit: String,
    lowerlimitref: String,
    starttime: String,
    endtime: String,
    startdate: String,
    enddate: String,
    note: String,
    freq_tx: String,
    freq_rx: String,
    timeref: String,
    lat_sum: f64,
    lon_sum: f64,
    num_pos: f64,
    p_airspace: bool,
    p_rfcomm: bool,
    p_airspaceusage: bool,
    p_designator: bool,
    p_name: bool,
    p_upperlimit: bool,
    p_upperlimitref: bool,
    p_lowerlimit: bool,
    p_lowerlimitref: bool,
    p_starttime: bool,
    p_endtime: bool,
    p_startdate: bool,
    p_enddate: bool,
    p_pos: bool,
    p_note: bool,
    p_ftx: bool,
    p_frx: bool,
    p_timeref: bool,
}

fn sanitize_saa_text(s: &str) -> String {
    s.replace(',', " ")
}

fn handle_saa_text(state: &mut SaaParseResult, raw: &str) {
    let text = sanitize_saa_text(raw);
    if state.p_name {
        state.name = text.clone();
    }
    if state.p_designator {
        state.designator = text.clone();
    }
    if state.p_upperlimit {
        state.upperlimit = text.clone();
    }
    if state.p_lowerlimit {
        state.lowerlimit = text.clone();
    }
    if state.p_upperlimitref {
        state.upperlimitref = text.clone();
    }
    if state.p_lowerlimitref {
        state.lowerlimitref = text.clone();
    }
    if state.p_starttime {
        state.starttime = text.clone();
    }
    if state.p_endtime {
        state.endtime = text.clone();
    }
    if state.p_startdate {
        state.startdate = text.clone();
    }
    if state.p_enddate {
        state.enddate = text.clone();
    }
    if state.p_note {
        if let Some(last_line) = text.lines().filter(|line| !line.trim().is_empty()).last() {
            state.note = last_line.trim_start().to_string();
        }
    }
    if state.p_ftx {
        state.freq_tx.push_str(&text);
        state.freq_tx.push(' ');
    }
    if state.p_frx {
        state.freq_rx.push_str(&text);
        state.freq_rx.push(' ');
    }
    if state.p_timeref {
        state.timeref = text.clone();
    }
    if state.p_pos {
        let values = text.split_whitespace().collect::<Vec<_>>();
        if values.len() >= 2 {
            state.lon_sum += perl_num(values[0]);
            state.lat_sum += perl_num(values[1]);
            state.num_pos += 1.0;
        }
    }
}

fn parse_saa_file(path: &Path) -> anyhow::Result<SaaParseResult> {
    let mut reader = Reader::from_file(path)
        .with_context(|| format!("failed to open xml {}", path.display()))?;
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut state = SaaParseResult::default();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(event)) => {
                let name = String::from_utf8_lossy(event.local_name().as_ref()).into_owned();
                if name == "Airspace" {
                    state.p_airspace = true;
                }
                if name == "AirspaceUsage" {
                    state.p_airspaceusage = true;
                }
                if name == "RadioCommunicationChannel" {
                    state.p_rfcomm = true;
                }
                if state.p_airspaceusage {
                    if name == "startTime" {
                        state.p_starttime = true;
                    }
                    if name == "endTime" {
                        state.p_endtime = true;
                    }
                    if name == "startDate" {
                        state.p_startdate = true;
                    }
                    if name == "endDate" {
                        state.p_enddate = true;
                    }
                    if name == "timeReference" {
                        state.p_timeref = true;
                    }
                }
                if state.p_airspace {
                    if name == "designator" {
                        state.p_designator = true;
                    }
                    if name == "name" {
                        state.p_name = true;
                    }
                    if name == "upperLimit" {
                        state.p_upperlimit = true;
                    }
                    if name == "lowerLimit" {
                        state.p_lowerlimit = true;
                    }
                    if name == "upperLimitReference" {
                        state.p_upperlimitref = true;
                    }
                    if name == "lowerLimitReference" {
                        state.p_lowerlimitref = true;
                    }
                    if name == "pos" {
                        state.p_pos = true;
                    }
                    if name == "note" {
                        state.p_note = true;
                    }
                }
                if state.p_rfcomm {
                    if name == "frequencyTransmission" {
                        state.p_ftx = true;
                    }
                    if name == "frequencyReception" {
                        state.p_frx = true;
                    }
                }
            }
            Ok(Event::End(event)) => {
                let name = String::from_utf8_lossy(event.local_name().as_ref()).into_owned();
                if name == "Airspace" {
                    state.p_airspace = false;
                }
                if name == "AirspaceUsage" {
                    state.p_airspaceusage = false;
                }
                if name == "RadioCommunicationChannel" {
                    state.p_rfcomm = false;
                }
                if state.p_airspace {
                    if name == "designator" {
                        state.p_designator = false;
                    }
                    if name == "name" {
                        state.p_name = false;
                    }
                    if name == "upperLimit" {
                        state.p_upperlimit = false;
                    }
                    if name == "lowerLimit" {
                        state.p_lowerlimit = false;
                    }
                    if name == "upperLimitReference" {
                        state.p_upperlimitref = false;
                    }
                    if name == "lowerLimitReference" {
                        state.p_lowerlimitref = false;
                    }
                    if name == "pos" {
                        state.p_pos = false;
                    }
                    if name == "note" {
                        state.p_note = false;
                    }
                }
                if state.p_airspaceusage {
                    if name == "startTime" {
                        state.p_starttime = false;
                    }
                    if name == "endTime" {
                        state.p_endtime = false;
                    }
                    if name == "startDate" {
                        state.p_startdate = false;
                    }
                    if name == "endDate" {
                        state.p_enddate = false;
                    }
                    if name == "timeReference" {
                        state.p_timeref = false;
                    }
                }
                if state.p_rfcomm {
                    if name == "frequencyTransmission" {
                        state.p_ftx = false;
                    }
                    if name == "frequencyReception" {
                        state.p_frx = false;
                    }
                }
            }
            Ok(Event::Text(event)) => {
                let text = event
                    .decode()
                    .with_context(|| format!("failed to decode text in {}", path.display()))?;
                handle_saa_text(&mut state, &text);
            }
            Ok(Event::CData(event)) => {
                let text = event
                    .decode()
                    .with_context(|| format!("failed to decode cdata in {}", path.display()))?;
                handle_saa_text(&mut state, &text);
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => {
                bail!("failed to parse xml {}: {}", path.display(), error);
            }
        }
        buf.clear();
    }
    Ok(state)
}

fn insert_saa(conn: &Connection, input_dir: &Path) -> anyhow::Result<usize> {
    let mut files = fs::read_dir(input_dir)
        .with_context(|| format!("failed to read {}", input_dir.display()))?
        .collect::<Result<Vec<_>, _>>()
        .context("failed to iterate input dir")?;
    files.sort_by_key(|entry| entry.path());
    let mut stmt = conn.prepare(
        "INSERT INTO saa VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
    )?;
    let mut count = 0;
    for entry in files {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("xml") {
            continue;
        }
        let parsed = parse_saa_file(&path)?;
        let upperlimit = format!("{} {}", parsed.upperlimit, parsed.upperlimitref);
        let lowerlimit = format!("{} {}", parsed.lowerlimit, parsed.lowerlimitref);
        let lat = if parsed.num_pos == 0.0 {
            0.0
        } else {
            parsed.lat_sum / parsed.num_pos
        };
        let lon = if parsed.num_pos == 0.0 {
            0.0
        } else {
            parsed.lon_sum / parsed.num_pos
        };
        stmt.execute(params![
            parsed.designator,
            parsed.name,
            upperlimit,
            lowerlimit,
            parsed.starttime,
            parsed.endtime,
            parsed.timeref,
            parsed.startdate,
            parsed.enddate,
            parsed.note,
            parsed.freq_tx,
            parsed.freq_rx,
            format!("{lat:.4}"),
            format!("{lon:.4}")
        ])?;
        count += 1;
    }
    Ok(count)
}

fn insert_cifp_with_ids(
    conn: &Connection,
    input_dir: &Path,
    airport_ids: &BTreeMap<String, String>,
) -> anyhow::Result<usize> {
    let path = input_dir.join("FAACIFP18");
    let text = read_text_lossy(&path)?;
    let mut stmt = conn.prepare("INSERT INTO cifp_sid_star_app VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38, ?39, ?40, ?41, ?42, ?43, ?44, ?45, ?46, ?47, ?48)")?;
    let mut count = 0;
    for line in text.lines() {
        let bytes = line.as_bytes();
        if bytes.len() < 132 {
            continue;
        }
        let section = field(line, 4, 1);
        let subsection = field(line, 12, 1);
        if section != "P" || !matches!(subsection, "D" | "E" | "F") {
            continue;
        }
        let mut fields = vec![
            field(line, 0, 1),
            field(line, 1, 3),
            field(line, 4, 1),
            field(line, 6, 4),
            field(line, 10, 2),
            field(line, 12, 1),
            field(line, 13, 6),
            field(line, 19, 1),
            field(line, 20, 5),
            field(line, 26, 3),
            field(line, 29, 5),
            field(line, 34, 2),
            field(line, 36, 1),
            field(line, 37, 1),
            field(line, 38, 1),
            field(line, 39, 4),
            field(line, 43, 1),
            field(line, 44, 3),
            field(line, 47, 2),
            field(line, 49, 1),
            field(line, 50, 4),
            field(line, 54, 2),
            field(line, 56, 6),
            field(line, 62, 4),
            field(line, 66, 4),
            field(line, 70, 4),
            field(line, 74, 4),
            field(line, 78, 1),
            field(line, 79, 1),
            field(line, 80, 2),
            field(line, 82, 1),
            field(line, 83, 1),
            field(line, 84, 5),
            field(line, 89, 5),
            field(line, 94, 5),
            field(line, 99, 3),
            field(line, 102, 4),
            field(line, 106, 5),
            field(line, 111, 1),
            field(line, 112, 2),
            field(line, 114, 1),
            field(line, 115, 1),
            field(line, 116, 1),
            field(line, 117, 1),
            field(line, 118, 1),
            field(line, 119, 1),
            field(line, 123, 5),
            field(line, 128, 4),
        ]
        .into_iter()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
        fields[3] = canonicalize_airport_id(&fields[3], airport_ids);
        stmt.execute(rusqlite::params_from_iter(fields))?;
        count += 1;
    }
    Ok(count)
}

pub fn build_data_package(request: &DataBuildRequest) -> anyhow::Result<DataBuildResult> {
    fs::create_dir_all(&request.output_dir)
        .with_context(|| format!("failed to create {}", request.output_dir.display()))?;
    let main_db = request.output_dir.join(INTERMEDIATE_SQLITE_BASENAME);
    if main_db.exists() {
        fs::remove_file(&main_db)
            .with_context(|| format!("failed to remove {}", main_db.display()))?;
    }
    let conn = Connection::open(&main_db)
        .with_context(|| format!("failed to create {}", main_db.display()))?;
    setup_schema(&conn, request.mode)?;
    let airport_ids = load_airport_id_map(&request.input_dir)?;
    let tx = conn.unchecked_transaction()?;
    let mut row_counts = BTreeMap::new();
    row_counts.insert(
        "airports".to_string(),
        insert_airports(&tx, &request.input_dir)?,
    );
    row_counts.insert(
        "airport_aliases".to_string(),
        insert_airport_aliases(&tx, &request.input_dir)?,
    );
    row_counts.insert(
        "airportfreq".to_string(),
        insert_airport_freq_with_ids(&tx, &request.input_dir, &airport_ids)?,
    );
    row_counts.insert(
        "airportrunways".to_string(),
        insert_runways(&tx, &request.input_dir)?,
    );
    row_counts.insert("nav".to_string(), insert_nav(&tx, &request.input_dir)?);
    row_counts.insert("fix".to_string(), insert_fix(&tx, &request.input_dir)?);
    if request.mode == DataBuildMode::LegacyAvare {
        row_counts.insert("obs".to_string(), insert_obs(&tx, &request.input_dir)?);
    }
    row_counts.insert(
        "awos".to_string(),
        insert_awos_with_ids(&tx, &request.input_dir, &airport_ids)?,
    );
    row_counts.insert("saa".to_string(), insert_saa(&tx, &request.input_dir)?);
    match request.mode {
        DataBuildMode::Production => {
            row_counts.insert(
                "airways_branch".to_string(),
                insert_airways(&tx, &request.input_dir)?,
            );
        }
        DataBuildMode::LegacyAvare => {
            row_counts.insert(
                "airways".to_string(),
                insert_airways_legacy(&tx, &request.input_dir)?,
            );
        }
    }
    row_counts.insert(
        "cifp_sid_star_app".to_string(),
        insert_cifp_with_ids(&tx, &request.input_dir, &airport_ids)?,
    );
    tx.commit()?;

    let artifact_stem = request
        .artifact_stem
        .as_deref()
        .unwrap_or("databases");
    let manifest_path = request.output_dir.join(format!("{artifact_stem}.manifest"));
    fs::write(
        &manifest_path,
        format!("{}\n{}\n", request.manifest_version, INTERMEDIATE_SQLITE_BASENAME),
    )
    .with_context(|| format!("failed to write {}", manifest_path.display()))?;
    let zip_path = request.output_dir.join(format!("{artifact_stem}.zip"));
    if zip_path.exists() {
        fs::remove_file(&zip_path)
            .with_context(|| format!("failed to remove {}", zip_path.display()))?;
    }
    let zip_file = fs::File::create(&zip_path)
        .with_context(|| format!("failed to create {}", zip_path.display()))?;
    let mut zip = ZipWriter::new(zip_file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, path) in [("databases", &manifest_path), (INTERMEDIATE_SQLITE_BASENAME, &main_db)]
    {
        zip.start_file(name, options)?;
        let mut file =
            fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        zip.write_all(&bytes)?;
    }
    zip.finish()?;

    Ok(DataBuildResult {
        main_db,
        manifest_path,
        zip_path,
        row_counts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn put_field(line: &mut [u8], start: usize, len: usize, value: &str) {
        let end = start + len;
        line[start..end].fill(b' ');
        let bytes = value.as_bytes();
        let copy_len = bytes.len().min(len);
        line[start..start + copy_len].copy_from_slice(&bytes[..copy_len]);
    }

    fn build_apt_airport(faa: &str, icao: &str, dlid: &str) -> String {
        let mut line = vec![b' '; 1220];
        put_field(&mut line, 0, 3, "APT");
        put_field(&mut line, 3, 11, dlid);
        put_field(&mut line, 14, 12, "AIRPORT");
        put_field(&mut line, 27, 4, faa);
        put_field(&mut line, 91, 2, "WA");
        put_field(&mut line, 93, 40, "TEST CITY");
        put_field(&mut line, 133, 50, "TEST AIRPORT");
        put_field(&mut line, 185, 2, "PU");
        put_field(&mut line, 523, 14, "47 29 51.00N");
        put_field(&mut line, 550, 15, "122 12 34.00W");
        put_field(&mut line, 578, 7, "0032");
        put_field(&mut line, 586, 3, "015");
        put_field(&mut line, 593, 4, "1000");
        put_field(&mut line, 762, 16, "2065550000");
        put_field(&mut line, 877, 1, "N");
        put_field(&mut line, 878, 1, "N");
        put_field(&mut line, 900, 40, "100LL");
        put_field(&mut line, 966, 7, "SS-SR");
        put_field(&mut line, 980, 1, "Y");
        put_field(&mut line, 981, 7, "122.95");
        put_field(&mut line, 988, 7, "123.00");
        put_field(&mut line, 995, 4, "LEFT");
        put_field(&mut line, 999, 3, "BCN");
        put_field(&mut line, 1002, 1, "N");
        put_field(&mut line, 1210, 7, icao);
        String::from_utf8(line).unwrap()
    }

    fn build_rwy_line() -> String {
        let mut line = vec![b' '; 500];
        put_field(&mut line, 0, 3, "RWY");
        put_field(&mut line, 23, 5, "5000");
        put_field(&mut line, 28, 4, "100");
        put_field(&mut line, 32, 12, "ASPH");
        put_field(&mut line, 65, 3, "16L");
        put_field(&mut line, 68, 3, "160");
        put_field(&mut line, 71, 10, "ILS");
        put_field(&mut line, 81, 1, "L");
        put_field(&mut line, 88, 14, "47 29 51.00N");
        put_field(&mut line, 115, 15, "122 12 34.00W");
        put_field(&mut line, 142, 7, "0032");
        put_field(&mut line, 217, 4, "1000");
        put_field(&mut line, 228, 5, "PAPI");
        put_field(&mut line, 237, 8, "MIRL");
        put_field(&mut line, 287, 3, "34R");
        put_field(&mut line, 290, 3, "340");
        put_field(&mut line, 293, 10, "ILS");
        put_field(&mut line, 303, 1, "R");
        put_field(&mut line, 310, 14, "47 30 01.00N");
        put_field(&mut line, 337, 15, "122 12 44.00W");
        put_field(&mut line, 364, 7, "0032");
        put_field(&mut line, 439, 4, "1000");
        put_field(&mut line, 450, 5, "PAPI");
        put_field(&mut line, 459, 8, "MIRL");
        String::from_utf8(line).unwrap()
    }

    fn build_twr1_line(faa: &str, dlid: &str) -> String {
        let mut line = vec![b' '; 40];
        put_field(&mut line, 0, 4, "TWR1");
        put_field(&mut line, 4, 4, faa);
        put_field(&mut line, 18, 11, dlid);
        String::from_utf8(line).unwrap()
    }

    fn build_twr3_line(freq: &str, kind: &str) -> String {
        let mut line = vec![b' '; 120];
        put_field(&mut line, 0, 4, "TWR3");
        put_field(&mut line, 8, 44, freq);
        put_field(&mut line, 52, 50, kind);
        String::from_utf8(line).unwrap()
    }

    fn build_twr6_line(remark: &str) -> String {
        let mut line = vec![b' '; 64];
        put_field(&mut line, 0, 4, "TWR6");
        put_field(&mut line, 13, remark.len(), remark);
        String::from_utf8(line).unwrap()
    }

    fn build_awos1_line(faa: &str) -> String {
        let mut line = vec![b' '; 120];
        put_field(&mut line, 0, 5, "AWOS1");
        put_field(&mut line, 5, 4, faa);
        put_field(&mut line, 9, 10, "AWOS-3");
        put_field(&mut line, 19, 1, "Y");
        put_field(&mut line, 31, 14, "047 29 51.00N");
        put_field(&mut line, 45, 14, "122 12 34.00W");
        put_field(&mut line, 60, 7, "0032");
        put_field(&mut line, 68, 7, "118.00");
        put_field(&mut line, 75, 7, "121.50");
        put_field(&mut line, 82, 14, "2065551111");
        put_field(&mut line, 96, 14, "2065552222");
        String::from_utf8(line).unwrap()
    }

    fn build_awos2_line(remark: &str) -> String {
        let mut line = vec![b' '; 260];
        put_field(&mut line, 0, 5, "AWOS2");
        put_field(&mut line, 19, remark.len(), remark);
        String::from_utf8(line).unwrap()
    }

    fn build_cifp_line(faa: &str) -> String {
        let mut line = vec![b' '; 132];
        put_field(&mut line, 0, 1, "S");
        put_field(&mut line, 1, 3, "USA");
        put_field(&mut line, 4, 1, "P");
        put_field(&mut line, 6, 4, faa);
        put_field(&mut line, 10, 2, "US");
        put_field(&mut line, 12, 1, "D");
        put_field(&mut line, 13, 6, "TESTID");
        put_field(&mut line, 19, 1, "1");
        put_field(&mut line, 20, 5, "TRANS");
        put_field(&mut line, 26, 3, "001");
        put_field(&mut line, 29, 5, "FIX01");
        put_field(&mut line, 128, 4, "2604");
        String::from_utf8(line).unwrap()
    }

    fn write_empty(path: &Path) {
        fs::write(path, "").unwrap();
    }

    #[test]
    fn canonical_airport_id_prefers_icao_and_falls_back_to_faa() {
        let apt_with_icao = build_apt_airport("SEA", "KSEA", "26395.*A");
        let apt_without_icao = build_apt_airport("0S9", "", "12345.*A");
        assert_eq!(canonical_airport_id_from_apt_line(&apt_with_icao), "KSEA");
        assert_eq!(canonical_airport_id_from_apt_line(&apt_without_icao), "0S9");
    }

    #[test]
    fn build_data_package_canonicalizes_airport_linked_tables() {
        let dir = tempdir().unwrap();
        let input_dir = dir.path().join("input");
        let output_dir = dir.path().join("output");
        fs::create_dir_all(&input_dir).unwrap();

        fs::write(
            input_dir.join("APT.txt"),
            format!(
                "{}\n{}\n",
                build_apt_airport("SEA", "KSEA", "26395.*A"),
                build_rwy_line()
            ),
        )
        .unwrap();
        fs::write(
            input_dir.join("TWR.txt"),
            format!(
                "{}\n{}\n{}\n",
                build_twr1_line("SEA", "26395.*A"),
                build_twr3_line("118.00", "LCL/P"),
                build_twr6_line("tower remark")
            ),
        )
        .unwrap();
        fs::write(
            input_dir.join("AWOS.txt"),
            format!(
                "{}\n{}\n",
                build_awos1_line("SEA"),
                build_awos2_line("awos remark")
            ),
        )
        .unwrap();
        fs::write(
            input_dir.join("FAACIFP18"),
            format!("{}\n", build_cifp_line("SEA")),
        )
        .unwrap();
        for name in ["NAV.txt", "FIX.txt", "DOF.DAT", "AWY.txt"] {
            write_empty(&input_dir.join(name));
        }
        let request = DataBuildRequest {
            input_dir: input_dir.clone(),
            output_dir,
            manifest_version: "2604".to_string(),
            mode: DataBuildMode::Production,
            artifact_stem: None,
        };
        let result = build_data_package(&request).unwrap();
        let conn = Connection::open(result.main_db).unwrap();

        let airport_id: String = conn
            .query_row("SELECT LocationID FROM airports", [], |row| row.get(0))
            .unwrap();
        assert_eq!(airport_id, "KSEA");

        let alias_pairs = conn
            .prepare("SELECT alias_id, airport_id FROM airport_aliases ORDER BY alias_id")
            .unwrap()
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            alias_pairs,
            vec![
                ("KSEA".to_string(), "KSEA".to_string()),
                ("SEA".to_string(), "KSEA".to_string())
            ]
        );

        let freq_id: String = conn
            .query_row(
                "SELECT LocationID FROM airportfreq WHERE Type != 'Remark' LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(freq_id, "KSEA");

        let runway_id: String = conn
            .query_row("SELECT LocationID FROM airportrunways LIMIT 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(runway_id, "KSEA");

        let awos_id: String = conn
            .query_row("SELECT LocationID FROM awos LIMIT 1", [], |row| row.get(0))
            .unwrap();
        assert_eq!(awos_id, "KSEA");

        let cifp_id: String = conn
            .query_row(
                "SELECT airport_identifier FROM cifp_sid_star_app LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(cifp_id, "KSEA");
    }

    #[test]
    fn airways_branch_table_preserves_branch_token_from_raw_awy() {
        let dir = tempdir().unwrap();
        let input_dir = dir.path().join("input");
        let output_dir = dir.path().join("output");
        fs::create_dir_all(&input_dir).unwrap();

        write_empty(&input_dir.join("APT.txt"));
        write_empty(&input_dir.join("TWR.txt"));
        write_empty(&input_dir.join("AWOS.txt"));
        write_empty(&input_dir.join("FAACIFP18"));
        for name in ["NAV.txt", "FIX.txt", "DOF.DAT", "geo.csv"] {
            write_empty(&input_dir.join(name));
        }
        fs::write(
            input_dir.join("AWY.txt"),
            concat!(
                "AWY2V16      10LOS ANGELES                   VORTAC                            CA  33-55-59.337N 118-25-55.246W     LAX V16  *LAX*C                                                                                                                                                                                0000002\n",
                "AWY2V16  H   10SYVAD                         REP-PT             FIX            OPP 21-55-28.0N   162-45-28.78W 32000    V16  H*SYVAD*OP                                                                                                                                                                            0000002\n"
            ),
        )
        .unwrap();

        let request = DataBuildRequest {
            input_dir,
            output_dir,
            manifest_version: "2604".to_string(),
            mode: DataBuildMode::Production,
            artifact_stem: Some("data_2604".to_string()),
        };
        let result = build_data_package(&request).unwrap();
        let conn = Connection::open(result.main_db).unwrap();

        let rows = conn
            .prepare(
                "SELECT name, branch_key, sequence_number, sequence_token, point_name
                 FROM airways_branch
                 ORDER BY rowid",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i32>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(
            rows,
            vec![
                (
                    "V16".to_string(),
                    "".to_string(),
                    10,
                    "10".to_string(),
                    "LOS ANGELES".to_string(),
                ),
                (
                    "V16".to_string(),
                    "H".to_string(),
                    10,
                    "H   10".to_string(),
                    "SYVAD".to_string(),
                ),
            ]
        );
    }

    #[test]
    fn airway_branch_helpers_extract_branch_and_numeric_sequence() {
        assert_eq!(airway_branch_key("10"), "");
        assert_eq!(airway_branch_key("H   10"), "H");
        assert_eq!(airway_sequence_number("10"), 10);
        assert_eq!(airway_sequence_number("H   10"), 10);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColumnKind {
    Float,
    Other,
}

#[derive(Debug, Clone, Copy)]
struct ColumnSpec {
    kind: ColumnKind,
    normalize_text_as_float: bool,
}

fn column_specs(conn: &Connection, table: &str) -> anyhow::Result<Vec<ColumnSpec>> {
    let pragma = format!("PRAGMA table_info({table})");
    let mut pragma_stmt = conn.prepare(&pragma)?;
    let specs = pragma_stmt
        .query_map([], |row| {
            let name = row.get::<_, String>(1)?;
            let declared_type = row.get::<_, String>(2)?;
            let kind = if declared_type.to_ascii_uppercase().contains("FLOAT") {
                ColumnKind::Float
            } else {
                ColumnKind::Other
            };
            // Legacy runway coordinates are stored as text because the original pipeline printed
            // Perl float strings into CSV before sqlite imported them. Compare those fields by
            // normalized numeric value instead of the exact Perl string rendering.
            let normalize_text_as_float = table == "airportrunways"
                && matches!(
                    name.as_str(),
                    "LELatitude" | "HELatitude" | "LELongitude" | "HELongitude"
                );
            Ok(ColumnSpec {
                kind,
                normalize_text_as_float,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(specs)
}

fn sqlite_value_to_string(
    row: &rusqlite::Row<'_>,
    index: usize,
    column: ColumnSpec,
) -> rusqlite::Result<String> {
    use rusqlite::types::ValueRef;
    let value = row.get_ref(index)?;
    Ok(match value {
        ValueRef::Null => "NULL".to_string(),
        ValueRef::Integer(v) => {
            if column.kind == ColumnKind::Float {
                format!("{:.9}", v as f64)
            } else {
                v.to_string()
            }
        }
        // SQLite stores the numeric result of the legacy Perl/Python text parsers, so exact
        // binary float identity is not the compatibility contract. Normalize declared FLOAT
        // columns to a stable textual precision and compare that dump instead.
        ValueRef::Real(v) => {
            if column.kind == ColumnKind::Float {
                format!("{v:.9}")
            } else {
                format!("{v:.15}")
            }
        }
        ValueRef::Text(v) => {
            let text = String::from_utf8_lossy(v).into_owned();
            if column.normalize_text_as_float && !text.is_empty() {
                match text.parse::<f64>() {
                    Ok(value) => format!("{value:.9}"),
                    Err(_) => text,
                }
            } else {
                text
            }
        }
        ValueRef::Blob(v) => format!("0x{}", hex(v)),
    })
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}

pub fn normalized_database_dump(path: &Path) -> anyhow::Result<String> {
    let conn = Connection::open(path)
        .with_context(|| format!("failed to open database {}", path.display()))?;
    let mut lines = Vec::new();
    for table in TABLES {
        let columns = column_specs(&conn, table)?;
        let query = format!("SELECT * FROM {table}");
        let mut stmt = conn.prepare(&query)?;
        let rows = stmt.query_map([], |row| {
            let mut values = Vec::with_capacity(columns.len());
            for idx in 0..columns.len() {
                values.push(sqlite_value_to_string(row, idx, columns[idx])?);
            }
            Ok(values)
        })?;
        let mut table_rows = Vec::new();
        for row in rows {
            let values = row?;
            table_rows.push(format!("{table}|{}", values.join("|")));
        }
        table_rows.sort();
        lines.push(format!("TABLE {table}"));
        lines.extend(table_rows);
    }
    Ok(lines.join("\n") + "\n")
}

pub fn compare_databases(left: &Path, right: &Path) -> anyhow::Result<()> {
    let left_dump = normalized_database_dump(left)?;
    let right_dump = normalized_database_dump(right)?;
    if left_dump == right_dump {
        println!("status match");
        for table in TABLES {
            let left_count = table_count(left, table)?;
            let right_count = table_count(right, table)?;
            println!(
                "table {} left={} right={} status=match",
                table, left_count, right_count
            );
        }
        return Ok(());
    }
    println!("status mismatch");
    for table in TABLES {
        let left_count = table_count(left, table)?;
        let right_count = table_count(right, table)?;
        let status = if normalized_table_dump(left, table)? == normalized_table_dump(right, table)?
        {
            "match"
        } else {
            "mismatch"
        };
        println!(
            "table {} left={} right={} status={}",
            table, left_count, right_count, status
        );
    }
    bail!("database dumps differ");
}

fn normalized_table_dump(path: &Path, table: &str) -> anyhow::Result<String> {
    let conn = Connection::open(path)
        .with_context(|| format!("failed to open database {}", path.display()))?;
    let columns = column_specs(&conn, table)?;
    let query = format!("SELECT * FROM {table}");
    let mut stmt = conn.prepare(&query)?;
    let rows = stmt.query_map([], |row| {
        let mut values = Vec::with_capacity(columns.len());
        for idx in 0..columns.len() {
            values.push(sqlite_value_to_string(row, idx, columns[idx])?);
        }
        Ok(values.join("|"))
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    out.sort();
    Ok(out.join("\n"))
}

fn table_count(path: &Path, table: &str) -> anyhow::Result<i64> {
    let conn = Connection::open(path)
        .with_context(|| format!("failed to open database {}", path.display()))?;
    let query = format!("SELECT COUNT(*) FROM {table}");
    let count = conn.query_row(&query, [], |row| row.get(0))?;
    Ok(count)
}
