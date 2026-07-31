// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;
use preprocessor_core::runway::{
    parse_airport_magnetic_variation, parse_optional_position, resolve_true_heading,
    RunwayHeadingInput,
};

pub(super) fn nav_db_warning_text() -> Option<String> {
    None
}

pub(super) fn nav_kv_family_warning_text(family_id: &str) -> Option<String> {
    let _ = family_id;
    None
}

const NAV_COORDINATE_DECIMAL_SCALE: f64 = 10_000_000.0;
const NAV_DB_DIAGNOSTICS_FORMAT: &str = "nav-db-diagnostics-v1";

pub(super) fn round_nav_coordinate(value: f64) -> f64 {
    let rounded = (value * NAV_COORDINATE_DECIMAL_SCALE).round() / NAV_COORDINATE_DECIMAL_SCALE;
    if rounded == 0.0 {
        0.0
    } else {
        rounded
    }
}

pub(super) fn nav_lat_lon_json(lat: f64, lon: f64) -> serde_json::Value {
    // Nav-db coordinates use the repo-wide 7-decimal degree rule. Move other
    // product-family lat/lon emitters to the same precision as they are touched.
    serde_json::json!({
        "lat": round_nav_coordinate(lat),
        "lon": round_nav_coordinate(lon),
    })
}

#[derive(Debug, Clone)]
pub(super) struct StaticRasterCatalogEntry {
    pub(super) product_id: String,
    pub(super) label: String,
    pub(super) chart_family: String,
    pub(super) tile_url_root: String,
    pub(super) tile_path_template: String,
    pub(super) tile_size: u32,
    pub(super) min_zoom: u32,
    pub(super) max_source_zoom: u32,
    pub(super) max_display_zoom: f64,
    pub(super) initial_viewport: DefaultView,
    pub(super) levels: Vec<TileLevelRecord>,
}

pub(super) fn collect_static_raster_tile_levels(
    task_values: &BTreeMap<String, ProductTaskValue>,
    _config: &ProductBuildConfig,
) -> anyhow::Result<Vec<StaticRasterCatalogEntry>> {
    let mut entries = Vec::new();
    let world_levels = match task_values.get("build-world-basemap") {
        Some(ProductTaskValue::BuiltStaticTileProduct { tile_levels, .. }) => tile_levels.clone(),
        _ => bail!("missing world basemap build output"),
    };
    entries.push(StaticRasterCatalogEntry {
        product_id: stable_product_id_with_contract("world-basemap")?,
        label: "World Basemap".to_string(),
        chart_family: "world-basemap".to_string(),
        tile_url_root: "tiles".to_string(),
        tile_path_template: "0/{z}/{x}/{y}.png".to_string(),
        tile_size: WORLD_BASEMAP_TILE_SIZE,
        min_zoom: WORLD_BASEMAP_MIN_ZOOM,
        max_source_zoom: WORLD_BASEMAP_MAX_SOURCE_ZOOM,
        max_display_zoom: WORLD_BASEMAP_MAX_DISPLAY_ZOOM,
        initial_viewport: DefaultView {
            lat: 20.0,
            lon: 0.0,
            zoom: 1.5,
        },
        levels: world_levels,
    });
    if include_static_terrain_products() {
        let wide_task_id = format!("build-shaded-relief-{WIDE_ANGLE_REGION_ID}");
        let wide_tile_levels = match task_values.get(&wide_task_id) {
            Some(ProductTaskValue::BuiltStaticTileProduct { tile_levels, .. }) => {
                tile_levels.clone()
            }
            _ => bail!("missing shaded relief wide-angle build output"),
        };
        entries.push(StaticRasterCatalogEntry {
            product_id: stable_product_id_with_contract(&format!(
                "shaded-relief-{WIDE_ANGLE_REGION_ID}"
            ))?,
            label: "Wide Shaded Relief".to_string(),
            chart_family: "shaded-relief".to_string(),
            tile_url_root: String::new(),
            tile_path_template: "0/{z}/{x}/{y}.webp".to_string(),
            tile_size: TERRAIN_TILE_SIZE,
            min_zoom: TERRAIN_MIN_ZOOM,
            max_source_zoom: FULL_COVERAGE_ZOOM,
            max_display_zoom: RASTER_BASEMAP_MAX_DISPLAY_ZOOM,
            initial_viewport: DefaultView {
                lat: 0.0,
                lon: 0.0,
                zoom: 0.0,
            },
            levels: wide_tile_levels,
        });
        for region in Region::ALL.iter() {
            let region_id = region.code().to_ascii_lowercase();
            let task_id = format!("build-shaded-relief-{region_id}");
            let tile_levels = match task_values.get(&task_id) {
                Some(ProductTaskValue::BuiltStaticTileProduct { tile_levels, .. }) => {
                    tile_levels.clone()
                }
                _ => bail!("missing shaded relief build output for {}", region.code()),
            };
            entries.push(StaticRasterCatalogEntry {
                product_id: stable_product_id_with_contract(&format!("shaded-relief-{region_id}"))?,
                label: String::new(),
                chart_family: "shaded-relief".to_string(),
                tile_url_root: String::new(),
                tile_path_template: "0/{z}/{x}/{y}.webp".to_string(),
                tile_size: TERRAIN_TILE_SIZE,
                min_zoom: TERRAIN_MIN_ZOOM,
                max_source_zoom: TERRAIN_ZOOM,
                max_display_zoom: RASTER_BASEMAP_MAX_DISPLAY_ZOOM,
                initial_viewport: DefaultView {
                    lat: 0.0,
                    lon: 0.0,
                    zoom: 0.0,
                },
                levels: tile_levels,
            });
        }
    }
    Ok(entries)
}

