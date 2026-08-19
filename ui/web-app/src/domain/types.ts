// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import type {
  NavSymbolFeature,
} from "../generated/navQueryWire";

export type {
  NavSymbolFeature,
  WaypointIdentifierSuggestion,
  WaypointSuggestionNavRef,
} from "../generated/navQueryWire";
import type { FlightPlanControlUiView } from "../generated/sessionPageWire";
export type {
  FlightPlanControlId,
  FlightPlanControlUiView,
} from "../generated/sessionPageWire";

export type ChartFamilyId =
  | "none"
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

export type AirwaySuggestion = {
  airway_name: string;
  nearest_branch_key: string | null;
  nearest_nav_ref: NavRef;
  nearest_sequence: number;
  distance_from_anchor_nm: number;
};

export type AirwayPresentationPoint = {
  uid: string;
  sequence: number;
  nav_ref: NavRef;
};

export type AirwayPresentationPlan = {
  airway_name: string;
  branch_key: string;
  points: AirwayPresentationPoint[];
  suggested_entry_uid: string;
  suggested_exit_uid: string | null;
};

export type ProcedureKind = "sid" | "star" | "approach";

export type ProcedureDiscontinuity = "vectors" | "hold" | string;

export type ProcedureSummary = {
  airport_id: string;
  procedure_id: string;
  display_label: string;
  kind: ProcedureKind;
  accent_category: string;
  enabled: boolean;
  disabled_reason?: string | null;
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

export type FlightPlanRouteSegment = {
  id: string;
  leg_id: string;
  from: LatLon;
  to: LatLon;
  path: LatLon[];
  style: "solid" | "dashed" | "vectors";
  distance_nm: number;
  course_deg: number;
  status: "completed" | "active" | "active_leg_remaining" | "remaining";
  finish_lines?: { start: LatLon; end: LatLon }[];
};

export type FlightPlanRouteDistanceAnnotation = {
  id: string;
  segment_indexes: number[];
  text: string;
  distance_nm: number;
  status: FlightPlanRouteSegment["status"];
  required_feature_ids: string[];
  minimum_path_to_pill_width_ratio: number;
};

export type FlightPlanRouteProjection = {
  flight_plan_route_revision: number;
  segments: FlightPlanRouteSegment[];
  distance_annotations: FlightPlanRouteDistanceAnnotation[];
};

export type SequencingMode = "follow_plan" | "suspended" | "direct_to";

export type RouteComponentViewKind = "waypoint" | "airway" | "procedure";

export type DirectToUiView = {
  start: NavRef;
  target: NavRef;
  target_row_id: string;
  on_plan_target: boolean;
};

export type GuidanceUiView = {
  sequencing_mode: SequencingMode;
  active_from_row_uid: string | null;
  active_to_row_uid: string | null;
  active_leg: PlanLeg | null;
  nav_element: NavElementUiView;
  direct_to: DirectToUiView | null;
  suspend_boundary_after_active_leg: boolean;
};

export type NavElementUiView = {
  active_leg_summary: string;
  cdi_indicator_dots: number | null;
  cdi_offscale_readout: string | null;
};

export type FlightPlanUiState = {
  plan_id: string;
  plan_version: number;
  data_columns: FlightDataColumn[];
  display_rows: FlightPlanDisplayRowUiView[];
  controls: FlightPlanControlUiView[];
  altitude_planner: AltitudePlannerUiView;
  guidance: GuidanceUiView | null;
};

export type AltitudePlannerControlUiView = {
  id: "aircraft" | "aircraft_profile" | "wind_model";
  label: string;
  enabled: boolean;
  action_uid?: string | null;
  disabled_reason?: string | null;
  options?: AltitudePlannerControlOptionUiView[];
};

export type AltitudePlannerControlOptionUiView = {
  label: string;
  action_uid: string;
  selected: boolean;
};

export type AltitudePlannerDepartureEditorUiView = {
  title: string;
  time_label: string;
  time_value: string;
  basis_label: string;
  time_display_action_id: string;
  when_label: string;
  when_value: string;
  when_suffix: string;
  when_is_past: boolean;
  enabled: boolean;
  disabled_reason?: string | null;
};

export type AltitudePlannerUnavailableReason = {
  code:
    | "aircraft_profile_unavailable"
    | "cruise_altitude_unavailable"
    | "plan_origin_altitude_unavailable"
    | "plan_destination_altitude_unavailable"
    | "ownship_altitude_unavailable"
    | "wind_model_unavailable"
    | "performance_regime_unavailable";
  message: string;
};

export type AltitudeComparisonUiView = {
  action_uid?: string | null;
  selected: boolean;
  enabled: boolean;
  disabled_reason?: string | null;
  advisory?: string | null;
  cells: FlightDataCell[];
};

export type AltitudeComparisonPanelUiView = {
  columns: FlightDataColumn[];
  rows: AltitudeComparisonUiView[];
  advisories?: string[];
};

export type AltitudePlannerUiView = {
  title: string;
  estimate_kind: FlightEstimateKind;
  estimate_summary: {
    label: string;
    estimate_kind: FlightEstimateKind;
  };
  controls: AltitudePlannerControlUiView[];
  departure: AltitudePlannerDepartureEditorUiView;
  forecast?: { summary: string } | null;
  advisories?: string[];
  unavailable_reasons?: AltitudePlannerUnavailableReason[];
};

export type FlightPlanDisplayRowKind = "waypoint" | "group" | "discontinuity" | "summary";

export type WeatherDetailUiView = {
  station_id: string;
  advisory_text: string;
  metar_text?: string | null;
  metar_age_label?: string | null;
  metar_age_warning?: boolean | null;
  taf_text?: string | null;
  taf_age_label?: string | null;
  taf_age_warning?: boolean | null;
  notams?: AirportNotamUiView[];
};

export type AirportNotamUiView = {
  id: string;
  label: string;
  text: string;
};

export type FlightPlanRowActionUiView = {
  id: string;
  uid: string;
  menu_column?: number;
  label: string;
  enabled: boolean;
  disabled_reason?: string | null;
};

export type FlightPlanDisplayRowUiView = {
  uid: string;
  label: string;
  row_kind: FlightPlanDisplayRowKind;
  component_kind: RouteComponentViewKind | null;
  component_uid: string | null;
  procedure_id: string | null;
  procedure_kind: ProcedureKind | null;
  data_cells: FlightDataCell[];
  show_plate_target_id: string | null;
  chart_airport_id: string | null;
  nav_ref: NavRef | null;
  symbol_feature: NavSymbolFeature | null;
  weather_badge?: FlightPlanWeatherBadgeUiView | null;
  depth: number;
  active: boolean;
  enabled?: boolean;
  disabled_reason?: string | null;
  synthetic_direct_to?: boolean;
  can_add_airway_after: boolean;
  can_add_procedure_before: boolean;
  can_remove_component: boolean;
  can_reorder_component: boolean;
  can_reorder_up: boolean;
  can_reorder_down: boolean;
  origin_anchor: NavRef | null;
  destination_anchor: NavRef | null;
  preceding_waypoint: NavRef | null;
  following_waypoint: NavRef | null;
  action_matrix?: FlightPlanRowActionUiView[][];
};

export type FlightPlanWeatherBadgeUiView = {
  flight_category: string;
  ceiling_amount: string;
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

export type ProcedureLoadMenu = {
  procedure_kind: ProcedureKind | null;
  launcher_label: string;
  header: string;
  header_tone: "normal" | "destructive";
  options: ProcedureLoadOption[];
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

export type ContentReport = {
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
  tick_interval_ms: number;
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
  disabled_reason?: string | null;
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
  aircraft_plan_view_path: string;
  ownship: OwnshipUiState;
  flight_data_banner: FlightDataBannerModel;
  content_policy: ContentPolicy;
  last_content_report: ContentReport | null;
};

export type {
  FlightDataBannerModel,
  FlightDataCell,
  FlightDataColumn,
  FlightEstimateKind,
} from "../generated/sessionPageWire";
import type {
  FlightDataBannerModel,
  FlightDataCell,
  FlightDataColumn,
  FlightEstimateKind,
} from "../generated/sessionPageWire";

export type OwnshipMode = "none" | "live" | "replay" | "simulated";

export type OwnshipBannerSeverity = "info" | "caution" | "warning";

export type OwnshipControlTone = "ready" | "unavailable" | "neutral";
export type OwnshipLauncherTextTone = "normal" | "unavailable";

export type SituationControlInput =
  | "skip_backward"
  | "fast_rewind"
  | "fast_forward"
  | "skip_forward"
  | "pause"
  | "resume";

export type OwnshipSourcePowerState = "running" | "paused" | "sleeping";

export type OwnshipSourceKind =
  | "device_gps"
  | "external_gps"
  | "external_ahrs"
  | "gpx_playback"
  | "adsb_track_playback"
  | "live_network_track"
  | "flight_plan_simulator"
  | "bad_autopilot";

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
  vertical_speed_fpm?: number | null;
};

export type OwnshipSourceRegistration = {
  source_id: { 0: string } | string;
  source_kind: OwnshipSourceKind;
  display_name: string;
  selectable: boolean;
  auto_eligible: boolean;
  power_state?: OwnshipSourcePowerState | null;
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
  track_deg_true: number | null;
  orientation_deg: number | null;
  magnetic_variation_deg: number | null;
  speed_kt: number | null;
  altitude_msl_ft: number | null;
  pressure_altitude_ft: number | null;
  terrain_altitude_bucket_ft: number | null;
};

export type OwnshipControlModel = {
  mode: OwnshipMode;
  selection?: OwnshipSelectionCommand;
  policy?: OwnshipSelectionCommand;
  launcher_label: string;
  launcher_tone: OwnshipControlTone;
  launcher_text_tone: OwnshipLauncherTextTone;
  sources: Array<{
    source_id: { 0: string } | string;
    source_kind: OwnshipSourceKind;
    label: string;
    launcher_label: string;
    tone: OwnshipControlTone;
    enabled: boolean;
    disabled_reason?: string | null;
    active: boolean;
    status_label: string;
    power_state?: OwnshipSourcePowerState | null;
    keep_tray_open_on_select?: boolean;
  }>;
  situation_controls: Array<{
    input: SituationControlInput;
    label: string;
    enabled: boolean;
    disabled_reason?: string | null;
  }>;
  text_action?: {
    action_id: string;
    label: string;
    value: string;
    placeholder: string;
    submit_label: string;
    enabled: boolean;
    disabled_reason?: string | null;
  } | null;
  next_refresh_epoch_ms?: number | null;
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
      airport_id?: string | null;
      collection_id?: string;
      label: string;
      kind: string;
      folder_category: string;
      has_thumbnail: boolean;
      procedure_geometry_warning_count: number;
      procedure_notam_badge?: {
        label: string;
        count: number;
        action_id: string;
        accessibility_label: string;
        detail: {
          title: string;
          advisory_text: string;
          notams: AirportNotamUiView[];
        };
      } | null;
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
    label: string;
    asset_kind: string;
    document_type: string;
    has_thumbnail?: boolean;
    georef?: PlateGeoref | null;
  }>;
  csups: Array<{
    id: string;
    airport_id: string;
    region_id: RegionId;
    label: string;
    asset_kind: string;
    document_type: string;
    has_thumbnail?: boolean;
  }>;
};
