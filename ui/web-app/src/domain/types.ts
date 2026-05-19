export type ChartFamilyId =
  | "sec"
  | "tac"
  | "wac"
  | "enr-l"
  | "enr-h"
  | "enr-a"
  | "flyway"
  | "heli"
  | "shaded-relief"
  | "world-basemap"
  | "misc";

export type RegionId = "ne" | "nc" | "nw" | "se" | "sc" | "sw" | "ec" | "ak" | "pac";

export type ContentPolicy = "OfflineRequired" | "PreferLocal" | "StreamAllowed";

export type ContentAvailability = "LocalOnly" | "RemoteOnly" | "LocalAndRemote" | "Unavailable";
export type TileStorageKind = "asset_tree" | "sectional_package" | "static_product";

export type FlightPlan = {
  id: string;
  name: string;
  legs: Array<{
    from: { Airport: string } | { Navaid: string } | { Fix: string };
    to: { Airport: string } | { Navaid: string } | { Fix: string };
    airway: string | null;
  }>;
  route_components: RouteComponent[];
  route_component_uids: string[];
  route_component_uid_counter: number;
  resolved_legs: ResolvedLeg[];
  guidance: GuidanceState | null;
  departure: string | null;
  destination: string | null;
  alternate: string | null;
  cruise_altitude_ft: number | null;
  notes: string | null;
  updated_at_epoch_ms: number;
  version: number;
};

export type NavRef =
  | { Airport: string }
  | { Navaid: string }
  | { Fix: string }
  | { ArincNavaid: { identifier: string; icao_code: string; section_code: string; subsection_code: string } }
  | { TerminalNavaid: { airport_id: string; identifier: string; icao_code: string; section_code: string; subsection_code: string } }
  | { LatLon: { lat: number; lon: number } }
  | { Spot: { lat: number; lon: number } };

export type LatLon = {
  lat: number;
  lon: number;
};

export type PlateGeoref =
  | {
      kind: "plate_transform_v1";
      pixels_per_longitude: number;
      pixels_per_latitude: number;
      top_left_lon: number;
      top_left_lat: number;
    }
  | {
      kind: "airport_diagram_transform_v1";
      pixel_x_from_lon: number;
      pixel_x_from_lat: number;
      pixel_x_offset: number;
      pixel_y_from_lon: number;
      pixel_y_from_lat: number;
      pixel_y_offset: number;
    };

export type PlanLeg = {
  from: NavRef;
  to: NavRef;
  airway: string | null;
};

export type RouteComponent =
  | { kind: "waypoint"; waypoint: NavRef }
  | { kind: "airway"; airway: AirwaySegment }
  | { kind: "procedure"; procedure: ProcedureSegment };

export type AirwaySegment = {
  name: string;
  branch_key?: string | null;
  entry: NavRef;
  exit: NavRef;
};

export type AirwaySuggestion = {
  airway_name: string;
  nearest_branch_key: string | null;
  nearest_nav_ref: NavRef;
  nearest_sequence: number;
  distance_from_anchor_nm: number;
};

export type WaypointIdentifierSuggestion = {
  identifier: string;
  nav_ref: NavRef;
  kind: string;
  display_name: string;
  distance_from_anchor_nm: number;
  distance_text: string;
  symbol_feature?: NavSymbolFeature | null;
};

export type AirwayEntryCandidate = {
  airway_name: string;
  branch_key: string;
  branch_point_index: number;
  sequence: number;
  nav_ref: NavRef;
  distance_from_anchor_nm: number;
  previous_nav_ref: NavRef | null;
  next_nav_ref: NavRef | null;
};

export type AirwayExitCandidate = {
  airway_name: string;
  branch_key: string;
  branch_point_index: number;
  sequence: number;
  nav_ref: NavRef;
  leg_offset_from_entry: number;
  is_entry: boolean;
  distance_from_target_nm: number | null;
};

export type AirwayAutoSelection = {
  airway_name: string;
  branch_key: string;
  entry: AirwayEntryCandidate;
  exit: AirwayExitCandidate;
  origin_distance_nm: number;
  destination_distance_nm: number;
  total_anchor_distance_nm: number;
};

export type AirwayFixPoint = {
  airway_name: string;
  sequence: number;
  position: LatLon;
  nav_ref: NavRef;
};