pub(super) fn build_bundle_manifest(
    config: &ProductBuildConfig,
    build_manifest: &BuildManifest,
    stable_packages: &[BundlePackageArtifact],
    nav_db_package: &BundlePackageArtifact,
) -> anyhow::Result<BundleManifest> {
    let resource_index_record = build_manifest
        .nodes
        .iter()
        .find(|node| node.name == "resource-index")
        .context("build manifest missing resource-index node")?;
    let resource_index_path = resolve_artifact_path(
        config,
        output_path(resource_index_record, "resource_index")?,
    );
    let index: ResourceIndex = serde_json::from_slice(
        &fs::read(&resource_index_path)
            .with_context(|| format!("failed to read {}", resource_index_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", resource_index_path.display()))?;
    let start_valid = index
        .temporal_summary
        .uniform_good_beyond_date
        .clone()
        .or_else(|| index.temporal_summary.uniform_effective_date.clone())
        .context("resource-index missing start-valid date")?;
    let end_valid = index
        .temporal_summary
        .uniform_expiration_date
        .clone()
        .or_else(|| index.temporal_summary.expiration_dates.first().cloned())
        .context("resource-index missing end-valid date")?;
    let cycle = build_manifest.cycle.clone();

    let mut package_artifacts = index
        .packages
        .iter()
        .map(|package| {
            let contract_id = product_contract_id_for_family(&package.family_id)?;
            let package_path = resolve_bundle_package_source_path(config, build_manifest, package)?;
            let filename = canonical_package_filename_hashed(
                &package.family_id,
                &package.region_id,
                Path::new(&package_path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default(),
                &package.checksum_sha256,
                resource_package_chart_tier(package)?,
            )?;
            publish_flat_artifact(&package_path, &config.packaged_dir.join(&filename))?;
            Ok(BundlePackageArtifact {
                id: package.id.clone(),
                family_id: package.family_id.clone(),
                contract_id: contract_id.to_string(),
                region_id: Some(package.region_id.clone()),
                filename: filename.clone(),
                relative_path: filename,
                cycle: package_version_from_filename(
                    Path::new(&package_path)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or_default(),
                )
                .ok(),
                cycle_version: Some(PACKAGE_CYCLE_VERSION.to_string()),
                checksum_sha256: package.checksum_sha256.clone(),
                size_bytes: fs::metadata(&package_path)
                    .with_context(|| format!("failed to stat {}", package_path.display()))?
                    .len(),
                published_at_utc: None,
                source_generated_at_utc: None,
                source_version: None,
                source_fetched_at_utc: None,
                effective_date: package.effective_date.clone(),
                expiration_date: package.expiration_date.clone(),
                warning_text: None,
                metadata: package_metadata_with_contract_id(package.metadata.clone(), contract_id),
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    package_artifacts.extend(stable_packages.iter().cloned());
    package_artifacts.push(nav_db_package.clone());

    let ancillary = vec![];

    Ok(BundleManifest {
        schema_version: 2,
        bundle_id: format!("cycle_{cycle}_{PACKAGE_CYCLE_VERSION}"),
        bundle_type: "cycle".to_string(),
        cycle: cycle.clone(),
        cycle_version: PACKAGE_CYCLE_VERSION.to_string(),
        generated_at_utc: build_manifest.generated_at_utc.clone(),
        effective_date: start_valid.clone(),
        expiration_date: end_valid.clone(),
        start_valid: start_valid.clone(),
        end_valid: end_valid.clone(),
        packages: package_artifacts,
        ancillary,
    })
}

pub(super) fn nav_db_magvar_decimal_year(resource_index: &ResourceIndex) -> anyhow::Result<f64> {
    let date = resource_index
        .temporal_summary
        .uniform_good_beyond_date
        .as_ref()
        .or(resource_index
            .temporal_summary
            .uniform_effective_date
            .as_ref())
        .context("resource-index missing date for magnetic variation generation")?;
    let date = NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .with_context(|| format!("failed to parse magnetic variation date {date}"))?;
    Ok(decimal_year(date))
}

pub(super) fn decimal_year(date: NaiveDate) -> f64 {
    let year = date.year();
    let year_start = NaiveDate::from_ymd_opt(year, 1, 1).expect("valid year start");
    let next_year_start = NaiveDate::from_ymd_opt(year + 1, 1, 1).expect("valid next year start");
    let day = date.signed_duration_since(year_start).num_days() as f64;
    let year_days = next_year_start.signed_duration_since(year_start).num_days() as f64;
    f64::from(year) + day / year_days
}

#[derive(Debug, Clone)]
pub(super) struct WmmCoefficient {
    g: f64,
    h: f64,
    g_dot: f64,
    h_dot: f64,
}

#[derive(Debug, Clone)]
pub(super) struct WmmModel {
    epoch: f64,
    model_name: String,
    release_date: String,
    n_max: usize,
    coefficients: Vec<WmmCoefficient>,
}

impl WmmModel {
    fn from_cof(path: &Path) -> anyhow::Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let mut lines = text.lines();
        let header = lines
            .next()
            .ok_or_else(|| anyhow::anyhow!("{} is empty", path.display()))?;
        let header_columns = header.split_whitespace().collect::<Vec<_>>();
        if header_columns.len() < 3 {
            bail!("invalid WMM.COF header in {}", path.display());
        }
        let epoch = header_columns[0]
            .parse::<f64>()
            .with_context(|| format!("failed to parse WMM epoch in {}", path.display()))?;
        let model_name = header_columns[1].to_string();
        let release_date = header_columns[2].to_string();
        let mut raw = Vec::new();
        let mut n_max = 0usize;
        for line in lines {
            let columns = line.split_whitespace().collect::<Vec<_>>();
            if columns.first() == Some(&"999999999999999999999999999999999999999999999999") {
                break;
            }
            if columns.len() != 6 {
                bail!("invalid WMM coefficient line in {}: {line}", path.display());
            }
            let n = columns[0].parse::<usize>()?;
            let m = columns[1].parse::<usize>()?;
            n_max = n_max.max(n);
            raw.push((
                n,
                m,
                WmmCoefficient {
                    g: columns[2].parse::<f64>()?,
                    h: columns[3].parse::<f64>()?,
                    g_dot: columns[4].parse::<f64>()?,
                    h_dot: columns[5].parse::<f64>()?,
                },
            ));
        }
        let mut coefficients = vec![
            WmmCoefficient {
                g: 0.0,
                h: 0.0,
                g_dot: 0.0,
                h_dot: 0.0,
            };
            wmm_index(n_max, n_max) + 1
        ];
        for (n, m, coefficient) in raw {
            coefficients[wmm_index(n, m)] = coefficient;
        }
        Ok(Self {
            epoch,
            model_name,
            release_date,
            n_max,
            coefficients,
        })
    }

    fn declination_degrees(&self, latitude: f64, longitude: f64, decimal_year: f64) -> f64 {
        self.declination_degrees_at_ellipsoid_km(latitude, longitude, 0.0, decimal_year)
    }

    fn declination_degrees_at_ellipsoid_km(
        &self,
        latitude: f64,
        longitude: f64,
        height_above_ellipsoid_km: f64,
        decimal_year: f64,
    ) -> f64 {
        let spherical = wmm_geodetic_to_spherical(latitude, longitude, height_above_ellipsoid_km);
        let spherical_harmonic = wmm_spherical_harmonic_variables(spherical, self.n_max);
        let legendre = wmm_associated_legendre_low(spherical.phig, self.n_max);
        let mut sph_bx = 0.0;
        let mut sph_by = 0.0;
        let mut sph_bz = 0.0;
        let years_since_epoch = decimal_year - self.epoch;
        for n in 1..=self.n_max {
            for m in 0..=n {
                let index = wmm_index(n, m);
                let coefficient = &self.coefficients[index];
                let g = coefficient.g + years_since_epoch * coefficient.g_dot;
                let h = coefficient.h + years_since_epoch * coefficient.h_dot;
                let relative_radius_power = spherical_harmonic.relative_radius_power[n];
                let cos_mlambda = spherical_harmonic.cos_mlambda[m];
                let sin_mlambda = spherical_harmonic.sin_mlambda[m];
                let gauss = g * cos_mlambda + h * sin_mlambda;
                sph_bz -= relative_radius_power * gauss * (n as f64 + 1.0) * legendre.pcup[index];
                sph_by += relative_radius_power
                    * (g * sin_mlambda - h * cos_mlambda)
                    * m as f64
                    * legendre.pcup[index];
                sph_bx -= relative_radius_power * gauss * legendre.dpcup[index];
            }
        }
        let cos_phi = spherical.phig.to_radians().cos();
        if cos_phi.abs() > 1.0e-10 {
            sph_by /= cos_phi;
        }
        let psi = (spherical.phig - latitude).to_radians();
        let geo_x = sph_bx * psi.cos() - sph_bz * psi.sin();
        let geo_y = sph_by;
        geo_y.atan2(geo_x).to_degrees()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct WmmFetchedSourceMetadata {
    pub(super) source_url: String,
    pub(super) source_zip_sha256: String,
    pub(super) source_fetched_at_utc: Option<String>,
    pub(super) model: String,
    pub(super) model_epoch: f64,
    pub(super) model_effective_date: String,
    pub(super) coefficient_release_date: String,
    pub(super) valid_decimal_year_start: f64,
    pub(super) valid_decimal_year_end: f64,
}

#[derive(Debug, Clone)]
pub(super) struct BuiltWmmSource {
    pub(super) cof_path: PathBuf,
    pub(super) metadata_path: PathBuf,
    pub(super) node_record: NodeRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Egm2008GeoidSourceMetadata {
    source: String,
    source_url: String,
    source_zip_sha256: String,
    source_fetched_at_utc: Option<String>,
    model: String,
    model_effective_date: String,
    grid_release_date: String,
    official_approval_date: String,
    grid_spacing: String,
    vertical_reference: String,
    generated_grid: String,
    generated_grid_units: String,
    citation: String,
}

#[derive(Debug, Clone)]
pub(super) struct BuiltGeoidSource {
    pub(super) csv_path: PathBuf,
    pub(super) metadata_path: PathBuf,
    pub(super) source_fetched_at_utc: Option<String>,
    pub(super) node_record: NodeRecord,
}

pub(super) fn build_wmm_source_node(config: &ProductBuildConfig) -> anyhow::Result<BuiltWmmSource> {
    let fetch_cache = static_source_fetch_cache_config(config)?;
    let inputs = BTreeMap::from([
        ("source_url".to_string(), WMM_COEFFICIENTS_URL.to_string()),
        (
            "fetch_cache_mode".to_string(),
            format!("{:?}", fetch_cache.mode),
        ),
        (
            "wmm_source_pipeline".to_string(),
            "wmm-source-v1".to_string(),
        ),
    ]);
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, "wmm-source")?,
        "wmm-source",
        &inputs,
    )?;
    let output_dir = prepared.dir.join("output");
    let input_dir = output_dir.join("input");
    let zip_path = input_dir.join("WMM2025COF.zip");
    let cof_path = input_dir.join("WMM2025COF").join("WMM.COF");
    let test_values_path = input_dir.join("WMM2025COF").join("WMM2025_TestValues.txt");
    let metadata_path = output_dir.join("wmm-source.json");
    let expected = vec![cof_path.clone(), metadata_path.clone()];
    let record = run_cached_node(prepared, inputs, &expected, |_prepared| {
        if output_dir.exists() {
            fs::remove_dir_all(&output_dir)
                .with_context(|| format!("failed to remove {}", output_dir.display()))?;
        }
        fs::create_dir_all(&input_dir)
            .with_context(|| format!("failed to create {}", input_dir.display()))?;
        let provenance_dir = output_dir.join("provenance");
        prefetch_requests_with_provenance(
            &[PrefetchRequest::new(WMM_COEFFICIENTS_URL).with_logical_file_name("WMM2025COF.zip")],
            &input_dir,
            1,
            Some(&fetch_cache),
            &provenance_dir,
            "wmm-source",
        )?;
        let model = WmmModel::from_cof(&cof_path)?;
        validate_wmm_model_against_test_values(&model, &test_values_path)?;
        let metadata = WmmFetchedSourceMetadata {
            source_url: WMM_COEFFICIENTS_URL.to_string(),
            source_zip_sha256: hash_file(&zip_path)?,
            source_fetched_at_utc: source_fetched_at_utc_for_urls(
                &fetch_cache,
                &[WMM_COEFFICIENTS_URL],
            )?,
            model: model.model_name.clone(),
            model_epoch: model.epoch,
            model_effective_date: decimal_year_effective_date(model.epoch)?,
            coefficient_release_date: model.release_date.clone(),
            valid_decimal_year_start: model.epoch,
            valid_decimal_year_end: model.epoch + 5.0,
        };
        fs::write(&metadata_path, serde_json::to_vec_pretty(&metadata)?)
            .with_context(|| format!("failed to write {}", metadata_path.display()))?;
        Ok(BTreeMap::from([
            (
                "wmm_cof".to_string(),
                relative_artifact_path(&cof_path, &config.build_root),
            ),
            (
                "metadata".to_string(),
                relative_artifact_path(&metadata_path, &config.build_root),
            ),
            (
                "provenance_dir".to_string(),
                relative_artifact_path(&provenance_dir, &config.build_root),
            ),
        ]))
    })?;
    Ok(BuiltWmmSource {
        cof_path,
        metadata_path,
        node_record: record,
    })
}

pub(super) fn build_egm2008_geoid_source_node(
    config: &ProductBuildConfig,
) -> anyhow::Result<BuiltGeoidSource> {
    let fetch_cache = static_source_fetch_cache_config(config)?;
    let inputs = BTreeMap::from([
        (
            "source_url".to_string(),
            EGM2008_INTERPOLATION_GRID_URL.to_string(),
        ),
        (
            "fetch_cache_mode".to_string(),
            format!("{:?}", fetch_cache.mode),
        ),
        (
            "geoid_source_pipeline".to_string(),
            "egm2008-geoid-source-v1".to_string(),
        ),
    ]);
    let prepared = prepare_node_at(
        &build_shared_node_dir(config, "egm2008-geoid-source")?,
        "egm2008-geoid-source",
        &inputs,
    )?;
    let output_dir = prepared.dir.join("output");
    let input_dir = output_dir.join("input");
    let zip_path = input_dir.join("EGM2008_Interpolation_Grid.zip");
    let source_grid_path = input_dir.join(EGM2008_GRID_MEMBER);
    let csv_path = output_dir.join("egm2008_geoid_1deg_feet.csv");
    let metadata_path = output_dir.join("egm2008-geoid-source.json");
    let expected = vec![csv_path.clone(), metadata_path.clone()];
    let record = run_cached_node(prepared, inputs, &expected, |_prepared| {
        if output_dir.exists() {
            fs::remove_dir_all(&output_dir)
                .with_context(|| format!("failed to remove {}", output_dir.display()))?;
        }
        fs::create_dir_all(&input_dir)
            .with_context(|| format!("failed to create {}", input_dir.display()))?;
        let provenance_dir = output_dir.join("provenance");
        prefetch_requests_with_provenance(
            &[PrefetchRequest::new(EGM2008_INTERPOLATION_GRID_URL)
                .with_logical_file_name("EGM2008_Interpolation_Grid.zip")],
            &input_dir,
            1,
            Some(&fetch_cache),
            &provenance_dir,
            "egm2008-geoid-source",
        )?;
        build_egm2008_one_degree_geoid_csv(&source_grid_path, &csv_path)?;
        validate_egm2008_one_degree_geoid_csv(&csv_path)?;
        let source_fetched_at_utc =
            source_fetched_at_utc_for_urls(&fetch_cache, &[EGM2008_INTERPOLATION_GRID_URL])?;
        let metadata = Egm2008GeoidSourceMetadata {
            source: "NGA Earth Gravitational Model 2008".to_string(),
            source_url: EGM2008_INTERPOLATION_GRID_URL.to_string(),
            source_zip_sha256: hash_file(&zip_path)?,
            source_fetched_at_utc,
            model: "EGM2008".to_string(),
            model_effective_date: "2008-07-08".to_string(),
            grid_release_date: "2009-05-01".to_string(),
            official_approval_date: "2014-07-08".to_string(),
            grid_spacing: "2.5 arc-minutes".to_string(),
            vertical_reference: "WGS84 ellipsoid to EGM2008 geoid height".to_string(),
            generated_grid: "one-degree integer latitude/longitude grid sampled from NGA 2.5-minute grid".to_string(),
            generated_grid_units: "feet".to_string(),
            citation: "NGA EGM2008; Pavlis, N. K., Holmes, S. A., Kenyon, S. C., and Factor, J. K. 2012. The development and evaluation of the Earth Gravitational Model 2008 (EGM2008), Journal of Geophysical Research: Solid Earth, 117(B4). https://doi.org/10.1029/2011JB008916".to_string(),
        };
        fs::write(&metadata_path, serde_json::to_vec_pretty(&metadata)?)
            .with_context(|| format!("failed to write {}", metadata_path.display()))?;
        Ok(BTreeMap::from([
            (
                "geoid_csv".to_string(),
                relative_artifact_path(&csv_path, &config.build_root),
            ),
            (
                "metadata".to_string(),
                relative_artifact_path(&metadata_path, &config.build_root),
            ),
            (
                "provenance_dir".to_string(),
                relative_artifact_path(&provenance_dir, &config.build_root),
            ),
        ]))
    })?;
    let metadata: Egm2008GeoidSourceMetadata = serde_json::from_slice(
        &fs::read(&metadata_path)
            .with_context(|| format!("failed to read {}", metadata_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", metadata_path.display()))?;
    Ok(BuiltGeoidSource {
        csv_path,
        metadata_path,
        source_fetched_at_utc: metadata.source_fetched_at_utc,
        node_record: record,
    })
}

pub(super) fn build_egm2008_one_degree_geoid_csv(
    source_grid_path: &Path,
    csv_path: &Path,
) -> anyhow::Result<()> {
    const NROWS: usize = 4321;
    const NCOLS: usize = 8640;
    const ROW_BYTES: usize = NCOLS * 4;
    const METERS_TO_FEET: f64 = 3.280_839_895;

    let mut source = File::open(source_grid_path)
        .with_context(|| format!("failed to open {}", source_grid_path.display()))?;
    let mut rows_by_lat = BTreeMap::<i32, Vec<f32>>::new();
    for row_index in 0..NROWS {
        let mut marker = [0u8; 4];
        source
            .read_exact(&mut marker)
            .with_context(|| format!("failed to read row marker {row_index}"))?;
        let record_bytes = u32::from_le_bytes(marker) as usize;
        if record_bytes != ROW_BYTES {
            bail!(
                "unexpected EGM2008 row byte count at row {row_index}: expected {ROW_BYTES}, got {record_bytes}"
            );
        }
        let mut row_bytes = vec![0u8; ROW_BYTES];
        source
            .read_exact(&mut row_bytes)
            .with_context(|| format!("failed to read EGM2008 row {row_index}"))?;
        source
            .read_exact(&mut marker)
            .with_context(|| format!("failed to read trailing row marker {row_index}"))?;
        let trailing_record_bytes = u32::from_le_bytes(marker) as usize;
        if trailing_record_bytes != ROW_BYTES {
            bail!(
                "unexpected EGM2008 trailing row byte count at row {row_index}: expected {ROW_BYTES}, got {trailing_record_bytes}"
            );
        }
        if row_index % 24 != 0 {
            continue;
        }
        let latitude = 90 - (row_index / 24) as i32;
        if !(-90..90).contains(&latitude) {
            continue;
        }
        let row = row_bytes
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
            .collect::<Vec<_>>();
        rows_by_lat.insert(latitude, row);
    }
    let mut output = String::with_capacity(180 * 360 * 16);
    for latitude in -90..90 {
        let row = rows_by_lat
            .get(&latitude)
            .with_context(|| format!("missing EGM2008 latitude row {latitude}"))?;
        for longitude in -180..180 {
            let normalized_longitude = if longitude < 0 {
                longitude + 360
            } else {
                longitude
            };
            let column = (normalized_longitude as usize) * 24;
            let geoid_height_feet = (f64::from(row[column]) * METERS_TO_FEET).round() as i32;
            output.push_str(&format!("{latitude},{longitude},{geoid_height_feet},0\n"));
        }
    }
    fs::write(csv_path, output).with_context(|| format!("failed to write {}", csv_path.display()))
}

pub(super) fn validate_egm2008_one_degree_geoid_csv(csv_path: &Path) -> anyhow::Result<()> {
    let text = fs::read_to_string(csv_path)
        .with_context(|| format!("failed to read {}", csv_path.display()))?;
    let values = text
        .lines()
        .map(|line| {
            let columns = line.split(',').collect::<Vec<_>>();
            if columns.len() != 4 {
                bail!("invalid geoid CSV row: {line}");
            }
            Ok((
                columns[0].parse::<i32>()?,
                columns[1].parse::<i32>()?,
                columns[2].parse::<i32>()?,
            ))
        })
        .collect::<anyhow::Result<BTreeSet<_>>>()?;
    if values.len() != 180 * 360 {
        bail!(
            "expected {} one-degree EGM2008 geoid rows, got {}",
            180 * 360,
            values.len()
        );
    }
    if !values.contains(&(-90, -180, -99)) {
        bail!("EGM2008 geoid sanity check failed at -90,-180");
    }
    if !values.contains(&(37, -119, -86)) {
        bail!("EGM2008 geoid sanity check failed at 37,-119");
    }
    Ok(())
}

pub(super) fn validate_wmm_model_against_test_values(
    model: &WmmModel,
    path: &Path,
) -> anyhow::Result<()> {
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut checked = 0usize;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let columns = trimmed.split_whitespace().collect::<Vec<_>>();
        if columns.len() < 5 {
            continue;
        }
        let Ok(decimal_year) = columns[0].parse::<f64>() else {
            continue;
        };
        let hae_km = columns[1].parse::<f64>()?;
        let lat = columns[2].parse::<f64>()?;
        let lon = columns[3].parse::<f64>()?;
        let expected_declination = columns[4].parse::<f64>()?;
        let actual = model.declination_degrees_at_ellipsoid_km(lat, lon, hae_km, decimal_year);
        if (actual - expected_declination).abs() > 0.01 {
            bail!(
                "WMM test value mismatch at {lat},{lon} {decimal_year}: expected {expected_declination}, got {actual}"
            );
        }
        checked += 1;
    }
    if checked == 0 {
        bail!("{} contained no WMM test rows", path.display());
    }
    Ok(())
}

pub(super) fn decimal_year_effective_date(decimal_year: f64) -> anyhow::Result<String> {
    if !decimal_year.is_finite() {
        bail!("invalid WMM decimal year {decimal_year}");
    }
    let year = decimal_year.floor() as i32;
    let start = NaiveDate::from_ymd_opt(year, 1, 1)
        .with_context(|| format!("invalid WMM epoch year {year}"))?;
    let next = NaiveDate::from_ymd_opt(year + 1, 1, 1)
        .with_context(|| format!("invalid WMM epoch year {}", year + 1))?;
    let days_in_year = next.signed_duration_since(start).num_days() as f64;
    let day_offset = ((decimal_year - f64::from(year)) * days_in_year).round() as i64;
    Ok((start + chrono::Duration::days(day_offset))
        .format("%Y-%m-%d")
        .to_string())
}

#[derive(Debug, Clone, Copy)]
pub(super) struct WmmSphericalCoord {
    lambda: f64,
    phig: f64,
    r: f64,
}

#[derive(Debug, Clone)]
pub(super) struct WmmSphericalHarmonicVariables {
    relative_radius_power: Vec<f64>,
    cos_mlambda: Vec<f64>,
    sin_mlambda: Vec<f64>,
}

#[derive(Debug, Clone)]
pub(super) struct WmmLegendre {
    pcup: Vec<f64>,
    dpcup: Vec<f64>,
}

pub(super) fn wmm_index(n: usize, m: usize) -> usize {
    n * (n + 1) / 2 + m
}

pub(super) fn wmm_geodetic_to_spherical(
    latitude: f64,
    longitude: f64,
    height_above_ellipsoid_km: f64,
) -> WmmSphericalCoord {
    const WGS84_A_KM: f64 = 6378.137;
    const WGS84_EPS_SQ: f64 = 0.006_694_379_990_141_316_5;
    let cos_lat = latitude.to_radians().cos();
    let sin_lat = latitude.to_radians().sin();
    let rc = WGS84_A_KM / (1.0 - WGS84_EPS_SQ * sin_lat * sin_lat).sqrt();
    let xp = (rc + height_above_ellipsoid_km) * cos_lat;
    let zp = (rc * (1.0 - WGS84_EPS_SQ) + height_above_ellipsoid_km) * sin_lat;
    let r = (xp * xp + zp * zp).sqrt();
    WmmSphericalCoord {
        lambda: longitude,
        phig: (zp / r).asin().to_degrees(),
        r,
    }
}

pub(super) fn wmm_spherical_harmonic_variables(
    spherical: WmmSphericalCoord,
    n_max: usize,
) -> WmmSphericalHarmonicVariables {
    const WMM_REFERENCE_RADIUS_KM: f64 = 6371.2;
    let mut relative_radius_power = vec![0.0; n_max + 1];
    let mut cos_mlambda = vec![0.0; n_max + 1];
    let mut sin_mlambda = vec![0.0; n_max + 1];
    let radius_ratio = WMM_REFERENCE_RADIUS_KM / spherical.r;
    relative_radius_power[0] = radius_ratio * radius_ratio;
    for n in 1..=n_max {
        relative_radius_power[n] = relative_radius_power[n - 1] * radius_ratio;
    }
    let cos_lambda = spherical.lambda.to_radians().cos();
    let sin_lambda = spherical.lambda.to_radians().sin();
    cos_mlambda[0] = 1.0;
    sin_mlambda[0] = 0.0;
    if n_max >= 1 {
        cos_mlambda[1] = cos_lambda;
        sin_mlambda[1] = sin_lambda;
    }
    for m in 2..=n_max {
        cos_mlambda[m] = cos_mlambda[m - 1] * cos_lambda - sin_mlambda[m - 1] * sin_lambda;
        sin_mlambda[m] = cos_mlambda[m - 1] * sin_lambda + sin_mlambda[m - 1] * cos_lambda;
    }
    WmmSphericalHarmonicVariables {
        relative_radius_power,
        cos_mlambda,
        sin_mlambda,
    }
}

pub(super) fn wmm_associated_legendre_low(phig: f64, n_max: usize) -> WmmLegendre {
    let x = phig.to_radians().sin();
    let z = ((1.0 - x) * (1.0 + x)).sqrt();
    let terms = (n_max + 1) * (n_max + 2) / 2;
    let mut pcup = vec![0.0; terms];
    let mut dpcup = vec![0.0; terms];
    let mut schmidt = vec![0.0; terms + 1];
    pcup[0] = 1.0;
    dpcup[0] = 0.0;
    for n in 1..=n_max {
        for m in 0..=n {
            let index = wmm_index(n, m);
            if n == m {
                let index1 = wmm_index(n - 1, m - 1);
                pcup[index] = z * pcup[index1];
                dpcup[index] = z * dpcup[index1] + x * pcup[index1];
            } else if n == 1 && m == 0 {
                let index1 = wmm_index(n - 1, m);
                pcup[index] = x * pcup[index1];
                dpcup[index] = x * dpcup[index1] - z * pcup[index1];
            } else {
                let index1 = wmm_index(n - 2, m);
                let index2 = wmm_index(n - 1, m);
                if m > n - 2 {
                    pcup[index] = x * pcup[index2];
                    dpcup[index] = x * dpcup[index2] - z * pcup[index2];
                } else {
                    let k =
                        (((n - 1) * (n - 1) - m * m) as f64) / (((2 * n - 1) * (2 * n - 3)) as f64);
                    pcup[index] = x * pcup[index2] - k * pcup[index1];
                    dpcup[index] = x * dpcup[index2] - z * pcup[index2] - k * dpcup[index1];
                }
            }
        }
    }
    schmidt[0] = 1.0;
    for n in 1..=n_max {
        let index = wmm_index(n, 0);
        let index1 = wmm_index(n - 1, 0);
        schmidt[index] = schmidt[index1] * (2 * n - 1) as f64 / n as f64;
        for m in 1..=n {
            let index = wmm_index(n, m);
            let index1 = wmm_index(n, m - 1);
            let numerator = ((n - m + 1) * if m == 1 { 2 } else { 1 }) as f64;
            schmidt[index] = schmidt[index1] * (numerator / (n + m) as f64).sqrt();
        }
    }
    for n in 1..=n_max {
        for m in 0..=n {
            let index = wmm_index(n, m);
            pcup[index] *= schmidt[index];
            dpcup[index] *= -schmidt[index];
        }
    }
    WmmLegendre { pcup, dpcup }
}

pub(super) fn build_nav_kv_magvar_pairs(
    path: &Path,
    source_metadata_path: &Path,
    decimal_year: f64,
) -> anyhow::Result<Vec<NavKvPair>> {
    let model = WmmModel::from_cof(path)?;
    let source_metadata: WmmFetchedSourceMetadata = serde_json::from_slice(
        &fs::read(source_metadata_path)
            .with_context(|| format!("failed to read {}", source_metadata_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", source_metadata_path.display()))?;
    if source_metadata.model != model.model_name {
        bail!(
            "WMM source metadata names model {} but COF contains {}",
            source_metadata.model,
            model.model_name
        );
    }
    if source_metadata.model_epoch != model.epoch {
        bail!(
            "WMM source metadata epoch {} but COF contains {}",
            source_metadata.model_epoch,
            model.epoch
        );
    }
    if source_metadata.coefficient_release_date != model.release_date {
        bail!(
            "WMM source metadata release date {} but COF contains {}",
            source_metadata.coefficient_release_date,
            model.release_date
        );
    }
    if decimal_year < source_metadata.valid_decimal_year_start
        || decimal_year >= source_metadata.valid_decimal_year_end
    {
        bail!(
            "WMM model {} is valid for [{valid_decimal_year_start}, {valid_decimal_year_end}) but nav-db needs {decimal_year}",
            model.model_name,
            valid_decimal_year_start = source_metadata.valid_decimal_year_start,
            valid_decimal_year_end = source_metadata.valid_decimal_year_end
        );
    }
    let mut pairs = Vec::with_capacity(64_801);
    pairs.push(json_pair(
        "magvar/source".to_string(),
        &serde_json::json!({
            "source": "NOAA/NCEI World Magnetic Model",
            "source_url": source_metadata.source_url,
            "source_zip_sha256": source_metadata.source_zip_sha256,
            "source_fetched_at_utc": source_metadata.source_fetched_at_utc,
            "model": source_metadata.model,
            "model_epoch": source_metadata.model_epoch,
            "model_effective_date": source_metadata.model_effective_date,
            "coefficient_release_date": source_metadata.coefficient_release_date,
            "valid_decimal_year_start": source_metadata.valid_decimal_year_start,
            "valid_decimal_year_end": source_metadata.valid_decimal_year_end,
            "computed_decimal_year": decimal_year,
            "grid": {
                "latitude_min": -90,
                "latitude_max_exclusive": 90,
                "longitude_min": -180,
                "longitude_max_exclusive": 180,
                "step_degrees": 1,
                "altitude_reference": "WGS84 ellipsoid",
                "altitude_km": 0.0,
                "value_units": "degrees east"
            },
            "citation": "NOAA NCEI Geomagnetic Modeling Team; British Geological Survey. 2024: World Magnetic Model 2025. NOAA National Centers for Environmental Information. https://doi.org/10.25921/aqfd-sd83."
        }),
        "magvar source",
    )?);
    for lat in -90..90 {
        for lon in -180..180 {
            let declination = model.declination_degrees(lat as f64, lon as f64, decimal_year);
            pairs.push(json_pair(
                format!("magvar/{lat}/{lon}"),
                &serde_json::json!((declination * 10.0).round() / 10.0),
                "magnetic variation",
            )?);
        }
    }
    Ok(pairs)
}

pub(super) fn build_nav_kv_artifact(
    config: &ProductBuildConfig,
    resource_index_path: &Path,
    intermediate_sqlite_db_path: &Path,
    cycle: &str,
    vector_had_pairs_path: &Path,
    wmm_cof_path: &Path,
    wmm_metadata_path: &Path,
    stable_packages: &[BundlePackageArtifact],
    static_raster_tile_levels: &[StaticRasterCatalogEntry],
) -> anyhow::Result<BuiltNavDbArtifacts> {
    let resource_index: ResourceIndex = serde_json::from_slice(
        &fs::read(resource_index_path)
            .with_context(|| format!("failed to read {}", resource_index_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", resource_index_path.display()))?;
    let mut package_artifacts = bundle_package_artifacts_from_resource_index(&resource_index)?;
    package_artifacts.extend(stable_packages.iter().cloned());
    let package_index_json = serde_json::to_string(&package_artifacts)
        .context("failed to encode nav-db package inputs")?;
    let static_raster_json = static_raster_tile_levels
        .iter()
        .map(|entry| {
            serde_json::to_string(&(entry.product_id.as_str(), &entry.levels)).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let magvar_decimal_year = nav_db_magvar_decimal_year(&resource_index)?;
    let inputs = BTreeMap::from([
        (
            "resource_index".to_string(),
            hash_file(resource_index_path)?,
        ),
        (
            "intermediate_sqlite_db".to_string(),
            hash_file(intermediate_sqlite_db_path)?,
        ),
        (
            "vector_had_pairs".to_string(),
            hash_file(vector_had_pairs_path)?,
        ),
        ("cycle".to_string(), cycle.to_string()),
        (
            "package_artifacts".to_string(),
            hash_text(&package_index_json),
        ),
        (
            "static_raster_tile_levels".to_string(),
            hash_text(&static_raster_json),
        ),
        ("magvar_model".to_string(), hash_file(&wmm_cof_path)?),
        (
            "magvar_source_metadata".to_string(),
            hash_file(wmm_metadata_path)?,
        ),
        (
            "magvar_decimal_year".to_string(),
            format!("{magvar_decimal_year:.6}"),
        ),
        ("nav_kv_page_bytes".to_string(), (64 * 1024).to_string()),
        (
            "nav_kv_storage_format".to_string(),
            NAV_KV_STORAGE_FORMAT.to_string(),
        ),
        (
            "nav_db_contract_id".to_string(),
            NAV_DB_CONTRACT_ID.to_string(),
        ),
        (
            "nav_db_diagnostics_format".to_string(),
            NAV_DB_DIAGNOSTICS_FORMAT.to_string(),
        ),
        (
            "nav_kv_builder".to_string(),
            source_fingerprints::nav_kv_builder_fingerprint()?,
        ),
    ]);
    let prepared = prepare_node_at(&build_shared_node_dir(config, "nav-db")?, "nav-db", &inputs)?;
    let output_dir = prepared.dir.join("output");
    let source_dir = output_dir.join("nav_db");
    let root_filename = "root";
    let nav_db_zip_source_path = output_dir.join(format!("nav_db_{cycle}.zip"));
    let diagnostics_path = output_dir.join("diagnostics.json");
    let record =
        match claim_or_wait_for_node(&prepared, std::slice::from_ref(&nav_db_zip_source_path))? {
            NodeCacheState::CacheHit(record) => record,
            NodeCacheState::Build(_lock) => {
                if output_dir.exists() {
                    fs::remove_dir_all(&output_dir)
                        .with_context(|| format!("failed to remove {}", output_dir.display()))?;
                }
                fs::create_dir_all(&source_dir)
                    .with_context(|| format!("failed to create {}", source_dir.display()))?;
                let started_at_utc = utc_now_string();
                let started = Instant::now();
                let chart_cutline_polygon_sets =
                    build_chart_cutline_polygon_sets(&config.chart_metadata_root, &resource_index)?;
                let chart_catalog =
                    build_nav_kv_chart_catalog(&resource_index, static_raster_tile_levels);
                let chart_catalog_bytes = serde_json::to_vec(&chart_catalog)
                    .context("failed to encode nav_kv chart/catalog value")?;
                let mut pairs = vec![
                    NavKvPair {
                        key: "contract/nav-db".to_string(),
                        value: serde_json::to_vec(&serde_json::json!({
                            "contract_id": NAV_DB_CONTRACT_ID,
                        }))
                        .context("failed to encode nav_kv contract/nav-db value")?,
                    },
                    NavKvPair {
                        key: "chart/catalog".to_string(),
                        value: chart_catalog_bytes,
                    },
                ];
                pairs.extend(build_nav_kv_offline_region_pairs(
                    &resource_index,
                    &chart_cutline_polygon_sets,
                )?);
                pairs.extend(build_nav_kv_resource_summary_pairs(&resource_index)?);
                pairs.extend(build_nav_kv_plate_pairs(&resource_index)?);
                pairs.extend(build_nav_kv_chart_reference_pairs(&resource_index)?);
                pairs.extend(build_nav_kv_package_pairs(&package_artifacts)?);
                pairs.extend(build_nav_kv_navref_pairs(intermediate_sqlite_db_path)?);
                pairs.extend(build_nav_kv_vector_pairs(vector_had_pairs_path)?);
                pairs.extend(build_nav_kv_magvar_pairs(
                    &wmm_cof_path,
                    wmm_metadata_path,
                    magvar_decimal_year,
                )?);
                let diagnostics = nav_db_build_diagnostics_from_pairs(&pairs)?;
                fs::write(&diagnostics_path, serde_json::to_vec_pretty(&diagnostics)?)
                    .with_context(|| format!("failed to write {}", diagnostics_path.display()))?;
                let built = build_nav_kv_sorted(pairs, 64 * 1024)
                    .map_err(|err| anyhow::anyhow!("failed to build nav_kv: {err}"))?;
                let root_source_path = source_dir.join(root_filename);
                fs::write(&root_source_path, &built.root_bytes)
                    .with_context(|| format!("failed to write {}", root_source_path.display()))?;

                let mut page_filenames = Vec::new();
                for (index, page) in built.pages.iter().enumerate() {
                    let page_filename = format!("page_{index:04}");
                    let page_source_path = source_dir.join(&page_filename);
                    fs::write(&page_source_path, page).with_context(|| {
                        format!("failed to write {}", page_source_path.display())
                    })?;
                    page_filenames.push(page_filename);
                }
                let manifest_bytes = serde_json::to_vec_pretty(&serde_json::json!({
                    "schema_version": 1,
                    "product_id": "nav-db",
                    "contract_id": NAV_DB_CONTRACT_ID,
                    "encoding": format!("had-nav-kv-v{}", NAV_KV_STORAGE_FORMAT),
                    "root": root_filename,
                    "page_path_template": "page_{page:04}",
                    "page_count": built.pages.len(),
                    "page_size": built.page_size,
                    "logical_bytes_len": built.logical_bytes_len,
                    "value_bytes_len": built.value_bytes_len,
                }))
                .context("failed to encode nav-db package manifest")?;
                fs::write(source_dir.join("manifest.json"), &manifest_bytes)
                    .context("failed to write nav-db package manifest")?;
                let zip_bytes = nav_kv_package::write_stored_xz_package_bytes_with_encoder(
                    &manifest_bytes,
                    &built.root_bytes,
                    &built.pages,
                    producer_xz_compress_bytes,
                )
                .map_err(|err| anyhow::anyhow!("failed to write nav-db package bytes: {err}"))?;
                fs::write(&nav_db_zip_source_path, zip_bytes).with_context(|| {
                    format!("failed to write {}", nav_db_zip_source_path.display())
                })?;
                let outputs = BTreeMap::from([
                    (
                        "nav_db_zip".to_string(),
                        relative_artifact_path(&nav_db_zip_source_path, &config.build_root),
                    ),
                    (
                        "diagnostics".to_string(),
                        relative_artifact_path(&diagnostics_path, &config.build_root),
                    ),
                ]);
                write_node_record(
                    prepared,
                    inputs,
                    outputs,
                    false,
                    started_at_utc,
                    utc_now_string(),
                    started.elapsed().as_millis() as u64,
                )?
            }
        };
    let diagnostics: NavDbBuildDiagnostics = serde_json::from_slice(
        &fs::read(&diagnostics_path)
            .with_context(|| format!("failed to read {}", diagnostics_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", diagnostics_path.display()))?;
    let nav_db_sha256 = output_sha_or_hash(&record, "nav_db_zip", &nav_db_zip_source_path)?;
    let nav_db_published_filename =
        format!("nav_db_{NAV_DB_CONTRACT_ID}_{cycle}_{PACKAGE_CYCLE_VERSION}_{nav_db_sha256}.zip");
    let nav_db_package_artifact =
        publish_bundle_artifact(config, &nav_db_zip_source_path, &nav_db_published_filename)?;
    let startup_prefetch_members = nav_db_startup_prefetch_members(&nav_db_zip_source_path)?;
    Ok(BuiltNavDbArtifacts {
        node_record: record,
        package: BundlePackageArtifact {
            id: format!("NAV_DB_{NAV_DB_CONTRACT_ID}_{cycle}_{PACKAGE_CYCLE_VERSION}"),
            family_id: "nav-db".to_string(),
            contract_id: NAV_DB_CONTRACT_ID.to_string(),
            region_id: None,
            filename: nav_db_package_artifact.filename.clone(),
            relative_path: nav_db_package_artifact.relative_path.clone(),
            cycle: Some(cycle.to_string()),
            cycle_version: Some(PACKAGE_CYCLE_VERSION.to_string()),
            checksum_sha256: nav_db_package_artifact.checksum_sha256.clone(),
            size_bytes: nav_db_package_artifact.size_bytes,
            published_at_utc: None,
            source_generated_at_utc: None,
            source_version: None,
            source_fetched_at_utc: None,
            effective_date: resource_index
                .temporal_summary
                .uniform_good_beyond_date
                .clone()
                .or_else(|| {
                    resource_index
                        .temporal_summary
                        .uniform_effective_date
                        .clone()
                }),
            expiration_date: resource_index
                .temporal_summary
                .uniform_expiration_date
                .clone()
                .or_else(|| {
                    resource_index
                        .temporal_summary
                        .expiration_dates
                        .first()
                        .cloned()
                }),
            warning_text: nav_db_warning_text(),
            metadata: BTreeMap::from([
                (
                    "contract_id".to_string(),
                    serde_json::json!(NAV_DB_CONTRACT_ID),
                ),
                (
                    NAV_DB_STARTUP_PREFETCH_MEMBERS_METADATA_KEY.to_string(),
                    serde_json::json!(startup_prefetch_members),
                ),
                (
                    "procedure_geometry_warning_count".to_string(),
                    serde_json::json!(diagnostics.procedure_geometry_warning_count),
                ),
                (
                    "procedure_geometry_error_count".to_string(),
                    serde_json::json!(diagnostics.procedure_geometry_error_count),
                ),
                (
                    "procedure_geometry_records_with_data_quality".to_string(),
                    serde_json::json!(diagnostics.procedure_geometry_records_with_data_quality),
                ),
            ]),
        },
    })
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct NavDbBuildDiagnostics {
    schema_version: u32,
    procedure_geometry_warning_count: usize,
    procedure_geometry_error_count: usize,
    procedure_geometry_records_with_data_quality: usize,
    procedure_geometry_data_quality_messages: BTreeMap<String, usize>,
}

fn nav_db_build_diagnostics_from_pairs(
    pairs: &[NavKvPair],
) -> anyhow::Result<NavDbBuildDiagnostics> {
    let mut diagnostics = NavDbBuildDiagnostics {
        schema_version: 1,
        ..Default::default()
    };
    for pair in pairs
        .iter()
        .filter(|pair| pair.key.starts_with("procedure/geometry/"))
    {
        let value: serde_json::Value = serde_json::from_slice(&pair.value)
            .with_context(|| format!("failed to decode {}", pair.key))?;
        let annotations = value
            .get("data_quality")
            .and_then(|value| value.as_array())
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if annotations.is_empty() {
            continue;
        }
        diagnostics.procedure_geometry_records_with_data_quality += 1;
        diagnostics.procedure_geometry_warning_count += annotations.len();
        for annotation in annotations {
            if let Some(message) = annotation.get("message").and_then(|value| value.as_str()) {
                *diagnostics
                    .procedure_geometry_data_quality_messages
                    .entry(message.to_string())
                    .or_default() += 1;
            }
        }
    }
    Ok(diagnostics)
}

fn nav_db_startup_prefetch_members(nav_db_zip_path: &Path) -> anyhow::Result<Vec<String>> {
    let file = File::open(nav_db_zip_path)
        .with_context(|| format!("failed to open {}", nav_db_zip_path.display()))?;
    let mut zip = ZipArchive::new(file)
        .with_context(|| format!("failed to open zip {}", nav_db_zip_path.display()))?;
    let mut root_bytes = Vec::new();
    zip.by_name("root")
        .with_context(|| format!("{} is missing nav-kv root", nav_db_zip_path.display()))?
        .read_to_end(&mut root_bytes)
        .with_context(|| {
            format!(
                "failed to read nav-kv root from {}",
                nav_db_zip_path.display()
            )
        })?;
    let root = NavKvRoot::parse(&root_bytes)
        .map_err(|err| anyhow::anyhow!("invalid nav-kv root: {err}"))?;
    let mut members = Vec::with_capacity(root.prefetch_pages().len() + 1);
    members.push("root".to_string());
    members.extend(
        root.prefetch_pages()
            .iter()
            .map(|page_index| format!("page_{page_index:04}")),
    );
    Ok(members)
}

pub(super) fn bundle_package_artifacts_from_resource_index(
    resource_index: &ResourceIndex,
) -> anyhow::Result<Vec<BundlePackageArtifact>> {
    resource_index
        .packages
        .iter()
        .map(bundle_package_artifact_from_resource_package)
        .collect()
}

pub(super) fn bundle_package_artifact_from_resource_package(
    package: &preprocessor_resource_index::ResourcePackage,
) -> anyhow::Result<BundlePackageArtifact> {
    let contract_id = product_contract_id_for_family(&package.family_id)?;
    let artifact_path = package
        .artifact_path
        .as_deref()
        .with_context(|| format!("package {} missing artifact_path", package.id))?;
    let source_filename = Path::new(artifact_path)
        .file_name()
        .and_then(|name| name.to_str())
        .with_context(|| format!("package {} artifact_path has no filename", package.id))?;
    let filename = canonical_package_filename_hashed(
        &package.family_id,
        &package.region_id,
        source_filename,
        &package.checksum_sha256,
        resource_package_chart_tier(package)?,
    )?;
    Ok(BundlePackageArtifact {
        id: package.id.clone(),
        family_id: package.family_id.clone(),
        contract_id: contract_id.to_string(),
        region_id: Some(package.region_id.clone()),
        filename: filename.clone(),
        relative_path: filename,
        cycle: package_version_from_filename(source_filename).ok(),
        cycle_version: Some(PACKAGE_CYCLE_VERSION.to_string()),
        checksum_sha256: package.checksum_sha256.clone(),
        size_bytes: package.size_bytes,
        published_at_utc: None,
        source_generated_at_utc: None,
        source_version: None,
        source_fetched_at_utc: None,
        effective_date: package.effective_date.clone(),
        expiration_date: package.expiration_date.clone(),
        warning_text: None,
        metadata: package_metadata_with_contract_id(package.metadata.clone(), contract_id),
    })
}

fn resource_package_chart_tier(
    package: &preprocessor_resource_index::ResourcePackage,
) -> anyhow::Result<Option<ChartPackageTier>> {
    package
        .metadata
        .get(CHART_PACKAGE_TIER_METADATA_KEY)
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .with_context(|| {
            format!(
                "package {} has invalid {CHART_PACKAGE_TIER_METADATA_KEY}",
                package.id
            )
        })
}

pub(super) fn build_nav_kv_chart_catalog(
    resource_index: &ResourceIndex,
    static_raster_tile_levels: &[StaticRasterCatalogEntry],
) -> serde_json::Value {
    let wide_angle_collections = resource_index
        .chart_collections
        .iter()
        .filter(|collection| collection.region_id == WIDE_ANGLE_REGION_ID)
        .map(|collection| (collection.family_id.clone(), collection))
        .collect::<BTreeMap<_, _>>();
    let mut collections = resource_index
        .chart_collections
        .iter()
        .filter(|collection| {
            matches!(
                collection.family_id.as_str(),
                "sec" | "tac" | "flyway" | "enr-l" | "enr-h"
            ) && collection.region_id != WIDE_ANGLE_REGION_ID
        })
        .map(|collection| {
            let levels = tile_levels_json(&collection.levels);
            let reference_assets = resource_index
                .chart_references
                .iter()
                .filter(|reference| reference.family_id == collection.family_id)
                .map(|reference| {
                    serde_json::json!({
                        "id": reference.id,
                        "kind": reference.kind,
                        "source_coverage": reference.source_coverage,
                    })
                })
                .collect::<Vec<_>>();
            let wide_angle = wide_angle_collections
                .get(&collection.family_id)
                .map(|wide_collection| {
                    let wide_levels = tile_levels_json(&wide_collection.levels);
                    serde_json::json!({
                        "region_id": WIDE_ANGLE_REGION_ID,
                        "max_zoom": FULL_COVERAGE_ZOOM,
                        "package_name": wide_collection.package_id,
                        "tile_url_root": "tiles",
                        "tile_path_template": wide_collection.tile_path_template.strip_prefix("tiles/").unwrap_or(&wide_collection.tile_path_template),
                        "levels": wide_levels,
                    })
                })
                .unwrap_or(serde_json::Value::Null);
            let detail = collection
                .detail_package_id
                .as_ref()
                .map(|package_id| {
                    serde_json::json!({
                        "package_name": package_id,
                        "tile_url_root": "tiles",
                        "tile_path_template": collection.tile_path_template.strip_prefix("tiles/").unwrap_or(&collection.tile_path_template),
                        "levels": tile_levels_json(&collection.detail_levels),
                    })
                })
                .unwrap_or(serde_json::Value::Null);
            serde_json::json!({
                "id": collection.id,
                "label": format!(
                    "{} {}",
                    region_display_name(resource_index, &collection.region_id),
                    family_display_name(resource_index, &collection.family_id),
                ),
                "region_id": collection.region_id,
                "reference_assets": reference_assets,
                "map_view": {
                    "chart_family": collection.family_id,
                    "chart_name": format!(
                        "{} {}",
                        region_display_name(resource_index, &collection.region_id),
                        family_display_name(resource_index, &collection.family_id),
                    ),
                    "chart_index": collection.chart_index,
                    "tile_root": "tiles",
                    "tile_url_root": "tiles",
                    "tile_path_template": collection.tile_path_template.strip_prefix("tiles/").unwrap_or(&collection.tile_path_template),
                    "tile_size": 512,
                    "min_zoom": min_zoom_for_levels(collection),
                    "max_zoom": max_zoom_for_levels(collection),
                    "wide_angle": wide_angle,
                    "detail": detail,
                    "storage_kind": "sectional_package",
                    "package_name": collection.package_id,
                    "initial_viewport": {
                        "lat": round_nav_coordinate(collection.default_view.lat),
                        "lon": round_nav_coordinate(collection.default_view.lon),
                        "zoom": collection.default_view.zoom,
                    },
                    "levels": levels,
                },
            })
        })
        .collect::<Vec<_>>();
    collections.extend(build_nav_kv_static_raster_catalog_entries(
        resource_index,
        static_raster_tile_levels,
    ));
    serde_json::Value::Array(collections)
}

pub(super) fn tile_levels_json(levels: &[TileLevelRecord]) -> Vec<serde_json::Value> {
    levels
        .iter()
        .map(|level| {
            serde_json::json!({
                "zoom": level.zoom,
                "boxes": level.boxes.iter().map(|bbox| {
                    serde_json::json!({
                        "x_min": bbox.x_min,
                        "x_max": bbox.x_max,
                        "y_tms_min": bbox.y_tms_min,
                        "y_tms_max": bbox.y_tms_max,
                    })
                }).collect::<Vec<_>>(),
            })
        })
        .collect()
}

pub(super) fn build_nav_kv_offline_region_pairs(
    resource_index: &ResourceIndex,
    chart_cutline_polygon_sets: &BTreeMap<String, ChartCutlinePolygonSetRecord>,
) -> anyhow::Result<Vec<NavKvPair>> {
    let catalog = build_offline_region_catalog(resource_index, chart_cutline_polygon_sets);
    Ok(vec![NavKvPair {
        key: "offline-region/catalog".to_string(),
        value: serde_json::to_vec(&catalog).context("failed to encode offline region catalog")?,
    }])
}

pub(super) fn build_offline_region_catalog(
    resource_index: &ResourceIndex,
    chart_cutline_polygon_sets: &BTreeMap<String, ChartCutlinePolygonSetRecord>,
) -> OfflineRegionCatalogRecord {
    let mut regions = Vec::new();
    for region in Region::ALL.iter() {
        regions.push(chart_offline_region_record(
            *region,
            resource_index,
            chart_cutline_polygon_sets,
        ));
    }
    regions.extend(plate_offline_region_records(resource_index));
    deconflict_offline_region_labels(&mut regions);
    OfflineRegionCatalogRecord {
        schema_version: 2,
        regions,
    }
}

pub(super) fn chart_offline_region_record(
    region: Region,
    resource_index: &ResourceIndex,
    chart_cutline_polygon_sets: &BTreeMap<String, ChartCutlinePolygonSetRecord>,
) -> OfflineRegionRecord {
    let region_id = region.code().to_ascii_lowercase();
    let bounds_list = region.bounds_list();
    let polygons =
        pretty_chart_offline_region_polygons(&region_id, bounds_list, chart_cutline_polygon_sets)
            .unwrap_or_else(|| chart_offline_region_bounds_polygons(bounds_list));
    let label_position = offline_region_label_position(&polygons, bounds_list);
    OfflineRegionRecord {
        id: format!("chart:{region_id}"),
        kind: "chart".to_string(),
        region_id: region_id.clone(),
        label: format!("{} Charts", region.code()),
        color_key: "class_b_d_blue".to_string(),
        summary: offline_region_summary_entries(
            resource_index,
            &region_id,
            &["sec", "tac", "enr-l", "enr-h"],
        ),
        polygons,
        label_position,
    }
}

pub(super) fn pretty_chart_offline_region_polygons(
    region_id: &str,
    bounds_list: &[RegionBounds],
    chart_cutline_polygon_sets: &BTreeMap<String, ChartCutlinePolygonSetRecord>,
) -> Option<Vec<Vec<OfflineRegionLatLon>>> {
    let mut union = MultiPolygon(Vec::new());
    for family_id in ["sec", "tac", "flyway", "enr-l", "enr-h"] {
        let collection_id = format!("{family_id}:{region_id}");
        let Some(polygon_set) = chart_cutline_polygon_sets.get(&collection_id) else {
            continue;
        };
        for polygon in &polygon_set.polygons {
            let Some(expanded) = expanded_union_polygon_from_closed_ring(
                &polygon.points,
                OFFLINE_CHART_REGION_UNION_SNAP_GRID_DEGREES,
                OFFLINE_CHART_REGION_UNION_EXPAND_DEGREES,
            ) else {
                continue;
            };
            union = if union.0.is_empty() {
                MultiPolygon(vec![expanded])
            } else {
                union.union(&expanded)
            };
        }
    }
    union = union.intersection(&region_bounds_multi_polygon(bounds_list));
    let polygons = union
        .0
        .iter()
        .filter_map(|polygon| {
            let exterior = polygon
                .exterior()
                .0
                .iter()
                .map(|coord| [coord.x, coord.y])
                .collect::<Vec<_>>();
            let simplified =
                simplify_closed_ring(&exterior, OFFLINE_CHART_REGION_SIMPLIFY_TOLERANCE_DEGREES);
            let points = simplified
                .into_iter()
                .map(|point| OfflineRegionLatLon {
                    lat: point[1],
                    lon: point[0],
                })
                .collect::<Vec<_>>();
            (points.len() >= 4).then_some(points)
        })
        .collect::<Vec<_>>();
    (!polygons.is_empty()).then_some(polygons)
}

pub(super) fn region_bounds_multi_polygon(bounds_list: &[RegionBounds]) -> MultiPolygon {
    MultiPolygon(
        bounds_list
            .iter()
            .map(|bounds| {
                Polygon::new(
                    LineString::new(vec![
                        Coord {
                            x: bounds.lon_min,
                            y: bounds.lat_max,
                        },
                        Coord {
                            x: bounds.lon_max,
                            y: bounds.lat_max,
                        },
                        Coord {
                            x: bounds.lon_max,
                            y: bounds.lat_min,
                        },
                        Coord {
                            x: bounds.lon_min,
                            y: bounds.lat_min,
                        },
                        Coord {
                            x: bounds.lon_min,
                            y: bounds.lat_max,
                        },
                    ]),
                    Vec::new(),
                )
            })
            .collect(),
    )
}

pub(super) fn chart_offline_region_bounds_polygons(
    bounds_list: &[RegionBounds],
) -> Vec<Vec<OfflineRegionLatLon>> {
    bounds_list
        .iter()
        .map(|bounds| {
            vec![
                OfflineRegionLatLon {
                    lat: bounds.lat_max,
                    lon: bounds.lon_min,
                },
                OfflineRegionLatLon {
                    lat: bounds.lat_max,
                    lon: bounds.lon_max,
                },
                OfflineRegionLatLon {
                    lat: bounds.lat_min,
                    lon: bounds.lon_max,
                },
                OfflineRegionLatLon {
                    lat: bounds.lat_min,
                    lon: bounds.lon_min,
                },
            ]
        })
        .collect()
}

pub(super) fn offline_region_label_position(
    polygons: &[Vec<OfflineRegionLatLon>],
    fallback_bounds_list: &[RegionBounds],
) -> OfflineRegionLatLon {
    let points = polygons
        .iter()
        .flat_map(|polygon| polygon.iter().copied())
        .collect::<Vec<_>>();
    if points.is_empty() {
        offline_region_bounds_label_position(fallback_bounds_list)
    } else {
        polygon_label_position(&points)
    }
}

pub(super) fn offline_region_bounds_label_position(
    bounds_list: &[RegionBounds],
) -> OfflineRegionLatLon {
    let points = bounds_list
        .iter()
        .flat_map(|bounds| {
            [
                OfflineRegionLatLon {
                    lat: bounds.lat_max,
                    lon: bounds.lon_min,
                },
                OfflineRegionLatLon {
                    lat: bounds.lat_max,
                    lon: bounds.lon_max,
                },
                OfflineRegionLatLon {
                    lat: bounds.lat_min,
                    lon: bounds.lon_max,
                },
                OfflineRegionLatLon {
                    lat: bounds.lat_min,
                    lon: bounds.lon_min,
                },
            ]
        })
        .collect::<Vec<_>>();
    polygon_label_position(&points)
}

pub(super) fn plate_offline_region_records(
    resource_index: &ResourceIndex,
) -> Vec<OfflineRegionRecord> {
    let airports_by_id = resource_index
        .airports
        .iter()
        .map(|airport| (airport.id.as_str(), airport))
        .collect::<BTreeMap<_, _>>();
    let mut points_by_region: BTreeMap<String, Vec<OfflineRegionLatLon>> = BTreeMap::new();

    for plate in &resource_index.plates {
        if let Some(airport) = airports_by_id.get(plate.airport_id.as_str()) {
            points_by_region
                .entry(plate.region_id.clone())
                .or_default()
                .push(OfflineRegionLatLon {
                    lat: airport.lat,
                    lon: airport.lon,
                });
        }
    }
    for csup in &resource_index.csups {
        if let Some(airport) = airports_by_id.get(csup.airport_id.as_str()) {
            points_by_region
                .entry(csup.region_id.clone())
                .or_default()
                .push(OfflineRegionLatLon {
                    lat: airport.lat,
                    lon: airport.lon,
                });
        }
    }

    Region::ALL
        .into_iter()
        .filter_map(|region| {
            let region_id = region.code().to_ascii_lowercase();
            let points = points_by_region.remove(&region_id)?;
            let polygon = convex_hull_lat_lon(points);
            if polygon.is_empty() {
                return None;
            }
            let label_position = polygon_label_position(&polygon);
            Some(OfflineRegionRecord {
                id: format!("plate:{region_id}"),
                kind: "plate".to_string(),
                region_id: region_id.clone(),
                label: format!("{} Plates", region.code()),
                color_key: "class_c_magenta".to_string(),
                summary: offline_region_summary_entries(
                    resource_index,
                    &region_id,
                    &["tpp", "csup"],
                ),
                polygons: vec![polygon],
                label_position,
            })
        })
        .collect()
}

pub(super) fn offline_region_summary_entries(
    resource_index: &ResourceIndex,
    region_id: &str,
    family_ids: &[&str],
) -> Vec<OfflineRegionSummaryEntry> {
    let family_ids = family_ids.iter().copied().collect::<BTreeSet<_>>();
    let mut counts_by_cycle: BTreeMap<String, usize> = BTreeMap::new();
    for package in &resource_index.packages {
        if package.region_id != region_id || !family_ids.contains(package.family_id.as_str()) {
            continue;
        }
        let cycle = package
            .cycle_code
            .as_deref()
            .or(package.version_label.as_deref())
            .unwrap_or("----")
            .to_string();
        *counts_by_cycle.entry(cycle).or_default() += 1;
    }
    counts_by_cycle
        .into_iter()
        .map(|(cycle, count)| OfflineRegionSummaryEntry {
            action: "available".to_string(),
            cycle,
            count,
        })
        .collect()
}

pub(super) fn convex_hull_lat_lon(
    mut points: Vec<OfflineRegionLatLon>,
) -> Vec<OfflineRegionLatLon> {
    points = unwrap_antimeridian_points(points);
    points.sort_by(|left, right| {
        left.lon
            .total_cmp(&right.lon)
            .then_with(|| left.lat.total_cmp(&right.lat))
    });
    points.dedup_by(|left, right| left.lat == right.lat && left.lon == right.lon);
    let polygon = match points.len() {
        0 => Vec::new(),
        1 => buffered_point_polygon(points[0]),
        2 => buffered_segment_polygon(points[0], points[1]),
        _ => monotonic_chain_hull(&points),
    };
    polygon
        .into_iter()
        .map(normalize_offline_region_lon)
        .collect()
}

pub(super) fn unwrap_antimeridian_points(
    mut points: Vec<OfflineRegionLatLon>,
) -> Vec<OfflineRegionLatLon> {
    if points.len() < 2 {
        return points;
    }
    let mut lons = points
        .iter()
        .map(|point| point.lon.rem_euclid(360.0))
        .collect::<Vec<_>>();
    lons.sort_by(f64::total_cmp);
    let mut largest_gap = -1.0;
    let mut cut = 0.0;
    for index in 0..lons.len() {
        let current = lons[index];
        let next = if index + 1 < lons.len() {
            lons[index + 1]
        } else {
            lons[0] + 360.0
        };
        let gap = next - current;
        if gap > largest_gap {
            largest_gap = gap;
            cut = next.rem_euclid(360.0);
        }
    }
    for point in &mut points {
        let mut lon = point.lon.rem_euclid(360.0);
        if lon < cut {
            lon += 360.0;
        }
        point.lon = lon;
    }
    points
}

pub(super) fn normalize_offline_region_lon(mut point: OfflineRegionLatLon) -> OfflineRegionLatLon {
    point.lon = ((point.lon + 180.0).rem_euclid(360.0)) - 180.0;
    point
}

pub(super) fn monotonic_chain_hull(points: &[OfflineRegionLatLon]) -> Vec<OfflineRegionLatLon> {
    let mut lower: Vec<OfflineRegionLatLon> = Vec::new();
    for point in points {
        while lower.len() >= 2
            && hull_cross(lower[lower.len() - 2], lower[lower.len() - 1], *point) <= 0.0
        {
            lower.pop();
        }
        lower.push(*point);
    }
    let mut upper: Vec<OfflineRegionLatLon> = Vec::new();
    for point in points.iter().rev() {
        while upper.len() >= 2
            && hull_cross(upper[upper.len() - 2], upper[upper.len() - 1], *point) <= 0.0
        {
            upper.pop();
        }
        upper.push(*point);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

pub(super) fn hull_cross(
    origin: OfflineRegionLatLon,
    a: OfflineRegionLatLon,
    b: OfflineRegionLatLon,
) -> f64 {
    (a.lon - origin.lon) * (b.lat - origin.lat) - (a.lat - origin.lat) * (b.lon - origin.lon)
}

pub(super) fn buffered_point_polygon(point: OfflineRegionLatLon) -> Vec<OfflineRegionLatLon> {
    const BUFFER_DEG: f64 = 0.25;
    vec![
        OfflineRegionLatLon {
            lat: point.lat + BUFFER_DEG,
            lon: point.lon - BUFFER_DEG,
        },
        OfflineRegionLatLon {
            lat: point.lat + BUFFER_DEG,
            lon: point.lon + BUFFER_DEG,
        },
        OfflineRegionLatLon {
            lat: point.lat - BUFFER_DEG,
            lon: point.lon + BUFFER_DEG,
        },
        OfflineRegionLatLon {
            lat: point.lat - BUFFER_DEG,
            lon: point.lon - BUFFER_DEG,
        },
    ]
}

pub(super) fn buffered_segment_polygon(
    a: OfflineRegionLatLon,
    b: OfflineRegionLatLon,
) -> Vec<OfflineRegionLatLon> {
    let lat_min = a.lat.min(b.lat) - 0.25;
    let lat_max = a.lat.max(b.lat) + 0.25;
    let lon_min = a.lon.min(b.lon) - 0.25;
    let lon_max = a.lon.max(b.lon) + 0.25;
    vec![
        OfflineRegionLatLon {
            lat: lat_max,
            lon: lon_min,
        },
        OfflineRegionLatLon {
            lat: lat_max,
            lon: lon_max,
        },
        OfflineRegionLatLon {
            lat: lat_min,
            lon: lon_max,
        },
        OfflineRegionLatLon {
            lat: lat_min,
            lon: lon_min,
        },
    ]
}

pub(super) fn polygon_label_position(polygon: &[OfflineRegionLatLon]) -> OfflineRegionLatLon {
    let polygon = unwrap_antimeridian_points(polygon.to_vec());
    let (lat_sum, lon_sum) = polygon.iter().fold((0.0, 0.0), |(lat, lon), point| {
        (lat + point.lat, lon + point.lon)
    });
    let count = polygon.len().max(1) as f64;
    normalize_offline_region_lon(OfflineRegionLatLon {
        lat: lat_sum / count,
        lon: lon_sum / count,
    })
}

#[derive(Debug, Clone, Copy)]
pub(super) struct OfflineRegionLabelLayout {
    pub(super) x: f64,
    pub(super) y: f64,
    pub(super) width: f64,
    pub(super) height: f64,
}

const OFFLINE_REGION_LABEL_LAYOUT_ZOOM: f64 = 4.0;
const OFFLINE_REGION_LABEL_TILE_SIZE_PX: f64 = 256.0;
const OFFLINE_REGION_LABEL_FONT_PX: f64 = 13.0;
const OFFLINE_REGION_LABEL_WIDTH_PER_CHAR_PX: f64 = 7.5;
const OFFLINE_REGION_LABEL_BOX_GROWTH: f64 = 1.5;
const OFFLINE_REGION_LABEL_MIN_SEPARATION_PX: f64 = 2.0;
const OFFLINE_REGION_LABEL_MAX_ITERATIONS: usize = 32;

pub(super) fn deconflict_offline_region_labels(regions: &mut [OfflineRegionRecord]) {
    let mut labels = regions
        .iter()
        .map(|region| offline_region_label_layout(region))
        .collect::<Vec<_>>();
    let world_size = offline_region_label_world_size();

    for _ in 0..OFFLINE_REGION_LABEL_MAX_ITERATIONS {
        let mut moved = false;
        for left_index in 0..labels.len() {
            for right_index in (left_index + 1)..labels.len() {
                let (left, right) = labels.split_at_mut(right_index);
                let left_label = &mut left[left_index];
                let right_label = &mut right[0];
                let dx = shortest_world_delta(right_label.x - left_label.x, world_size);
                let dy = right_label.y - left_label.y;
                let overlap_x = (left_label.width + right_label.width) / 2.0 - dx.abs();
                let overlap_y = (left_label.height + right_label.height) / 2.0 - dy.abs();
                if overlap_x <= 0.0 || overlap_y <= 0.0 {
                    continue;
                }

                moved = true;
                if overlap_x < overlap_y {
                    let direction = if dx < 0.0 { -1.0 } else { 1.0 };
                    let shift = (overlap_x + OFFLINE_REGION_LABEL_MIN_SEPARATION_PX) / 2.0;
                    left_label.x -= direction * shift;
                    right_label.x += direction * shift;
                } else {
                    let direction = if dy < 0.0 { -1.0 } else { 1.0 };
                    let shift = (overlap_y + OFFLINE_REGION_LABEL_MIN_SEPARATION_PX) / 2.0;
                    left_label.y -= direction * shift;
                    right_label.y += direction * shift;
                }
                left_label.x = left_label.x.rem_euclid(world_size);
                right_label.x = right_label.x.rem_euclid(world_size);
            }
        }
        if !moved {
            break;
        }
    }

    for (region, label) in regions.iter_mut().zip(labels) {
        region.label_position = offline_region_label_from_world(label.x, label.y);
    }
}

pub(super) fn offline_region_label_layout(
    region: &OfflineRegionRecord,
) -> OfflineRegionLabelLayout {
    let (x, y) = offline_region_label_to_world(region.label_position);
    let width = region.label.chars().count() as f64
        * OFFLINE_REGION_LABEL_WIDTH_PER_CHAR_PX
        * OFFLINE_REGION_LABEL_BOX_GROWTH;
    let height = OFFLINE_REGION_LABEL_FONT_PX * OFFLINE_REGION_LABEL_BOX_GROWTH;
    OfflineRegionLabelLayout {
        x,
        y,
        width,
        height,
    }
}

pub(super) fn offline_region_label_world_size() -> f64 {
    OFFLINE_REGION_LABEL_TILE_SIZE_PX * 2.0_f64.powf(OFFLINE_REGION_LABEL_LAYOUT_ZOOM)
}

pub(super) fn offline_region_label_to_world(point: OfflineRegionLatLon) -> (f64, f64) {
    let world_size = offline_region_label_world_size();
    let x = ((point.lon + 180.0) / 360.0 * world_size).rem_euclid(world_size);
    let lat_rad = point.lat.clamp(-85.051_128_78, 85.051_128_78).to_radians();
    let y = (0.5
        - ((std::f64::consts::PI / 4.0) + (lat_rad / 2.0)).tan().ln()
            / (2.0 * std::f64::consts::PI))
        * world_size;
    (x, y)
}

pub(super) fn offline_region_label_from_world(x: f64, y: f64) -> OfflineRegionLatLon {
    let world_size = offline_region_label_world_size();
    let lon = (x.rem_euclid(world_size) / world_size) * 360.0 - 180.0;
    let mercator = std::f64::consts::PI * (1.0 - 2.0 * (y / world_size).clamp(0.0, 1.0));
    let lat = mercator.sinh().atan().to_degrees();
    OfflineRegionLatLon { lat, lon }
}

pub(super) fn shortest_world_delta(delta: f64, world_size: f64) -> f64 {
    delta - (delta / world_size).round() * world_size
}

pub(super) fn build_nav_kv_static_raster_catalog_entries(
    resource_index: &ResourceIndex,
    static_raster_tile_levels: &[StaticRasterCatalogEntry],
) -> Vec<serde_json::Value> {
    let shaded_relief_wide_id =
        format!("shaded-relief-{WIDE_ANGLE_REGION_ID}_{SHADED_RELIEF_CONTRACT_ID}");
    let shaded_relief_wide = static_raster_tile_levels
        .iter()
        .find(|entry| entry.product_id == shaded_relief_wide_id);
    static_raster_tile_levels
        .iter()
        .filter(|entry| entry.product_id != shaded_relief_wide_id)
        .map(|entry| {
            let base_product_id = stable_product_base_id(&entry.product_id);
            let (label, region_id, initial_viewport, tile_url_root) =
                if let Some(region_id) = base_product_id.strip_prefix("shaded-relief-") {
                    let region_display_name = region_display_name(resource_index, region_id);
                    let region = Region::ALL
                        .iter()
                        .copied()
                        .find(|region| region.code().eq_ignore_ascii_case(region_id))
                        .unwrap_or(Region::Nw);
                    (
                        format!("{region_display_name} Shaded Relief"),
                        serde_json::Value::String(region_id.to_string()),
                        default_view_for_static_region(resource_index, region),
                        "tiles".to_string(),
                    )
                } else {
                    (
                        entry.label.clone(),
                        serde_json::Value::String("world".to_string()),
                        entry.initial_viewport.clone(),
                        entry.tile_url_root.clone(),
                    )
                };
            let levels = tile_levels_json(&entry.levels);
            let wide_angle = if base_product_id.starts_with("shaded-relief-") {
                shaded_relief_wide
                    .map(|wide_entry| {
                        let wide_levels = tile_levels_json(&wide_entry.levels);
                        serde_json::json!({
                            "region_id": WIDE_ANGLE_REGION_ID,
                            "max_zoom": FULL_COVERAGE_ZOOM,
                            "package_name": wide_entry.product_id,
                            "tile_url_root": "tiles",
                            "tile_path_template": wide_entry.tile_path_template.clone(),
                            "levels": wide_levels,
                        })
                    })
                    .unwrap_or(serde_json::Value::Null)
            } else {
                serde_json::Value::Null
            };
            serde_json::json!({
                "id": entry.product_id.clone(),
                "label": label.clone(),
                "region_id": region_id,
                "map_view": {
                    "chart_family": entry.chart_family.clone(),
                    "chart_name": label,
                    "chart_index": 0,
                    "tile_root": "tiles",
                    "tile_url_root": tile_url_root,
                    "tile_path_template": entry.tile_path_template.clone(),
                    "tile_size": entry.tile_size,
                    "min_zoom": entry.min_zoom,
                    "max_source_zoom": entry.max_source_zoom,
                    "max_display_zoom": entry.max_display_zoom,
                    "max_zoom": entry.max_display_zoom,
                    "wide_angle": wide_angle,
                    "storage_kind": "static_product",
                    "package_name": entry.product_id.clone(),
                    "initial_viewport": {
                        "lat": round_nav_coordinate(initial_viewport.lat),
                        "lon": round_nav_coordinate(initial_viewport.lon),
                        "zoom": initial_viewport.zoom,
                    },
                    "levels": levels,
                },
            })
        })
        .collect()
}

pub(super) fn build_chart_cutline_polygon_sets(
    chart_metadata_root: &Path,
    resource_index: &ResourceIndex,
) -> anyhow::Result<BTreeMap<String, ChartCutlinePolygonSetRecord>> {
    let mut sets = BTreeMap::new();
    for family_id in ["sec", "tac", "flyway", "enr-l", "enr-h"] {
        let Some(cutline_dir_name) = chart_cutline_dir_name(family_id) else {
            continue;
        };
        let family_collections = resource_index
            .chart_collections
            .iter()
            .filter(|collection| {
                collection.family_id == family_id && collection.region_id != WIDE_ANGLE_REGION_ID
            })
            .collect::<Vec<_>>();
        if family_collections.is_empty() {
            continue;
        }
        let polygons = read_chart_cutline_polygons(&chart_metadata_root.join(cutline_dir_name))?;
        for cutline in polygons {
            for target_collection in
                collections_for_cutline_polygon(&cutline.points, &family_collections)
            {
                let polygon_set = sets.entry(target_collection.id.clone()).or_insert_with(|| {
                    ChartCutlinePolygonSetRecord {
                        schema_version: 1,
                        id: format!("chart-coverage:{}", target_collection.id),
                        polygons: Vec::new(),
                    }
                });
                let polygon_index = polygon_set.polygons.len();
                polygon_set.polygons.push(ChartCutlinePolygonRecord {
                    id: format!("{}:{}", polygon_set.id, polygon_index),
                    points: cutline.points.clone(),
                });
            }
        }
    }
    Ok(sets)
}

pub(super) fn chart_cutline_dir_name(family_id: &str) -> Option<&'static str> {
    match family_id {
        "sec" => Some("SEC"),
        "tac" => Some("TAC"),
        "flyway" => Some("FLY"),
        "enr-l" => Some("ENR_L"),
        "enr-h" => Some("ENR_H"),
        _ => None,
    }
}

pub(super) fn read_chart_cutline_polygons(
    dir: &Path,
) -> anyhow::Result<Vec<RawChartCutlinePolygon>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut paths = fs::read_dir(dir)
        .with_context(|| format!("failed to read {}", dir.display()))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("failed to enumerate {}", dir.display()))?;
    paths.sort();
    let mut polygons = Vec::new();
    for path in paths {
        if path.extension().and_then(|ext| ext.to_str()) != Some("geojson") {
            continue;
        }
        polygons.extend(read_chart_cutline_polygons_from_file(&path)?);
    }
    Ok(polygons)
}

pub(super) fn read_chart_cutline_polygons_from_file(
    path: &Path,
) -> anyhow::Result<Vec<RawChartCutlinePolygon>> {
    let value: serde_json::Value = serde_json::from_slice(
        &fs::read(path).with_context(|| format!("failed to read {}", path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", path.display()))?;
    let feature_values = match value.get("type").and_then(|value| value.as_str()) {
        Some("FeatureCollection") => value
            .get("features")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default(),
        Some("Feature") => vec![value],
        Some(other) => bail!(
            "unsupported geojson root type {other} in {}",
            path.display()
        ),
        None => bail!("geojson root missing type in {}", path.display()),
    };

    let mut polygons = Vec::new();
    for feature in feature_values {
        let geometry = feature
            .get("geometry")
            .context("geojson feature missing geometry")?;
        let geometry_type = geometry
            .get("type")
            .and_then(|value| value.as_str())
            .context("geojson geometry missing type")?;
        match geometry_type {
            "Polygon" => polygons.push(RawChartCutlinePolygon {
                points: polygon_points_from_geojson_coordinates(
                    geometry
                        .get("coordinates")
                        .context("polygon missing coordinates")?,
                )?,
            }),
            other => bail!(
                "unsupported cutline geometry type {other} in {}",
                path.display()
            ),
        }
    }
    Ok(polygons)
}

pub(super) fn polygon_points_from_geojson_coordinates(
    coordinates: &serde_json::Value,
) -> anyhow::Result<Vec<[f64; 2]>> {
    let rings = coordinates
        .as_array()
        .context("polygon coordinates were not an array")?;
    let exterior = rings
        .first()
        .and_then(|ring| ring.as_array())
        .context("polygon had no exterior ring")?;
    exterior
        .iter()
        .map(|point| {
            let point = point.as_array().context("polygon point was not an array")?;
            let x = point
                .first()
                .and_then(|value| value.as_f64())
                .context("polygon point missing x/lon")?;
            let y = point
                .get(1)
                .and_then(|value| value.as_f64())
                .context("polygon point missing y/lat")?;
            Ok(if x.abs() > 180.0 || y.abs() > 90.0 {
                web_mercator_to_lon_lat(x, y)
            } else {
                [x, y]
            })
        })
        .collect()
}

pub(super) fn web_mercator_to_lon_lat(x: f64, y: f64) -> [f64; 2] {
    let origin_shift = 20_037_508.342_789_244_f64;
    let lon = (x / origin_shift) * 180.0;
    let lat = (y / origin_shift) * 180.0;
    let lat = 180.0 / std::f64::consts::PI
        * (2.0 * ((lat * std::f64::consts::PI / 180.0).exp()).atan() - std::f64::consts::PI / 2.0);
    [lon, lat]
}

pub(super) fn collections_for_cutline_polygon<'a>(
    points: &[[f64; 2]],
    collections: &[&'a preprocessor_resource_index::ChartCollectionRecord],
) -> Vec<&'a preprocessor_resource_index::ChartCollectionRecord> {
    let Some(polygon_bounds) = polygon_bounds(points) else {
        return Vec::new();
    };
    let overlapping = collections
        .iter()
        .copied()
        .filter(|collection| overlap_area(&polygon_bounds, &collection.coverage_bounds) > 0.0)
        .collect::<Vec<_>>();
    if !overlapping.is_empty() {
        return overlapping;
    }
    collections
        .iter()
        .copied()
        .max_by(|left, right| {
            overlap_area(&polygon_bounds, &left.coverage_bounds)
                .partial_cmp(&overlap_area(&polygon_bounds, &right.coverage_bounds))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .into_iter()
        .collect()
}

pub(super) fn overlap_area(
    left: &preprocessor_resource_index::CoverageBounds,
    right: &preprocessor_resource_index::CoverageBounds,
) -> f64 {
    let lon_overlap = (left.lon_max.min(right.lon_max) - left.lon_min.max(right.lon_min)).max(0.0);
    let lat_overlap = (left.lat_max.min(right.lat_max) - left.lat_min.max(right.lat_min)).max(0.0);
    lon_overlap * lat_overlap
}

pub(super) fn polygon_bounds(
    points: &[[f64; 2]],
) -> Option<preprocessor_resource_index::CoverageBounds> {
    let first = points.first()?;
    let mut lon_min = first[0];
    let mut lon_max = first[0];
    let mut lat_min = first[1];
    let mut lat_max = first[1];
    for point in points.iter().skip(1) {
        lon_min = lon_min.min(point[0]);
        lon_max = lon_max.max(point[0]);
        lat_min = lat_min.min(point[1]);
        lat_max = lat_max.max(point[1]);
    }
    Some(preprocessor_resource_index::CoverageBounds {
        lat_min,
        lat_max,
        lon_min,
        lon_max,
    })
}

pub(super) fn default_view_for_static_region(
    resource_index: &ResourceIndex,
    region: Region,
) -> preprocessor_resource_index::DefaultView {
    let region_id = region.code().to_ascii_lowercase();
    if let Some(reference) = resource_index
        .chart_collections
        .iter()
        .find(|collection| collection.region_id == region_id && collection.family_id == "sec")
        .or_else(|| {
            resource_index
                .chart_collections
                .iter()
                .find(|collection| collection.region_id == region_id)
        })
    {
        return reference.default_view.clone();
    }

    let bounds = region.bounds();
    preprocessor_resource_index::DefaultView {
        lat: (bounds.lat_min + bounds.lat_max) / 2.0,
        lon: (bounds.lon_min + bounds.lon_max) / 2.0,
        zoom: 4.0,
    }
}

pub(super) fn build_nav_kv_plate_pairs(
    resource_index: &ResourceIndex,
) -> anyhow::Result<Vec<NavKvPair>> {
    let airports = build_nav_kv_plate_airports(resource_index);
    let airport_index = airports
        .iter()
        .map(|airport| {
            serde_json::json!({
                "id": airport.record.get("id").and_then(|value| value.as_str()).unwrap_or_default(),
                "label": airport.record.get("label").and_then(|value| value.as_str()).unwrap_or_default(),
            })
        })
        .collect::<Vec<_>>();
    let mut pairs = vec![NavKvPair {
        key: "plate/airport-index".to_string(),
        value: serde_json::to_vec(&airport_index)
            .context("failed to encode nav_kv plate/airport-index value")?,
    }];
    for airport in airports {
        let Some(airport_id) = airport.record.get("id").and_then(|value| value.as_str()) else {
            continue;
        };
        pairs.push(NavKvPair {
            key: format!("plate/airport/{}", had_upper_key_component(airport_id)),
            value: serde_json::to_vec(&airport.record).with_context(|| {
                format!("failed to encode nav_kv plate/airport/{airport_id} value")
            })?,
        });
        for chart in &airport.charts {
            let Some(plate_id) = chart.get("id").and_then(|value| value.as_str()) else {
                continue;
            };
            pairs.push(NavKvPair {
                key: format!("plate/by-id/{}", had_key_component(plate_id)),
                value: serde_json::to_vec(chart).with_context(|| {
                    format!("failed to encode nav_kv plate/by-id/{plate_id} value")
                })?,
            });
        }
    }
    Ok(pairs)
}

pub(super) fn build_nav_kv_chart_reference_pairs(
    resource_index: &ResourceIndex,
) -> anyhow::Result<Vec<NavKvPair>> {
    let mut by_family =
        BTreeMap::<String, Vec<&preprocessor_resource_index::ChartReferenceRecord>>::new();
    for reference in &resource_index.chart_references {
        by_family
            .entry(reference.family_id.clone())
            .or_default()
            .push(reference);
    }
    let family_index = by_family
        .keys()
        .map(|family_id| {
            serde_json::json!({
                "id": family_id,
                "label": family_display_name(resource_index, family_id),
            })
        })
        .collect::<Vec<_>>();
    let mut pairs = vec![json_pair(
        "chart-reference/family-index".to_string(),
        &serde_json::Value::Array(family_index),
        "chart-reference/family-index",
    )?];
    for (family_id, mut references) in by_family {
        references.sort_by(|left, right| {
            let left_rank = if left.kind == "legend" { 0 } else { 1 };
            let right_rank = if right.kind == "legend" { 0 } else { 1 };
            left_rank
                .cmp(&right_rank)
                .then_with(|| left.label.cmp(&right.label))
        });
        let chart_ids = references
            .iter()
            .map(|reference| reference.id.as_str())
            .collect::<Vec<_>>();
        pairs.push(json_pair(
            format!("chart-reference/family/{}", had_key_component(&family_id)),
            &serde_json::json!({
                "id": family_id,
                "label": family_display_name(resource_index, &family_id),
                "chart_ids": chart_ids,
            }),
            "chart-reference/family",
        )?);
        for reference in references {
            let package_id = reference.package_ids.first().with_context(|| {
                format!("chart reference {} has no package sources", reference.id)
            })?;
            pairs.push(json_pair(
                format!("plate/by-id/{}", had_key_component(&reference.id)),
                &serde_json::json!({
                    "id": reference.id,
                    "collection_id": format!("reference:{}", reference.family_id),
                    "package_id": package_id,
                    "package_ids": reference.package_ids,
                    "label": reference.label,
                    "kind": reference.kind,
                    "folder_category": reference.kind,
                    "asset_path": reference.asset_path,
                    "thumbnail_path": reference.thumbnail_path,
                }),
                "chart-reference/by-id",
            )?);
        }
    }
    Ok(pairs)
}

pub(super) fn build_nav_kv_resource_summary_pairs(
    resource_index: &ResourceIndex,
) -> anyhow::Result<Vec<NavKvPair>> {
    let families = resource_index
        .families
        .iter()
        .map(|family| {
            let mut value = serde_json::json!({
                "id": family.id,
                "display_name": family.display_name,
                "kind": family.kind,
            });
            if let Some(warning_text) = nav_kv_family_warning_text(&family.id) {
                value["warning_text"] = serde_json::json!(warning_text);
            }
            value
        })
        .collect::<Vec<_>>();
    let regions = resource_index
        .regions
        .iter()
        .map(|region| {
            serde_json::json!({
                "id": region.id,
                "display_name": region.display_name,
                "sort_order": region.sort_order,
            })
        })
        .collect::<Vec<_>>();
    let temporal_summary = serde_json::json!({
        "cycle_codes": resource_index.temporal_summary.cycle_codes,
        "effective_dates": resource_index.temporal_summary.effective_dates,
        "expiration_dates": resource_index.temporal_summary.expiration_dates,
        "uniform_cycle_code": resource_index.temporal_summary.uniform_cycle_code,
        "uniform_effective_date": resource_index.temporal_summary.uniform_effective_date,
        "uniform_expiration_date": resource_index.temporal_summary.uniform_expiration_date,
        "uniform_good_beyond_date": resource_index.temporal_summary.uniform_good_beyond_date,
    });
    Ok(vec![
        json_pair(
            "resource/families".to_string(),
            &serde_json::Value::Array(families),
            "resource/families",
        )?,
        json_pair(
            "resource/regions".to_string(),
            &serde_json::Value::Array(regions),
            "resource/regions",
        )?,
        json_pair(
            "resource/temporal-summary".to_string(),
            &temporal_summary,
            "resource/temporal-summary",
        )?,
    ])
}

pub(super) fn build_nav_kv_package_pairs(
    package_artifacts: &[BundlePackageArtifact],
) -> anyhow::Result<Vec<NavKvPair>> {
    let mut package_index = Vec::with_capacity(package_artifacts.len());
    let mut pairs = Vec::with_capacity(package_artifacts.len());
    for package in package_artifacts {
        let mut value = serde_json::json!({
            "id": package.id,
            "family_id": package.family_id,
            "contract_id": package.contract_id,
            "region_id": package.region_id,
            "relative_path": package.relative_path,
            "size_bytes": package.size_bytes,
            "checksum_sha256": package.checksum_sha256,
            "cycle": package.cycle,
            "cycle_version": package.cycle_version,
            "effective_date": package.effective_date,
            "expiration_date": package.expiration_date,
            "metadata": package.metadata,
        });
        if let Some(warning_text) = &package.warning_text {
            value["warning_text"] = serde_json::json!(warning_text);
        }
        let mut index_entry = serde_json::json!({
            "id": package.id,
            "family_id": package.family_id,
            "contract_id": package.contract_id,
            "region_id": package.region_id,
            "metadata": &package.metadata,
        });
        if let Some(warning_text) = &package.warning_text {
            index_entry["warning_text"] = serde_json::json!(warning_text);
        }
        package_index.push(index_entry);
        pairs.push(json_pair(
            format!("package/by-id/{}", had_key_component(&package.id)),
            &value,
            &format!("package/by-id/{}", package.id),
        )?);
    }
    pairs.push(json_pair(
        "package/index".to_string(),
        &serde_json::Value::Array(package_index),
        "package/index",
    )?);
    Ok(pairs)
}

pub(super) fn build_nav_kv_vector_pairs(path: &Path) -> anyhow::Result<Vec<NavKvPair>> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut pairs = Vec::new();
    for (line_index, line) in std::io::BufReader::new(file).lines().enumerate() {
        let line = line.with_context(|| format!("failed to read {}", path.display()))?;
        if line.trim().is_empty() {
            continue;
        }
        let pair: VectorHadPairLine = serde_json::from_str(&line).with_context(|| {
            format!(
                "failed to parse vector HAD pair line {} in {}",
                line_index + 1,
                path.display()
            )
        })?;
        if pair.key.is_empty() {
            bail!(
                "vector HAD pair line {} in {} had empty key",
                line_index + 1,
                path.display()
            );
        }
        pairs.push(NavKvPair {
            key: pair.key,
            value: pair.value_json.into_bytes(),
        });
    }
    if pairs.is_empty() {
        bail!("vector HAD pair file {} had no records", path.display());
    }
    Ok(pairs)
}

#[derive(Debug)]
pub(super) struct NavKvPlateAirport {
    record: serde_json::Value,
    charts: Vec<serde_json::Value>,
}

pub(super) fn build_nav_kv_plate_airports(
    resource_index: &ResourceIndex,
) -> Vec<NavKvPlateAirport> {
    let airport_by_id = resource_index
        .airports
        .iter()
        .map(|airport| (airport.id.as_str(), airport))
        .collect::<BTreeMap<_, _>>();
    let plate_by_id = resource_index
        .plates
        .iter()
        .map(|plate| (plate.id.as_str(), plate))
        .collect::<BTreeMap<_, _>>();
    let csup_by_id = resource_index
        .csups
        .iter()
        .map(|csup| (csup.id.as_str(), csup))
        .collect::<BTreeMap<_, _>>();
    resource_index
        .airport_resources
        .iter()
        .filter_map(|airport_resources| {
            let airport_id = &airport_resources.airport_id;
            let mut charts = Vec::new();
            for plate_id in &airport_resources.plate_ids {
                if let Some(plate) = plate_by_id.get(plate_id.as_str()) {
                    charts.push(nav_kv_plate_asset(airport_id, plate));
                }
            }
            for csup_id in &airport_resources.csup_ids {
                if let Some(csup) = csup_by_id.get(csup_id.as_str()) {
                    charts.push(nav_kv_csup_asset(airport_id, csup));
                }
            }
            charts.sort_by(|left, right| {
                let left_category = left
                    .get("folder_category")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default();
                let right_category = right
                    .get("folder_category")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default();
                let left_label = left
                    .get("label")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default();
                let right_label = right
                    .get("label")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default();
                folder_category_rank(left_category)
                    .cmp(&folder_category_rank(right_category))
                    .then_with(|| left_label.cmp(right_label))
            });
            if charts.is_empty() {
                return None;
            }
            let airport = airport_by_id.get(airport_id.as_str());
            let chart_ids = charts
                .iter()
                .filter_map(|chart| chart.get("id").and_then(|value| value.as_str()))
                .collect::<Vec<_>>();
            Some(NavKvPlateAirport {
                record: serde_json::json!({
                "id": airport_id,
                "label": airport
                    .map(|airport| airport.facility_name.as_str())
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or(airport_id),
                "airport_type": airport.map(|airport| airport.airport_type.as_str()),
                "package_ids": airport_resources.package_ids.clone(),
                "chart_ids": chart_ids,
                }),
                charts,
            })
        })
        .collect::<Vec<_>>()
}

pub(super) fn build_nav_kv_navref_pairs(main_db_path: &Path) -> anyhow::Result<Vec<NavKvPair>> {
    let connection = rusqlite::Connection::open(main_db_path)
        .with_context(|| format!("failed to open {}", main_db_path.display()))?;
    let mut pairs = Vec::new();
    pairs.extend(build_nav_kv_airport_navref_pairs(&connection)?);
    pairs.extend(build_nav_kv_airport_info_pairs(&connection)?);
    pairs.extend(build_nav_kv_navaid_navref_pairs(&connection)?);
    pairs.extend(build_nav_kv_arinc_navaid_navref_pairs(&connection)?);
    pairs.extend(build_nav_kv_fix_navref_pairs(&connection)?);
    pairs.extend(build_nav_kv_runway_position_pairs(&connection)?);
    pairs.extend(build_nav_kv_waypoint_lookup_pairs(&connection)?);
    pairs.extend(build_nav_kv_procedure_pairs(&connection, None)?);
    pairs.extend(build_nav_kv_airway_pairs(&connection)?);
    let mut deduped = BTreeMap::<String, Vec<u8>>::new();
    for pair in pairs {
        deduped.entry(pair.key).or_insert(pair.value);
    }
    validate_airway_navrefs_resolve(&deduped)?;
    Ok(deduped
        .into_iter()
        .map(|(key, value)| NavKvPair { key, value })
        .collect())
}

pub(super) fn validate_airway_navrefs_resolve(
    pairs: &BTreeMap<String, Vec<u8>>,
) -> anyhow::Result<()> {
    for (key, value) in pairs {
        if !key.starts_with("airway/") {
            continue;
        }
        let json: serde_json::Value = serde_json::from_slice(value)
            .with_context(|| format!("failed to parse nav_kv airway value {key}"))?;
        validate_airway_navrefs_in_value(pairs, key, &json)?;
    }
    Ok(())
}

pub(super) fn validate_airway_navrefs_in_value(
    pairs: &BTreeMap<String, Vec<u8>>,
    source_key: &str,
    value: &serde_json::Value,
) -> anyhow::Result<()> {
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                validate_airway_navrefs_in_value(pairs, source_key, value)?;
            }
        }
        serde_json::Value::Object(object) => {
            if let Some(nav_ref) = object.get("nav_ref") {
                validate_airway_nav_ref_resolves(pairs, source_key, nav_ref)?;
            }
            for value in object.values() {
                validate_airway_navrefs_in_value(pairs, source_key, value)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn validate_airway_nav_ref_resolves(
    pairs: &BTreeMap<String, Vec<u8>>,
    source_key: &str,
    nav_ref: &serde_json::Value,
) -> anyhow::Result<()> {
    let required_key = if let Some(id) = nav_ref.get("Airport").and_then(|value| value.as_str()) {
        Some(format!(
            "navref/position/airport/{}",
            id.trim().to_ascii_uppercase()
        ))
    } else if let Some(arinc) = nav_ref
        .get("ArincNavaid")
        .and_then(|value| value.as_object())
    {
        match (
            arinc.get("identifier").and_then(|value| value.as_str()),
            arinc.get("icao_code").and_then(|value| value.as_str()),
            arinc.get("section_code").and_then(|value| value.as_str()),
            arinc
                .get("subsection_code")
                .and_then(|value| value.as_str()),
        ) {
            (Some(identifier), Some(icao_code), Some(section_code), Some(subsection_code)) => {
                Some(format!(
                    "navref/position/arinc-navaid/{}",
                    arinc_navaid_had_key(identifier, icao_code, section_code, subsection_code)
                ))
            }
            _ => None,
        }
    } else if let Some(terminal) = nav_ref
        .get("TerminalNavaid")
        .and_then(|value| value.as_object())
    {
        match (
            terminal.get("airport_id").and_then(|value| value.as_str()),
            terminal.get("identifier").and_then(|value| value.as_str()),
            terminal.get("icao_code").and_then(|value| value.as_str()),
            terminal
                .get("section_code")
                .and_then(|value| value.as_str()),
            terminal
                .get("subsection_code")
                .and_then(|value| value.as_str()),
        ) {
            (
                Some(airport_id),
                Some(identifier),
                Some(icao_code),
                Some(section_code),
                Some(subsection_code),
            ) => Some(format!(
                "navref/position/terminal-navaid/{}",
                terminal_navaid_had_key(
                    airport_id,
                    identifier,
                    icao_code,
                    section_code,
                    subsection_code
                )
            )),
            _ => None,
        }
    } else if let Some(id) = nav_ref.get("Navaid").and_then(|value| value.as_str()) {
        Some(format!(
            "navref/position/navaid/{}",
            id.trim().to_ascii_uppercase()
        ))
    } else {
        nav_ref
            .get("Fix")
            .and_then(|value| value.as_str())
            .map(|id| format!("navref/position/fix/{}", id.trim().to_ascii_uppercase()))
    };

    let Some(required_key) = required_key else {
        return Ok(());
    };
    anyhow::ensure!(
        pairs.contains_key(&required_key),
        "nav_kv airway value {source_key} emits unresolved nav_ref {nav_ref}; missing {required_key}"
    );
    Ok(())
}

pub(super) fn build_nav_kv_airport_navref_pairs(
    connection: &rusqlite::Connection,
) -> anyhow::Result<Vec<NavKvPair>> {
    let mut stmt = connection.prepare(
        "
        SELECT trim(LocationID), CAST(ARPLatitude AS REAL), CAST(ARPLongitude AS REAL),
               trim(FacilityName), trim(Type), trim(ATCT), trim(FuelTypes), trim(ARPElevation)
        FROM airports
        WHERE trim(LocationID) <> ''
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, f64>(1)?,
            row.get::<_, f64>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
        ))
    })?;
    let runway_info = airport_runway_symbol_info_by_airport(connection)?;
    let mut pairs = Vec::new();
    let mut important_metar_station_ids = BTreeSet::new();
    let mut airport_ids = BTreeSet::new();
    for row in rows {
        let (id, lat, lon, facility_name, kind, atct, fuel_types, elevation) = row?;
        let key_id = had_upper_key_component(&id);
        let station_id = id.trim().to_ascii_uppercase();
        airport_ids.insert(station_id.clone());
        if atct.trim().eq_ignore_ascii_case("Y") {
            important_metar_station_ids.insert(station_id);
        }
        pairs.push(json_pair(
            format!("navref/position/airport/{key_id}"),
            &nav_lat_lon_json(lat, lon),
            "navref airport position",
        )?);
        let info = runway_info.get(&id.trim().to_ascii_uppercase());
        let has_water_runway = info.map(|info| info.has_water_runway).unwrap_or(false)
            || kind.trim().eq_ignore_ascii_case("SEAPLANE BAS");
        pairs.push(json_pair(
            format!("navref/symbol/airport/{key_id}"),
            &serde_json::json!({
                "kind": kind.to_ascii_lowercase(),
                "label": airport_display_label(&id),
                "symbol_kind": "airport",
                "style_class": "airport",
                "towered": atct.trim().eq_ignore_ascii_case("Y"),
                "fuel_available": !fuel_types.trim().is_empty(),
                "has_paved_runway": info.map(|info| info.has_paved_runway),
                "heliport": kind.trim().to_ascii_uppercase().contains("HELIPORT"),
                "has_water_runway": has_water_runway,
                "runway_length_ratio": runway_length_ratio(info.map(|info| info.length_ft)),
                "longest_runway_heading_true_deg": info.map(|info| info.heading_true_deg),
                "elevation_msl_ft": parse_optional_float(&elevation),
            }),
            "navref airport symbol",
        )?);
        let _ = facility_name;
    }
    pairs.push(metar_important_stations_pair(&important_metar_station_ids)?);
    pairs.push(weather_station_airport_aliases_pair(
        connection,
        &airport_ids,
    )?);
    Ok(pairs)
}

#[derive(Debug)]
struct AirportRunwayInfoRecord {
    length_ft: Option<f64>,
    width_ft: Option<f64>,
    surface: String,
    end_a_ident: String,
    end_b_ident: String,
    end_a_heading_true_deg: Option<f64>,
    end_b_heading_true_deg: Option<f64>,
    end_a_latitude: Option<f64>,
    end_a_longitude: Option<f64>,
    end_b_latitude: Option<f64>,
    end_b_longitude: Option<f64>,
    end_a_right_pattern: bool,
    end_b_right_pattern: bool,
}

fn build_nav_kv_airport_info_pairs(
    connection: &rusqlite::Connection,
) -> anyhow::Result<Vec<NavKvPair>> {
    let frequencies = airport_frequencies_by_airport(connection)?;
    let contacts = airport_contacts_by_airport(connection)?;
    let weather = airport_weather_communications_by_airport(connection)?;
    let runways = airport_info_runways_by_airport(connection)?;
    let timezone_finder = tzf_rs::DefaultFinder::new();
    let mut stmt = connection.prepare(
        "
        SELECT trim(LocationID), trim(FacilityName),
               CAST(ARPLatitude AS REAL), CAST(ARPLongitude AS REAL),
               trim(ARPElevation), trim(TrafficPatternAltitude),
               trim(UNICOMFrequencies), trim(CTAFFrequency)
        FROM airports
        WHERE trim(LocationID) <> ''
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, f64>(2)?,
            row.get::<_, f64>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
        ))
    })?;
    let mut pairs = Vec::new();
    for row in rows {
        let (id, name, lat, lon, elevation, pattern_altitude, unicom, ctaf) = row?;
        let airport_key = id.trim().to_ascii_uppercase();
        let mut communications = Vec::new();
        let ctaf = normalize_airport_frequency(&ctaf);
        if !ctaf.is_empty() {
            communications.push(serde_json::json!({"label": "CTAF", "frequency": ctaf}));
        }
        let unicom = normalize_airport_frequency(&unicom);
        if !unicom.is_empty() {
            communications.push(serde_json::json!({"label": "Unicom", "frequency": unicom}));
        }
        communications.extend(frequencies.get(&airport_key).into_iter().flatten().cloned());
        communications.extend(
            weather
                .get(&airport_key)
                .into_iter()
                .flatten()
                .filter(|entry| entry.get("frequency").is_some())
                .cloned(),
        );
        dedupe_airport_communications(&mut communications);

        let airport_runways = runways
            .get(&airport_key)
            .into_iter()
            .flatten()
            .map(|runway| {
                serde_json::json!({
                    "length_ft": runway.length_ft,
                    "width_ft": runway.width_ft,
                    "surface": runway.surface,
                    "end_a": {
                        "ident": runway.end_a_ident,
                        "heading_true_deg": runway.end_a_heading_true_deg,
                        "latitude": runway.end_a_latitude,
                        "longitude": runway.end_a_longitude,
                        "right_pattern": runway.end_a_right_pattern,
                    },
                    "end_b": {
                        "ident": runway.end_b_ident,
                        "heading_true_deg": runway.end_b_heading_true_deg,
                        "latitude": runway.end_b_latitude,
                        "longitude": runway.end_b_longitude,
                        "right_pattern": runway.end_b_right_pattern,
                    },
                })
            })
            .collect::<Vec<_>>();
        let mut airport_contacts = contacts.get(&airport_key).cloned().unwrap_or_default();
        airport_contacts.extend(
            weather
                .get(&airport_key)
                .into_iter()
                .flatten()
                .filter_map(|entry| {
                    entry.get("phone").and_then(|phone| {
                        phone.as_str().map(|phone| {
                            serde_json::json!({
                                "label": entry.get("label").and_then(|value| value.as_str()).unwrap_or("Weather"),
                                "phone": phone,
                            })
                        })
                    })
                }),
        );
        dedupe_airport_contacts(&mut airport_contacts);
        let time_zone = timezone_finder.get_tz_name(lon, lat);
        pairs.push(json_pair(
            format!("airport/info/{}", had_upper_key_component(&id)),
            &serde_json::json!({
                "schema_version": 1,
                "airport_id": airport_key,
                "name": name,
                "latitude": lat,
                "longitude": lon,
                "time_zone": time_zone,
                "elevation_msl_ft": parse_optional_float(&elevation),
                "traffic_pattern_altitude_msl_ft": parse_optional_float(&pattern_altitude),
                "communications": communications,
                "contacts": airport_contacts,
                "runways": airport_runways,
            }),
            "airport info",
        )?);
    }
    Ok(pairs)
}

fn airport_frequencies_by_airport(
    connection: &rusqlite::Connection,
) -> anyhow::Result<BTreeMap<String, Vec<serde_json::Value>>> {
    let mut stmt = connection.prepare(
        "
        SELECT upper(trim(LocationID)), trim(Type), trim(Freq)
        FROM airportfreq
        WHERE trim(Type) <> 'Remark' AND trim(Freq) <> ''
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    let mut by_airport = BTreeMap::<String, Vec<serde_json::Value>>::new();
    for row in rows {
        let (airport_id, kind, frequency) = row?;
        let frequency = normalize_airport_frequency(&frequency);
        if frequency.is_empty() {
            continue;
        }
        let label = airport_frequency_label(&kind);
        by_airport
            .entry(airport_id)
            .or_default()
            .push(serde_json::json!({
                "label": label,
                "frequency": frequency,
            }));
    }
    for values in by_airport.values_mut() {
        values.sort_by_key(|value| {
            airport_communication_rank(
                value
                    .get("label")
                    .and_then(|label| label.as_str())
                    .unwrap_or_default(),
            )
        });
    }
    Ok(by_airport)
}

fn airport_contacts_by_airport(
    connection: &rusqlite::Connection,
) -> anyhow::Result<BTreeMap<String, Vec<serde_json::Value>>> {
    let mut stmt = connection.prepare(
        "
        SELECT upper(trim(LocationID)), trim(Type), trim(Phone)
        FROM airportcontacts
        WHERE trim(Phone) <> ''
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    let mut by_airport = BTreeMap::<String, Vec<serde_json::Value>>::new();
    for row in rows {
        let (airport_id, label, phone) = row?;
        by_airport
            .entry(airport_id)
            .or_default()
            .push(serde_json::json!({"label": label, "phone": phone}));
    }
    Ok(by_airport)
}

fn airport_weather_communications_by_airport(
    connection: &rusqlite::Connection,
) -> anyhow::Result<BTreeMap<String, Vec<serde_json::Value>>> {
    let mut stmt = connection.prepare(
        "
        SELECT upper(trim(LocationID)), trim(Type),
               trim(Frequency1), trim(Frequency2),
               trim(Telephone1), trim(Telephone2)
        FROM awos
        WHERE upper(trim(Status)) = 'Y'
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
        ))
    })?;
    let mut by_airport = BTreeMap::<String, Vec<serde_json::Value>>::new();
    for row in rows {
        let (airport_id, kind, frequency1, frequency2, phone1, phone2) = row?;
        for frequency in [frequency1, frequency2]
            .into_iter()
            .map(|value| normalize_airport_frequency(&value))
            .filter(|value| !value.is_empty())
        {
            by_airport
                .entry(airport_id.clone())
                .or_default()
                .push(serde_json::json!({"label": kind, "frequency": frequency}));
        }
        for phone in [phone1, phone2]
            .into_iter()
            .filter(|value| !value.is_empty())
        {
            by_airport
                .entry(airport_id.clone())
                .or_default()
                .push(serde_json::json!({"label": kind, "phone": phone}));
        }
    }
    Ok(by_airport)
}

fn airport_info_runways_by_airport(
    connection: &rusqlite::Connection,
) -> anyhow::Result<BTreeMap<String, Vec<AirportRunwayInfoRecord>>> {
    let airport_variations = load_variation_map(connection, "airports", "MagneticVariation", true)?;
    let mut stmt = connection.prepare(
        "
        SELECT upper(trim(LocationID)), trim(Length), trim(Width), trim(Surface),
               trim(LEIdent), trim(HEIdent), trim(LEHeadingT), trim(HEHeading),
               trim(LEPattern), trim(HEPattern),
               trim(LELatitude), trim(LELongitude), trim(HELatitude), trim(HELongitude)
        FROM airportrunways
        WHERE trim(LEIdent) <> '' OR trim(HEIdent) <> ''
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, String>(8)?,
            row.get::<_, String>(9)?,
            row.get::<_, String>(10)?,
            row.get::<_, String>(11)?,
            row.get::<_, String>(12)?,
            row.get::<_, String>(13)?,
        ))
    })?;
    let mut by_airport = BTreeMap::<String, Vec<AirportRunwayInfoRecord>>::new();
    for row in rows {
        let (
            airport_id,
            length,
            width,
            surface,
            end_a_ident,
            end_b_ident,
            end_a_heading,
            end_b_heading,
            end_a_pattern,
            end_b_pattern,
            end_a_lat,
            end_a_lon,
            end_b_lat,
            end_b_lon,
        ) = row?;
        let length_ft = parse_optional_float(&length);
        let width_ft = parse_optional_float(&width);
        if closed_zero_size_runway_placeholder(&end_a_ident, &end_b_ident, length_ft, width_ft) {
            continue;
        }
        let magnetic_variation_deg = airport_variations.get(&airport_id).copied().flatten();
        let end_a_heading_true_deg = resolve_runway_true_heading(
            &end_a_heading,
            &end_a_lat,
            &end_a_lon,
            &end_b_lat,
            &end_b_lon,
            &end_a_ident,
            magnetic_variation_deg,
        );
        let end_b_heading_true_deg = resolve_runway_true_heading(
            &end_b_heading,
            &end_b_lat,
            &end_b_lon,
            &end_a_lat,
            &end_a_lon,
            &end_b_ident,
            magnetic_variation_deg,
        );
        let end_a_position = parse_optional_position(&end_a_lat, &end_a_lon);
        let end_b_position = parse_optional_position(&end_b_lat, &end_b_lon);
        by_airport
            .entry(airport_id)
            .or_default()
            .push(AirportRunwayInfoRecord {
                length_ft,
                width_ft,
                surface,
                end_a_ident,
                end_b_ident,
                end_a_heading_true_deg,
                end_b_heading_true_deg,
                end_a_latitude: end_a_position.map(|position| position.0),
                end_a_longitude: end_a_position.map(|position| position.1),
                end_b_latitude: end_b_position.map(|position| position.0),
                end_b_longitude: end_b_position.map(|position| position.1),
                end_a_right_pattern: end_a_pattern.eq_ignore_ascii_case("Y"),
                end_b_right_pattern: end_b_pattern.eq_ignore_ascii_case("Y"),
            });
    }
    for values in by_airport.values_mut() {
        values.sort_by(|left, right| {
            right
                .length_ft
                .unwrap_or_default()
                .total_cmp(&left.length_ft.unwrap_or_default())
        });
    }
    Ok(by_airport)
}

fn closed_zero_size_runway_placeholder(
    end_a_ident: &str,
    end_b_ident: &str,
    length_ft: Option<f64>,
    width_ft: Option<f64>,
) -> bool {
    length_ft.unwrap_or_default() <= 0.0
        && width_ft.unwrap_or_default() <= 0.0
        && [end_a_ident, end_b_ident]
            .into_iter()
            .any(|ident| ident.trim().to_ascii_uppercase().ends_with('X'))
}

fn airport_frequency_label(kind: &str) -> &'static str {
    let kind = kind.to_ascii_uppercase();
    if kind.contains("ATIS") {
        "ATIS"
    } else if kind.contains("GND") {
        "Ground"
    } else if kind.contains("LCL") {
        "Tower"
    } else if kind.contains("CD") {
        "Clearance"
    } else if kind.contains("APCH") && kind.contains("DEP") {
        "Approach/Departure"
    } else if kind.contains("APCH") {
        "Approach"
    } else if kind.contains("DEP") {
        "Departure"
    } else {
        "Other"
    }
}

fn airport_communication_rank(label: &str) -> usize {
    match label {
        "ATIS" => 0,
        "Tower" => 1,
        "Ground" => 2,
        "Unicom" => 3,
        "CTAF" => 4,
        "Clearance" => 5,
        "Approach" | "Approach/Departure" => 6,
        "Departure" => 7,
        _ => 8,
    }
}

fn normalize_airport_frequency(value: &str) -> String {
    let mut frequencies = Vec::new();
    for token in value.split(|character: char| !character.is_ascii_digit() && character != '.') {
        let Ok(frequency) = token.parse::<f64>() else {
            continue;
        };
        if !((118.0..=137.0).contains(&frequency) || (225.0..=400.0).contains(&frequency)) {
            continue;
        }
        let mut formatted = format!("{frequency:.3}");
        while formatted.ends_with('0') && !formatted.ends_with(".0") {
            formatted.pop();
        }
        if !frequencies.contains(&formatted) {
            frequencies.push(formatted);
        }
    }
    frequencies.join(" / ")
}

fn dedupe_airport_communications(values: &mut Vec<serde_json::Value>) {
    let mut grouped = BTreeMap::<String, (String, Vec<String>)>::new();
    for value in values.drain(..) {
        let label = value
            .get("label")
            .and_then(|label| label.as_str())
            .unwrap_or("Other")
            .to_string();
        let entry = grouped
            .entry(label.to_ascii_uppercase())
            .or_insert_with(|| (label, Vec::new()));
        for frequency in value
            .get("frequency")
            .and_then(|frequency| frequency.as_str())
            .unwrap_or_default()
            .split(" / ")
        {
            let frequency = frequency.trim();
            if !frequency.is_empty() && !entry.1.iter().any(|known| known == frequency) {
                entry.1.push(frequency.to_string());
            }
        }
    }
    values.extend(grouped.into_values().filter_map(|(label, frequencies)| {
        (!frequencies.is_empty()).then(|| {
            serde_json::json!({
                "label": label,
                "frequency": frequencies.join(" / "),
            })
        })
    }));
    values.sort_by_key(|value| {
        airport_communication_rank(
            value
                .get("label")
                .and_then(|label| label.as_str())
                .unwrap_or_default(),
        )
    });
}

fn dedupe_airport_contacts(values: &mut Vec<serde_json::Value>) {
    let mut seen = BTreeSet::new();
    values.retain(|value| {
        let key = value
            .get("phone")
            .and_then(|phone| phone.as_str())
            .unwrap_or_default()
            .to_string();
        !key.is_empty() && seen.insert(key)
    });
}

pub(super) fn metar_important_stations_pair(
    station_ids: &BTreeSet<String>,
) -> anyhow::Result<NavKvPair> {
    json_pair(
        "weather/metar-important-stations".to_string(),
        &serde_json::json!({
            "schema_version": 1,
            "station_ids": station_ids.iter().collect::<Vec<_>>(),
        }),
        "METAR important station ids",
    )
}

fn weather_station_airport_aliases_pair(
    connection: &rusqlite::Connection,
    airport_ids: &BTreeSet<String>,
) -> anyhow::Result<NavKvPair> {
    let mut stmt = connection.prepare(
        "
        SELECT DISTINCT upper(trim(airports.LocationID)), upper(trim(airports.State)),
                        CAST(airports.ARPLatitude AS REAL),
                        CAST(airports.ARPLongitude AS REAL)
        FROM airports
        JOIN awos
          ON upper(trim(awos.LocationID)) = upper(trim(airports.LocationID))
        WHERE upper(trim(awos.Status)) = 'Y'
          AND length(trim(airports.LocationID)) = 3
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, f64>(2)?,
            row.get::<_, f64>(3)?,
        ))
    })?;
    let mut aliases = BTreeMap::new();
    for row in rows {
        let (airport_id, state, lat, lon) = row?;
        if !is_contiguous_us_state(&state) {
            continue;
        }
        let station_id = format!("K{airport_id}");
        if airport_ids.contains(&station_id) {
            continue;
        }
        aliases.insert(
            station_id,
            serde_json::json!({
                "airport_id": airport_id,
                "position": nav_lat_lon_json(lat, lon),
            }),
        );
    }
    json_pair(
        "weather/station-airport-aliases".to_string(),
        &serde_json::json!({
            "schema_version": 1,
            "aliases": aliases,
        }),
        "weather station airport aliases",
    )
}

fn is_contiguous_us_state(state: &str) -> bool {
    matches!(
        state.trim().to_ascii_uppercase().as_str(),
        "AL" | "AZ"
            | "AR"
            | "CA"
            | "CO"
            | "CT"
            | "DE"
            | "FL"
            | "GA"
            | "ID"
            | "IL"
            | "IN"
            | "IA"
            | "KS"
            | "KY"
            | "LA"
            | "ME"
            | "MD"
            | "MA"
            | "MI"
            | "MN"
            | "MS"
            | "MO"
            | "MT"
            | "NE"
            | "NV"
            | "NH"
            | "NJ"
            | "NM"
            | "NY"
            | "NC"
            | "ND"
            | "OH"
            | "OK"
            | "OR"
            | "PA"
            | "RI"
            | "SC"
            | "SD"
            | "TN"
            | "TX"
            | "UT"
            | "VT"
            | "VA"
            | "WA"
            | "WV"
            | "WI"
            | "WY"
            | "DC"
    )
}

pub(super) fn build_nav_kv_navaid_navref_pairs(
    connection: &rusqlite::Connection,
) -> anyhow::Result<Vec<NavKvPair>> {
    let mut stmt = connection.prepare(
        "
        SELECT trim(LocationID), CAST(ARPLatitude AS REAL), CAST(ARPLongitude AS REAL),
               trim(FacilityName), trim(Type)
        FROM nav
        WHERE trim(LocationID) <> ''
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, f64>(1)?,
            row.get::<_, f64>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;
    let mut pairs = Vec::new();
    for row in rows {
        let (id, lat, lon, facility_name, kind) = row?;
        let key_id = had_upper_key_component(&id);
        pairs.push(json_pair(
            format!("navref/position/navaid/{key_id}"),
            &nav_lat_lon_json(lat, lon),
            "navref navaid position",
        )?);
        if navaid_is_waypoint_symbol_eligible(&kind) {
            pairs.push(json_pair(
                format!("navref/symbol/navaid/{key_id}"),
                &serde_json::json!({
                    "kind": kind.to_ascii_lowercase(),
                    "label": navaid_display_label(&id, &facility_name),
                    "symbol_kind": "nav",
                    "style_class": "nav",
                }),
                "navref navaid symbol",
            )?);
        }
    }
    Ok(pairs)
}

pub(super) fn build_nav_kv_arinc_navaid_navref_pairs(
    connection: &rusqlite::Connection,
) -> anyhow::Result<Vec<NavKvPair>> {
    let mut stmt = connection.prepare(
        "
        SELECT trim(identifier), trim(icao_code), trim(section_code), trim(subsection_code),
               trim(airport_id),
               CAST(ARPLatitude AS REAL), CAST(ARPLongitude AS REAL)
        FROM arinc_navaids
        WHERE trim(identifier) <> ''
          AND trim(icao_code) <> ''
          AND trim(section_code) <> ''
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, f64>(5)?,
            row.get::<_, f64>(6)?,
        ))
    })?;
    let mut pairs = Vec::new();
    for row in rows {
        let (identifier, icao_code, section_code, subsection_code, airport_id, lat, lon) = row?;
        let position = nav_lat_lon_json(lat, lon);
        if section_code.trim().eq_ignore_ascii_case("P")
            && subsection_code.trim().eq_ignore_ascii_case("N")
            && !airport_id.trim().is_empty()
        {
            pairs.push(json_pair(
                format!(
                    "navref/position/terminal-navaid/{}",
                    terminal_navaid_had_key(
                        &airport_id,
                        &identifier,
                        &icao_code,
                        &section_code,
                        &subsection_code
                    )
                ),
                &position,
                "navref terminal navaid position",
            )?);
            continue;
        }
        pairs.push(json_pair(
            format!(
                "navref/position/arinc-navaid/{}",
                arinc_navaid_had_key(&identifier, &icao_code, &section_code, &subsection_code)
            ),
            &position,
            "navref ARINC navaid position",
        )?);
    }
    Ok(pairs)
}

pub(super) fn build_nav_kv_fix_navref_pairs(
    connection: &rusqlite::Connection,
) -> anyhow::Result<Vec<NavKvPair>> {
    let mut stmt = connection.prepare(
        "
        SELECT trim(LocationID), CAST(ARPLatitude AS REAL), CAST(ARPLongitude AS REAL),
               trim(FacilityName), trim(Type)
        FROM fix
        WHERE trim(LocationID) <> ''
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, f64>(1)?,
            row.get::<_, f64>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;
    let mut pairs = Vec::new();
    for row in rows {
        let (id, lat, lon, facility_name, kind) = row?;
        let key_id = had_upper_key_component(&id);
        pairs.push(json_pair(
            format!("navref/position/fix/{key_id}"),
            &nav_lat_lon_json(lat, lon),
            "navref fix position",
        )?);
        pairs.push(json_pair(
            format!("navref/symbol/fix/{key_id}"),
            &serde_json::json!({
                "kind": kind.to_ascii_lowercase(),
                "label": titlecase_nav_label(&facility_name).to_ascii_uppercase(),
                "symbol_kind": "fix",
                "style_class": "fix",
            }),
            "navref fix symbol",
        )?);
    }
    Ok(pairs)
}

pub(super) fn build_nav_kv_runway_position_pairs(
    connection: &rusqlite::Connection,
) -> anyhow::Result<Vec<NavKvPair>> {
    let mut stmt = connection.prepare(
        "
        SELECT trim(LocationID), trim(LEIdent), CAST(LELatitude AS REAL), CAST(LELongitude AS REAL),
               trim(HEIdent), CAST(HELatitude AS REAL), CAST(HELongitude AS REAL)
        FROM airportrunways
        WHERE trim(LocationID) <> ''
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, f64>(2)?,
            row.get::<_, f64>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, f64>(5)?,
            row.get::<_, f64>(6)?,
        ))
    })?;
    let mut pairs = Vec::new();
    for row in rows {
        let (airport_id, le_ident, le_lat, le_lon, he_ident, he_lat, he_lon) = row?;
        for (ident, lat, lon) in [(le_ident, le_lat, le_lon), (he_ident, he_lat, he_lon)] {
            let ident = ident.trim();
            if ident.is_empty() {
                continue;
            }
            pairs.push(json_pair(
                format!(
                    "navref/position/runway/{}/{}",
                    had_upper_key_component(&airport_id),
                    had_upper_key_component(&format!("RW{ident}")),
                ),
                &nav_lat_lon_json(lat, lon),
                "navref runway position",
            )?);
        }
    }
    Ok(pairs)
}

pub(super) fn build_nav_kv_waypoint_lookup_pairs(
    connection: &rusqlite::Connection,
) -> anyhow::Result<Vec<NavKvPair>> {
    let mut candidates = Vec::<WaypointSearchCandidate>::new();
    let mut kind_by_identifier = BTreeMap::<String, String>::new();
    collect_waypoint_candidates(
        connection,
        "airports",
        "airport",
        &mut candidates,
        &mut kind_by_identifier,
    )?;
    collect_waypoint_candidates(
        connection,
        "nav",
        "navaid",
        &mut candidates,
        &mut kind_by_identifier,
    )?;
    collect_waypoint_candidates(
        connection,
        "fix",
        "fix",
        &mut candidates,
        &mut kind_by_identifier,
    )?;

    let mut pairs = Vec::new();
    for (identifier, kind) in kind_by_identifier {
        let nav_ref = if is_runway_identifier(&identifier) {
            serde_json::json!({ "Fix": identifier })
        } else if kind == "navaid" {
            serde_json::json!({ "Navaid": identifier })
        } else if kind == "airport" {
            serde_json::json!({ "Airport": identifier })
        } else {
            serde_json::json!({ "Fix": identifier })
        };
        pairs.push(json_pair(
            format!(
                "waypoint/identifier/{}",
                had_upper_key_component(&identifier)
            ),
            &nav_ref,
            "waypoint identifier",
        )?);
    }

    let mut search_candidates = Vec::new();
    for candidate in &candidates {
        for (matched_term, match_kind) in &candidate.search_terms {
            search_candidates.push(WaypointSearchRecord {
                identifier: candidate.identifier.clone(),
                kind: candidate.kind.clone(),
                display_name: candidate.display_name.clone(),
                lat: candidate.lat,
                lon: candidate.lon,
                matched_term: matched_term.clone(),
                match_kind: *match_kind,
            });
        }
    }
    search_candidates.sort_by(|left, right| {
        left.matched_term
            .cmp(&right.matched_term)
            .then_with(|| left.identifier.cmp(&right.identifier))
            .then_with(|| left.match_kind.cmp(&right.match_kind))
    });
    pairs.extend(build_sparse_waypoint_search_prefix_pairs(
        &search_candidates,
    )?);

    Ok(pairs)
}

fn build_sparse_waypoint_search_prefix_pairs(
    candidates: &[WaypointSearchRecord],
) -> anyhow::Result<Vec<NavKvPair>> {
    let mut by_prefix = BTreeMap::<String, Vec<WaypointSearchRecord>>::new();
    for candidate in candidates {
        let chars = candidate.matched_term.chars().collect::<Vec<_>>();
        for length in 1..=chars.len() {
            let prefix = chars.iter().take(length).collect::<String>();
            by_prefix.entry(prefix).or_default().push(candidate.clone());
        }
    }
    let mut pairs = Vec::new();
    for (prefix, candidates) in &by_prefix {
        if distinct_waypoint_search_candidates(candidates) > WAYPOINT_SEARCH_MAX_RESULTS {
            continue;
        }
        let parent_too_large = prefix
            .chars()
            .next_back()
            .map(|last_char| {
                let parent_len = prefix.len() - last_char.len_utf8();
                if parent_len == 0 {
                    return true;
                }
                by_prefix.get(&prefix[..parent_len]).is_none_or(|parent| {
                    distinct_waypoint_search_candidates(parent) > WAYPOINT_SEARCH_MAX_RESULTS
                })
            })
            .unwrap_or(true);
        if !parent_too_large {
            continue;
        }
        pairs.push(json_pair(
            format!("waypoint/search-prefix/{}", had_upper_key_component(prefix)),
            &serde_json::to_value(candidates).context("failed to encode waypoint search shard")?,
            "waypoint search prefix",
        )?);
    }
    Ok(pairs)
}

fn distinct_waypoint_search_candidates(candidates: &[WaypointSearchRecord]) -> usize {
    candidates
        .iter()
        .map(|candidate| (candidate.kind.as_str(), candidate.identifier.as_str()))
        .collect::<BTreeSet<_>>()
        .len()
}

#[derive(Debug, Clone)]
pub(super) struct WaypointSearchCandidate {
    identifier: String,
    kind: String,
    display_name: String,
    lat: f64,
    lon: f64,
    search_terms: BTreeMap<String, WaypointSearchMatchKind>,
}

pub(super) fn collect_waypoint_candidates(
    connection: &rusqlite::Connection,
    table: &str,
    kind: &str,
    candidates: &mut Vec<WaypointSearchCandidate>,
    kind_by_identifier: &mut BTreeMap<String, String>,
) -> anyhow::Result<()> {
    let sql = if kind == "airport" {
        format!(
            "
        SELECT trim(LocationID), trim(City), trim(State), trim(FacilityName),
               CAST(ARPLatitude AS REAL), CAST(ARPLongitude AS REAL), ''
        FROM {table}
        WHERE trim(LocationID) <> ''
        "
        )
    } else if kind == "navaid" {
        format!(
            "
        SELECT trim(LocationID), '', '', trim(FacilityName),
               CAST(ARPLatitude AS REAL), CAST(ARPLongitude AS REAL), trim(Type)
        FROM {table}
        WHERE trim(LocationID) <> ''
        "
        )
    } else {
        format!(
            "
        SELECT trim(LocationID), '', '', trim(FacilityName),
               CAST(ARPLatitude AS REAL), CAST(ARPLongitude AS REAL), ''
        FROM {table}
        WHERE trim(LocationID) <> ''
        "
        )
    };
    let mut stmt = connection.prepare(&sql)?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1).unwrap_or_default(),
            row.get::<_, String>(2).unwrap_or_default(),
            row.get::<_, String>(3)?,
            row.get::<_, f64>(4)?,
            row.get::<_, f64>(5)?,
            row.get::<_, String>(6)?,
        ))
    })?;
    for row in rows {
        let (identifier, city, state, facility_name, lat, lon, navaid_type) = row?;
        let identifier = identifier.trim().to_ascii_uppercase();
        if identifier.is_empty() {
            continue;
        }
        if kind == "navaid" && !navaid_is_waypoint_symbol_eligible(&navaid_type) {
            continue;
        }
        record_waypoint_identifier_kind(kind_by_identifier, &identifier, kind)?;
        let mut search_terms =
            BTreeMap::from([(identifier.clone(), WaypointSearchMatchKind::Identifier)]);
        if kind == "airport" {
            for term in had_key::search_terms(&city)
                .into_iter()
                .chain(had_key::search_terms(&facility_name))
            {
                search_terms
                    .entry(term)
                    .or_insert(WaypointSearchMatchKind::AirportName);
            }
        }
        candidates.push(WaypointSearchCandidate {
            identifier,
            kind: kind.to_string(),
            display_name: waypoint_identifier_display_name(kind, &city, &state, &facility_name),
            lat: round_nav_coordinate(lat),
            lon: round_nav_coordinate(lon),
            search_terms,
        });
    }
    Ok(())
}

pub(super) fn record_waypoint_identifier_kind(
    kind_by_identifier: &mut BTreeMap<String, String>,
    identifier: &str,
    kind: &str,
) -> anyhow::Result<()> {
    if let Some(existing) = kind_by_identifier.insert(identifier.to_string(), kind.to_string()) {
        bail!(
            "waypoint identifier {identifier} is emitted as both {existing} and {kind}; search lookup requires one kind per identifier"
        );
    }
    Ok(())
}

pub(super) fn waypoint_identifier_display_name(
    kind: &str,
    city: &str,
    state: &str,
    facility_name: &str,
) -> String {
    let facility_name = facility_name.trim();
    let location = [city.trim(), state.trim()]
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    if !facility_name.is_empty() && !location.is_empty() {
        format!("{facility_name}\n{location}")
    } else if !facility_name.is_empty() {
        facility_name.to_string()
    } else if !location.is_empty() {
        location
    } else {
        kind.to_string()
    }
}

pub(super) fn navaid_is_waypoint_symbol_eligible(kind: &str) -> bool {
    matches!(
        kind.trim().to_ascii_uppercase().as_str(),
        "VOR" | "VOR/DME" | "VORTAC"
    )
}

#[derive(Debug, Default, Clone)]
pub struct ProcedureGeometryAuditFilter {
    pub airport_id: Option<String>,
    pub procedure_id: Option<String>,
    pub enroute_transition: Option<String>,
}

impl ProcedureGeometryAuditFilter {
    fn matches_procedure(&self, airport_id: &str, procedure_id: &str) -> bool {
        self.airport_id
            .as_ref()
            .is_none_or(|filter| filter.trim().eq_ignore_ascii_case(airport_id.trim()))
            && self
                .procedure_id
                .as_ref()
                .is_none_or(|filter| filter.trim().eq_ignore_ascii_case(procedure_id.trim()))
    }

    fn matches_geometry_record(&self, record: &pgt::ProcedureGeometryRecord) -> bool {
        self.matches_procedure(&record.key.airport_id, &record.key.procedure_id)
            && self.enroute_transition.as_ref().is_none_or(|filter| {
                record
                    .key
                    .enroute_transition
                    .as_deref()
                    .unwrap_or("")
                    .trim()
                    .eq_ignore_ascii_case(filter.trim())
            })
    }
}

pub(super) fn build_nav_kv_procedure_pairs(
    connection: &rusqlite::Connection,
    procedure_geometry_filter: Option<&ProcedureGeometryAuditFilter>,
) -> anyhow::Result<Vec<NavKvPair>> {
    let mut pairs = Vec::new();
    let cifp_matches = load_nav_kv_cifp_tpp_matches(connection)?;
    let mut matches_by_procedure = BTreeMap::<(String, String), Vec<serde_json::Value>>::new();
    let mut matches_by_plate = BTreeMap::<String, Vec<serde_json::Value>>::new();
    let mut approach_lists = BTreeMap::<String, BTreeSet<String>>::new();
    for row in cifp_matches {
        let airport_id = row
            .get("airport_id")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string();
        let cifp_id = row
            .get("cifp_id")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string();
        let plate_id = row
            .get("plate_id")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string();
        if !airport_id.is_empty() && !cifp_id.is_empty() {
            matches_by_procedure
                .entry((airport_id.clone(), cifp_id.clone()))
                .or_default()
                .push(row.clone());
            approach_lists
                .entry(airport_id)
                .or_default()
                .insert(cifp_id);
        }
        if !plate_id.is_empty() {
            matches_by_plate.entry(plate_id).or_default().push(row);
        }
    }
    for ((airport_id, cifp_id), rows) in matches_by_procedure {
        pairs.push(json_pair(
            format!(
                "plate/cifp/{}/{}",
                had_upper_key_component(&airport_id),
                had_upper_key_component(&cifp_id)
            ),
            &serde_json::Value::Array(rows),
            "plate cifp matches",
        )?);
    }
    for (plate_id, rows) in matches_by_plate {
        pairs.push(json_pair(
            format!(
                "plate/procedure-candidates/{}",
                had_key_component(&plate_id)
            ),
            &serde_json::Value::Array(rows),
            "plate procedure candidates",
        )?);
    }
    let mut sid_lists = BTreeMap::<String, BTreeSet<String>>::new();
    let mut star_lists = BTreeMap::<String, BTreeSet<String>>::new();
    let mut distinct_by_procedure = BTreeMap::<(String, String), Vec<serde_json::Value>>::new();
    let mut materialization_by_procedure =
        BTreeMap::<(String, String), Vec<serde_json::Value>>::new();
    load_nav_kv_procedure_rows(
        connection,
        &mut sid_lists,
        &mut star_lists,
        &mut distinct_by_procedure,
        &mut materialization_by_procedure,
    )?;
    if let Some(filter) = procedure_geometry_filter {
        distinct_by_procedure.retain(|(airport_id, procedure_id), _| {
            filter.matches_procedure(airport_id, procedure_id)
        });
        materialization_by_procedure.retain(|(airport_id, procedure_id), _| {
            filter.matches_procedure(airport_id, procedure_id)
        });
    }
    let procedure_kinds = procedure_kinds_from_lists(approach_lists, sid_lists, star_lists);
    let mut geometry_records = build_procedure_geometry_records(
        procedure_kinds,
        distinct_by_procedure,
        materialization_by_procedure,
    )?;
    if let Some(filter) = procedure_geometry_filter {
        geometry_records.retain(|record| filter.matches_geometry_record(record));
    }
    pairs.extend(build_nav_kv_procedure_geometry_pairs(geometry_records)?);

    Ok(pairs)
}

pub(super) fn build_nav_kv_procedure_geometry_pairs(
    geometry_records: Vec<pgt::ProcedureGeometryRecord>,
) -> anyhow::Result<Vec<NavKvPair>> {
    let mut segment_counts = BTreeMap::<String, usize>::new();
    let mut segment_records = BTreeMap::<String, pgt::ProcedureGeometrySegmentRecord>::new();
    let mut segment_bytes = BTreeMap::<String, Vec<u8>>::new();

    for record in &geometry_records {
        let mut record_segment_refs = BTreeSet::<String>::new();
        for leg_bundles in procedure_geometry_role_groups(&record.leg_bundles) {
            let segment = pgt::ProcedureGeometrySegmentRecord { leg_bundles };
            let bytes = serde_json::to_vec(&segment)
                .context("failed to encode procedure geometry segment candidate")?;
            let segment_ref = sha256_hex(&bytes);
            record_segment_refs.insert(segment_ref.clone());
            if let Some(existing) = segment_bytes.get(&segment_ref) {
                if existing != &bytes {
                    bail!("procedure geometry segment sha256 collision: {segment_ref}");
                }
            } else {
                segment_bytes.insert(segment_ref.clone(), bytes);
                segment_records.insert(segment_ref, segment);
            }
        }
        for segment_ref in record_segment_refs {
            *segment_counts.entry(segment_ref).or_default() += 1;
        }
    }

    let mut pairs = Vec::new();
    for mut record in geometry_records {
        let groups = procedure_geometry_role_groups(&record.leg_bundles);
        record.components = groups
            .into_iter()
            .map(|leg_bundles| {
                let segment = pgt::ProcedureGeometrySegmentRecord {
                    leg_bundles: leg_bundles.clone(),
                };
                let bytes = serde_json::to_vec(&segment)
                    .expect("procedure geometry segment candidate should serialize");
                let segment_ref = sha256_hex(&bytes);
                if segment_counts
                    .get(&segment_ref)
                    .copied()
                    .unwrap_or_default()
                    >= 2
                {
                    pgt::ProcedureGeometryComponent::SegmentRef { segment_ref }
                } else {
                    pgt::ProcedureGeometryComponent::LegBundles { leg_bundles }
                }
            })
            .collect();
        record.leg_bundles.clear();
        pairs.push(json_pair(
            pgt::procedure_geometry_navdb_key(&record.key),
            &serde_json::to_value(record)?,
            "procedure geometry",
        )?);
    }

    for (segment_ref, segment_record) in segment_records {
        if segment_counts
            .get(&segment_ref)
            .copied()
            .unwrap_or_default()
            < 2
        {
            continue;
        }
        pairs.push(json_pair(
            pgt::procedure_geometry_segment_navdb_key(&segment_ref),
            &serde_json::to_value(segment_record)?,
            "procedure geometry segment",
        )?);
    }

    Ok(pairs)
}

pub(super) fn procedure_geometry_role_groups(
    leg_bundles: &[pgt::ProcedureGeometryLegBundle],
) -> Vec<Vec<pgt::ProcedureGeometryLegBundle>> {
    let mut groups = Vec::<Vec<pgt::ProcedureGeometryLegBundle>>::new();
    for bundle in leg_bundles {
        if groups
            .last()
            .and_then(|group| group.last())
            .is_some_and(|previous| previous.role == bundle.role)
        {
            groups
                .last_mut()
                .expect("last group exists after role match")
                .push(bundle.clone());
        } else {
            groups.push(vec![bundle.clone()]);
        }
    }
    groups
}

#[derive(Debug, Clone)]
pub struct ProcedureGeometryAuditSummary {
    pub record_count: usize,
    pub records_with_data_quality: usize,
    pub data_quality_messages: BTreeMap<String, usize>,
}

pub fn audit_procedure_geometry_from_sqlite(
    main_db_path: &Path,
    filter: ProcedureGeometryAuditFilter,
) -> anyhow::Result<ProcedureGeometryAuditSummary> {
    let connection = rusqlite::Connection::open(main_db_path)
        .with_context(|| format!("failed to open {}", main_db_path.display()))?;
    let pairs = build_nav_kv_procedure_pairs(&connection, Some(&filter))?;
    let mut summary = ProcedureGeometryAuditSummary {
        record_count: 0,
        records_with_data_quality: 0,
        data_quality_messages: BTreeMap::new(),
    };
    for pair in pairs
        .iter()
        .filter(|pair| pair.key.starts_with("procedure/geometry/"))
    {
        summary.record_count += 1;
        let value: serde_json::Value = serde_json::from_slice(&pair.value)
            .with_context(|| format!("failed to decode {}", pair.key))?;
        let annotations = value
            .get("data_quality")
            .and_then(|value| value.as_array())
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if !annotations.is_empty() {
            summary.records_with_data_quality += 1;
        }
        for annotation in annotations {
            if let Some(message) = annotation.get("message").and_then(|value| value.as_str()) {
                *summary
                    .data_quality_messages
                    .entry(message.to_string())
                    .or_default() += 1;
            }
        }
    }
    Ok(summary)
}

pub(super) fn load_nav_kv_cifp_tpp_matches(
    connection: &rusqlite::Connection,
) -> anyhow::Result<Vec<serde_json::Value>> {
    let mut stmt = connection.prepare(
        "
        SELECT trim(airport_id), trim(cifp_id), trim(plate_id), trim(plate_label),
               trim(package_id), CAST(public AS INTEGER), CAST(priority AS INTEGER),
               trim(match_kind), CAST(is_primary AS INTEGER)
        FROM cifp_tpp_matches
        ORDER BY trim(cifp_id), CAST(is_primary AS INTEGER) DESC, CAST(priority AS INTEGER), trim(plate_label)
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(serde_json::json!({
            "airport_id": row.get::<_, String>(0)?,
            "cifp_id": row.get::<_, String>(1)?,
            "plate_id": row.get::<_, String>(2)?,
            "plate_label": row.get::<_, String>(3)?,
            "package_id": row.get::<_, String>(4)?,
            "public": row.get::<_, i64>(5)?,
            "priority": row.get::<_, i64>(6)?,
            "match_kind": row.get::<_, String>(7)?,
            "is_primary": row.get::<_, i64>(8)?,
        }))
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub(super) fn load_nav_kv_procedure_rows(
    connection: &rusqlite::Connection,
    sid_lists: &mut BTreeMap<String, BTreeSet<String>>,
    star_lists: &mut BTreeMap<String, BTreeSet<String>>,
    distinct_by_procedure: &mut BTreeMap<(String, String), Vec<serde_json::Value>>,
    materialization_by_procedure: &mut BTreeMap<(String, String), Vec<serde_json::Value>>,
) -> anyhow::Result<()> {
    let nav_context = NavLookupContext::load(connection)?;
    let mut distinct_seen = BTreeSet::<(String, String, String, String)>::new();
    let mut stmt = connection.prepare(
        "
        SELECT
          trim(airport_identifier),
          trim(sid_star_approach_identifier),
          trim(route_type),
          trim(transition_identifier),
          CAST(sequence_number AS INTEGER),
          trim(fix_identifier),
          trim(icao_code_2),
          trim(section_code_2),
          trim(subsection_code_2),
          trim(recommended_navaid),
          trim(icao_code_3),
          trim(recd_nav_section),
          trim(recd_nav_subsection),
          trim(altitude_1),
          trim(altitude_2),
          trim(path_and_termination),
          trim(turn_direction),
          trim(theta),
          trim(magnetic_course),
          trim(route_distance_holding_distance_or_time)
        FROM cifp_sid_star_app
        WHERE trim(airport_identifier) <> ''
          AND trim(sid_star_approach_identifier) <> ''
        ORDER BY trim(route_type), trim(transition_identifier), CAST(sequence_number AS INTEGER)
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i32>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, String>(8)?,
            row.get::<_, String>(9)?,
            row.get::<_, String>(10)?,
            row.get::<_, String>(11)?,
            row.get::<_, String>(12)?,
            row.get::<_, String>(13)?,
            row.get::<_, String>(14)?,
            row.get::<_, String>(15)?,
            row.get::<_, String>(16)?,
            row.get::<_, String>(17)?,
            row.get::<_, String>(18)?,
            row.get::<_, String>(19)?,
        ))
    })?;
    for row in rows {
        let (
            airport_id,
            procedure_id,
            route_type,
            transition_id,
            sequence,
            fix_identifier,
            fix_icao_code,
            fix_section_code,
            fix_subsection_code,
            recommended_navaid,
            recommended_nav_icao_code,
            recommended_nav_section,
            recommended_nav_subsection,
            altitude_1,
            altitude_2,
            path_termination,
            turn_direction,
            theta,
            magnetic_course,
            route_distance_or_time,
        ) = row?;
        match infer_nav_kv_procedure_kind(&route_type) {
            "sid" => {
                sid_lists
                    .entry(airport_id.clone())
                    .or_default()
                    .insert(procedure_id.clone());
            }
            "star" => {
                star_lists
                    .entry(airport_id.clone())
                    .or_default()
                    .insert(procedure_id.clone());
            }
            _ => {}
        }
        if distinct_seen.insert((
            airport_id.clone(),
            procedure_id.clone(),
            route_type.clone(),
            transition_id.clone(),
        )) {
            distinct_by_procedure
                .entry((airport_id.clone(), procedure_id.clone()))
                .or_default()
                .push(serde_json::json!({
                    "route_type": route_type,
                    "transition_id": transition_id,
                }));
        }
        if path_termination.trim().is_empty() {
            continue;
        }
        let nav_ref = nav_context.classify_cifp_reference_json(
            &fix_identifier,
            &fix_icao_code,
            &fix_section_code,
            &fix_subsection_code,
            &airport_id,
        );
        let defining_nav_ref = nav_context.classify_cifp_reference_json(
            &recommended_navaid,
            &recommended_nav_icao_code,
            &recommended_nav_section,
            &recommended_nav_subsection,
            &airport_id,
        );
        let nav_position = nav_context.resolve_position_json(&nav_ref, Some(&airport_id));
        let defining_nav_position =
            nav_context.resolve_position_json(&defining_nav_ref, Some(&airport_id));
        materialization_by_procedure
            .entry((airport_id.clone(), procedure_id.clone()))
            .or_default()
            .push(serde_json::json!({
                "key": {
                    "airport_id": airport_id,
                    "procedure_id": procedure_id,
                    "route_type": route_type,
                    "transition_id": transition_id,
                },
                "sequence": sequence,
                "nav_ref": nav_ref,
                "nav_position": nav_position,
                "nav_magnetic_variation_deg": nav_context.variation_for_nav_ref(&nav_ref),
                "defining_nav_ref": defining_nav_ref,
                "defining_nav_position": defining_nav_position,
                "defining_nav_magnetic_variation_deg": nav_context.variation_for_nav_ref(&defining_nav_ref),
                "airport_magnetic_variation_deg": nav_context.airport_variation.get(&airport_id.trim().to_ascii_uppercase()).copied().flatten(),
                "altitude_1_ft": parse_nav_kv_cifp_altitude_ft(&altitude_1),
                "altitude_2_ft": parse_nav_kv_cifp_altitude_ft(&altitude_2),
                "path_termination": path_termination,
                "turn_direction": non_empty_json_string(turn_direction),
                "theta_deg": parse_nav_kv_cifp_tenths_value(&theta),
                "magnetic_course_deg": parse_nav_kv_cifp_tenths_value(&magnetic_course),
                "route_distance_or_time": non_empty_json_string(route_distance_or_time),
            }));
    }
    Ok(())
}

pub(super) fn infer_nav_kv_procedure_kind(route_type: &str) -> &'static str {
    match route_type.trim() {
        "1" | "2" | "3" => "star",
        "4" | "5" | "6" => "sid",
        _ => "approach",
    }
}

