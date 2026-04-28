import type { CatalogJson } from "./types";

export const sampleCatalogFixture: CatalogJson = {
  schema_version: 1,
  cycle: "2604",
  catalog_revision: "local-fixture",
  families: [
    { id: "sec", display_name: "Sectional", kind: "chart", max_zoom: 12.5, tile_size: 512 },
    { id: "tac", display_name: "Terminal Area", kind: "chart", max_zoom: 12.5, tile_size: 512 },
    { id: "enr-l", display_name: "IFR Low", kind: "chart", max_zoom: 12.5, tile_size: 512 },
    { id: "enr-h", display_name: "IFR High", kind: "chart", max_zoom: 12.5, tile_size: 512 },
  ],
  regions: [
    { id: "nw", display_name: "Northwest", sort_order: 1 },
  ],
  packages: [
    {
      id: { region: "nw", family: "sec", cycle: "2604" },
      package_name: "SEC_NW_2604",
      family_id: "sec",
      region_id: "nw",
      cycle: "2604",
      artifact_kind: "package",
      relative_url: "sec_nw_2604_01_fixture.zip",
      manifest_name: "SEC_NW_2604.manifest",
      size_bytes: 1,
      checksum_sha256: null,
    },
  ],
  charts: [
    {
      id: { family: "sec", name: "nw", cycle: "2604" },
      family_id: "sec",
      name: "nw",
      display_name: "Northwest Sectional",
      cycle: "2604",
      region_ids: ["nw"],
      max_zoom: 12.5,
      tile_path_template: "tiles/{z}/{x}/{y}.webp",
      coverage: null,
    },
  ],
  plates: [],
  supplements: [],
};