export type AirwayBranch = {
  display_name: string;
  branch_key: string;
  points: AirwayFixPoint[];
};

export type AirwayPresentationPoint = {
  branch_point_index: number;
  sequence: number;
  nav_ref: NavRef;
};

export type AirwayPresentationPlan = {
  airway_name: string;
  branch_key: string;
  points: AirwayPresentationPoint[];
  suggested_entry_index: number;
  suggested_exit_index: number | null;
};

export type ProcedureKind = "sid" | "star" | "approach";

export type ProcedureDiscontinuity = "vectors" | "hold" | string;

export type ProcedureSegment = {
  airport_id: string;
  procedure_id: string;
  kind: ProcedureKind;
  runway_transition: string | null;
  enroute_transition: string | null;
  terminal_discontinuity?: ProcedureDiscontinuity | null;
  data_quality?: string[];
};

export type ProcedureSummary = {
  airport_id: string;
  procedure_id: string;
  kind: ProcedureKind;
};

export type CifpTppMatchRow = {
  airport_id: string;
  cifp_id: string;
  plate_id: string;
  plate_label: string;
  package_id: string;
  public: number;
  priority: number;
  match_kind: string;
  is_primary: number;
};

export type CifpTppMatch = {
  airport_id: string;
  cifp_id: string;
  plate_id: string;
  plate_label: string;
  package_id: string;
  match_kind: string;
  is_primary: boolean;
};

export type ProcedureDistinctRow = {
  route_type: string;
  transition_id: string;
};

export type ProcedureSpecChoice = {
  runway_transition: string | null;
  enroute_transition: string | null;
};

export type ProcedureOptions = {
  airport_id: string;
  procedure_id: string;
  kind: ProcedureKind;
  runway_transitions: string[];
  enroute_transitions: string[];
  has_common_segment: boolean;
  valid_choices: ProcedureSpecChoice[];
};

export type MaterializedProcedure = {
  procedure: ProcedureSegment;
  concretized_items: ConcretizedNavItem[];
  resolved_legs: ResolvedLeg[];
};

export type ProcedureLegMaterializationRecord = {
  key: {
    airport_id: string;
    procedure_id: string;
    route_type: string;
    transition_id: string;
  };
  sequence: number;
  nav_ref: NavRef | null;
  path_termination: string;
};

export type ResolvedLeg =
  {
    id: string;
    from: NavRef;
    to: NavRef;
    procedure_airport_id?: string | null;
    procedure_provenance?: ProcedureLegProvenance | null;
  } & (
  | { source: { kind: "legacy_plan_leg"; leg_index: number } }
    | { source: { kind: "route_component"; component_index: number } }
    | { source: { kind: "synthetic_bridge"; from_component_index: number; to_component_index: number } }
  );

export type ProcedureLegProvenance = {
  airport_id: string;
  procedure_id: string;
  kind: ProcedureKind;
  role: "enroute_transition" | "common" | "runway_transition";
  path_termination: string | { other: string };
  leg_sequence: number;
  display_path?: LegDisplayPath | null;
};

export type LegDisplayPath = {
  style?: "solid" | "dashed";
  elements: LegDisplayElement[];
  effective_terminal_course_deg?: number | null;
};

export type LegDisplayElement =
  | { segment: { start: LatLon; end: LatLon } }
  | {
      arc: {
        center: LatLon;
        radius_nm: number;
        start: LatLon;
        end: LatLon;
        clockwise: boolean;
        sweep_degrees: number;
      };
    };

export type FlightPlanRouteSegment = {
  id: string;
  leg_id: string;
  from: LatLon;
  to: LatLon;
  path: LatLon[];
  style: "solid" | "dashed";
  distance_nm: number;
  course_deg: number;
  status: "completed" | "active" | "active_leg_remaining" | "remaining";
  finish_lines?: { start: LatLon; end: LatLon }[];
};

export type SequencingMode = "follow_plan" | "suspended" | "direct_to";

export type DirectToState = {
  start: NavRef;
  target: NavRef;
  target_component_uid: string | null;
  target_leg_id: string | null;
  resume_leg_id: string | null;
};