pub(super) struct NavLookupContext {
    pub(super) airport_positions: BTreeMap<String, serde_json::Value>,
    pub(super) navaid_positions: BTreeMap<String, serde_json::Value>,
    pub(super) navaid_identifier_counts: BTreeMap<String, usize>,
    pub(super) arinc_navaid_positions: BTreeMap<ArincNavaidKey, serde_json::Value>,
    pub(super) terminal_navaid_positions: BTreeMap<TerminalNavaidKey, serde_json::Value>,
    pub(super) fix_positions: BTreeMap<String, serde_json::Value>,
    pub(super) airport_positions_by_coord: BTreeMap<(i64, i64), String>,
    pub(super) navaid_positions_by_coord: BTreeMap<(i64, i64), String>,
    pub(super) fix_positions_by_coord: BTreeMap<(i64, i64), String>,
    pub(super) runway_positions: BTreeMap<(String, String), serde_json::Value>,
    pub(super) navaid_variation: BTreeMap<String, Option<f64>>,
    pub(super) arinc_navaid_variation: BTreeMap<ArincNavaidKey, Option<f64>>,
    pub(super) terminal_navaid_variation: BTreeMap<TerminalNavaidKey, Option<f64>>,
    pub(super) airport_variation: BTreeMap<String, Option<f64>>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ArincNavaidKey {
    identifier: String,
    icao_code: String,
    section_code: String,
    subsection_code: String,
}

impl ArincNavaidKey {
    pub(super) fn new(
        identifier: &str,
        icao_code: &str,
        section_code: &str,
        subsection_code: &str,
    ) -> Self {
        Self {
            identifier: identifier.trim().to_ascii_uppercase(),
            icao_code: icao_code.trim().to_ascii_uppercase(),
            section_code: section_code.trim().to_ascii_uppercase(),
            subsection_code: subsection_code.trim().to_ascii_uppercase(),
        }
    }

