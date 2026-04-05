use app_core::{
    AirportId, CatalogBundle, CatalogFamily, CatalogPackage, CatalogRegion, ChartCoverage,
    ChartFamilyId, ChartId, ChartRecord, PackageId, PlateId, PlateRecord, RegionId,
};

pub fn sample_catalog() -> CatalogBundle {
    CatalogBundle {
        schema_version: 1,
        cycle: "2026-04-16".to_string(),
        catalog_revision: "2026-04-05T22:00:00Z".to_string(),
        families: vec![CatalogFamily {
            id: ChartFamilyId::Sectional,
            display_name: "VFR Sectional Charts".to_string(),
            kind: "tiled_raster".to_string(),
            max_zoom: Some(10),
            tile_size: Some(512),
        }],
        regions: vec![CatalogRegion {
            id: RegionId::Ne,
            display_name: "Northeast".to_string(),
            sort_order: 0,
        }],
        packages: vec![CatalogPackage {
            id: PackageId {
                region: RegionId::Ne,
                family: ChartFamilyId::Sectional,
                cycle: "2026-04-16".to_string(),
            },
            package_name: "NE_SEC".to_string(),
            family_id: ChartFamilyId::Sectional,
            region_id: RegionId::Ne,
            cycle: "2026-04-16".to_string(),
            artifact_kind: "zip".to_string(),
            relative_url: "/2026-04-16/NE_SEC.zip".to_string(),
            manifest_name: "NE_SEC".to_string(),
            size_bytes: None,
            checksum_sha256: None,
        }],
        charts: vec![ChartRecord {
            id: ChartId {
                family: ChartFamilyId::Sectional,
                name: "Boston".to_string(),
                cycle: "2026-04-16".to_string(),
            },
            family_id: ChartFamilyId::Sectional,
            name: "Boston".to_string(),
            display_name: "Boston".to_string(),
            cycle: "2026-04-16".to_string(),
            region_ids: vec![RegionId::Ne],
            max_zoom: 10,
            tile_path_template: "tiles/{chart_index}/{z}/{x}/{y}".to_string(),
            coverage: ChartCoverage::PolygonRef {
                polygon_id: "sectional:boston".to_string(),
            },
        }],
        plates: vec![PlateRecord {
            id: PlateId {
                airport_id: AirportId("KBOS".to_string()),
                procedure_code: "IAP-ILS-RWY-04R".to_string(),
                page: 1,
                cycle: "2026-04-16".to_string(),
            },
            airport_id: AirportId("KBOS".to_string()),
            region_id: RegionId::Ne,
            cycle: "2026-04-16".to_string(),
            procedure_code: "IAP-ILS-RWY-04R".to_string(),
            display_name: "ILS OR LOC RWY 04R".to_string(),
            kind: "approach".to_string(),
            georeferenced: true,
            page_count: 1,
            asset_base_path: "plates/KBOS/IAP-ILS-RWY-04R".to_string(),
        }],
        supplements: Vec::new(),
    }
}