export type GuidanceState = {
  active_leg_index: number;
  active_detail_index?: number | null;
  sequencing_mode: SequencingMode;
  direct_to: DirectToState | null;
};

export type ConcretizedNavItem =
  | { kind: "waypoint"; nav_ref: NavRef }
  | { kind: "discontinuity"; discontinuity: ProcedureDiscontinuity; label: string };

export type RouteComponentViewKind = "waypoint" | "airway" | "procedure";

export type RouteComponentUiView = {
  uid: string;
  component_index: number;
  kind: RouteComponentViewKind;
  summary: string;
  procedure_id: string | null;
  procedure_kind: ProcedureKind | null;
  chart_airport_id: string | null;
  nav_ref: NavRef | null;
  items: ConcretizedNavItem[];
  active: boolean;
  can_add_airway_after: boolean;
  can_add_procedure_before: boolean;
  can_remove: boolean;
  can_reorder: boolean;
  can_reorder_up: boolean;
  can_reorder_down: boolean;
  replace_procedure_component_index: number | null;
  preceding_waypoint: NavRef | null;
  following_waypoint: NavRef | null;
};

export type ResolvedLegUiView = {
  leg_index: number;
  leg_id: string;
  component_index: number | null;
  from: NavRef;
  to: NavRef;
  active: boolean;
  suspend_boundary_after: boolean;
  display_path?: LegDisplayPath | null;
};

export type DirectToUiView = {
  start: NavRef;
  target: NavRef;
  target_component_uid: string | null;
  target_leg_id: string | null;
  resume_leg_id: string | null;
  on_plan_target: boolean;
};

export type GuidanceUiView = {
  sequencing_mode: SequencingMode;
  active_leg_index: number | null;
  display_split_leg_index: number | null;
  active_from_row_uid: string | null;
  active_to_row_uid: string | null;
  active_component_index: number | null;
  active_leg: PlanLeg | null;
  nav_element: NavElementUiView;
  direct_to: DirectToUiView | null;
  can_sequence_active_leg: boolean;
  can_activate_next_leg: boolean;
  can_suspend: boolean;
  can_unsuspend: boolean;
  can_restore_direct_to?: boolean;
  suspend_boundary_after_active_leg: boolean;
};

export type NavElementUiView = {
  active_leg_summary: string;
  cdi_indicator_dots: number | null;
  cdi_offscale_readout: string | null;
};

export type FlightPlanUiState = {
  components: RouteComponentUiView[];
  resolved_legs: ResolvedLegUiView[];
  data_columns: FlightDataColumn[];
  display_rows: FlightPlanDisplayRowUiView[];
  guidance: GuidanceUiView | null;
};

export type FlightPlanDisplayRowKind = "waypoint" | "group" | "discontinuity";

export type FlightPlanRowActionUiView = {
  id: string;
  uid: string;
  label: string;
  enabled: boolean;
  execution?: "ui_controller" | "core_session";
  dismiss_tray_on_success?: boolean;
};

export type FlightPlanDisplayRowUiView = {
  uid: string;
  label: string;
  row_kind: FlightPlanDisplayRowKind;
  component_kind: RouteComponentViewKind | null;
  component_uid: string | null;
  component_index: number | null;
  procedure_id: string | null;
  procedure_kind: ProcedureKind | null;
  leg_index: number | null;
  data_cells: FlightDataCell[];
  show_plate_target_id: string | null;
  chart_airport_id: string | null;
  nav_ref: NavRef | null;
  symbol_feature: NavSymbolFeature | null;
  depth: number;
  active: boolean;
  enabled?: boolean;
  synthetic_direct_to?: boolean;
  can_add_airway_after: boolean;
  can_add_procedure_before: boolean;
  can_remove_component: boolean;
  can_reorder_component: boolean;
  can_reorder_up: boolean;
  can_reorder_down: boolean;
  replace_procedure_component_index: number | null;
  start_component_index: number | null;
  end_component_index: number | null;
  origin_anchor: NavRef | null;
  destination_anchor: NavRef | null;
  preceding_waypoint: NavRef | null;
  following_waypoint: NavRef | null;
  action_matrix?: FlightPlanRowActionUiView[][];
};