    fn is_complete(&self) -> bool {
        // CIFP section D navaids can carry a blank subsection. Procedure rows
        // still refer to the full ARINC tuple, e.g. JVL/K5/D/<blank>.
        !self.identifier.is_empty() && !self.icao_code.is_empty() && !self.section_code.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct TerminalNavaidKey {
    airport_id: String,
    identifier: String,
    icao_code: String,
    section_code: String,
    subsection_code: String,
}

impl TerminalNavaidKey {
    pub(super) fn new(
        airport_id: &str,
        identifier: &str,
        icao_code: &str,
        section_code: &str,
        subsection_code: &str,
    ) -> Self {
        Self {
            airport_id: airport_id.trim().to_ascii_uppercase(),
            identifier: identifier.trim().to_ascii_uppercase(),
            icao_code: icao_code.trim().to_ascii_uppercase(),
            section_code: section_code.trim().to_ascii_uppercase(),
            subsection_code: subsection_code.trim().to_ascii_uppercase(),
        }
    }

    fn is_complete(&self) -> bool {
        !self.airport_id.is_empty()
            && !self.identifier.is_empty()
            && !self.icao_code.is_empty()
            && !self.section_code.is_empty()
            && !self.subsection_code.is_empty()
    }
}

pub(super) fn is_runway_identifier(identifier: &str) -> bool {
    let trimmed = identifier.trim().to_ascii_uppercase();
    let suffix = match trimmed.strip_prefix("RW") {
        Some(suffix) => suffix,
        None => return false,
    };
    if suffix.is_empty() {
        return false;
    }
    let mut chars = suffix.chars();
    if !chars.next().is_some_and(|ch| ch.is_ascii_digit()) {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric())
}

impl NavLookupContext {
    fn load(connection: &rusqlite::Connection) -> anyhow::Result<Self> {
        let airport_positions =
            load_nav_position_map(connection, "airports", "ARPLatitude", "ARPLongitude")?;
        let navaid_positions =
            load_nav_position_map(connection, "nav", "ARPLatitude", "ARPLongitude")?;
        let navaid_identifier_counts = load_nav_identifier_counts(connection)?;
        let arinc_navaid_positions = load_arinc_navaid_position_map(connection)?;
        let arinc_navaid_variation = load_arinc_navaid_variation_map(connection)?;
        let terminal_navaid_positions = load_terminal_navaid_position_map(connection)?;
        let terminal_navaid_variation = load_terminal_navaid_variation_map(connection)?;
        let fix_positions =
            load_nav_position_map(connection, "fix", "ARPLatitude", "ARPLongitude")?;
        Ok(Self {
            airport_positions_by_coord: build_position_lookup(&airport_positions),
            navaid_positions_by_coord: build_position_lookup(&navaid_positions),
            fix_positions_by_coord: build_position_lookup(&fix_positions),
            airport_positions,
            navaid_positions,
            navaid_identifier_counts,
            arinc_navaid_positions,
            terminal_navaid_positions,
            fix_positions,
            runway_positions: load_runway_position_map(connection)?,
            navaid_variation: load_variation_map(connection, "nav", "Variation", false)?,
            arinc_navaid_variation,
            terminal_navaid_variation,
            airport_variation: load_variation_map(
                connection,
                "airports",
                "MagneticVariation",
                true,
            )?,
        })
    }

    pub(super) fn classify_json(&self, identifier: &str) -> serde_json::Value {
        let trimmed = identifier.trim().to_ascii_uppercase();
        if trimmed.is_empty() {
            return serde_json::Value::Null;
        }
        if is_runway_identifier(&trimmed) {
            return serde_json::json!({ "Fix": trimmed });
        }
        if self.navaid_positions.contains_key(&trimmed) {
            return serde_json::json!({ "Navaid": trimmed });
        }
        if self.airport_positions.contains_key(&trimmed) {
            return serde_json::json!({ "Airport": trimmed });
        }
        if self.fix_positions.contains_key(&trimmed) {
            return serde_json::json!({ "Fix": trimmed });
        }
        serde_json::Value::Null
    }

    fn navaid_identifier_count(&self, identifier: &str) -> usize {
        self.navaid_identifier_counts
            .get(identifier)
            .copied()
            .unwrap_or(0)
    }

    pub(super) fn classify_cifp_reference_json(
        &self,
        identifier: &str,
        icao_code: &str,
        section_code: &str,
        subsection_code: &str,
        procedure_airport_id: &str,
    ) -> serde_json::Value {
        let trimmed = identifier.trim().to_ascii_uppercase();
        if trimmed.is_empty() {
            return serde_json::Value::Null;
        }
        if is_runway_identifier(&trimmed) {
            return serde_json::json!({ "Fix": trimmed });
        }

        let terminal_key = TerminalNavaidKey::new(
            procedure_airport_id,
            &trimmed,
            icao_code,
            section_code,
            subsection_code,
        );
        if terminal_key.is_complete() && self.terminal_navaid_positions.contains_key(&terminal_key)
        {
            return serde_json::json!({
                "TerminalNavaid": {
                    "airport_id": terminal_key.airport_id,
                    "identifier": terminal_key.identifier,
                    "icao_code": terminal_key.icao_code,
                    "section_code": terminal_key.section_code,
                    "subsection_code": terminal_key.subsection_code,
                }
            });
        }

        let key = ArincNavaidKey::new(&trimmed, icao_code, section_code, subsection_code);
        if key.is_complete() && self.arinc_navaid_positions.contains_key(&key) {
            return serde_json::json!({
                "ArincNavaid": {
                    "identifier": key.identifier,
                    "icao_code": key.icao_code,
                    "section_code": key.section_code,
                    "subsection_code": key.subsection_code,
                }
            });
        }

        match section_code.trim().to_ascii_uppercase().as_str() {
            "D" => {
                if self.navaid_positions.contains_key(&trimmed)
                    && self.navaid_identifier_count(&trimmed) <= 1
                {
                    return serde_json::json!({ "Navaid": trimmed });
                }
            }
            "A" => {
                if self.airport_positions.contains_key(&trimmed) {
                    return serde_json::json!({ "Airport": trimmed });
                }
            }
            "P" => {
                let subsection = subsection_code.trim().to_ascii_uppercase();
                if subsection == "C" || subsection.is_empty() {
                    if self.fix_positions.contains_key(&trimmed) {
                        return serde_json::json!({ "Fix": trimmed });
                    }
                }
            }
            _ => {}
        }

        if self.navaid_positions.contains_key(&trimmed)
            && self.navaid_identifier_count(&trimmed) <= 1
        {
            return serde_json::json!({ "Navaid": trimmed });
        }
        if self.airport_positions.contains_key(&trimmed) {
            return serde_json::json!({ "Airport": trimmed });
        }
        if self.fix_positions.contains_key(&trimmed) {
            return serde_json::json!({ "Fix": trimmed });
        }
        serde_json::Value::Null
    }

    fn classify_airway_point_json(
        &self,
        identifier: &str,
        lat: f64,
        lon: f64,
    ) -> serde_json::Value {
        if let Some(nav_ref) = self.classify_by_position_json(lat, lon) {
            return nav_ref;
        }

        let nav_ref = self.classify_json(identifier);
        if !nav_ref.is_null() {
            return nav_ref;
        }

        serde_json::json!({ "LatLon": nav_lat_lon_json(lat, lon) })
    }

    fn classify_by_position_json(&self, lat: f64, lon: f64) -> Option<serde_json::Value> {
        let key = position_lookup_key(lat, lon);
        if let Some(id) = self.navaid_positions_by_coord.get(&key) {
            return Some(serde_json::json!({ "Navaid": id }));
        }
        if let Some(id) = self.fix_positions_by_coord.get(&key) {
            return Some(serde_json::json!({ "Fix": id }));
        }
        if let Some(id) = self.airport_positions_by_coord.get(&key) {
            return Some(serde_json::json!({ "Airport": id }));
        }
        None
    }

    fn assert_airway_point_nav_ref_invariant(
        &self,
        airway_name: &str,
        branch_key: &str,
        sequence: i32,
        point_name: &str,
        lat: f64,
        lon: f64,
        nav_ref: &serde_json::Value,
    ) -> anyhow::Result<()> {
        let Some(fix_id) = nav_ref.get("Fix").and_then(|value| value.as_str()) else {
            return Ok(());
        };
        let fix_id = fix_id.trim().to_ascii_uppercase();
        if self.classify_json(&fix_id) != serde_json::json!({ "Navaid": fix_id }) {
            return Ok(());
        }
        let Some(navaid_position) = self.navaid_positions.get(&fix_id) else {
            return Ok(());
        };
        if position_json_matches(navaid_position, lat, lon) {
            bail!(
                "airway {airway_name} branch {branch_key} sequence {sequence} point {point_name} emitted Fix({fix_id}) but waypoint/identifier/{fix_id} resolves to colocated Navaid({fix_id})"
            );
        }
        Ok(())
    }

    pub(super) fn resolve_position_json(
        &self,
        nav_ref: &serde_json::Value,
        procedure_airport_id: Option<&str>,
    ) -> serde_json::Value {
        if let Some(key) = terminal_navaid_key_from_nav_ref(nav_ref) {
            return self
                .terminal_navaid_positions
                .get(&key)
                .cloned()
                .unwrap_or(serde_json::Value::Null);
        }
        if let Some(key) = arinc_navaid_key_from_nav_ref(nav_ref) {
            return self
                .arinc_navaid_positions
                .get(&key)
                .cloned()
                .unwrap_or(serde_json::Value::Null);
        }
        if let Some(code) = nav_ref.get("Airport").and_then(|value| value.as_str()) {
            return self
                .airport_positions
                .get(&code.trim().to_ascii_uppercase())
                .cloned()
                .unwrap_or(serde_json::Value::Null);
        }
        if let Some(code) = nav_ref.get("Navaid").and_then(|value| value.as_str()) {
            return self
                .navaid_positions
                .get(&code.trim().to_ascii_uppercase())
                .cloned()
                .unwrap_or(serde_json::Value::Null);
        }
        if let Some(code) = nav_ref.get("Fix").and_then(|value| value.as_str()) {
            let code = code.trim().to_ascii_uppercase();
            if let Some(airport_id) = procedure_airport_id {
                if is_runway_identifier(&code) {
                    if let Some(position) = self
                        .runway_positions
                        .get(&(airport_id.trim().to_ascii_uppercase(), code.clone()))
                    {
                        return position.clone();
                    }
                }
            }
            return self
                .fix_positions
                .get(&code)
                .cloned()
                .unwrap_or(serde_json::Value::Null);
        }
        serde_json::Value::Null
    }

    pub(super) fn variation_for_nav_ref(&self, nav_ref: &serde_json::Value) -> serde_json::Value {
        if let Some(key) = terminal_navaid_key_from_nav_ref(nav_ref) {
            return self
                .terminal_navaid_variation
                .get(&key)
                .copied()
                .flatten()
                .map(serde_json::Value::from)
                .unwrap_or(serde_json::Value::Null);
        }
        if let Some(key) = arinc_navaid_key_from_nav_ref(nav_ref) {
            return self
                .arinc_navaid_variation
                .get(&key)
                .copied()
                .flatten()
                .map(serde_json::Value::from)
                .unwrap_or(serde_json::Value::Null);
        }
        if let Some(code) = nav_ref.get("Navaid").and_then(|value| value.as_str()) {
            return self
                .navaid_variation
                .get(&code.trim().to_ascii_uppercase())
                .copied()
                .flatten()
                .map(serde_json::Value::from)
                .unwrap_or(serde_json::Value::Null);
        }
        serde_json::Value::Null
    }
}

pub(super) fn arinc_navaid_key_from_nav_ref(nav_ref: &serde_json::Value) -> Option<ArincNavaidKey> {
    let arinc = nav_ref.get("ArincNavaid")?.as_object()?;
    let key = ArincNavaidKey::new(
        arinc.get("identifier")?.as_str()?,
        arinc.get("icao_code")?.as_str()?,
        arinc.get("section_code")?.as_str()?,
        arinc.get("subsection_code")?.as_str()?,
    );
    key.is_complete().then_some(key)
}

pub(super) fn terminal_navaid_key_from_nav_ref(
    nav_ref: &serde_json::Value,
) -> Option<TerminalNavaidKey> {
    let terminal = nav_ref.get("TerminalNavaid")?.as_object()?;
    let key = TerminalNavaidKey::new(
        terminal.get("airport_id")?.as_str()?,
        terminal.get("identifier")?.as_str()?,
        terminal.get("icao_code")?.as_str()?,
        terminal.get("section_code")?.as_str()?,
        terminal.get("subsection_code")?.as_str()?,
    );
    key.is_complete().then_some(key)
}

pub(super) fn load_nav_position_map(
    connection: &rusqlite::Connection,
    table: &str,
    lat_column: &str,
    lon_column: &str,
) -> anyhow::Result<BTreeMap<String, serde_json::Value>> {
    let mut stmt = connection.prepare(&format!(
        "
        SELECT trim(LocationID), CAST({lat_column} AS REAL), CAST({lon_column} AS REAL)
        FROM {table}
        WHERE trim(LocationID) <> ''
        "
    ))?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?.trim().to_ascii_uppercase(),
            row.get::<_, f64>(1)?,
            row.get::<_, f64>(2)?,
        ))
    })?;
    let mut map = BTreeMap::new();
    for row in rows {
        let (id, lat, lon) = row?;
        map.entry(id).or_insert_with(|| nav_lat_lon_json(lat, lon));
    }
    Ok(map)
}

