// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { afterEach, describe, expect, it, vi } from "vitest";
import { coreViewportForMap, createLiveFeedSubscription, loadBestAvailableAdapter, resolveLiveFeedResourceUrl, resolveLiveFeedSourceUrl, UI_SESSION_PAGE_CONTRACTS_WIRE_VERSION } from "./appCoreAdapter";
import * as navKv from "./navKv";

const TEST_SSE_TRANSPORT_POLICY = {
  heartbeat_interval_ms: 30_000,
  connect_timeout_ms: 5_000,
  idle_timeout_ms: 65_000,
  reconnect_initial_delay_ms: 5_000,
  reconnect_max_delay_ms: 65_000,
};

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

const snapshotJson = JSON.stringify({
  ui_contract_version: UI_SESSION_PAGE_CONTRACTS_WIRE_VERSION,
  session_revision: 0,
  app_ui_state: {
    active_plan: null,
    aircraft_plan_view_path: "",
    ownship: {
      render: {
        mode: "none",
        banner_text: "NO GPS POSITION",
        banner_severity: "warning",
        draw_aircraft: false,
        draw_predictor: false,
        draw_cdi: false,
        position: null,
        track_deg_true: null,
        orientation_deg: null,
        speed_kt: null,
      },
      controls: {
        mode: "none",
        selection: { kind: "auto" },
        launcher_label: "No GPS",
        launcher_tone: "unavailable",
        launcher_text_tone: "unavailable",
        sources: [],
        situation_controls: [],
      },
    },
    flight_data_banner: { cells: [] },
    content_policy: "PreferLocal",
    last_content_report: null,
  },
  chart_page_state: {
    ordered_airport_ids: [],
    recent_airport_ids: [],
    plate_target_airport_id: null,
    selected_airport_id: "",
    selected_chart_id: "",
  },
  map_layer_state: {
    options: [],
    world_basemap: { visible: true, enabled: true },
    vectors: { visible: true, enabled: true },
    metars: { visible: true, enabled: true },
    nexrad: { visible: false, enabled: true },
    traffic: { visible: false, enabled: true },
    terrain_warning: { visible: true, enabled: true },
    offline_regions: { visible: false, enabled: true },
  },
  data_status_state: {
    boxes: [],
    launcher_count: null,
    launcher_severity: "info",
  },
  data_status_page_state: {
    title: "Status",
    summary: "All tracked systems are usable.",
    rows: [],
  },
  settings_page_state: {
    title: "Settings",
    summary: "No platform settings are available.",
    rows: [],
    sections: [],
  },
  home_page_state: {
    buttons: [],
  },
  display_policy: null,
  disclaimer_state: {
    agreement_id: "no-warranty-v1",
    required: true,
    html: "<p><strong>NO WARRANTY</strong>: test</p>",
    text: "NO WARRANTY: test",
    accept_label: "I understand and agree",
  },
});