export type NavSymbolFeature = {
  kind: string;
  label: string;
  style_class: string;
  obstacle_variant?: "short" | "tall" | null;
  towered: boolean;
  fuel_available: boolean;
  has_paved_runway?: boolean | null;
  heliport?: boolean | null;
  has_water_runway?: boolean | null;
  runway_length_ratio: number;
  longest_runway_heading_true_deg: number | null;
};

export type PlateProcedureLoadCandidateInput = {
  airport_id: string;
  cifp_id: string;
  match_rows: CifpTppMatchRow[];
  distinct_rows: ProcedureDistinctRow[];
};

export type ProcedureLoadOption = {
  load_id: string;
  label: string;
};

export type FlightPlanUiMutation = {
  plan: FlightPlan;
  ui_state: FlightPlanUiState;
};

export type FlightPlanEntryTokenState = "neutral" | "recognized" | "invalid";

export type FlightPlanEntryToken = {
  start: number;
  end: number;
  state: FlightPlanEntryTokenState;
};

export type FlightPlanEntryIssue = {
  start: number;
  end: number;
  message: string;
};

export type FlightPlanEntryPreview = {
  can_commit: boolean;
  tokens: FlightPlanEntryToken[];
  issues: FlightPlanEntryIssue[];
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
  ownship: OwnshipState;
  content_policy: ContentPolicy;
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

export type UiSnapshotAppState = {
  active_plan: FlightPlan | null;
  content_policy: ContentPolicy;
  last_content_report: AppState["last_content_report"];
};

export type PlaybackStatus = "empty" | "paused" | "playing";

export type PlaybackUiState = {
  status: PlaybackStatus;
  source_path: string | null;
  title_label: string;
  registration: string | null;
  icao: string | null;
  aircraft_type: string | null;
  point_count: number;
  duration_seconds: number;
  cursor_seconds: number;
  cursor_label: string;
  duration_label: string;
  rate: number;
  speed_profile_norm: Array<number | null>;
  altitude_profile_norm: Array<number | null>;
  gap_spans: Array<{
    start_seconds: number;
    end_seconds: number;
  }>;
};

export type MapFollowUiState = {
  can_center_here: boolean;
  following: boolean;
};

export type SituationPosition =
  | { kind: "none" }
  | { kind: "lat_lon"; lat: number; lon: number };

export type Situation = {
  position: SituationPosition;
  orientation_deg: number | null;
  speed_kt: number | null;
};

export type AppUiState = {
  active_plan: FlightPlanUiState | null;
  ownship: OwnshipUiState;
  flight_data_banner: FlightDataBannerModel;
  content_policy: ContentPolicy;
  last_content_report: AppState["last_content_report"];
};

export type FlightDataCell = {
  id: string;
  label: string;
  value: string | null;
};

export type FlightDataColumn = {
  id: string;
  label: string;
};

export type FlightDataBannerModel = {
  cells: FlightDataCell[];
};

export type OwnshipMode = "none" | "live" | "replay" | "simulated";

export type OwnshipBannerSeverity = "info" | "caution" | "warning";

export type OwnshipControlTone = "ready" | "unavailable" | "neutral";

export type SituationControlInput = "skip_backward" | "fast_rewind" | "fast_forward" | "skip_forward";

export type OwnshipSourceKind =
  | "device_gps"
  | "external_gps"
  | "external_ahrs"
  | "gpx_playback"
  | "adsb_track_playback"
  | "live_network_track"
  | "flight_plan_simulator"
  | "debug_ownship_driver";

export type SourceConnectionState = "unavailable" | "searching" | "connected" | "stale" | "failed";

export type OwnshipSelectionCommand =
  | { kind: "auto" }
  | { kind: "source"; source_id: { 0: string } | string };

export type SituationSample = {
  source_id: { 0: string } | string;
  source_kind: OwnshipSourceKind;
  event_time_epoch_ms: number;
  received_time_epoch_ms: number;
  position: LatLon | null;
  horizontal_accuracy_m?: number | null;
  vertical_accuracy_m?: number | null;
  track_deg_true: number | null;
  heading_deg_true: number | null;
  ground_speed_kt: number | null;
  altitude_msl_ft: number | null;
  pressure_altitude_ft: number | null;
};

export type OwnshipSourceRegistration = {
  source_id: { 0: string } | string;
  source_kind: OwnshipSourceKind;
  display_name: string;
  selectable: boolean;
  auto_eligible: boolean;
};

export type OwnshipSourceStatusUpdate = {
  source_id: { 0: string } | string;
  connection_state: SourceConnectionState;
  enabled: boolean;
  status_label: string;
};

export type OwnshipRenderState = {
  mode: OwnshipMode;
  banner_text: string;
  banner_severity: OwnshipBannerSeverity;
  draw_aircraft: boolean;
  draw_predictor: boolean;
  draw_cdi: boolean;
  position: LatLon | null;
  orientation_deg: number | null;
  magnetic_variation_deg: number | null;
  speed_kt: number | null;
  altitude_msl_ft: number | null;
  pressure_altitude_ft: number | null;
};

export type OwnshipControlModel = {
  mode: OwnshipMode;
  selection?: OwnshipSelectionCommand;
  policy?: OwnshipSelectionCommand;
  launcher_label: string;
  launcher_tone: OwnshipControlTone;
  sources: Array<{
    source_id: { 0: string } | string;
    source_kind: OwnshipSourceKind;
    label: string;
    launcher_label: string;
    tone: OwnshipControlTone;
    enabled: boolean;
    active: boolean;
    status_label: string;
  }>;
  situation_controls: Array<{
    input: SituationControlInput;
    label: string;
    enabled: boolean;
  }>;
};

export type OwnshipUiState = {
  render: OwnshipRenderState;
  controls: OwnshipControlModel;
};

export type OwnshipState = {
  policy: {
    selection: OwnshipSelectionCommand;
    source_priority: Array<{ 0: string } | string>;
    allow_auto_replay: boolean;
    allow_auto_simulated: boolean;
  };
  resolved: {
    mode: OwnshipMode;
    active_source_id: { 0: string } | string | null;
    active_source_kind: OwnshipSourceKind | null;
    banner_text: string;
    banner_severity: OwnshipBannerSeverity;
    guidance_enabled: boolean;
    sequencing_enabled: boolean;
  };
  render: OwnshipRenderState;
  controls: OwnshipControlModel;
  sources: Array<unknown>;
};

export type MapViewJson = {
  chart_family: ChartFamilyId;
  chart_name: string;
  chart_index: number;
  tile_root: string;
  tile_url_root: string;
  tile_path_template: string;
  tile_size: number;
  min_zoom: number;
  max_zoom: number;
  max_source_zoom: number;
  max_display_zoom: number;
  storage_kind: TileStorageKind;
  package_name: string | null;
  full_coverage_zoom?: number | null;
  wide_angle?: {
    region_id: string;
    max_zoom: number;
    package_name: string;
    tile_url_root: string;
    tile_path_template: string;
    levels: Array<{
      zoom: number;
      boxes: Array<{
        x_min: number;
        x_max: number;
        y_tms_min: number;
        y_tms_max: number;
      }>;
    }>;
  } | null;
  initial_viewport: {
    lat: number;
    lon: number;
    zoom: number;
  };
  levels: Array<{
    zoom: number;
    boxes: Array<{
      x_min: number;
      x_max: number;
      y_tms_min: number;
      y_tms_max: number;
    }>;
  }>;
};

export type MapViewOptionJson = {
  id: string;
  label: string;
  region_id: RegionId;
  map_view: MapViewJson;
};

export type SituationRingCandidate = {
  radius_nm: number;
  label: string;
};

export type MapTileViewJson = {
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

export type GeometryJson = {
  schema_version: number;
  polygons: Array<{
    id: string;
    points: number[][];
  }>;
  polygon_sets?: Array<{
    id: string;
    polygon_ids: string[];
  }>;
};

export type DevBootstrapJson = {
  content_policy: ContentPolicy;
  recent_airport_ids: string[];
  selected_airport_id: string | null;
  selected_chart_id: string | null;
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
      thumbnail_source_path: string | null;
      thumbnail_path: string | null;
      georef: PlateGeoref | null;
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
      boxes: Array<{
        x_min: number;
        x_max: number;
        y_tms_min: number;
        y_tms_max: number;
      }>;
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
    document_type: string;
    georef?: PlateGeoref | null;
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
    document_type: string;
  }>;
};