pub(super) fn load_nav_identifier_counts(
    connection: &rusqlite::Connection,
) -> anyhow::Result<BTreeMap<String, usize>> {
    let mut stmt = connection.prepare(
        "
        SELECT trim(LocationID), COUNT(*)
        FROM nav
        WHERE trim(LocationID) <> ''
        GROUP BY trim(LocationID)
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?.trim().to_ascii_uppercase(),
            row.get::<_, i64>(1)?,
        ))
    })?;
    let mut map = BTreeMap::new();
    for row in rows {
        let (id, count) = row?;
        map.insert(id, usize::try_from(count).unwrap_or(usize::MAX));
    }
    Ok(map)
}

pub(super) fn load_arinc_navaid_position_map(
    connection: &rusqlite::Connection,
) -> anyhow::Result<BTreeMap<ArincNavaidKey, serde_json::Value>> {
    let mut stmt = connection.prepare(
        "
        SELECT trim(identifier), trim(icao_code), trim(section_code), trim(subsection_code),
               CAST(ARPLatitude AS REAL), CAST(ARPLongitude AS REAL)
        FROM arinc_navaids
        WHERE trim(identifier) <> ''
          AND trim(icao_code) <> ''
          AND trim(section_code) <> ''
          AND NOT (upper(trim(section_code)) = 'P' AND upper(trim(subsection_code)) = 'N')
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            ArincNavaidKey::new(
                &row.get::<_, String>(0)?,
                &row.get::<_, String>(1)?,
                &row.get::<_, String>(2)?,
                &row.get::<_, String>(3)?,
            ),
            row.get::<_, f64>(4)?,
            row.get::<_, f64>(5)?,
        ))
    })?;
    let mut map = BTreeMap::new();
    for row in rows {
        let (key, lat, lon) = row?;
        map.entry(key).or_insert_with(|| nav_lat_lon_json(lat, lon));
    }
    Ok(map)
}