describe("loadBestAvailableAdapter", () => {
  it("fails loudly when the generated wasm module is missing", async () => {
    await expect(loadBestAvailableAdapter(async () => {
      throw new Error("module not found");
    })).rejects.toThrow("module not found");
  });

  it("establishes the web resource policy before opening the NAV database", async () => {
    vi.stubGlobal("location", { href: "http://app.example.test/" });
    let sessionRevision = 0;
    let resourcePolicyEstablished = false;
    const startupCalls: string[] = [];
    vi.spyOn(navKv, "runCoreHadSessionMutationOperation")
      .mockImplementation(async (_sessionHandle, operation) => {
        startupCalls.push("nav-dependent-mutation");
        if (!resourcePolicyEstablished) {
          throw new Error(
            "web cannot fetch package_member resource nav_db/artifact/0/root; expected public_url",
          );
        }
        const response = JSON.parse(await operation(1)) as { state: string; result?: unknown };
        if (response.state !== "complete" || !Object.hasOwn(response, "result")) {
          throw new Error(`test mutation unexpectedly returned ${response.state}`);
        }
        return { kind: "session_update", update: response.result };
      });
    vi.spyOn(navKv, "attachNavKvStoreToSession")
      .mockImplementation(async () => {
        startupCalls.push("attach-nav-kv");
      });
    const mutationOutcomeJson = () => JSON.stringify({
      state: "complete",
      result: {
        ui_contract_version: UI_SESSION_PAGE_CONTRACTS_WIRE_VERSION,
        session_revision: ++sessionRevision,
      },
    });
    const snapshotOutcomeJson = () => JSON.stringify({
      state: "complete",
      result: {
        ...JSON.parse(snapshotJson),
        session_revision: sessionRevision,
      },
    });
    const loaded = await loadBestAvailableAdapter(async () => ({
      situation_ring_candidates_json: () => "[]",
      create_ui_session: async (_recentAirportIdsJson: string, _selectedAirportIdJson: string, _selectedChartIdJson: string, _nowEpochMs: number) => {
        return JSON.stringify({ handle: 1, snapshot: JSON.parse(snapshotJson) });
      },
      maintain_nav_db_in_session_at_epoch_ms: async () => JSON.stringify({
        state: "complete",
        result: {
          action: "none",
          snapshot: JSON.parse(snapshotJson),
        },
      }),
      perform_map_selection_action_in_session: async () => mutationOutcomeJson(),
      set_situation_in_session_paged: async () => mutationOutcomeJson(),
      tick_bad_autopilot_in_session_paged: async () => mutationOutcomeJson(),
      engage_map_follow_in_session: async () => mutationOutcomeJson(),
      disengage_map_follow_in_session: async () => mutationOutcomeJson(),
      set_map_follow_offset_in_session: async () => mutationOutcomeJson(),
      sync_map_follow_in_session: async () => mutationOutcomeJson(),
      load_playback_trace_in_session_paged: async () => mutationOutcomeJson(),
      play_playback_in_session_paged: async () => mutationOutcomeJson(),
      pause_playback_in_session_paged: async () => mutationOutcomeJson(),
      seek_playback_in_session_paged: async () => mutationOutcomeJson(),
      set_playback_rate_in_session_paged: async () => mutationOutcomeJson(),
      tick_playback_in_session_paged: async () => mutationOutcomeJson(),
      register_ownship_source_in_session_paged: async () => mutationOutcomeJson(),
      update_ownship_source_status_in_session_paged: async () => mutationOutcomeJson(),
      push_situation_sample_in_session_paged: async () => mutationOutcomeJson(),
      select_ownship_source_in_session_paged: async () => mutationOutcomeJson(),
      perform_ownship_text_action_in_session: async () => mutationOutcomeJson(),
      apply_situation_control_input_in_session: async () => mutationOutcomeJson(),
      set_map_layer_visibility_in_session_paged: async () => mutationOutcomeJson(),
      set_map_layer_enabled_in_session_paged: async () => mutationOutcomeJson(),
      set_debug_flag_in_session: async () => mutationOutcomeJson(),
      perform_settings_action_in_session: async () => mutationOutcomeJson(),
      accept_disclaimer_in_session: async () => mutationOutcomeJson(),
      set_resource_policy_in_session: async () => {
        startupCalls.push("set-resource-policy");
        resourcePolicyEstablished = true;
        return mutationOutcomeJson();
      },
      configure_platform_capabilities_in_session: async () => mutationOutcomeJson(),
      take_cloud_authorization_request_in_session: async () => "null",
      complete_cloud_authorization_in_session: async () => mutationOutcomeJson(),
      perform_cloud_ui_action_in_session: async () => mutationOutcomeJson(),
      record_offline_package_preferences_in_session: async () => mutationOutcomeJson(),
      take_cloud_provider_request_in_session: async () => "null",
      complete_cloud_provider_request_in_session: async () => mutationOutcomeJson(),
      cloud_event_stream_plan_in_session: async () => "null",
      report_cloud_event_stream_event_in_session: async () => mutationOutcomeJson(),
      should_prepare_live_feed_resource: () => true,
      load_raster_map_catalog_in_session: async () => mutationOutcomeJson(),
      select_map_family_in_session: async () => mutationOutcomeJson(),
      select_raster_map_in_session: async () => mutationOutcomeJson(),
      perform_flight_plan_command_in_session: async () => mutationOutcomeJson(),
      perform_time_display_action_in_session: async () => mutationOutcomeJson(),
      perform_flight_plan_column_action_in_session: async () => mutationOutcomeJson(),
      query_flight_plan_in_session: async () => JSON.stringify({ state: "complete", result: [] }),
      perform_status_action_in_session: async () => mutationOutcomeJson(),
      sync_guidance_geometry_in_session: async () => mutationOutcomeJson(),
      project_flight_plan_route_in_session: async () => JSON.stringify({
        state: "complete",
        result: { flight_plan_route_revision: 0, segments: [] },
      }),
      select_airport_in_session: async () => mutationOutcomeJson(),
      select_chart_in_session: async () => mutationOutcomeJson(),
      select_chart_reference_in_session: async () => mutationOutcomeJson(),
      ingest_point_tiles_in_session: async () => {},
      ingest_airspace_ref_tiles_in_session: async () => {},
      ingest_airspace_features_in_session: async () => {},
      ingest_airspace_label_tiles_in_session: async () => {},
      ingest_resource_in_session: async () => {},
      report_session_resource_failure_in_session: async () => mutationOutcomeJson(),
      report_session_resource_failure_in_session_at_epoch_ms: async () => mutationOutcomeJson(),
      resolve_chart_asset_resource_in_session: async () => JSON.stringify({ source: { kind: "unavailable", message: "test" } }),
      get_map_overlay_in_session: async () => "{\"state\":\"complete\",\"result\":{\"visible_features\":[],\"visible_metars\":[],\"visible_pireps\":[],\"airspace_paths\":[],\"tfr_paths\":[],\"airspace_labels\":[]}}",
      get_map_selection_in_session: async () => "{\"state\":\"complete\",\"result\":{\"click_lat\":0,\"click_lon\":0,\"categories\":[]}}",
      get_map_selection_distance_in_session: async () => "null",
      get_map_selection_for_nav_ref_in_session: async () => "{\"state\":\"complete\",\"result\":null}",
      get_terrain_overlay_in_session: async () => "{\"needed_terrain_tiles\":[],\"status\":\"hidden\"}",
      get_scheduled_terrain_overlay_in_session: async () => "{\"state\":\"complete\",\"result\":{\"status\":{\"state\":\"hidden\"},\"tile_requests\":[],\"altitude_bucket_ft\":null,\"frame_key\":null,\"schedule\":{\"cached_count\":0,\"in_flight_count\":0,\"missing_count\":0,\"frame_complete\":false,\"work_batch\":[]}}}",
      get_nexrad_overlay_in_session: async () => "{\"state\":\"complete\",\"result\":{\"status\":{\"state\":\"hidden\"},\"tiles\":[],\"stats\":{},\"animation\":{\"phase\":\"idle\",\"selected_frame_index\":null,\"frame_count\":0,\"age_labels\":[],\"age_summary\":\"---\",\"next_update_delay_ms\":null,\"next_update_epoch_ms\":null}}}",
      get_raster_tile_plan_in_session: async () => "{\"background_color\":\"#000000\",\"layers\":[]}",
      get_raster_tile_plan_in_session_with_display_scale: async () => "{\"background_color\":\"#000000\",\"layers\":[]}",
      render_terrain_overlay_tile_by_key_in_session: async () => new Uint8Array(),
      get_session_snapshot_paged: async () => snapshotOutcomeJson(),
      get_session_snapshot_at_epoch_ms_paged: async () => snapshotOutcomeJson(),
      create_session_snapshot_refresh_scheduler: async () => 1,
      destroy_session_snapshot_refresh_scheduler: async () => {},
      session_snapshot_refresh_scheduler_request: async () => JSON.stringify({ kind: "idle" }),
      session_snapshot_refresh_scheduler_viewport_gesture_active_changed: async () => JSON.stringify({ kind: "idle" }),
      session_snapshot_refresh_scheduler_viewport_activity: async () => JSON.stringify({ kind: "idle" }),
      session_snapshot_refresh_scheduler_refresh_completed: async () => JSON.stringify({ kind: "idle" }),
      session_snapshot_refresh_scheduler_poll: async () => JSON.stringify({ kind: "idle" }),
      restore_chart_page_state_in_session: async () => mutationOutcomeJson(),
      destroy_session: () => {},
      install_rust_debug_logger: () => {},
      nav_db_open_controller_create: async () => 1,
      nav_db_open_controller_destroy: async () => {},
      nav_db_open_controller_finish: async () => JSON.stringify({ nav_kv_handle: 1, open_result: { selected_package_id: "NAV_DB_2604", selected_filename: "nav_db.zip", statuses: [] } }),
      nav_db_open_controller_ingest_resource: async () => {},
      nav_db_open_controller_step: async () => JSON.stringify({ state: "complete", result: { selected_package_id: "NAV_DB_2604", selected_filename: "nav_db.zip", statuses: [] } }),
      nav_kv_insert_resource: async () => {},
      nav_kv_prefetch_pages: async () => "[]",
      nav_kv_destroy: async () => {},
      attach_nav_kv_store_to_session: async () => {},
      core_had_operation: async () => JSON.stringify({ state: "complete", result: null }),
      sync_live_feeds_in_session: async () => JSON.stringify({ state: "complete", result: { products: [] } }),
      configure_live_feed_source_in_session: async () => {},
      live_feed_events_url: async (sourceRootUrl: string) => `${sourceRootUrl}/live-feeds/v3/events`,
      live_feed_status_url: async (sourceRootUrl: string) => `${sourceRootUrl}/live-feeds/status.html`,
      refresh_live_feed_current_in_session: async () => JSON.stringify({ state: "complete", result: { products: [] } }),
      live_feed_runtime_decision_in_session: async (_handle: number, inputJson: string) => {
        const input = JSON.parse(inputJson) as { kind: string };
        return JSON.stringify({
          transport_policy: TEST_SSE_TRANSPORT_POLICY,
          connection_event: input.kind === "start" || input.kind === "online" ? null : input,
          refresh_current: input.kind === "connected" || input.kind === "network_status" || input.kind === "online",
          reconnect_delay_ms: input.kind === "error" ? 5000 : null,
        });
      },
      ingest_live_feed_sse_event_in_session: async () => JSON.stringify({ state: "complete", result: { products: [] } }),
      ingest_live_feed_sse_events_in_session: async () => JSON.stringify({ state: "complete", result: { products: [] } }),
      report_live_feed_connection_event_in_session: async () => mutationOutcomeJson(),
    }));

    expect(loaded.backend).toBe("wasm");
    expect(loaded.detail).toContain("Rust WASM");
    const session = await loaded.adapter.createUiSession([]);
    expect(startupCalls[0]).toBe("set-resource-policy");
    expect(startupCalls).toContain("attach-nav-kv");
    expect(startupCalls.indexOf("set-resource-policy"))
      .toBeLessThan(startupCalls.indexOf("nav-dependent-mutation"));
    expect(startupCalls.indexOf("set-resource-policy"))
      .toBeLessThan(startupCalls.indexOf("attach-nav-kv"));
    await session.destroy();
  });
});

