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
  charts: unknown[];
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

export type ContentFixtureBundle = {
  catalog: CatalogJson;
  flight_plan: FlightPlan;
  remote_only_inventory: ContentInventory;
  installed_inventory: ContentInventory;
};