pub(super) fn load_terminal_navaid_position_map(
    connection: &rusqlite::Connection,
) -> anyhow::Result<BTreeMap<TerminalNavaidKey, serde_json::Value>> {
    let mut stmt = connection.prepare(
        "
        SELECT trim(airport_id), trim(identifier), trim(icao_code), trim(section_code), trim(subsection_code),
               CAST(ARPLatitude AS REAL), CAST(ARPLongitude AS REAL)
        FROM arinc_navaids
        WHERE trim(airport_id) <> ''
          AND trim(identifier) <> ''
          AND trim(icao_code) <> ''
          AND upper(trim(section_code)) = 'P'
          AND upper(trim(subsection_code)) = 'N'
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            TerminalNavaidKey::new(
                &row.get::<_, String>(0)?,
                &row.get::<_, String>(1)?,
                &row.get::<_, String>(2)?,
                &row.get::<_, String>(3)?,
                &row.get::<_, String>(4)?,
            ),
            row.get::<_, f64>(5)?,
            row.get::<_, f64>(6)?,
        ))
    })?;
    let mut map = BTreeMap::new();
    for row in rows {
        let (key, lat, lon) = row?;
        map.entry(key).or_insert_with(|| nav_lat_lon_json(lat, lon));
    }
    Ok(map)
}