describe("coreViewportForMap", () => {
  it("passes web map-up rotation through to core planning", () => {
    expect(coreViewportForMap({
      centerWorldX: 128,
      centerWorldY: 128,
      zoom: 8,
      rotationDeg: -73,
    })).toMatchObject({
      zoom: 8,
      rotation_deg: -73,
      pitch_deg: 0,
    });
  });
});

describe("resolveLiveFeedSourceUrl", () => {
  it("uses the worker global location when window is unavailable", () => {
    expect(resolveLiveFeedSourceUrl(null, {
      location: { origin: "https://aerobag.org" },
    })).toBe("https://aerobag.org");
  });

  it("prefers the configured live-feed origin and trims trailing slashes", () => {
    expect(resolveLiveFeedSourceUrl(" https://feeds.example.test/// ", {
      location: { origin: "https://aerobag.org" },
    })).toBe("https://feeds.example.test");
  });
});

describe("resolveLiveFeedResourceUrl", () => {
  it("resolves core-relative live-feed resources against the configured live-feed origin", () => {
    expect(resolveLiveFeedResourceUrl(
      "/live-feeds/v3/states/nexrad/state-v1/tiles/res0/1/1.png",
      " http://feeds.example.test:18080/// ",
      { location: { origin: "http://app.example.test" } },
    )).toBe("http://feeds.example.test:18080/live-feeds/v3/states/nexrad/state-v1/tiles/res0/1/1.png");
  });

  it("resolves core-relative live-feed resources against same origin when no feed origin is configured", () => {
    expect(resolveLiveFeedResourceUrl(
      "/live-feeds/v3/states/nexrad/state-v1/tiles/res0/1/1.png",
      null,
      { location: { origin: "http://app.example.test" } },
    )).toBe("http://app.example.test/live-feeds/v3/states/nexrad/state-v1/tiles/res0/1/1.png");
  });

  it("leaves absolute and non-live-feed resource URLs unchanged", () => {
    expect(resolveLiveFeedResourceUrl(
      "https://cdn.example.test/live-feeds/v3/states/nexrad/state-v1/tiles/res0/1/1.png",
      "http://feeds.example.test",
      { location: { origin: "http://app.example.test" } },
    )).toBe("https://cdn.example.test/live-feeds/v3/states/nexrad/state-v1/tiles/res0/1/1.png");
    expect(resolveLiveFeedResourceUrl(
      "/packages/cycle/manifest.json",
      "http://feeds.example.test",
      { location: { origin: "http://app.example.test" } },
    )).toBe("/packages/cycle/manifest.json");
  });
});

