export type ChartFamilyId =
  | "sectional"
  | "tac"
  | "wac"
  | "ifr_low"
  | "ifr_high"
  | "ifr_area"
  | "flyway"
  | "heli"
  | "misc";

export type RegionId = "ne" | "nc" | "nw" | "se" | "sc" | "sw" | "ec" | "ak" | "pac";

export type ContentPolicy = "OfflineRequired" | "PreferLocal" | "StreamAllowed";

export type ContentAvailability = "LocalOnly" | "RemoteOnly" | "LocalAndRemote" | "Unavailable";
export type TileStorageKind = "asset_tree" | "sectional_package";

export type FlightPlan = {
  id: string;
  name: string;
  legs: Array<{
    from: { Airport: string } | { Navaid: string } | { Fix: string };
    to: { Airport: string } | { Navaid: string } | { Fix: string };
    airway: string | null;
  }>;
  departure: string | null;
  destination: string | null;
  alternate: string | null;
  cruise_altitude_ft: number | null;
  notes: string | null;
  updated_at_epoch_ms: number;
  version: number;
};

export type CatalogJson = {
  schema_version: number;
  cycle: string;
  catalog_revision: string;
  families: Array<{
    id: ChartFamilyId;
    display_name: string;
    kind: string;
    max_zoom: number | null;
    tile_size: number | null;
  }>;
  regions: Array<{
    id: RegionId;
    display_name: string;
    sort_order: number;
  }>;
  packages: Array<{
    id: {
      region: RegionId;
      family: ChartFamilyId;
      cycle: string;
    };
    package_name: string;
    family_id: ChartFamilyId;
    region_id: RegionId;
    cycle: string;
    artifact_kind: string;
    relative_url: string;
    manifest_name: string;
    size_bytes: number | null;
    checksum_sha256: string | null;
  }>;
  charts: Array<{
    id: {
      family: ChartFamilyId;
      name: string;
      cycle: string;
    };
    family_id: ChartFamilyId;
    name: string;
    display_name: string;
    cycle: string;
    region_ids: RegionId[];
    max_zoom: number;
    tile_path_template: string;
    coverage: unknown;
  }>;
  plates: Array<{
    id: {
      airport_id: string;
      procedure_code: string;
      page: number;
      cycle: string;
    };
    airport_id: string;
    region_id: RegionId;
    cycle: string;
    procedure_code: string;
    display_name: string;
    kind: string;
    georeferenced: boolean;
    page_count: number;
    asset_base_path: string;
  }>;
  supplements: unknown[];
};

export type ContentInventory = {
  installed_packages: Array<{
    package_id: {
      region: RegionId;
      family: ChartFamilyId;
      cycle: string;
    };
    integrity_ok: boolean;
  }>;
  cached_tilesets: Array<unknown>;
  cached_plates: Array<unknown>;
};

export type AppState = {
  active_plan: FlightPlan | null;
  situation: Situation;
  content_policy: ContentPolicy;
  last_content_requirements: Array<{
    package_ids: Array<{
      region: RegionId;
      family: ChartFamilyId;
      cycle: string;
    }>;
    chart_ids: unknown[];
    plate_ids: unknown[];
  }>;
  last_content_report: {
    fully_satisfied: boolean;
    items: Array<{
      label: string;
      availability: {
        availability: ContentAvailability;
        cycle_current: boolean;
        integrity_ok: boolean;
        cached: boolean;
        offline_usable: boolean;
      };
    }>;
  } | null;
};

export type Situation = {
  position: SituationPosition;
  orientation_deg: number | null;
  speed_kt: number | null;
};

export type SituationPosition =
  | { kind: "unknown" }
  | { kind: "lat_lon"; lat: number; lon: number }
  | { kind: "flight_plan_location"; leg_index: number; lat: number; lon: number };

export type ContentFixtureBundle = {
  catalog: CatalogJson;
  geometry: {
    schema_version: number;
    polygons: Array<{
      id: string;
      points: number[][];
    }>;
  };
  initial_probe: {
    family: ChartFamilyId;
    lat: number;
    lon: number;
  };
  map_view: {
    chart_family: ChartFamilyId;
    chart_name: string;
    chart_index: number;
    tile_root: string;
    tile_url_root: string;
    tile_size: number;
    min_zoom: number;
    max_zoom: number;
    storage_kind: TileStorageKind;
    package_name: string | null;
    initial_viewport: {
      lat: number;
      lon: number;
      zoom: number;
    };
    levels: Array<{
      zoom: number;
      x_min: number;
      x_max: number;
      y_tms_min: number;
      y_tms_max: number;
    }>;
  };
  map_views?: Array<{
    id: string;
    label: string;
    region_id: RegionId;
    map_view: ContentFixtureBundle["map_view"];
  }>;
  map_tile_view: {
    chart_family: ChartFamilyId;
    chart_name: string;
    chart_index: number;
    tile_root: string;
    zoom: number;
    tile_size: number;
    radius: number;
    center_x: number;
    center_y_tms: number;
    probe_offset_x: number;
    probe_offset_y: number;
  };
  flight_plan: FlightPlan;
  remote_only_inventory: ContentInventory;
  installed_inventory: ContentInventory;
};

export type ChartPageData = {
  airports: Array<{
    id: string;
    label: string;
    charts: Array<{
      id: string;
      airport_id: string;
      package_id: string;
      label: string;
      kind: string;
      folder_category: string;
      source_asset_path: string;
      asset_path: string;
      asset_url: string;
      thumbnail_source_path: string | null;
      thumbnail_path: string | null;
      thumbnail_url: string | null;
    }>;
  }>;
};

export type ResourceIndexJson = {
  schema_version: number;
  cycle: string | null;
  generated_at_utc: string;
  families: Array<{
    id: ChartFamilyId | "tpp" | "csup";
    display_name: string;
    kind: string;
  }>;
  regions: Array<{
    id: RegionId;
    display_name: string;
    sort_order: number;
  }>;
  packages: Array<{
    id: string;
    family_id: ChartFamilyId | "tpp" | "csup";
    region_id: RegionId;
    artifact_path: string;
    size_bytes: number;
    checksum_sha256: string;
  }>;
  chart_collections: Array<{
    id: string;
    family_id: ChartFamilyId;
    region_id: RegionId;
    package_id: string;
    chart_index: number;
    tile_path_template: string;
    levels: Array<{
      zoom: number;
      x_min: number;
      x_max: number;
      y_tms_min: number;
      y_tms_max: number;
    }>;
    coverage_bounds: {
      lat_min: number;
      lat_max: number;
      lon_min: number;
      lon_max: number;
    };
    default_view: {
      lat: number;
      lon: number;
      zoom: number;
    };
  }>;
  airports: Array<{
    id: string;
    facility_name: string;
    lat: number;
    lon: number;
    airport_type: string;
  }>;
  airport_resources: Array<{
    airport_id: string;
    plate_ids: string[];
    csup_ids: string[];
    package_ids: string[];
  }>;
  plates: Array<{
    id: string;
    airport_id: string;
    region_id: RegionId;
    package_id: string;
    asset_path: string;
    thumbnail_path?: string | null;
    label: string;
    asset_kind: string;
  }>;
  csups: Array<{
    id: string;
    airport_id: string;
    region_id: RegionId;
    package_id: string;
    asset_path: string;
    thumbnail_path?: string | null;
    label: string;
    asset_kind: string;
  }>;
};