pub(super) fn load_arinc_navaid_variation_map(
    connection: &rusqlite::Connection,
) -> anyhow::Result<BTreeMap<ArincNavaidKey, Option<f64>>> {
    let mut stmt = connection.prepare(
        "
        SELECT trim(identifier), trim(icao_code), trim(section_code), trim(subsection_code),
               CAST(Variation AS REAL)
        FROM arinc_navaids
        WHERE trim(identifier) <> ''
          AND trim(icao_code) <> ''
          AND trim(section_code) <> ''
          AND NOT (upper(trim(section_code)) = 'P' AND upper(trim(subsection_code)) = 'N')
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            ArincNavaidKey::new(
                &row.get::<_, String>(0)?,
                &row.get::<_, String>(1)?,
                &row.get::<_, String>(2)?,
                &row.get::<_, String>(3)?,
            ),
            row.get::<_, Option<f64>>(4)?,
        ))
    })?;
    let mut map = BTreeMap::new();
    for row in rows {
        let (key, variation) = row?;
        map.entry(key).or_insert(variation);
    }
    Ok(map)
}

pub(super) fn load_terminal_navaid_variation_map(
    connection: &rusqlite::Connection,
) -> anyhow::Result<BTreeMap<TerminalNavaidKey, Option<f64>>> {
    let mut stmt = connection.prepare(
        "
        SELECT trim(airport_id), trim(identifier), trim(icao_code), trim(section_code), trim(subsection_code),
               CAST(Variation AS REAL)
        FROM arinc_navaids
        WHERE trim(airport_id) <> ''
          AND trim(identifier) <> ''
          AND trim(icao_code) <> ''
          AND upper(trim(section_code)) = 'P'
          AND upper(trim(subsection_code)) = 'N'
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            TerminalNavaidKey::new(
                &row.get::<_, String>(0)?,
                &row.get::<_, String>(1)?,
                &row.get::<_, String>(2)?,
                &row.get::<_, String>(3)?,
                &row.get::<_, String>(4)?,
            ),
            row.get::<_, Option<f64>>(5)?,
        ))
    })?;
    let mut map = BTreeMap::new();
    for row in rows {
        let (key, variation) = row?;
        map.entry(key).or_insert(variation);
    }
    Ok(map)
}

pub(super) fn position_lookup_key(lat: f64, lon: f64) -> (i64, i64) {
    let lat = round_nav_coordinate(lat);
    let lon = round_nav_coordinate(lon);
    (
        (lat * 1_000_000.0).round() as i64,
        (lon * 1_000_000.0).round() as i64,
    )
}

pub(super) fn position_json_matches(position: &serde_json::Value, lat: f64, lon: f64) -> bool {
    let Some(position_lat) = position.get("lat").and_then(|value| value.as_f64()) else {
        return false;
    };
    let Some(position_lon) = position.get("lon").and_then(|value| value.as_f64()) else {
        return false;
    };
    position_lookup_key(position_lat, position_lon) == position_lookup_key(lat, lon)
}

pub(super) fn build_position_lookup(
    positions: &BTreeMap<String, serde_json::Value>,
) -> BTreeMap<(i64, i64), String> {
    let mut lookup = BTreeMap::new();
    for (id, position) in positions {
        let Some(lat) = position.get("lat").and_then(|value| value.as_f64()) else {
            continue;
        };
        let Some(lon) = position.get("lon").and_then(|value| value.as_f64()) else {
            continue;
        };
        lookup
            .entry(position_lookup_key(lat, lon))
            .or_insert_with(|| id.clone());
    }
    lookup
}

pub(super) fn build_canonical_position_lookup(
    positions: &BTreeMap<String, serde_json::Value>,
) -> BTreeMap<(i64, i64), BTreeSet<String>> {
    let mut lookup = BTreeMap::<_, BTreeSet<_>>::new();
    for (id, position) in positions {
        let Some(lat) = position.get("lat").and_then(|value| value.as_f64()) else {
            continue;
        };
        let Some(lon) = position.get("lon").and_then(|value| value.as_f64()) else {
            continue;
        };
        lookup
            .entry(canonical_position_lookup_key(lat, lon))
            .or_default()
            .insert(id.clone());
    }
    lookup
}

pub(super) fn canonical_position_lookup_key(lat: f64, lon: f64) -> (i64, i64) {
    (
        (lat * NAV_COORDINATE_DECIMAL_SCALE).round() as i64,
        (lon * NAV_COORDINATE_DECIMAL_SCALE).round() as i64,
    )
}

pub(super) fn load_runway_position_map(
    connection: &rusqlite::Connection,
) -> anyhow::Result<BTreeMap<(String, String), serde_json::Value>> {
    let mut stmt = connection.prepare(
        "
        SELECT trim(LocationID), trim(LEIdent), CAST(LELatitude AS REAL), CAST(LELongitude AS REAL),
               trim(HEIdent), CAST(HELatitude AS REAL), CAST(HELongitude AS REAL)
        FROM airportrunways
        WHERE trim(LocationID) <> ''
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, f64>(2)?,
            row.get::<_, f64>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, f64>(5)?,
            row.get::<_, f64>(6)?,
        ))
    })?;
    let mut map = BTreeMap::new();
    for row in rows {
        let (airport_id, le_ident, le_lat, le_lon, he_ident, he_lat, he_lon) = row?;
        let airport_id = airport_id.trim().to_ascii_uppercase();
        for (ident, lat, lon) in [(le_ident, le_lat, le_lon), (he_ident, he_lat, he_lon)] {
            let ident = ident.trim();
            if ident.is_empty() {
                continue;
            }
            map.insert(
                (
                    airport_id.clone(),
                    format!("RW{}", ident.to_ascii_uppercase()),
                ),
                nav_lat_lon_json(lat, lon),
            );
        }
    }
    Ok(map)
}

pub(super) fn load_variation_map(
    connection: &rusqlite::Connection,
    table: &str,
    column: &str,
    airport_format: bool,
) -> anyhow::Result<BTreeMap<String, Option<f64>>> {
    let mut stmt = connection.prepare(&format!(
        "
        SELECT trim(LocationID), trim({column})
        FROM {table}
        WHERE trim(LocationID) <> ''
        "
    ))?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?.trim().to_ascii_uppercase(),
            row.get::<_, String>(1)?,
        ))
    })?;
    let mut map = BTreeMap::new();
    for row in rows {
        let (id, raw) = row?;
        let variation = if airport_format {
            parse_nav_kv_airport_magnetic_variation(&raw)
        } else {
            raw.trim().parse::<f64>().ok()
        };
        map.entry(id).or_insert(variation);
    }
    Ok(map)
}

pub(super) fn parse_nav_kv_cifp_tenths_value(raw: &str) -> Option<f64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let parsed = trimmed.parse::<f64>().ok()?;
    Some(parsed / 10.0)
}

pub(super) fn parse_nav_kv_cifp_altitude_ft(raw: &str) -> Option<f64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    trimmed.parse::<f64>().ok()
}

pub(super) fn parse_nav_kv_airport_magnetic_variation(raw: &str) -> Option<f64> {
    parse_airport_magnetic_variation(raw)
}

pub(super) fn non_empty_json_string(value: String) -> serde_json::Value {
    if value.trim().is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::Value::String(value)
    }
}

pub(super) fn build_nav_kv_airway_pairs(
    connection: &rusqlite::Connection,
) -> anyhow::Result<Vec<NavKvPair>> {
    let nav_context = NavLookupContext::load(connection)?;
    let navaid_ids_by_canonical_position =
        build_canonical_position_lookup(&nav_context.navaid_positions);
    let mut stmt = connection.prepare(
        "
        SELECT trim(name), trim(branch_key), CAST(sequence_number AS INTEGER),
               trim(point_name), Latitude, Longitude
        FROM airways_branch
        WHERE trim(name) <> ''
        ORDER BY trim(name), trim(branch_key), CAST(sequence_number AS INTEGER)
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i32>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, f64>(4)?,
            row.get::<_, f64>(5)?,
        ))
    })?;
    let mut branch_points = BTreeMap::<(String, String), Vec<serde_json::Value>>::new();
    let mut spatial_points = BTreeMap::<(i32, i32), Vec<serde_json::Value>>::new();
    let mut colocated_navaid_violations = Vec::new();
    for row in rows {
        let (name, branch_key, sequence, point_name, lat, lon) = row?;
        let position = nav_lat_lon_json(lat, lon);
        let nav_ref = nav_context.classify_airway_point_json(&point_name, lat, lon);
        if let Some(expected_navaids) =
            navaid_ids_by_canonical_position.get(&canonical_position_lookup_key(lat, lon))
        {
            let actual_navaid = nav_ref.get("Navaid").and_then(serde_json::Value::as_str);
            if actual_navaid.is_none_or(|actual| !expected_navaids.contains(actual)) {
                colocated_navaid_violations.push(format!(
                    "{name}/{branch_key}/{sequence} {point_name}: expected one of {expected_navaids:?}, got {nav_ref}"
                ));
            }
        }
        nav_context.assert_airway_point_nav_ref_invariant(
            &name,
            &branch_key,
            sequence,
            &point_name,
            lat,
            lon,
            &nav_ref,
        )?;
        let point = serde_json::json!({
            "airway_name": name,
            "sequence": sequence,
            "position": position.clone(),
            "nav_ref": nav_ref.clone(),
        });
        branch_points
            .entry((name.clone(), branch_key.clone()))
            .or_default()
            .push(point);
        let spatial_point = serde_json::json!({
            "airway_name": name,
            "branch_key": branch_key,
            "sequence": sequence,
            "position": position,
            "nav_ref": nav_ref,
        });
        spatial_points
            .entry((lat.floor() as i32, lon.floor() as i32))
            .or_default()
            .push(spatial_point);
    }
    anyhow::ensure!(
        colocated_navaid_violations.is_empty(),
        "{} airway point(s) colocated with known navaids lost navaid identity:\n{}",
        colocated_navaid_violations.len(),
        colocated_navaid_violations.join("\n")
    );

    let mut branches_by_airway = BTreeMap::<String, Vec<serde_json::Value>>::new();
    for ((name, branch_key), points) in branch_points {
        branches_by_airway
            .entry(name.clone())
            .or_default()
            .push(serde_json::json!({
                "display_name": name,
                "branch_key": branch_key,
                "points": points,
            }));
    }

    let mut pairs = Vec::new();
    for (airway_name, branches) in branches_by_airway {
        pairs.push(json_pair(
            format!("airway/{}", had_upper_key_component(&airway_name)),
            &serde_json::Value::Array(branches),
            "airway branches",
        )?);
    }
    for ((lat_tile, lon_tile), points) in spatial_points {
        pairs.push(json_pair(
            format!("airway/spatial/{lat_tile}/{lon_tile}"),
            &serde_json::Value::Array(points),
            "airway spatial tile",
        )?);
    }
    Ok(pairs)
}

#[derive(Clone, Copy)]
pub(super) struct AirportRunwaySymbolInfo {
    length_ft: f64,
    heading_true_deg: f64,
    has_paved_runway: bool,
    has_water_runway: bool,
}

pub(super) fn airport_runway_symbol_info_by_airport(
    connection: &rusqlite::Connection,
) -> anyhow::Result<BTreeMap<String, AirportRunwaySymbolInfo>> {
    let airport_variations = load_variation_map(connection, "airports", "MagneticVariation", true)?;
    let mut stmt = connection.prepare(
        "
        SELECT trim(LocationID), trim(Length), trim(Surface), trim(LEHeadingT),
               trim(LELatitude), trim(LELongitude), trim(HELatitude), trim(HELongitude),
               trim(LEIdent), trim(HEHeading), trim(HEIdent)
        FROM airportrunways
        WHERE trim(LocationID) <> ''
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, String>(8)?,
            row.get::<_, String>(9)?,
            row.get::<_, String>(10)?,
        ))
    })?;
    let mut by_airport = BTreeMap::<String, AirportRunwaySymbolInfo>::new();
    for row in rows {
        let (
            airport_id,
            length,
            surface,
            le_heading,
            le_lat,
            le_lon,
            he_lat,
            he_lon,
            le_ident,
            he_heading,
            he_ident,
        ) = row?;
        let length = parse_optional_float(&length).unwrap_or_default();
        if length <= 0.0 {
            continue;
        }
        let surface = surface.trim().to_ascii_uppercase();
        let has_paved_runway = surface_is_paved(&surface);
        let has_water_runway = surface.contains("WATER");
        let key = airport_id.trim().to_ascii_uppercase();
        let magnetic_variation_deg = airport_variations.get(&key).copied().flatten();
        let Some(heading) = resolve_runway_true_heading(
            &le_heading,
            &le_lat,
            &le_lon,
            &he_lat,
            &he_lon,
            &le_ident,
            magnetic_variation_deg,
        )
        .or_else(|| {
            resolve_runway_true_heading(
                &he_heading,
                &he_lat,
                &he_lon,
                &le_lat,
                &le_lon,
                &he_ident,
                magnetic_variation_deg,
            )
        }) else {
            continue;
        };
        match by_airport.get_mut(&key) {
            Some(existing) if existing.length_ft >= length => {
                existing.has_paved_runway |= has_paved_runway;
                existing.has_water_runway |= has_water_runway;
            }
            _ => {
                by_airport.insert(
                    key,
                    AirportRunwaySymbolInfo {
                        length_ft: length,
                        heading_true_deg: heading,
                        has_paved_runway,
                        has_water_runway,
                    },
                );
            }
        }
    }
    Ok(by_airport)
}