describe("createLiveFeedSubscription", () => {
  class FakeEventSource {
    static CONNECTING = 0;
    static OPEN = 1;
    static CLOSED = 2;
    static instances: FakeEventSource[] = [];

    readonly url: string;
    readyState = FakeEventSource.CONNECTING;
    onopen: (() => void) | null = null;
    onerror: (() => void) | null = null;
    closed = false;

    constructor(url: string) {
      this.url = url;
      FakeEventSource.instances.push(this);
    }

    addEventListener(_eventName: string, _listener: EventListenerOrEventListenerObject): void {}

    close(): void {
      this.closed = true;
      this.readyState = FakeEventSource.CLOSED;
    }

    emitError(): void {
      this.readyState = FakeEventSource.CONNECTING;
      this.onerror?.();
    }

    markOpen(): void {
      this.readyState = FakeEventSource.OPEN;
    }
  }

  type TestGlobal = {
    window?: unknown;
    EventSource?: unknown;
    __aerobagLiveFeedE2eState?: unknown;
  };

  const originalWindow = (globalThis as unknown as TestGlobal).window;
  const originalEventSource = (globalThis as unknown as TestGlobal).EventSource;

  afterEach(() => {
    vi.useRealTimers();
    FakeEventSource.instances = [];
    const global = globalThis as unknown as TestGlobal;
    if (originalWindow === undefined) {
      delete global.window;
    } else {
      global.window = originalWindow;
    }
    if (originalEventSource === undefined) {
      delete global.EventSource;
    } else {
      global.EventSource = originalEventSource;
    }
    delete global.__aerobagLiveFeedE2eState;
  });

  it("lets an online event interrupt a pending reconnect backoff", async () => {
    vi.useFakeTimers();
    const testWindow = new EventTarget();
    const global = globalThis as unknown as TestGlobal;
    global.window = testWindow;
    global.EventSource = FakeEventSource;
    delete global.__aerobagLiveFeedE2eState;

    const runtimeEvents: string[] = [];
    const subscription = createLiveFeedSubscription(
      () => "https://feeds.example.test/live-feeds/v3/events",
      async (input) => {
        runtimeEvents.push(input.kind);
        return {
          transport_policy: TEST_SSE_TRANSPORT_POLICY,
          reconnect_delay_ms: input.kind === "error" ? 5000 : 0,
          refresh_current: input.kind === "online",
        };
      },
      async () => {},
      () => {},
    );

    await Promise.resolve();
    await Promise.resolve();
    expect(FakeEventSource.instances).toHaveLength(1);

    FakeEventSource.instances[0].emitError();
    await Promise.resolve();
    await Promise.resolve();
    expect(runtimeEvents).toContain("error");

    vi.advanceTimersByTime(1000);
    testWindow.dispatchEvent(new Event("online"));
    await Promise.resolve();
    await Promise.resolve();
    vi.advanceTimersByTime(0);
    await Promise.resolve();
    await Promise.resolve();

    expect(runtimeEvents).toContain("online");
    expect(FakeEventSource.instances).toHaveLength(2);
    expect(FakeEventSource.instances[0].closed).toBe(true);
    subscription.close();
  });

  it("reopens the stream on online even if EventSource still reports open", async () => {
    vi.useFakeTimers();
    const testWindow = new EventTarget();
    const global = globalThis as unknown as TestGlobal;
    global.window = testWindow;
    global.EventSource = FakeEventSource;
    delete global.__aerobagLiveFeedE2eState;

    const subscription = createLiveFeedSubscription(
      () => "https://feeds.example.test/live-feeds/v3/events",
      async (input) => ({
        transport_policy: TEST_SSE_TRANSPORT_POLICY,
        reconnect_delay_ms: input.kind === "online" ? 0 : null,
        refresh_current: input.kind === "online",
      }),
      async () => {},
      () => {},
    );

    await Promise.resolve();
    await Promise.resolve();
    expect(FakeEventSource.instances).toHaveLength(1);
    FakeEventSource.instances[0].markOpen();

    testWindow.dispatchEvent(new Event("online"));
    await Promise.resolve();
    await Promise.resolve();
    vi.advanceTimersByTime(0);
    await Promise.resolve();
    await Promise.resolve();

    expect(FakeEventSource.instances).toHaveLength(2);
    expect(FakeEventSource.instances[0].closed).toBe(true);
    subscription.close();
  });
});
