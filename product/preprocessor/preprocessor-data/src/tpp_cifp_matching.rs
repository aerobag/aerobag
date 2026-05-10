use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::LazyLock,
};

use anyhow::{Context, Result};
use preprocessor_core::{PackageAssetManifest, PACKAGE_ASSET_MANIFEST_NAME};
use preprocessor_zip::{write_deterministic_zip, ZipSource};
use regex::Regex;
use rusqlite::Connection;
use serde::Serialize;
use zip::ZipArchive;

use crate::INTERMEDIATE_SQLITE_BASENAME;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlateRecord {
    pub plate_id: String,
    pub airport_id: String,
    pub package_id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Claimer {
    pub plate_id: String,
    pub plate_label: String,
    pub package_id: String,
    pub public: bool,
    pub priority: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RelationAirportExample {
    pub airport: String,
    pub cifp_total: usize,
    pub uniquely_bound: usize,
    pub multiply_bound: usize,
    pub copter_only: usize,
    pub unresolved_count: usize,
    pub copter_only_cids: Vec<String>,
    pub unresolved_cids: Vec<String>,
    pub multiply_bound_examples: BTreeMap<String, Vec<Claimer>>,
    pub ignored_noheur_plates: usize,
    pub ignored_nomatch_plates: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct MatchSummary {
    pub matched_unique: usize,
    pub matched_partial: usize,
    pub matched_none: usize,
    pub matched_ambiguous: usize,
    pub no_heuristic: usize,
    pub airport_missing_from_cifp: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MatchExample {
    pub airport_id: String,
    pub label: String,
    pub candidate_groups: Vec<Vec<String>>,
    pub matched: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct RelationSummary {
    pub airports_considered: usize,
    pub airports_with_no_unresolved_cids: usize,
    pub airports_with_unresolved_cids: usize,
    pub uniquely_bound_cids_total: usize,
    pub multiply_bound_cids_total: usize,
    pub copter_only_cids_total: usize,
    pub unresolved_cids_total: usize,
    pub ignored_noheur_plates_total: usize,
    pub ignored_nomatch_plates_total: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TppCifpAuditReport {
    pub approach_plate_count: usize,
    pub airports_with_approach_plates: usize,
    pub airports_with_cifp_approaches: usize,
    pub exact_count_match: usize,
    pub count_mismatch: usize,
    pub count_rows: Vec<(String, usize, usize)>,
    pub match_summary: MatchSummary,
    pub match_examples: Vec<MatchExample>,
    pub relation_summary: RelationSummary,
    pub relation_examples: Vec<RelationAirportExample>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PublishedMatchRow {
    pub airport_id: String,
    pub cifp_id: String,
    pub plate_id: String,
    pub plate_label: String,
    pub package_id: String,
    pub public: bool,
    pub priority: i64,
    pub match_kind: String,
    pub is_primary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct PublishedMatchSummary {
    pub unique_rows: usize,
    pub multiply_bound_rows: usize,
    pub copter_only_cids: usize,
    pub unresolved_cids: usize,
}

#[derive(Debug, Clone)]
pub struct DataTppMatchRequest {
    pub input_main_db: PathBuf,
    pub input_zip: PathBuf,
    pub output_dir: PathBuf,
    pub artifact_stem: String,
    pub tpp_package_zips: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct DataTppMatchResult {
    pub main_db: PathBuf,
    pub manifest_path: PathBuf,
    pub zip_path: PathBuf,
    pub published: PublishedMatchSummary,
}

#[derive(Debug, Clone)]
struct MatchAnalysis {
    summary: MatchSummary,
    examples: Vec<MatchExample>,
}

#[derive(Debug, Clone)]
struct RelationAnalysis {
    summary: RelationSummary,
    examples: Vec<RelationAirportExample>,
    published_rows: Vec<PublishedMatchRow>,
}

static IAP_PREFIX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^IAP-[A-Z]{2}-(.+)$").expect("valid iap prefix regex"));
static CAT_SUFFIX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r" \((?:SA )?CAT[^)]*\)$").expect("valid cat suffix regex"));
static RUNWAY_PAIR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([0-9]{1,2})([LRC]?)(?: AND )([LRC])$").expect("valid runway pair regex")
});

macro_rules! regex {
    ($name:ident, $value:literal) => {
        static $name: LazyLock<Regex> =
            LazyLock::new(|| Regex::new($value).expect(concat!("valid regex: ", $value)));
    };
}

regex!(
    RE_VOR_DME_OR_TACAN_RUNWAY,
    r"^VOR(?: AND DME|/DME) OR TACAN RWY ([0-9]{1,2}[LRC]?)$"
);
regex!(
    RE_VOR_DME_OR_TACAN_RUNWAY_PAIR,
    r"^VOR(?: AND DME|/DME) OR TACAN RWY ([0-9]{1,2}[LRC]? AND [LRC])$"
);
regex!(
    RE_ILS_OR_LOC_RUNWAY,
    r"^ILS OR LOC(?: OR DME|/DME)? RWY ([0-9]{1,2}[LRC]?)$"
);
regex!(
    RE_ILS_VARIANT_OR_LOC_RUNWAY,
    r"^ILS ([XYZ]) OR LOC(?: OR DME|/DME)?(?: [XYZ])? RWY ([0-9]{1,2}[LRC]?)$"
);
regex!(RE_ILS_PRM_RUNWAY, r"^ILS PRM RWY ([0-9]{1,2}[LRC]?)$");
regex!(
    RE_ILS_PRM_VARIANT_RUNWAY,
    r"^ILS PRM ([UVWXYZ]) RWY ([0-9]{1,2}[LRC]?)$"
);
regex!(
    RE_ILS_OR_LOC_AND_DME_RUNWAY,
    r"^ILS OR LOC AND DME RWY ([0-9]{1,2}[LRC]?)$"
);
regex!(
    RE_ILS_VARIANT_OR_LOC_AND_DME_RUNWAY,
    r"^ILS ([XYZ]) OR LOC AND DME(?: [XYZ])? RWY ([0-9]{1,2}[LRC]?)$"
);
regex!(
    RE_VOR_OR_TACAN_RUNWAY,
    r"^VOR(?: OR DME|/DME)? OR TACAN RWY ([0-9]{1,2}[LRC]?)$"
);
regex!(
    RE_VOR_OR_TACAN_RUNWAY_PAIR,
    r"^VOR(?: OR DME|/DME)? OR TACAN RWY ([0-9]{1,2}[LRC]? AND [LRC])$"
);
regex!(RE_LOC_RUNWAY, r"^LOC(?:/DME)? RWY ([0-9]{1,2}[LRC]?)$");
regex!(
    RE_LOC_AND_DME_RUNWAY,
    r"^LOC AND DME RWY ([0-9]{1,2}[LRC]?)$"
);
regex!(
    RE_LOC_VARIANT_RUNWAY,
    r"^LOC(?:/DME)? ([XYZ]) RWY ([0-9]{1,2}[LRC]?)$"
);
regex!(
    RE_LOC_AND_DME_VARIANT_RUNWAY,
    r"^LOC AND DME ([XYZ]) RWY ([0-9]{1,2}[LRC]?)$"
);
regex!(RE_ILS_RUNWAY, r"^ILS RWY ([0-9]{1,2}[LRC]?)$");
regex!(
    RE_ILS_VARIANT_RUNWAY,
    r"^ILS ([UVWXYZ]) RWY ([0-9]{1,2}[LRC]?)$"
);
regex!(RE_RNAV_GPS_RUNWAY, r"^RNAV \(GPS\) RWY ([0-9]{1,2}[LRC]?)$");
regex!(RE_GPS_RUNWAY, r"^GPS RWY ([0-9]{1,2}[LRC]?)$");
regex!(
    RE_RNAV_GPS_VARIANT_RUNWAY,
    r"^RNAV \(GPS\) ([UVWXYZ]) RWY ([0-9]{1,2}[LRC]?)$"
);
regex!(
    RE_RNAV_RNP_VARIANT_RUNWAY,
    r"^RNAV \(RNP\) ([UVWXYZ]) RWY ([0-9]{1,2}[LRC]?)$"
);
regex!(RE_RNAV_RNP_RUNWAY, r"^RNAV \(RNP\) RWY ([0-9]{1,2}[LRC]?)$");
regex!(
    RE_RNAV_RNP_RUNWAY_PAIR,
    r"^RNAV \(RNP\) RWY ([0-9]{1,2}[LRC]? AND [LRC])$"
);
regex!(RE_GLS_RUNWAY, r"^GLS RWY ([0-9]{1,2}[LRC]?)$");
regex!(RE_SDF_RUNWAY, r"^SDF RWY ([0-9]{1,2}[LRC]?)$");
regex!(
    RE_SDF_VARIANT_RUNWAY,
    r"^SDF ([UVWXYZ]) RWY ([0-9]{1,2}[LRC]?)$"
);
regex!(RE_NDB_RUNWAY, r"^NDB(?:/DME)? RWY ([0-9]{1,2}[LRC]?)$");
regex!(
    RE_NDB_VARIANT_RUNWAY,
    r"^NDB(?:/DME)? ([UVWXYZ]) RWY ([0-9]{1,2}[LRC]?)$"
);
regex!(
    RE_VOR_DME_RUNWAY,
    r"^VOR(?: AND DME|/DME) RWY ([0-9]{1,2}[LRC]?)$"
);
regex!(
    RE_VOR_DME_RUNWAY_PAIR,
    r"^VOR(?: AND DME|/DME) RWY ([0-9]{1,2}[LRC]? AND [LRC])$"
);
regex!(
    RE_VOR_RUNWAY,
    r"^VOR(?: OR DME|/DME)? RWY ([0-9]{1,2}[LRC]?)$"
);
regex!(
    RE_VOR_RUNWAY_PAIR,
    r"^VOR(?: OR DME|/DME)? RWY ([0-9]{1,2}[LRC]? AND [LRC])$"
);
regex!(
    RE_VOR_VARIANT_RUNWAY,
    r"^VOR(?: OR DME|/DME)? ([UVWXYZ]) RWY ([0-9]{1,2}[LRC]?)$"
);
regex!(
    RE_VOR_VARIANT_OR_TACAN_RUNWAY,
    r"^VOR(?: OR DME|/DME)? ([UVWXYZ]) OR TACAN(?: [UVWXYZ])? RWY ([0-9]{1,2}[LRC]?)$"
);
regex!(RE_TACAN_RUNWAY, r"^TACAN RWY ([0-9]{1,2}[LRC]?)$");
regex!(
    RE_TACAN_VARIANT_RUNWAY,
    r"^TACAN ([XYZ]) RWY ([0-9]{1,2}[LRC]?)$"
);
regex!(RE_HI_TACAN_RUNWAY, r"^HI-TACAN RWY ([0-9]{1,2}[LRC]?)$");
regex!(
    RE_HI_TACAN_VARIANT_RUNWAY,
    r"^HI-TACAN ([XYZ]) RWY ([0-9]{1,2}[LRC]?)$"
);
regex!(RE_VOR_CIRCLING, r"^VOR(?: OR DME|/DME)?-([A-Z])$");
regex!(RE_VOR_OR_TACAN_CIRCLING, r"^VOR OR TACAN-([A-Z])$");
regex!(RE_VOR_OR_GPS_CIRCLING, r"^VOR OR GPS-([A-Z])$");
regex!(RE_VDM_CIRCLING, r"^VOR(?: AND DME|/DME)-([A-Z])$");
regex!(RE_VDM_OR_GPS_CIRCLING, r"^VOR AND DME OR GPS-([A-Z])$");
regex!(RE_NDB_CIRCLING, r"^NDB(?:/DME)?-([A-Z])$");
regex!(RE_RNAV_CIRCLING, r"^RNAV \((?:GPS|RNP)\)-([A-Z])$");
regex!(RE_LOC_CIRCLING, r"^LOC(?: AND DME)?-([A-Z])$");
regex!(RE_LDA_CIRCLING, r"^LDA-([A-Z])$");
regex!(
    RE_LDA_VARIANT_RUNWAY,
    r"^LDA ([XYZ]) RWY ([0-9]{1,2}[LRC]?)$"
);
regex!(RE_LDA_RUNWAY, r"^LDA RWY ([0-9]{1,2}[LRC]?)$");
regex!(RE_LOC_BC_RUNWAY, r"^LOC BC RWY ([0-9]{1,2}[LRC]?)$");

fn strip_iap_prefix(label: &str) -> String {
    IAP_PREFIX_RE
        .captures(label)
        .and_then(|captures| captures.get(1).map(|value| value.as_str().to_string()))
        .unwrap_or_else(|| label.to_string())
}

fn runway_candidate(prefix: &str, runway: &str, variant: Option<&str>, style: &str) -> String {
    let Some(variant) = variant else {
        return format!("{prefix}{runway}");
    };
    let resolved_style = if style == "auto" {
        if runway.ends_with(['L', 'R', 'C']) {
            "suffix"
        } else {
            "hyphen"
        }
    } else {
        style
    };
    if resolved_style == "suffix" {
        format!("{prefix}{runway}{variant}")
    } else {
        format!("{prefix}{runway}-{variant}")
    }
}

fn circling_candidate(prefix: &str, variant: &str) -> String {
    format!("{prefix}-{variant}")
}

fn expand_runway_pair(text: &str) -> Vec<String> {
    let Some(captures) = RUNWAY_PAIR_RE.captures(text) else {
        return vec![text.to_string()];
    };
    let base = captures
        .get(1)
        .map(|value| value.as_str())
        .unwrap_or_default();
    let first_suffix = captures
        .get(2)
        .map(|value| value.as_str())
        .unwrap_or_default();
    let second_suffix = captures
        .get(3)
        .map(|value| value.as_str())
        .unwrap_or_default();
    let first = format!("{base}{first_suffix}");
    let second = format!("{base}{second_suffix}");
    if first == second {
        vec![first]
    } else {
        vec![first, second]
    }
}

fn singleton_group(candidate: String) -> Vec<BTreeSet<String>> {
    vec![BTreeSet::from([candidate])]
}

fn runway_pair_groups(
    prefixes: &[&str],
    runways: &[String],
    variant: Option<&str>,
    style: &str,
) -> Vec<BTreeSet<String>> {
    let mut groups = Vec::new();
    for prefix in prefixes {
        for runway in runways {
            groups.push(BTreeSet::from([runway_candidate(
                prefix, runway, variant, style,
            )]));
        }
    }
    groups
}

fn heuristic_candidate_groups(label: &str) -> Vec<BTreeSet<String>> {
    let mut body = strip_iap_prefix(&label.to_ascii_uppercase());
    body = CAT_SUFFIX_RE.replace(&body, "").to_string();
    let body = body.trim().to_string();

    if let Some(captures) = RE_VOR_DME_OR_TACAN_RUNWAY.captures(&body) {
        let runway = captures.get(1).unwrap().as_str();
        return vec![
            BTreeSet::from([runway_candidate("V", runway, None, "hyphen")]),
            BTreeSet::from([runway_candidate("T", runway, None, "hyphen")]),
            BTreeSet::from([runway_candidate("S", runway, None, "hyphen")]),
            BTreeSet::from([runway_candidate("D", runway, None, "hyphen")]),
        ];
    }
    if let Some(captures) = RE_VOR_DME_OR_TACAN_RUNWAY_PAIR.captures(&body) {
        let runways = expand_runway_pair(captures.get(1).unwrap().as_str());
        return runway_pair_groups(&["V", "T", "S", "D"], &runways, None, "hyphen");
    }
    if let Some(captures) = RE_ILS_OR_LOC_RUNWAY.captures(&body) {
        let runway = captures.get(1).unwrap().as_str();
        return vec![
            BTreeSet::from([runway_candidate("I", runway, None, "auto")]),
            BTreeSet::from([runway_candidate("L", runway, None, "auto")]),
        ];
    }
    if let Some(captures) = RE_ILS_VARIANT_OR_LOC_RUNWAY.captures(&body) {
        let variant = captures.get(1).map(|value| value.as_str());
        let runway = captures.get(2).unwrap().as_str();
        return vec![
            BTreeSet::from([runway_candidate("I", runway, variant, "auto")]),
            BTreeSet::from([runway_candidate("L", runway, variant, "auto")]),
        ];
    }
    if let Some(captures) = RE_ILS_PRM_RUNWAY.captures(&body) {
        let runway = captures.get(1).unwrap().as_str();
        return vec![
            BTreeSet::from([runway_candidate("I", runway, None, "hyphen")]),
            BTreeSet::from([runway_candidate("L", runway, None, "hyphen")]),
        ];
    }
    if let Some(captures) = RE_ILS_PRM_VARIANT_RUNWAY.captures(&body) {
        let variant = captures.get(1).unwrap().as_str();
        let runway = captures.get(2).unwrap().as_str();
        return vec![
            BTreeSet::from([runway_candidate("I", runway, Some(variant), "auto")]),
            BTreeSet::from([runway_candidate("L", runway, None, "hyphen")]),
        ];
    }
    if let Some(captures) = RE_ILS_OR_LOC_AND_DME_RUNWAY.captures(&body) {
        let runway = captures.get(1).unwrap().as_str();
        return vec![
            BTreeSet::from([runway_candidate("I", runway, None, "auto")]),
            BTreeSet::from([runway_candidate("L", runway, None, "auto")]),
        ];
    }
    if let Some(captures) = RE_ILS_VARIANT_OR_LOC_AND_DME_RUNWAY.captures(&body) {
        let variant = captures.get(1).map(|value| value.as_str());
        let runway = captures.get(2).unwrap().as_str();
        return vec![
            BTreeSet::from([runway_candidate("I", runway, variant, "auto")]),
            BTreeSet::from([runway_candidate("L", runway, variant, "auto")]),
        ];
    }
    if let Some(captures) = RE_VOR_OR_TACAN_RUNWAY.captures(&body) {
        let runway = captures.get(1).unwrap().as_str();
        return vec![
            BTreeSet::from([runway_candidate("V", runway, None, "hyphen")]),
            BTreeSet::from([runway_candidate("T", runway, None, "hyphen")]),
            BTreeSet::from([runway_candidate("S", runway, None, "hyphen")]),
        ];
    }
    if let Some(captures) = RE_VOR_OR_TACAN_RUNWAY_PAIR.captures(&body) {
        let runways = expand_runway_pair(captures.get(1).unwrap().as_str());
        return runway_pair_groups(&["V", "T", "S"], &runways, None, "hyphen");
    }
    if let Some(captures) = RE_LOC_RUNWAY.captures(&body) {
        return singleton_group(runway_candidate(
            "L",
            captures.get(1).unwrap().as_str(),
            None,
            "hyphen",
        ));
    }
    if let Some(captures) = RE_LOC_AND_DME_RUNWAY.captures(&body) {
        return singleton_group(runway_candidate(
            "L",
            captures.get(1).unwrap().as_str(),
            None,
            "hyphen",
        ));
    }
    if let Some(captures) = RE_LOC_VARIANT_RUNWAY.captures(&body) {
        return singleton_group(runway_candidate(
            "L",
            captures.get(2).unwrap().as_str(),
            captures.get(1).map(|value| value.as_str()),
            "auto",
        ));
    }
    if let Some(captures) = RE_LOC_AND_DME_VARIANT_RUNWAY.captures(&body) {
        return singleton_group(runway_candidate(
            "L",
            captures.get(2).unwrap().as_str(),
            captures.get(1).map(|value| value.as_str()),
            "auto",
        ));
    }
    if let Some(captures) = RE_ILS_RUNWAY.captures(&body) {
        return singleton_group(runway_candidate(
            "I",
            captures.get(1).unwrap().as_str(),
            None,
            "hyphen",
        ));
    }
    if let Some(captures) = RE_ILS_VARIANT_RUNWAY.captures(&body) {
        let variant = captures.get(1).map(|value| value.as_str());
        let runway = captures.get(2).unwrap().as_str();
        return vec![
            BTreeSet::from([runway_candidate("I", runway, variant, "auto")]),
            BTreeSet::from([runway_candidate("L", runway, variant, "auto")]),
        ];
    }
    if let Some(captures) = RE_RNAV_GPS_RUNWAY.captures(&body) {
        return singleton_group(runway_candidate(
            "R",
            captures.get(1).unwrap().as_str(),
            None,
            "hyphen",
        ));
    }
    if let Some(captures) = RE_GPS_RUNWAY.captures(&body) {
        return singleton_group(runway_candidate(
            "P",
            captures.get(1).unwrap().as_str(),
            None,
            "hyphen",
        ));
    }
    if let Some(captures) = RE_RNAV_GPS_VARIANT_RUNWAY.captures(&body) {
        return singleton_group(runway_candidate(
            "R",
            captures.get(2).unwrap().as_str(),
            captures.get(1).map(|value| value.as_str()),
            "auto",
        ));
    }
    if let Some(captures) = RE_RNAV_RNP_VARIANT_RUNWAY.captures(&body) {
        return singleton_group(runway_candidate(
            "H",
            captures.get(2).unwrap().as_str(),
            captures.get(1).map(|value| value.as_str()),
            "auto",
        ));
    }
    if let Some(captures) = RE_RNAV_RNP_RUNWAY.captures(&body) {
        return singleton_group(runway_candidate(
            "H",
            captures.get(1).unwrap().as_str(),
            None,
            "hyphen",
        ));
    }
    if let Some(captures) = RE_RNAV_RNP_RUNWAY_PAIR.captures(&body) {
        let runways = expand_runway_pair(captures.get(1).unwrap().as_str());
        return runway_pair_groups(&["H"], &runways, None, "hyphen");
    }
    if let Some(captures) = RE_GLS_RUNWAY.captures(&body) {
        return singleton_group(runway_candidate(
            "G",
            captures.get(1).unwrap().as_str(),
            None,
            "hyphen",
        ));
    }
    if let Some(captures) = RE_SDF_RUNWAY.captures(&body) {
        return singleton_group(runway_candidate(
            "S",
            captures.get(1).unwrap().as_str(),
            None,
            "hyphen",
        ));
    }
    if let Some(captures) = RE_SDF_VARIANT_RUNWAY.captures(&body) {
        return singleton_group(runway_candidate(
            "S",
            captures.get(2).unwrap().as_str(),
            captures.get(1).map(|value| value.as_str()),
            "auto",
        ));
    }
    if let Some(captures) = RE_NDB_RUNWAY.captures(&body) {
        let runway = captures.get(1).unwrap().as_str();
        return vec![
            BTreeSet::from([runway_candidate("N", runway, None, "hyphen")]),
            BTreeSet::from([runway_candidate("S", runway, None, "hyphen")]),
        ];
    }
    if let Some(captures) = RE_NDB_VARIANT_RUNWAY.captures(&body) {
        let variant = captures.get(1).map(|value| value.as_str());
        let runway = captures.get(2).unwrap().as_str();
        return vec![
            BTreeSet::from([runway_candidate("N", runway, variant, "auto")]),
            BTreeSet::from([runway_candidate("S", runway, variant, "auto")]),
        ];
    }
    if let Some(captures) = RE_VOR_DME_RUNWAY.captures(&body) {
        let runway = captures.get(1).unwrap().as_str();
        return vec![
            BTreeSet::from([runway_candidate("V", runway, None, "hyphen")]),
            BTreeSet::from([runway_candidate("S", runway, None, "hyphen")]),
            BTreeSet::from([runway_candidate("D", runway, None, "hyphen")]),
        ];
    }
    if let Some(captures) = RE_VOR_DME_RUNWAY_PAIR.captures(&body) {
        let runways = expand_runway_pair(captures.get(1).unwrap().as_str());
        return runway_pair_groups(&["V", "S", "D"], &runways, None, "hyphen");
    }
    if let Some(captures) = RE_VOR_RUNWAY.captures(&body) {
        let runway = captures.get(1).unwrap().as_str();
        return vec![
            BTreeSet::from([runway_candidate("V", runway, None, "hyphen")]),
            BTreeSet::from([runway_candidate("S", runway, None, "hyphen")]),
        ];
    }
    if let Some(captures) = RE_VOR_RUNWAY_PAIR.captures(&body) {
        let runways = expand_runway_pair(captures.get(1).unwrap().as_str());
        return runway_pair_groups(&["V", "S"], &runways, None, "hyphen");
    }
    if let Some(captures) = RE_VOR_VARIANT_RUNWAY.captures(&body) {
        let variant = captures.get(1).map(|value| value.as_str());
        let runway = captures.get(2).unwrap().as_str();
        return vec![
            BTreeSet::from([runway_candidate("V", runway, variant, "auto")]),
            BTreeSet::from([runway_candidate("S", runway, variant, "auto")]),
        ];
    }
    if let Some(captures) = RE_VOR_VARIANT_OR_TACAN_RUNWAY.captures(&body) {
        let variant = captures.get(1).map(|value| value.as_str());
        let runway = captures.get(2).unwrap().as_str();
        return vec![
            BTreeSet::from([runway_candidate("V", runway, variant, "auto")]),
            BTreeSet::from([runway_candidate("T", runway, variant, "auto")]),
            BTreeSet::from([runway_candidate("S", runway, variant, "auto")]),
        ];
    }
    if let Some(captures) = RE_TACAN_RUNWAY.captures(&body) {
        let runway = captures.get(1).unwrap().as_str();
        return vec![
            BTreeSet::from([runway_candidate("T", runway, None, "hyphen")]),
            BTreeSet::from([runway_candidate("S", runway, None, "hyphen")]),
        ];
    }
    if let Some(captures) = RE_TACAN_VARIANT_RUNWAY.captures(&body) {
        return singleton_group(runway_candidate(
            "T",
            captures.get(2).unwrap().as_str(),
            captures.get(1).map(|value| value.as_str()),
            "auto",
        ));
    }
    if let Some(captures) = RE_HI_TACAN_RUNWAY.captures(&body) {
        return singleton_group(runway_candidate(
            "H",
            captures.get(1).unwrap().as_str(),
            None,
            "hyphen",
        ));
    }
    if let Some(captures) = RE_HI_TACAN_VARIANT_RUNWAY.captures(&body) {
        return singleton_group(runway_candidate(
            "H",
            captures.get(2).unwrap().as_str(),
            captures.get(1).map(|value| value.as_str()),
            "auto",
        ));
    }
    if let Some(captures) = RE_VOR_CIRCLING.captures(&body) {
        return vec![BTreeSet::from([
            circling_candidate("VOR", captures.get(1).unwrap().as_str()),
            circling_candidate("VDM", captures.get(1).unwrap().as_str()),
        ])];
    }
    if let Some(captures) = RE_VOR_OR_TACAN_CIRCLING.captures(&body) {
        return singleton_group(circling_candidate("VOR", captures.get(1).unwrap().as_str()));
    }
    if let Some(captures) = RE_VOR_OR_GPS_CIRCLING.captures(&body) {
        return singleton_group(circling_candidate("VOR", captures.get(1).unwrap().as_str()));
    }
    if let Some(captures) = RE_VDM_CIRCLING.captures(&body) {
        return singleton_group(circling_candidate("VDM", captures.get(1).unwrap().as_str()));
    }
    if let Some(captures) = RE_VDM_OR_GPS_CIRCLING.captures(&body) {
        return singleton_group(circling_candidate("VDM", captures.get(1).unwrap().as_str()));
    }
    if let Some(captures) = RE_NDB_CIRCLING.captures(&body) {
        return singleton_group(circling_candidate("NDB", captures.get(1).unwrap().as_str()));
    }
    if let Some(captures) = RE_RNAV_CIRCLING.captures(&body) {
        return singleton_group(circling_candidate("RNV", captures.get(1).unwrap().as_str()));
    }
    if let Some(captures) = RE_LOC_CIRCLING.captures(&body) {
        return vec![BTreeSet::from([
            circling_candidate("LOC", captures.get(1).unwrap().as_str()),
            circling_candidate("LDA", captures.get(1).unwrap().as_str()),
        ])];
    }
    if let Some(captures) = RE_LDA_CIRCLING.captures(&body) {
        return singleton_group(circling_candidate("LDA", captures.get(1).unwrap().as_str()));
    }
    if let Some(captures) = RE_LDA_VARIANT_RUNWAY.captures(&body) {
        return singleton_group(runway_candidate(
            "X",
            captures.get(2).unwrap().as_str(),
            captures.get(1).map(|value| value.as_str()),
            "auto",
        ));
    }
    if let Some(captures) = RE_LDA_RUNWAY.captures(&body) {
        return singleton_group(runway_candidate(
            "X",
            captures.get(1).unwrap().as_str(),
            None,
            "hyphen",
        ));
    }
    if let Some(captures) = RE_LOC_BC_RUNWAY.captures(&body) {
        return singleton_group(runway_candidate(
            "B",
            captures.get(1).unwrap().as_str(),
            None,
            "hyphen",
        ));
    }
    Vec::new()
}

fn heuristic_candidate_groups_for_copter_plate(label: &str) -> Vec<BTreeSet<String>> {
    let stripped = Regex::new(r"^(IAP-[A-Z]{2}-)COPTER ")
        .expect("valid copter regex")
        .replace(label, "$1")
        .to_string();
    heuristic_candidate_groups(&stripped)
}

fn is_visual_plate(label: &str) -> bool {
    label.to_ascii_uppercase().contains("VISUAL")
}

fn is_public_plate(label: &str) -> bool {
    let upper = label.to_ascii_uppercase();
    !upper.contains("SA CAT") && !upper.contains("SPECIAL AIRCREW") && !upper.contains("COPTER")
}

fn plate_priority(label: &str) -> i64 {
    let upper = label.to_ascii_uppercase();
    if upper.contains("COPTER") {
        3
    } else if upper.contains("SA CAT") || upper.contains("SPECIAL AIRCREW") {
        2
    } else if upper.contains("CAT II") || upper.contains("CAT III") {
        1
    } else {
        0
    }
}

fn canonical_airport_id(raw_id: &str, aliases: &BTreeMap<String, String>) -> String {
    aliases
        .get(raw_id.trim())
        .cloned()
        .unwrap_or_else(|| raw_id.trim().to_string())
}

pub fn choose_bundle(artifact_root: &Path, explicit_bundle: Option<&Path>) -> Result<PathBuf> {
    if let Some(bundle) = explicit_bundle {
        return Ok(bundle.to_path_buf());
    }
    let production_root = artifact_root.join("published-packaged").join("production");
    let mut bundles = fs::read_dir(&production_root)
        .with_context(|| format!("failed to read {}", production_root.display()))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("failed to iterate {}", production_root.display()))?
        .into_iter()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|value| value.to_str())
                .map(|value| value.starts_with("bundle_") && value.ends_with(".json"))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    bundles.sort();
    bundles.pop().ok_or_else(|| {
        anyhow::anyhow!(
            "no bundle_*.json files found under {}",
            production_root.display()
        )
    })
}

pub fn load_bundle(bundle_path: &Path) -> Result<serde_json::Value> {
    serde_json::from_slice(
        &fs::read(bundle_path)
            .with_context(|| format!("failed to read {}", bundle_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", bundle_path.display()))
}

pub fn resolve_db_path(artifact_root: &Path, bundle: &serde_json::Value) -> Result<PathBuf> {
    let cycle = bundle
        .get("cycle")
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow::anyhow!("bundle missing cycle"))?;
    let relative_zip = bundle
        .get("data")
        .and_then(|value| value.get("relative_path"))
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow::anyhow!("bundle missing data.relative_path"))?;
    let relative_zip = PathBuf::from(relative_zip);
    let unpacked_dir = artifact_root
        .join("published-unpacked")
        .join("production")
        .join(cycle)
        .join(relative_zip.with_extension(""));
    for candidate in [
        unpacked_dir.join(INTERMEDIATE_SQLITE_BASENAME),
        unpacked_dir
            .parent()
            .unwrap_or(&unpacked_dir)
            .join(INTERMEDIATE_SQLITE_BASENAME),
        unpacked_dir.join("main.db"),
        unpacked_dir
            .parent()
            .unwrap_or(&unpacked_dir)
            .join("main.db"),
    ] {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    anyhow::bail!("could not locate intermediate sqlite for bundle {cycle}");
}

pub fn tpp_zip_paths_from_bundle(
    artifact_root: &Path,
    bundle: &serde_json::Value,
) -> Result<Vec<PathBuf>> {
    let packages = bundle
        .get("packages")
        .and_then(|value| value.as_array())
        .ok_or_else(|| anyhow::anyhow!("bundle missing packages"))?;
    let mut zips = Vec::new();
    for package in packages {
        if package
            .get("family_id")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            != "tpp"
        {
            continue;
        }
        let relative = package
            .get("relative_path")
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow::anyhow!("bundle package missing relative_path"))?;
        zips.push(artifact_root.join(relative));
    }
    Ok(zips)
}

pub fn load_tpp_approach_plates(package_zips: &[PathBuf]) -> Result<Vec<PlateRecord>> {
    let mut plates = Vec::new();
    for zip_path in package_zips {
        let zip_file = fs::File::open(zip_path)
            .with_context(|| format!("failed to open {}", zip_path.display()))?;
        let mut archive = ZipArchive::new(zip_file)
            .with_context(|| format!("failed to read {}", zip_path.display()))?;
        let mut entry = archive
            .by_name(PACKAGE_ASSET_MANIFEST_NAME)
            .with_context(|| {
                format!(
                    "missing {} in {}",
                    PACKAGE_ASSET_MANIFEST_NAME,
                    zip_path.display()
                )
            })?;
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut bytes)?;
        let manifest: PackageAssetManifest =
            serde_json::from_slice(&bytes).context("failed to parse package asset manifest")?;
        for asset in manifest.assets {
            if asset.document_type != "approach" {
                continue;
            }
            let canonical_airport_id = asset
                .icao_airport_id
                .clone()
                .unwrap_or_else(|| asset.airport_id.trim().to_string());
            plates.push(PlateRecord {
                plate_id: asset.id,
                airport_id: canonical_airport_id,
                package_id: manifest.package_id.clone(),
                label: asset.label.trim().to_string(),
            });
        }
    }
    Ok(plates)
}

pub fn load_cifp_approaches(db_path: &Path) -> Result<BTreeMap<String, BTreeSet<String>>> {
    let conn = Connection::open(db_path)
        .with_context(|| format!("failed to open {}", db_path.display()))?;
    let mut stmt = conn.prepare(
        "SELECT DISTINCT trim(airport_identifier), trim(sid_star_approach_identifier)
         FROM cifp_sid_star_app
         WHERE trim(route_type) NOT IN ('1','2','3','4','5','6','T')",
    )?;
    let mut rows = stmt.query([])?;
    let mut by_airport = BTreeMap::<String, BTreeSet<String>>::new();
    while let Some(row) = rows.next()? {
        let airport_id: String = row.get(0)?;
        let procedure_id: String = row.get(1)?;
        if airport_id.is_empty() || procedure_id.is_empty() {
            continue;
        }
        by_airport
            .entry(airport_id)
            .or_default()
            .insert(procedure_id);
    }
    Ok(by_airport)
}

pub fn load_airport_aliases(db_path: &Path) -> Result<BTreeMap<String, String>> {
    let conn = Connection::open(db_path)
        .with_context(|| format!("failed to open {}", db_path.display()))?;
    let mut stmt = conn.prepare("SELECT trim(alias_id), trim(airport_id) FROM airport_aliases")?;
    let mut rows = stmt.query([])?;
    let mut aliases = BTreeMap::new();
    while let Some(row) = rows.next()? {
        let alias_id: String = row.get(0)?;
        let airport_id: String = row.get(1)?;
        if !alias_id.is_empty() && !airport_id.is_empty() {
            aliases.insert(alias_id, airport_id);
        }
    }
    Ok(aliases)
}

fn compare_counts(
    plates: &[PlateRecord],
    cifp: &BTreeMap<String, BTreeSet<String>>,
    aliases: &BTreeMap<String, String>,
) -> Vec<(String, usize, usize)> {
    let mut plate_counts = BTreeMap::<String, usize>::new();
    for plate in plates {
        let airport_id = canonical_airport_id(&plate.airport_id, aliases);
        *plate_counts.entry(airport_id).or_default() += 1;
    }
    let airports = plate_counts
        .keys()
        .cloned()
        .chain(cifp.keys().cloned())
        .collect::<BTreeSet<_>>();
    airports
        .into_iter()
        .map(|airport_id| {
            (
                airport_id.clone(),
                *plate_counts.get(&airport_id).unwrap_or(&0),
                cifp.get(&airport_id).map(|value| value.len()).unwrap_or(0),
            )
        })
        .collect()
}

fn analyze_matches(
    plates: &[PlateRecord],
    cifp: &BTreeMap<String, BTreeSet<String>>,
    aliases: &BTreeMap<String, String>,
) -> MatchAnalysis {
    let mut summary = MatchSummary::default();
    let mut examples = Vec::new();

    for plate in plates {
        let airport_id = canonical_airport_id(&plate.airport_id, aliases);
        let procedure_ids = cifp.get(&airport_id).cloned().unwrap_or_default();
        let candidate_groups = heuristic_candidate_groups(&plate.label);
        let matched_groups = candidate_groups
            .iter()
            .map(|group| {
                group
                    .intersection(&procedure_ids)
                    .cloned()
                    .collect::<BTreeSet<_>>()
            })
            .collect::<Vec<_>>();
        let matched = matched_groups
            .iter()
            .flat_map(|group| group.iter().cloned())
            .collect::<BTreeSet<_>>();

        if procedure_ids.is_empty() {
            summary.airport_missing_from_cifp += 1;
        } else if candidate_groups.is_empty() {
            summary.no_heuristic += 1;
        } else {
            let ambiguous_groups = matched_groups
                .iter()
                .filter(|group| group.len() > 1)
                .count();
            let missing_groups = matched_groups
                .iter()
                .filter(|group| group.is_empty())
                .count();
            let singleton_groups = matched_groups
                .iter()
                .filter(|group| group.len() == 1)
                .count();
            if ambiguous_groups > 0 {
                summary.matched_ambiguous += 1;
            } else if missing_groups > 0 && singleton_groups == 0 {
                summary.matched_none += 1;
            } else if missing_groups > 0 {
                summary.matched_partial += 1;
            } else {
                summary.matched_unique += 1;
            }
        }

        if examples.len() < 50
            && (procedure_ids.is_empty()
                || candidate_groups.is_empty()
                || matched_groups.iter().any(|group| group.len() != 1))
        {
            examples.push(MatchExample {
                airport_id: plate.airport_id.clone(),
                label: plate.label.clone(),
                candidate_groups: candidate_groups
                    .iter()
                    .map(|group| group.iter().cloned().collect())
                    .collect(),
                matched: matched.into_iter().collect(),
            });
        }
    }

    MatchAnalysis { summary, examples }
}

fn classify_relation(
    plates: &[PlateRecord],
    cifp: &BTreeMap<String, BTreeSet<String>>,
    aliases: &BTreeMap<String, String>,
) -> RelationAnalysis {
    let mut plates_by_airport = BTreeMap::<String, Vec<PlateRecord>>::new();
    for plate in plates {
        if is_visual_plate(&plate.label) {
            continue;
        }
        let airport_id = canonical_airport_id(&plate.airport_id, aliases);
        if !cifp.contains_key(&airport_id) {
            continue;
        }
        plates_by_airport
            .entry(airport_id)
            .or_default()
            .push(plate.clone());
    }

    let mut summary = RelationSummary::default();
    let mut examples = Vec::new();
    let mut published_rows = Vec::new();

    for (airport_id, airport_plates) in plates_by_airport {
        let procedure_ids = cifp.get(&airport_id).cloned().unwrap_or_default();
        let mut cid_claimers = BTreeMap::<String, Vec<Claimer>>::new();
        let mut copter_claimers = BTreeMap::<String, Vec<String>>::new();
        let mut ignored_noheur = 0;
        let mut ignored_nomatch = 0;

        for plate in &airport_plates {
            if plate.label.to_ascii_uppercase().contains("COPTER") {
                for group in heuristic_candidate_groups_for_copter_plate(&plate.label) {
                    let matched = group
                        .intersection(&procedure_ids)
                        .cloned()
                        .collect::<BTreeSet<_>>();
                    if matched.len() == 1 {
                        let cid = matched.iter().next().cloned().unwrap();
                        copter_claimers
                            .entry(cid)
                            .or_default()
                            .push(plate.label.clone());
                    }
                }
            }

            let groups = heuristic_candidate_groups(&plate.label);
            if groups.is_empty() {
                ignored_noheur += 1;
                continue;
            }

            let mut any_group_bound = false;
            for group in groups {
                let matched = group
                    .intersection(&procedure_ids)
                    .cloned()
                    .collect::<BTreeSet<_>>();
                if matched.len() == 1 {
                    let cid = matched.iter().next().cloned().unwrap();
                    cid_claimers.entry(cid).or_default().push(Claimer {
                        plate_id: plate.plate_id.clone(),
                        plate_label: plate.label.clone(),
                        package_id: plate.package_id.clone(),
                        public: is_public_plate(&plate.label),
                        priority: plate_priority(&plate.label),
                    });
                    any_group_bound = true;
                }
            }

            if !any_group_bound {
                ignored_nomatch += 1;
            }
        }

        let uniquely_bound = cid_claimers
            .iter()
            .filter_map(|(cid, claimers)| {
                (claimers.len() == 1).then(|| (cid.clone(), claimers[0].clone()))
            })
            .collect::<BTreeMap<_, _>>();
        let multiply_bound = cid_claimers
            .iter()
            .filter_map(|(cid, claimers)| {
                (claimers.len() > 1).then(|| (cid.clone(), claimers.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        let unresolved_set = procedure_ids
            .iter()
            .filter(|cid| !uniquely_bound.contains_key(*cid) && !multiply_bound.contains_key(*cid))
            .cloned()
            .collect::<BTreeSet<_>>();
        let copter_only = unresolved_set
            .iter()
            .filter(|cid| copter_claimers.contains_key(*cid))
            .cloned()
            .collect::<Vec<_>>();
        let unresolved = unresolved_set
            .into_iter()
            .filter(|cid| !copter_only.contains(cid))
            .collect::<Vec<_>>();

        summary.airports_considered += 1;
        summary.uniquely_bound_cids_total += uniquely_bound.len();
        summary.multiply_bound_cids_total += multiply_bound.len();
        summary.copter_only_cids_total += copter_only.len();
        summary.unresolved_cids_total += unresolved.len();
        summary.ignored_noheur_plates_total += ignored_noheur;
        summary.ignored_nomatch_plates_total += ignored_nomatch;
        if unresolved.is_empty() {
            summary.airports_with_no_unresolved_cids += 1;
        } else {
            summary.airports_with_unresolved_cids += 1;
        }

        for (cid, claimer) in &uniquely_bound {
            published_rows.push(PublishedMatchRow {
                airport_id: airport_id.clone(),
                cifp_id: cid.clone(),
                plate_id: claimer.plate_id.clone(),
                plate_label: pretty_match_plate_label(&claimer.plate_label),
                package_id: claimer.package_id.clone(),
                public: claimer.public,
                priority: claimer.priority,
                match_kind: "unique".to_string(),
                is_primary: true,
            });
        }
        for (cid, claimers) in &multiply_bound {
            let mut claimers = claimers.clone();
            claimers.sort_by_key(|claimer| {
                (
                    claimer.priority,
                    !claimer.public,
                    claimer.plate_label.clone(),
                    claimer.plate_id.clone(),
                )
            });
            for (index, claimer) in claimers.into_iter().enumerate() {
                published_rows.push(PublishedMatchRow {
                    airport_id: airport_id.clone(),
                    cifp_id: cid.clone(),
                    plate_id: claimer.plate_id.clone(),
                    plate_label: pretty_match_plate_label(&claimer.plate_label),
                    package_id: claimer.package_id.clone(),
                    public: claimer.public,
                    priority: claimer.priority,
                    match_kind: "multiple".to_string(),
                    is_primary: index == 0,
                });
            }
        }

        if !unresolved.is_empty() || !multiply_bound.is_empty() {
            examples.push(RelationAirportExample {
                airport: airport_id,
                cifp_total: procedure_ids.len(),
                uniquely_bound: uniquely_bound.len(),
                multiply_bound: multiply_bound.len(),
                copter_only: copter_only.len(),
                unresolved_count: unresolved.len(),
                copter_only_cids: copter_only,
                unresolved_cids: unresolved,
                multiply_bound_examples: multiply_bound,
                ignored_noheur_plates: ignored_noheur,
                ignored_nomatch_plates: ignored_nomatch,
            });
        }
    }

    examples.sort_by_key(|row| {
        (
            usize::MAX - row.unresolved_count,
            usize::MAX - row.multiply_bound,
            row.airport.clone(),
        )
    });
    RelationAnalysis {
        summary,
        examples,
        published_rows,
    }
}

fn pretty_match_plate_label(label: &str) -> String {
    let trimmed = label.trim();
    if let Some(captures) = IAP_PREFIX_RE.captures(trimmed) {
        return captures
            .get(1)
            .map(|m| {
                m.as_str()
                    .replace("RNAV (GPS)", "RNAV")
                    .replace(" RWY ", " ")
                    .replace(" OR ", " or ")
                    .replace(" AND ", " and ")
            })
            .unwrap_or_else(|| trimmed.to_string());
    }
    let Some((prefix, remainder)) = split_non_iap_tpp_prefix(trimmed) else {
        return trimmed.to_string();
    };
    match prefix {
        "APD" => "Airport Diagram".to_string(),
        "MIN" if remainder.starts_with("ALTERNATE MINIMUMS-") => {
            format!(
                "Alt Minimums {}",
                remainder.trim_start_matches("ALTERNATE MINIMUMS-")
            )
        }
        "MIN" if remainder == "ALTERNATE MINIMUMS" => "Alt Minimums".to_string(),
        "MIN" if remainder.starts_with("TAKEOFF MINIMUMS-") => {
            format!(
                "Takeoff Minimums {}",
                remainder.trim_start_matches("TAKEOFF MINIMUMS-")
            )
        }
        "MIN" if remainder == "TAKEOFF MINIMUMS" => "Takeoff Minimums".to_string(),
        "DP" | "ODP" | "STAR" => remainder.to_string(),
        _ => trimmed.to_string(),
    }
}

fn split_non_iap_tpp_prefix(label: &str) -> Option<(&str, &str)> {
    let mut parts = label.splitn(3, '-');
    let prefix = parts.next()?;
    let state = parts.next()?;
    let remainder = parts.next()?;
    if state.len() != 2 {
        return None;
    }
    Some((prefix, remainder))
}

pub fn audit_tpp_cifp_matching(
    main_db: &Path,
    tpp_package_zips: &[PathBuf],
) -> Result<TppCifpAuditReport> {
    let plates = load_tpp_approach_plates(tpp_package_zips)?;
    let cifp = load_cifp_approaches(main_db)?;
    let aliases = load_airport_aliases(main_db)?;
    let count_rows = compare_counts(&plates, &cifp, &aliases);
    let exact_count_match = count_rows
        .iter()
        .filter(|(_, plates, cifp)| plates == cifp)
        .count();
    let count_mismatch = count_rows.len() - exact_count_match;
    let match_analysis = analyze_matches(&plates, &cifp, &aliases);
    let relation = classify_relation(&plates, &cifp, &aliases);
    Ok(TppCifpAuditReport {
        approach_plate_count: plates.len(),
        airports_with_approach_plates: count_rows.iter().filter(|(_, count, _)| *count > 0).count(),
        airports_with_cifp_approaches: count_rows.iter().filter(|(_, _, count)| *count > 0).count(),
        exact_count_match,
        count_mismatch,
        count_rows,
        match_summary: match_analysis.summary,
        match_examples: match_analysis.examples,
        relation_summary: relation.summary,
        relation_examples: relation.examples,
    })
}

pub fn publish_tpp_cifp_matches(
    main_db: &Path,
    tpp_package_zips: &[PathBuf],
) -> Result<PublishedMatchSummary> {
    let plates = load_tpp_approach_plates(tpp_package_zips)?;
    let cifp = load_cifp_approaches(main_db)?;
    let aliases = load_airport_aliases(main_db)?;
    let relation = classify_relation(&plates, &cifp, &aliases);
    let conn = Connection::open(main_db)
        .with_context(|| format!("failed to open {}", main_db.display()))?;
    conn.execute_batch(
        "DROP TABLE IF EXISTS cifp_tpp_matches;
         CREATE TABLE cifp_tpp_matches(
           airport_id TEXT NOT NULL,
           cifp_id TEXT NOT NULL,
           plate_id TEXT NOT NULL,
           plate_label TEXT NOT NULL,
           package_id TEXT NOT NULL,
           public INTEGER NOT NULL,
           priority INTEGER NOT NULL,
           match_kind TEXT NOT NULL,
           is_primary INTEGER NOT NULL,
           UNIQUE(airport_id, cifp_id, plate_id)
         );
         CREATE INDEX idx_cifp_tpp_matches_cifp
           ON cifp_tpp_matches(airport_id, cifp_id, priority, plate_label);
         CREATE INDEX idx_cifp_tpp_matches_plate
           ON cifp_tpp_matches(plate_id);",
    )?;
    let tx = conn.unchecked_transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO cifp_tpp_matches
             (airport_id, cifp_id, plate_id, plate_label, package_id, public, priority, match_kind, is_primary)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )?;
        for row in &relation.published_rows {
            stmt.execute(rusqlite::params![
                row.airport_id,
                row.cifp_id,
                row.plate_id,
                row.plate_label,
                row.package_id,
                if row.public { 1 } else { 0 },
                row.priority,
                row.match_kind,
                if row.is_primary { 1 } else { 0 },
            ])?;
        }
    }
    tx.commit()?;
    Ok(PublishedMatchSummary {
        unique_rows: relation
            .published_rows
            .iter()
            .filter(|row| row.match_kind == "unique")
            .count(),
        multiply_bound_rows: relation
            .published_rows
            .iter()
            .filter(|row| row.match_kind == "multiple")
            .count(),
        copter_only_cids: relation.summary.copter_only_cids_total,
        unresolved_cids: relation.summary.unresolved_cids_total,
    })
}

pub fn build_data_package_with_tpp_matches(
    request: &DataTppMatchRequest,
) -> Result<DataTppMatchResult> {
    fs::create_dir_all(&request.output_dir)
        .with_context(|| format!("failed to create {}", request.output_dir.display()))?;
    let main_db = request.output_dir.join(INTERMEDIATE_SQLITE_BASENAME);
    if main_db.exists() {
        fs::remove_file(&main_db)
            .with_context(|| format!("failed to remove {}", main_db.display()))?;
    }
    fs::copy(&request.input_main_db, &main_db).with_context(|| {
        format!(
            "failed to copy {} to {}",
            request.input_main_db.display(),
            main_db.display()
        )
    })?;

    let published = publish_tpp_cifp_matches(&main_db, &request.tpp_package_zips)?;

    let mut manifest_bytes = None;
    let zip_file = fs::File::open(&request.input_zip)
        .with_context(|| format!("failed to open {}", request.input_zip.display()))?;
    let mut archive = ZipArchive::new(zip_file)
        .with_context(|| format!("failed to read {}", request.input_zip.display()))?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        if entry.name() == INTERMEDIATE_SQLITE_BASENAME || entry.name() == "main.db" {
            continue;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        manifest_bytes = Some(bytes);
        break;
    }
    let manifest_bytes = manifest_bytes.ok_or_else(|| {
        anyhow::anyhow!("missing manifest entry in {}", request.input_zip.display())
    })?;
    let manifest_path = request
        .output_dir
        .join(format!("{}.manifest", request.artifact_stem));
    fs::write(&manifest_path, &manifest_bytes)
        .with_context(|| format!("failed to write {}", manifest_path.display()))?;

    let zip_path = request
        .output_dir
        .join(format!("{}.zip", request.artifact_stem));
    write_deterministic_zip(
        &zip_path,
        &[
            ZipSource::new("databases", &manifest_path),
            ZipSource::new(INTERMEDIATE_SQLITE_BASENAME, &main_db),
        ],
    )?;

    Ok(DataTppMatchResult {
        main_db,
        manifest_path,
        zip_path,
        published,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_gps_runway_as_p_family() {
        assert_eq!(
            heuristic_candidate_groups("IAP-CA-GPS RWY 27"),
            vec![BTreeSet::from([String::from("P27")])]
        );
    }

    #[test]
    fn recognizes_vor_or_tacan_circling() {
        assert_eq!(
            heuristic_candidate_groups("IAP-HI-VOR OR TACAN-B"),
            vec![BTreeSet::from([String::from("VOR-B")])]
        );
    }

    #[test]
    fn strips_copter_prefix_for_copter_matching() {
        assert_eq!(
            heuristic_candidate_groups_for_copter_plate("IAP-OR-COPTER ILS Y OR LOC Y RWY 05"),
            vec![
                BTreeSet::from([String::from("I05-Y")]),
                BTreeSet::from([String::from("L05-Y")]),
            ]
        );
    }
}