fn resolve_runway_true_heading(
    published_heading: &str,
    start_lat: &str,
    start_lon: &str,
    end_lat: &str,
    end_lon: &str,
    runway_ident: &str,
    magnetic_variation_deg: Option<f64>,
) -> Option<f64> {
    resolve_true_heading(RunwayHeadingInput {
        published_heading_deg: parse_optional_float(published_heading),
        start: parse_optional_position(start_lat, start_lon),
        end: parse_optional_position(end_lat, end_lon),
        runway_ident,
        magnetic_variation_deg,
    })
}

pub(super) fn json_pair(
    key: String,
    value: &serde_json::Value,
    context: &str,
) -> anyhow::Result<NavKvPair> {
    Ok(NavKvPair {
        key,
        value: serde_json::to_vec(value)
            .with_context(|| format!("failed to encode nav_kv {context} value"))?,
    })
}

pub(super) fn nav_kv_plate_asset(
    airport_id: &str,
    plate: &preprocessor_resource_index::PlateRecord,
) -> serde_json::Value {
    let filename = plate
        .asset_path
        .rsplit('/')
        .next()
        .unwrap_or(&plate.asset_path);
    let mut value = serde_json::json!({
        "id": format!("plate:{airport_id}:{filename}"),
        "airport_id": airport_id,
        "collection_id": format!("airport:{airport_id}"),
        "package_id": plate.package_id,
        "label": plate.label,
        "kind": "plate",
        "folder_category": folder_category_for_document_type(&plate.document_type),
        "asset_path": plate.asset_path,
    });
    if let Some(thumbnail_path) = non_empty_string(&plate.thumbnail_path) {
        value["thumbnail_path"] = serde_json::json!(thumbnail_path);
    }
    if let Some(georef) = &plate.georef {
        value["georef"] = serde_json::json!(georef);
    }
    value
}

pub(super) fn nav_kv_csup_asset(
    airport_id: &str,
    csup: &preprocessor_resource_index::CsupRecord,
) -> serde_json::Value {
    let filename = csup
        .asset_path
        .rsplit('/')
        .next()
        .unwrap_or(&csup.asset_path);
    let mut value = serde_json::json!({
        "id": format!("csup:{airport_id}:{filename}"),
        "airport_id": airport_id,
        "collection_id": format!("airport:{airport_id}"),
        "package_id": csup.package_id,
        "label": csup.label,
        "kind": "csup",
        "folder_category": "csup",
        "asset_path": csup.asset_path,
    });
    if let Some(thumbnail_path) = non_empty_string(&csup.thumbnail_path) {
        value["thumbnail_path"] = serde_json::json!(thumbnail_path);
    }
    value
}

pub(super) fn non_empty_string(value: &str) -> Option<&str> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

pub(super) fn arinc_navaid_had_key(
    identifier: &str,
    icao_code: &str,
    section_code: &str,
    subsection_code: &str,
) -> String {
    [
        had_upper_key_component(section_code),
        had_upper_key_component(subsection_code),
        had_upper_key_component(icao_code),
        had_upper_key_component(identifier),
    ]
    .join("/")
}

pub(super) fn terminal_navaid_had_key(
    airport_id: &str,
    identifier: &str,
    icao_code: &str,
    section_code: &str,
    subsection_code: &str,
) -> String {
    [
        had_upper_key_component(airport_id),
        had_upper_key_component(section_code),
        had_upper_key_component(subsection_code),
        had_upper_key_component(icao_code),
        had_upper_key_component(identifier),
    ]
    .join("/")
}

pub(super) fn airport_display_label(id: &str) -> String {
    id.trim().to_ascii_uppercase()
}

pub(super) fn navaid_display_label(id: &str, facility_name: &str) -> String {
    let frequency = facility_name
        .split_whitespace()
        .last()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(frequency) = frequency {
        format!("{} {frequency}", id.trim()).to_ascii_uppercase()
    } else {
        id.trim().to_ascii_uppercase()
    }
}

pub(super) fn titlecase_nav_label(value: &str) -> String {
    value
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => {
                    let mut normalized = first.to_uppercase().collect::<String>();
                    normalized.push_str(&chars.as_str().to_ascii_lowercase());
                    normalized
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn runway_length_ratio(longest_runway_length_ft: Option<f64>) -> f64 {
    (longest_runway_length_ft.unwrap_or(0.0) / 5000.0).clamp(0.0, 1.0)
}

pub(super) fn surface_is_paved(surface: &str) -> bool {
    surface
        .split('-')
        .any(|part| matches!(part.trim(), "ASPH" | "CONC" | "BIT" | "PEM"))
}

pub(super) fn parse_optional_float(value: &str) -> Option<f64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    trimmed
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
}

pub(super) fn folder_category_for_document_type(document_type: &str) -> &'static str {
    match document_type {
        "airport_diagram" => "airport-diagram",
        "takeoff_minimums" | "alternate_minimums" | "minimums" => "takeoff-mins",
        "departure" => "departure",
        "star" => "star",
        "csup" => "csup",
        "hotspot" => "hotspot",
        "other" => "other",
        "approach" => "approach",
        _ => "other",
    }
}

pub(super) fn folder_category_rank(category: &str) -> usize {
    match category {
        "approach" => 0,
        "departure" => 1,
        "star" => 2,
        "airport-diagram" => 3,
        "csup" => 4,
        "takeoff-mins" => 5,
        "other" => 6,
        "hotspot" => 7,
        _ => 8,
    }
}

pub(super) fn family_display_name(resource_index: &ResourceIndex, family_id: &str) -> String {
    resource_index
        .families
        .iter()
        .find(|family| family.id == family_id)
        .map(|family| family.display_name.clone())
        .unwrap_or_else(|| family_id.to_string())
}

pub(super) fn region_display_name(resource_index: &ResourceIndex, region_id: &str) -> String {
    resource_index
        .regions
        .iter()
        .find(|region| region.id == region_id)
        .map(|region| region.display_name.clone())
        .unwrap_or_else(|| region_id.to_ascii_uppercase())
}

pub(super) fn min_zoom_for_levels(
    collection: &preprocessor_resource_index::ChartCollectionRecord,
) -> f64 {
    let min_level = collection
        .levels
        .iter()
        .map(|level| level.zoom)
        .min()
        .unwrap_or(0);
    (min_level as f64 - 2.8).max(1.5)
}

pub(super) fn max_zoom_for_levels(
    _collection: &preprocessor_resource_index::ChartCollectionRecord,
) -> f64 {
    RASTER_BASEMAP_MAX_DISPLAY_ZOOM
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nav_lat_lon_json_rounds_to_seven_decimals() {
        assert_eq!(round_nav_coordinate(47.49313888888889), 47.4931389);
        assert_eq!(round_nav_coordinate(-122.215750055), -122.2157501);
        assert_eq!(round_nav_coordinate(-0.00000001), 0.0);
        assert_eq!(
            nav_lat_lon_json(47.49313888888889, -122.215750055),
            serde_json::json!({
                "lat": 47.4931389,
                "lon": -122.2157501,
            })
        );
    }

    #[test]
    fn waypoint_search_index_emits_identifier_city_and_airport_name_terms() {
        let connection = rusqlite::Connection::open_in_memory().expect("sqlite");
        connection
            .execute_batch(
                r#"
                CREATE TABLE airports (
                    LocationID TEXT,
                    City TEXT,
                    State TEXT,
                    FacilityName TEXT,
                    ARPLatitude REAL,
                    ARPLongitude REAL
                );
                CREATE TABLE nav (
                    LocationID TEXT,
                    FacilityName TEXT,
                    ARPLatitude REAL,
                    ARPLongitude REAL,
                    Type TEXT
                );
                CREATE TABLE fix (
                    LocationID TEXT,
                    FacilityName TEXT,
                    ARPLatitude REAL,
                    ARPLongitude REAL
                );
                INSERT INTO airports VALUES
                    ('KPAE', 'EVERETT', 'WA', 'SEATTLE PAINE FLD INTL', 47.9063, -122.2816),
                    ('KSAN', 'SAN DIEGO', 'CA', 'SAN DIEGO INTL', 32.7336, -117.1897),
                    ('KORD', 'CHICAGO', 'IL', 'CHICAGO O''HARE INTL', 41.9786, -87.9048);
                "#,
            )
            .expect("schema");

        let pairs = build_nav_kv_waypoint_lookup_pairs(&connection).expect("waypoint pairs");
        let search_records = pairs
            .iter()
            .filter(|pair| pair.key.starts_with("waypoint/search-prefix/"))
            .flat_map(|pair| {
                serde_json::from_slice::<Vec<WaypointSearchRecord>>(&pair.value)
                    .expect("search prefix records")
            })
            .collect::<Vec<_>>();
        let contains =
            |identifier: &str, matched_term: &str, match_kind: WaypointSearchMatchKind| {
                search_records.iter().any(|record| {
                    record.identifier == identifier
                        && record.matched_term == matched_term
                        && record.match_kind == match_kind
                })
            };

        assert!(contains(
            "KPAE",
            "KPAE",
            WaypointSearchMatchKind::Identifier
        ));
        assert!(contains(
            "KPAE",
            "EVERETT",
            WaypointSearchMatchKind::AirportName
        ));
        assert!(contains(
            "KPAE",
            "PAINE",
            WaypointSearchMatchKind::AirportName
        ));
        assert!(contains(
            "KPAE",
            "SEATTLE",
            WaypointSearchMatchKind::AirportName
        ));
        assert!(contains(
            "KSAN",
            "DIEGO",
            WaypointSearchMatchKind::AirportName
        ));
        assert!(contains(
            "KORD",
            "OHARE",
            WaypointSearchMatchKind::AirportName
        ));
    }

    #[test]
    fn waypoint_search_index_omits_terms_over_the_candidate_limit() {
        let candidates = (0..=WAYPOINT_SEARCH_MAX_RESULTS)
            .map(|index| WaypointSearchRecord {
                identifier: format!("K{index:04}"),
                kind: "airport".to_string(),
                display_name: "San".to_string(),
                lat: 0.0,
                lon: 0.0,
                matched_term: "SAN".to_string(),
                match_kind: WaypointSearchMatchKind::AirportName,
            })
            .collect::<Vec<_>>();

        let pairs =
            build_sparse_waypoint_search_prefix_pairs(&candidates).expect("search prefix pairs");

        assert!(pairs.is_empty());
    }

    #[test]
    fn plate_and_csup_asset_records_omit_duplicate_and_empty_fields() {
        let plate = preprocessor_resource_index::PlateRecord {
            id: "plate-id".to_string(),
            airport_id: "KRNT".to_string(),
            icao_airport_id: None,
            region_id: "NW".to_string(),
            package_id: "NW_TPP_TPP1_2606_01".to_string(),
            asset_path: "plates/RNT/APD-WA-AIRPORT DIAGRAM.png".to_string(),
            thumbnail_path: String::new(),
            label: "Airport Diagram".to_string(),
            asset_kind: "plate".to_string(),
            document_type: "airport_diagram".to_string(),
            procedure_uid: None,
            georef: None,
        };
        let value = nav_kv_plate_asset("KRNT", &plate);
        assert_eq!(
            value,
            serde_json::json!({
                "id": "plate:KRNT:APD-WA-AIRPORT DIAGRAM.png",
                "airport_id": "KRNT",
                "collection_id": "airport:KRNT",
                "package_id": "NW_TPP_TPP1_2606_01",
                "label": "Airport Diagram",
                "kind": "plate",
                "folder_category": "airport-diagram",
                "asset_path": "plates/RNT/APD-WA-AIRPORT DIAGRAM.png",
            })
        );
        assert!(value.get("source_asset_path").is_none());
        assert!(value.get("thumbnail_source_path").is_none());
        assert!(value.get("thumbnail_path").is_none());
        assert!(value.get("georef").is_none());

        let csup = preprocessor_resource_index::CsupRecord {
            id: "csup-id".to_string(),
            airport_id: "KRNT".to_string(),
            region_id: "NW".to_string(),
            package_id: "NW_CSUP_CSUP1_2606_01".to_string(),
            asset_path: "afd/01A/CSUP-WA_0.png".to_string(),
            thumbnail_path: "afd/01A/CSUP-WA_0_thumb.png".to_string(),
            label: "Chart Supplement".to_string(),
            asset_kind: "csup".to_string(),
            document_type: "csup".to_string(),
        };
        let value = nav_kv_csup_asset("KRNT", &csup);
        assert_eq!(
            value,
            serde_json::json!({
                "id": "csup:KRNT:CSUP-WA_0.png",
                "airport_id": "KRNT",
                "collection_id": "airport:KRNT",
                "package_id": "NW_CSUP_CSUP1_2606_01",
                "label": "Chart Supplement",
                "kind": "csup",
                "folder_category": "csup",
                "asset_path": "afd/01A/CSUP-WA_0.png",
                "thumbnail_path": "afd/01A/CSUP-WA_0_thumb.png",
            })
        );
        assert!(value.get("source_asset_path").is_none());
        assert!(value.get("thumbnail_source_path").is_none());
        assert!(value.get("georef").is_none());
    }

    #[test]
    fn airport_navref_pairs_emit_dense_metar_importance_record() {
        let connection = rusqlite::Connection::open_in_memory().expect("sqlite");
        connection
            .execute_batch(
                r#"
                CREATE TABLE airports (
                    LocationID TEXT,
                    ARPLatitude REAL,
                    ARPLongitude REAL,
                    FacilityName TEXT,
                    Type TEXT,
                    ATCT TEXT,
                    FuelTypes TEXT,
                    ARPElevation TEXT,
                    State TEXT,
                    MagneticVariation TEXT
                );
                CREATE TABLE airportrunways (
                    LocationID TEXT,
                    Length TEXT,
                    Surface TEXT,
                    LEHeadingT TEXT,
                    LELatitude TEXT,
                    LELongitude TEXT,
                    HELatitude TEXT,
                    HELongitude TEXT,
                    LEIdent TEXT,
                    HEHeading TEXT,
                    HEIdent TEXT
                );
                CREATE TABLE awos (
                    LocationID TEXT,
                    Status TEXT
                );
                INSERT INTO airports VALUES
                    ('kaaa', 1.0, 2.0, 'A Airport', 'AIRPORT', 'Y', '', '100', 'WA', ''),
                    ('KBBB', 3.0, 4.0, 'B Airport', 'AIRPORT', 'N', '', '200', 'WA', ''),
                    (' kccc ', 5.0, 6.0, 'C Airport', 'AIRPORT', ' y ', '', '300', 'WA', ''),
                    ('1S5', 7.0, 8.0, 'Sunnyside', 'AIRPORT', 'N', '', '400', 'WA', ''),
                    ('XYZ', 9.0, 10.0, 'Alias Candidate', 'AIRPORT', 'N', '', '500', 'WA', ''),
                    ('KXYZ', 11.0, 12.0, 'Real ICAO', 'AIRPORT', 'N', '', '600', 'WA', ''),
                    ('H01', 13.0, 14.0, 'Hawaii Candidate', 'AIRPORT', 'N', '', '700', 'HI', '');
                INSERT INTO awos VALUES
                    ('1S5', 'Y'),
                    ('XYZ', 'Y'),
                    ('H01', 'Y');
                "#,
            )
            .expect("schema");

        let pairs = build_nav_kv_airport_navref_pairs(&connection).expect("pairs");
        let importance_pair = pairs
            .iter()
            .find(|pair| pair.key == "weather/metar-important-stations")
            .expect("importance pair");
        let value: serde_json::Value =
            serde_json::from_slice(&importance_pair.value).expect("importance json");

        assert_eq!(
            value,
            serde_json::json!({
                "schema_version": 1,
                "station_ids": ["KAAA", "KCCC"],
            })
        );
        let aliases_pair = pairs
            .iter()
            .find(|pair| pair.key == "weather/station-airport-aliases")
            .expect("weather station airport aliases pair");
        let aliases: serde_json::Value =
            serde_json::from_slice(&aliases_pair.value).expect("aliases json");
        assert_eq!(
            aliases,
            serde_json::json!({
                "schema_version": 1,
                "aliases": {
                    "K1S5": {
                        "airport_id": "1S5",
                        "position": {
                            "lat": 7.0,
                            "lon": 8.0,
                        },
                    },
                },
            })
        );
    }

    #[test]
    fn airport_info_pairs_are_self_contained_and_timezone_aware() {
        let connection = rusqlite::Connection::open_in_memory().expect("sqlite");
        connection
            .execute_batch(
                r#"
                CREATE TABLE airports (
                    LocationID TEXT,
                    FacilityName TEXT,
                    ARPLatitude REAL,
                    ARPLongitude REAL,
                    ARPElevation TEXT,
                    TrafficPatternAltitude TEXT,
                    UNICOMFrequencies TEXT,
                    CTAFFrequency TEXT,
                    MagneticVariation TEXT
                );
                CREATE TABLE airportfreq (LocationID TEXT, Type TEXT, Freq TEXT);
                CREATE TABLE airportcontacts (LocationID TEXT, Type TEXT, Phone TEXT);
                CREATE TABLE awos (
                    LocationID TEXT,
                    Type TEXT,
                    Status TEXT,
                    Frequency1 TEXT,
                    Frequency2 TEXT,
                    Telephone1 TEXT,
                    Telephone2 TEXT
                );
                CREATE TABLE airportrunways (
                    LocationID TEXT,
                    Length TEXT,
                    Width TEXT,
                    Surface TEXT,
                    LEIdent TEXT,
                    HEIdent TEXT,
                    LEHeadingT TEXT,
                    HEHeading TEXT,
                    LEPattern TEXT,
                    HEPattern TEXT,
                    LELatitude TEXT,
                    LELongitude TEXT,
                    HELatitude TEXT,
                    HELongitude TEXT
                );
                INSERT INTO airports VALUES
                    ('KRNT', 'Renton Municipal', 47.493, -122.216, '32', '1218', '122.95', '124.7', '15E'),
                    ('S88', 'Skykomish State', 47.711, -121.339, '1002', '', '', '122.9', '20E'),
                    ('KUOS', 'Franklin County', 35.2051458, -85.8981472, '1953.3', '', '122.8', '122.8', '01W');
                INSERT INTO airportfreq VALUES
                    ('KRNT', 'ATIS', '126.95'),
                    ('KRNT', 'GND/P', '121.6'),
                    ('KRNT', 'APCH/P DEP/P', '119.2 ;017-079 SEA RWY 34'),
                    ('KRNT', 'APCH/P DEP/P', '119.2 ;028-160 SEA RWY 16');
                INSERT INTO airportcontacts VALUES ('KRNT', 'ATIS', '425-555-1212');
                INSERT INTO awos VALUES
                    ('KRNT', 'ASOS', 'Y', '126.95', '', '425-255-6080', '');
                INSERT INTO airportrunways VALUES
                    ('KRNT', '3200', '35', 'TURF-F', '08', '26', '084', '264', 'Y', 'N',
                     '47.4930', '-122.2240', '47.4930', '-122.2080'),
                    ('KRNT', '5382', '200', 'ASPH-CONC-G', '16', '34', '174', '354', 'N', 'Y',
                     '47.5000', '-122.2160', '47.4860', '-122.2160'),
                    ('KRNT', '0', '0', '', '10X', '', '', '', 'N', 'N', '', '', '', ''),
                    ('S88', '2050', '100', 'TURF-G', '06', '24', '', '', 'N', 'N', '', '', '', ''),
                    ('KUOS', '3700', '50', 'ASPH-G', '07', '25', '', '', 'N', 'N',
                     '', '', '35.2071111', '-85.8931722');
                "#,
            )
            .expect("schema");

        let pairs = build_nav_kv_airport_info_pairs(&connection).expect("airport info pairs");
        let pair = pairs
            .iter()
            .find(|pair| pair.key == "airport/info/KRNT")
            .expect("KRNT airport info");
        let value: serde_json::Value =
            serde_json::from_slice(&pair.value).expect("airport info json");

        assert_eq!(value["time_zone"], "America/Los_Angeles");
        assert_eq!(value["traffic_pattern_altitude_msl_ft"], 1218.0);
        assert_eq!(value["runways"][0]["length_ft"], 5382.0);
        assert_eq!(value["runways"][0]["end_a"]["latitude"], 47.5);
        assert_eq!(value["runways"][0]["end_b"]["longitude"], -122.216);
        assert_eq!(value["runways"][0]["end_b"]["right_pattern"], true);
        assert_eq!(value["runways"].as_array().expect("runways").len(), 2);
        let approach = value["communications"]
            .as_array()
            .expect("communications")
            .iter()
            .find(|entry| entry["label"] == "Approach/Departure")
            .expect("approach communications");
        assert_eq!(approach["frequency"], "119.2");
        assert!(value["contacts"]
            .as_array()
            .expect("contacts")
            .iter()
            .any(|entry| entry["phone"] == "425-555-1212"));

        let s88 = pairs
            .iter()
            .find(|pair| pair.key == "airport/info/S88")
            .expect("S88 airport info");
        let s88: serde_json::Value =
            serde_json::from_slice(&s88.value).expect("S88 airport info json");
        assert_eq!(s88["runways"][0]["end_a"]["heading_true_deg"], 80.0);
        assert_eq!(s88["runways"][0]["end_b"]["heading_true_deg"], 260.0);

        let symbols =
            airport_runway_symbol_info_by_airport(&connection).expect("airport runway symbols");
        assert_eq!(
            symbols
                .get("S88")
                .expect("S88 runway symbol")
                .heading_true_deg,
            80.0
        );

        let kuos = pairs
            .iter()
            .find(|pair| pair.key == "airport/info/KUOS")
            .expect("KUOS airport info");
        let kuos: serde_json::Value =
            serde_json::from_slice(&kuos.value).expect("KUOS airport info json");
        let airport_info_heading = kuos["runways"][0]["end_a"]["heading_true_deg"]
            .as_f64()
            .expect("KUOS airport-info heading");
        let nav_symbol_heading = symbols
            .get("KUOS")
            .expect("KUOS NAV symbol")
            .heading_true_deg;
        let vector_symbol_heading = preprocessor_vectors::load_airport_runway_info(&connection)
            .expect("vector runway symbols")
            .get("KUOS")
            .expect("KUOS vector symbol")
            .heading_true_deg;
        assert_eq!(airport_info_heading, 69.0);
        assert_eq!(nav_symbol_heading, airport_info_heading);
        assert_eq!(vector_symbol_heading, airport_info_heading);
    }

    #[test]
    fn navref_symbol_pairs_emit_current_symbol_wire_shape() {
        let connection = rusqlite::Connection::open_in_memory().expect("sqlite");
        connection
            .execute_batch(
                r#"
                CREATE TABLE airports (
                    LocationID TEXT,
                    ARPLatitude REAL,
                    ARPLongitude REAL,
                    FacilityName TEXT,
                    Type TEXT,
                    ATCT TEXT,
                    FuelTypes TEXT,
                    ARPElevation TEXT,
                    State TEXT,
                    MagneticVariation TEXT
                );
                CREATE TABLE airportrunways (
                    LocationID TEXT,
                    Length TEXT,
                    Surface TEXT,
                    LEHeadingT TEXT,
                    LELatitude TEXT,
                    LELongitude TEXT,
                    HELatitude TEXT,
                    HELongitude TEXT,
                    LEIdent TEXT,
                    HEHeading TEXT,
                    HEIdent TEXT
                );
                CREATE TABLE awos (
                    LocationID TEXT,
                    Status TEXT
                );
                CREATE TABLE nav (
                    LocationID TEXT,
                    ARPLatitude REAL,
                    ARPLongitude REAL,
                    FacilityName TEXT,
                    Type TEXT
                );
                CREATE TABLE fix (
                    LocationID TEXT,
                    ARPLatitude REAL,
                    ARPLongitude REAL,
                    FacilityName TEXT,
                    Type TEXT
                );
                INSERT INTO airports VALUES
                    ('KRNT', 47.493, -122.216, 'Renton Municipal', 'AIRPORT', 'Y', '100LL', '32', 'WA', '15E');
                INSERT INTO nav VALUES
                    ('SEA', 47.435, -122.310, 'Seattle', 'VORTAC');
                INSERT INTO fix VALUES
                    ('EPH', 47.374, -119.424, 'EPH', 'FIX');
                "#,
            )
            .expect("schema");

        let mut pairs = Vec::new();
        pairs.extend(build_nav_kv_airport_navref_pairs(&connection).expect("airport pairs"));
        pairs.extend(build_nav_kv_navaid_navref_pairs(&connection).expect("navaid pairs"));
        pairs.extend(build_nav_kv_fix_navref_pairs(&connection).expect("fix pairs"));

        for (key, symbol_kind) in [
            ("navref/symbol/airport/KRNT", "airport"),
            ("navref/symbol/navaid/SEA", "nav"),
            ("navref/symbol/fix/EPH", "fix"),
        ] {
            let pair = pairs
                .iter()
                .find(|pair| pair.key == key)
                .unwrap_or_else(|| panic!("missing {key}"));
            let value: serde_json::Value =
                serde_json::from_slice(&pair.value).expect("symbol json");
            assert_eq!(
                value.get("symbol_kind").and_then(|value| value.as_str()),
                Some(symbol_kind),
                "{key}"
            );
            assert!(value.get("kind").is_some(), "{key}");
            assert!(value.get("label").is_some(), "{key}");
            assert!(value.get("style_class").is_some(), "{key}");
        }
        let airport_symbol = pairs
            .iter()
            .find(|pair| pair.key == "navref/symbol/airport/KRNT")
            .expect("airport symbol");
        let airport_symbol: serde_json::Value =
            serde_json::from_slice(&airport_symbol.value).expect("airport symbol json");
        assert_eq!(airport_symbol["label"], "KRNT");
    }
}
