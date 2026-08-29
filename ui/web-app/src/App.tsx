// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { Fragment, Profiler, createContext, memo, useCallback, useContext, useEffect, useId, useLayoutEffect, useMemo, useRef, useState, useSyncExternalStore, type CSSProperties, type Dispatch, type MouseEvent, type PointerEvent, type ProfilerOnRenderCallback, type ReactNode, type SetStateAction } from "react";
import { createPortal } from "react-dom";
import type {
  AltitudeComparisonPanelUiView,
  AirwayPresentationPlan,
  AirwaySuggestion,
  ChartPageData,
  ChartFamilyId,
  FlightPlanControlId,
  FlightPlanEntryPreview,
  FlightPlanRouteProjection,
  FlightPlanRouteSegment,
  FlightPlanUiState,
  FlightPlanWeatherBadgeUiView,
  FlightDataBannerModel,
  LatLon,
  NavSymbolFeature,
  NavElementUiView,
  NavRef,
  PlaybackUiState,
  MapFollowUiState,
  OwnshipControlModel,
  OwnshipRenderState,
  PlateGeoref,
  ProcedureOptions,
  ProcedureKind,
  ProcedureLoadMenu,
  ProcedureSummary,
  SituationControlInput,
  SituationSample,
  SituationRingCandidate,
  WaypointIdentifierSuggestion,
  WaypointSuggestionNavRef,
  WeatherDetailUiView,
} from "./domain/types";
import uiTheme from "@shared-ui-theme";
import aboutReadmeHtml from "./content/about-readme.html?raw";
import noWarrantyHtml from "@shared/no-warranty.html?raw";
import {
  airportCircleMarkerPath,
  airportFuelMarkerPath,
  airportOpenMarkerSymbol,
  compassSymbol,
  mapFollowActiveSymbol,
  mapFollowInactiveSymbol,
  dataStatusWarningSymbol,
  actionSymbol,
  heliportHPath,
  mapSelectionSpotSymbol,
  manualSequenceChevronPath,
  manualSequenceChevronSpacing,
  metarBknSymbol,
  metarClearSymbol,
  metarFewSymbol,
  metarMissingSymbol,
  metarOvcSymbol,
  metarSctSymbol,
  obstacleDotRadius,
  obstacleShortDotY,
  obstacleShortPath,
  obstacleTallDotY,
  obstacleTallPath,
  pirepGenericSymbol,
  pirepLightIcingSymbol,
  pirepLightTurbulenceSymbol,
  pirepModerateIcingSymbol,
  pirepModerateTurbulenceSymbol,
  pirepSevereIcingSymbol,
  pirepSevereTurbulenceSymbol,
  seaplaneAnchorPath,
  vorBandPath,
  vorOuterHexPath,
  weatherCameraSymbol,
} from "./generated/navSymbols";
import type { UiAircraftSymbol } from "./generated/sessionPageWire";
import {
  loadBestAvailableAdapter,
  resolveLiveFeedResourceUrl,
  type AdapterBackendKind,
  type AppCoreAdapter,
  type AirportInfoUiView,
  type DerivedChartPageState,
  type MapLayerId,
  type MapSelectionItem,
  type MapSelectionQueryResult,
  type RasterMapUiState,
  type RasterTileDraw,
  type RasterTilePlan,
  type SessionSnapshotRefreshDecision,
  type SessionSnapshotRefreshPriority,
  type UiMapLayerState,
  type UiMapLayerToggleState,
  type UiDebugState,
  type UiDisclaimerState,
  type UiPlaybackPanelState,
  type UiDataStatusPageState,
  type UiDataStatusPageRow,
  type UiDataStatusState,
  type UiSurfaceStatusControlId,
  type UiSurfaceStatusState,
  type UiSession,
  type UiSessionSnapshot,
  UI_SESSION_PAGE_CONTRACTS_WIRE_VERSION,
  type CloudPlatformEffect,
  type CloudUiActionId,
  type CloudUiFieldId,
  type CloudUiFieldValue,
  type UiHomeDestination,
  type UiHomePageState,
  type UiInvalidation,
  type UiQrCode,
} from "./domain/appCoreAdapter";
import { executeCloudHttpRequest } from "./domain/cloudProviderRuntime";
import { flightPlanWaypointUsesFullWidthLabel } from "./domain/flightPlanLayout";
import {
  flightPlanHistoryAriaKeyShortcuts,
  flightPlanHistoryControlForKey,
} from "./domain/flightPlanHistoryShortcut";
import {
  applyPinchGesture,
  compassNeedleRotationDegrees,
  committedViewportInvalidatesMapSelection,
  createPinchSnapshot,
  createInitialViewport,
  displayFrameCssTransform,
  dragViewport,
  latLonToWorld,
  preserveViewportForMap,
  resolveMapUpDegrees,
  rotatedViewportEnvelopeSize,
  sameMapViewport,
  scaleForZoom,
  screenToWorld,
  viewportCenterLatLon,
  worldToLatLon,
  worldToScreen,
  zoomAroundPoint,
  type MapDisplayFrame,
  type MapOrientationMode,
  type MapViewportState,
  type ScreenPoint,
} from "./domain/mapViewport";
import {
  FLIGHT_PLAN_ROUTE_DISTANCE_PILL_FONT_PX,
  FLIGHT_PLAN_ROUTE_DISTANCE_PILL_HEIGHT_PX,
  flightPlanRouteSegmentRenderKey,
  layoutFlightPlanRouteDistancePills,
  measureFlightPlanRouteDistancePillWidth,
  spacedRouteChevronPlacements,
} from "./domain/flightPlanRouteRender";
import { resolveSituationOverlay } from "./domain/situationGeometry";
import { plateImagePoint, projectPlateFlightPlanSegments } from "./domain/plateOverlay";
import { MapFollowTargetGate } from "./domain/mapFollowTargetGate";
import { shouldLandCompletedCoalescedWork } from "./domain/coalescedViewportWork";
import { CoalescedAsyncRunner } from "./domain/coalescedAsyncRunner";
import { fetchTextResource } from "./domain/fetchTextResource";
import { NexradFrameImageCache } from "./domain/nexradFrameCache";
import {
  RASTER_TILE_LOAD_RECOVERY_DELAY_MS,
  classifyRasterTileLoadRecovery,
  e2eRasterTileStallUrl,
  rasterTileLoadUrl,
} from "./domain/rasterTileLoadRecovery";
import { appPageForPath, appPageUrl } from "./domain/webRouteUrl";
import {
  clampImageViewport,
  clampImageZoom,
  createInitialImageViewport,
  dragImageViewport,
  imageDisplaySize,
  zoomImageAroundPoint,
  type ImageViewportState,
} from "./domain/imageViewport";
import type {
  AirspaceDisplayPath,
  MapOverlayQueryResult,
  NexradOverlayQueryResult,
  NexradOverlayTile,
  TerrainOverlayQueryResult,
  TerrainOverlayTileRequest,
  VisibleMetarFeature,
  VisiblePirepFeature,
} from "./domain/appCoreAdapter";
import {
  debugLog,
  debugTiming,
  installGlobalErrorLogging,
  perfDebugLog,
  readPersistedDebugLogDeveloperServerUploadEnabled,
  setDebugLogDeveloperServerUploadEnabled,
  VERBOSE_PERF_DEBUG_LOGS,
  writePersistedDebugLogDeveloperServerUploadEnabled,
} from "./domain/debugLog";
import { TerrainOverlayRenderer } from "./domain/terrainOverlayRenderer";
import {
  CLOUD_EFFECT_SESSION_UPDATE_GROUPS,
  HIGH_RATE_SESSION_UPDATE_GROUPS,
  NO_SESSION_UPDATE_GROUPS,
  RenderValueStore,
  SHELL_SESSION_UPDATE_GROUPS,
  SessionRenderStore,
  publicationAffectsGroups,
} from "./domain/sessionRenderStore";
import type { UiSessionUpdateGroup } from "./generated/sessionUpdateWire";

const FLIGHT_PLAN_SESSION_UPDATE_GROUPS = ["flight_plan"] as const satisfies readonly UiSessionUpdateGroup[];

declare const __AEROBAG_E2E_ENABLED__: boolean;

declare global {
  interface Window {
    __aerobagE2e?: {
      liveFeeds?: () => unknown;
      navDb?: () => unknown;
      navDbMaintainAt?: (nowEpochMs: number) => Promise<void>;
      cloud?: {
        state: () => unknown;
        setOfflinePackagePreferences: (preferences: unknown) => Promise<void>;
        dropEventStream: () => Promise<void>;
      };
      render?: () => unknown;
      raster?: () => unknown;
      rasterFaultOnce?: () => number;
    };
  }
}

const defaultDisclaimerText = noWarrantyHtml
  .replace(/<\/p>/g, "")
  .replace(/<p>/g, "")
  .replace(/<\/strong>/g, "")
  .replace(/<strong>/g, "")
  .replace(/\s+/g, " ")
  .trim();

type SurfaceSize = {
  width: number;
  height: number;
};

type WebIdleReason = "active" | "document-hidden" | "inactivity";

type WebIdleState = {
  idle: boolean;
  reason: WebIdleReason;
  lastActivityEpochMs: number;
  enteredIdleEpochMs: number | null;
};

const WebIdleTimeoutMs = 60 * 60 * 1000;

function dataStatusFactValue(row: UiDataStatusPageRow | null | undefined, label: string): string | null {
  return row?.facts.find((fact) => fact.label === label)?.value ?? null;
}

function webE2eLiveFeedStatus(state: UiDataStatusPageState): unknown {
  const rows = state.rows.filter((row) => row.id.startsWith("live_feed:"));
  const rowById = Object.fromEntries(rows.map((row) => [row.id, row]));
  const productVersion = (productId: string) => dataStatusFactValue(rowById[`live_feed:${productId}`], "Version");
  return {
    navigator_online: typeof navigator === "undefined" ? null : navigator.onLine,
    document_visibility: typeof document === "undefined" ? null : document.visibilityState,
    connection: rowById["live_feed:connection"] ?? null,
    product_versions: {
      tfrs: productVersion("tfrs"),
      metars: productVersion("metars"),
      tafs: productVersion("tafs"),
      pireps: productVersion("pireps"),
      nexrad: productVersion("nexrad"),
      obstacles: productVersion("obstacles"),
    },
    rows,
    adapter: (typeof window === "undefined"
      ? (globalThis as typeof globalThis & { __aerobagLiveFeedE2eState?: unknown }).__aerobagLiveFeedE2eState
      : (window as Window & { __aerobagLiveFeedE2eState?: unknown }).__aerobagLiveFeedE2eState) ?? null,
  };
}

function initialWebIdleState(): WebIdleState {
  const now = Date.now();
  if (typeof document !== "undefined" && document.visibilityState !== "visible") {
    return {
      idle: true,
      reason: "document-hidden",
      lastActivityEpochMs: now,
      enteredIdleEpochMs: now,
    };
  }
  return {
    idle: false,
    reason: "active",
    lastActivityEpochMs: now,
    enteredIdleEpochMs: null,
  };
}

function sameWebIdleState(left: WebIdleState, right: WebIdleState): boolean {
  return left.idle === right.idle
    && left.reason === right.reason
    && left.lastActivityEpochMs === right.lastActivityEpochMs
    && left.enteredIdleEpochMs === right.enteredIdleEpochMs;
}

function useWebIdleState(timeoutMs = WebIdleTimeoutMs): WebIdleState {
  const [idleState, setIdleState] = useState<WebIdleState>(initialWebIdleState);
  const lastActivityEpochMsRef = useRef(idleState.lastActivityEpochMs);

  useEffect(() => {
    lastActivityEpochMsRef.current = idleState.lastActivityEpochMs;
  }, [idleState.lastActivityEpochMs]);

  useEffect(() => {
    if (typeof window === "undefined" || typeof document === "undefined") {
      return;
    }

    let idleTimer: number | null = null;

    const clearIdleTimer = () => {
      if (idleTimer === null) {
        return;
      }
      window.clearTimeout(idleTimer);
      idleTimer = null;
    };

    const enterIdle = (reason: Exclude<WebIdleReason, "active">) => {
      clearIdleTimer();
      const now = Date.now();
      const next: WebIdleState = {
        idle: true,
        reason,
        lastActivityEpochMs: lastActivityEpochMsRef.current,
        enteredIdleEpochMs: now,
      };
      setIdleState((current) => sameWebIdleState(current, next) ? current : next);
    };

    const scheduleIdleTimer = () => {
      clearIdleTimer();
      if (document.visibilityState !== "visible") {
        return;
      }
      const remainingMs = Math.max(0, lastActivityEpochMsRef.current + timeoutMs - Date.now());
      idleTimer = window.setTimeout(() => {
        enterIdle("inactivity");
      }, remainingMs);
    };

    const markActive = () => {
      if (document.visibilityState !== "visible") {
        return;
      }
      const now = Date.now();
      lastActivityEpochMsRef.current = now;
      const next: WebIdleState = {
        idle: false,
        reason: "active",
        lastActivityEpochMs: now,
        enteredIdleEpochMs: null,
      };
      setIdleState((current) => current.idle ? next : current);
      scheduleIdleTimer();
    };

    const handleVisibilityChange = () => {
      if (document.visibilityState === "visible") {
        markActive();
      } else {
        enterIdle("document-hidden");
      }
    };
    const handleFocus = () => markActive();
    const handleBlur = () => scheduleIdleTimer();
    const handleActivity = () => markActive();
    const activityOptions: AddEventListenerOptions = { capture: true, passive: true };

    document.addEventListener("visibilitychange", handleVisibilityChange);
    window.addEventListener("focus", handleFocus);
    window.addEventListener("blur", handleBlur);
    window.addEventListener("pointerdown", handleActivity, activityOptions);
    window.addEventListener("keydown", handleActivity, { capture: true });
    window.addEventListener("wheel", handleActivity, activityOptions);
    window.addEventListener("touchstart", handleActivity, activityOptions);

    if (document.visibilityState !== "visible") {
      enterIdle("document-hidden");
    } else {
      markActive();
    }

    return () => {
      clearIdleTimer();
      document.removeEventListener("visibilitychange", handleVisibilityChange);
      window.removeEventListener("focus", handleFocus);
      window.removeEventListener("blur", handleBlur);
      window.removeEventListener("pointerdown", handleActivity, activityOptions);
      window.removeEventListener("keydown", handleActivity, { capture: true });
      window.removeEventListener("wheel", handleActivity, activityOptions);
      window.removeEventListener("touchstart", handleActivity, activityOptions);
    };
  }, [timeoutMs]);

  useEffect(() => {
    debugLog("web.idle.state", {
      idle: idleState.idle,
      reason: idleState.reason,
      last_activity_epoch_ms: idleState.lastActivityEpochMs,
      entered_idle_epoch_ms: idleState.enteredIdleEpochMs,
    });
  }, [idleState]);

  return idleState;
}

type UiInvalidationRevisions = Record<UiInvalidation, number>;

function initialUiInvalidationRevisions(): UiInvalidationRevisions {
  return {
    nav_data: 0,
    session_snapshot: 0,
    raster_tiles: 0,
    map_overlay: 0,
    nexrad_overlay: 0,
    terrain_overlay: 0,
    flight_plan_route: 0,
    debug_panel: 0,
  };
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function finiteOrNull(value: number | null | undefined): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

type NexradPaintTiming = {
  paintedAtMs: number;
  requestId: number;
  phase: string;
  selectedFrameIndex: number | null;
  frameCount: number;
  nextUpdateDelayMs: number | null;
  nextUpdateEpochMs: number | null;
  status: string;
};

function temporaryNexradTimingLog(event: string, data: Record<string, unknown>) {
  debugLog("nexrad.overlay.frame_timing", {
    event,
    now_ms: typeof performance !== "undefined" ? Math.round(performance.now()) : null,
    ...data,
  });
}

function isInvalidUiSessionHandleError(error: unknown): boolean {
  return error instanceof Error && error.message.includes("invalid ui session handle");
}

type StartupProgress = {
  phase: string;
  detail?: string;
  updatedAtMs: number;
};

type StartupFatalError = {
  source: string;
  message: string;
  phase: string;
  detail?: string;
  elapsedMs: number;
  stack?: string;
};

const startupStalledWarningMs = 15_000;
const startupFatalStalledMs = 45_000;
const mainThreadLagProbeIntervalMs = 25;
const mainThreadLagWarnMs = 50;
const animationFrameGapWarnMs = 50;
const dragViewportReactCommitThrottleMs = 90;

function installMainThreadResponsivenessInstrumentation(): () => void {
  if (!VERBOSE_PERF_DEBUG_LOGS || typeof window === "undefined" || typeof performance === "undefined") {
    return () => {};
  }

  let cancelled = false;
  let lagTimer: number | null = null;
  let rafHandle: number | null = null;
  let nextExpectedProbe = performance.now() + mainThreadLagProbeIntervalMs;
  let previousFrameAt = performance.now();

  const longTaskObserver = installLongTaskObserver();

  function probeLag() {
    if (cancelled) {
      return;
    }
    const now = performance.now();
    const lagMs = now - nextExpectedProbe;
    if (lagMs >= mainThreadLagWarnMs) {
      perfDebugLog("main_thread.event_loop_lag", () => ({
        lag_ms: Math.round(lagMs),
        probe_interval_ms: mainThreadLagProbeIntervalMs,
      }));
    }
    nextExpectedProbe = now + mainThreadLagProbeIntervalMs;
    lagTimer = window.setTimeout(probeLag, mainThreadLagProbeIntervalMs);
  }

  function probeFrame(frameAt: number) {
    if (cancelled) {
      return;
    }
    const gapMs = frameAt - previousFrameAt;
    if (gapMs >= animationFrameGapWarnMs) {
      perfDebugLog("main_thread.raf_gap", () => ({
        gap_ms: Math.round(gapMs),
      }));
    }
    previousFrameAt = frameAt;
    rafHandle = window.requestAnimationFrame(probeFrame);
  }

  lagTimer = window.setTimeout(probeLag, mainThreadLagProbeIntervalMs);
  rafHandle = window.requestAnimationFrame(probeFrame);

  return () => {
    cancelled = true;
    if (lagTimer !== null) {
      window.clearTimeout(lagTimer);
    }
    if (rafHandle !== null) {
      window.cancelAnimationFrame(rafHandle);
    }
    longTaskObserver?.disconnect();
  };
}

function installLongTaskObserver(): PerformanceObserver | null {
  if (typeof PerformanceObserver === "undefined") {
    perfDebugLog("main_thread.longtask.support", () => ({ supported: false, reason: "missing_performance_observer" }));
    return null;
  }
  const supportedEntryTypes = PerformanceObserver.supportedEntryTypes ?? [];
  if (!supportedEntryTypes.includes("longtask")) {
    perfDebugLog("main_thread.longtask.support", () => ({ supported: false, reason: "unsupported_entry_type" }));
    return null;
  }
  const observer = new PerformanceObserver((list) => {
    for (const entry of list.getEntries()) {
      perfDebugLog("main_thread.longtask", () => ({
        name: entry.name,
        start_time_ms: Math.round(entry.startTime),
        duration_ms: Math.round(entry.duration),
      }));
    }
  });
  observer.observe({ entryTypes: ["longtask"] });
  perfDebugLog("main_thread.longtask.support", () => ({ supported: true }));
  return observer;
}

function logAfterNextPaint(tag: string, startedAt: number, data: Record<string, unknown>) {
  if (!VERBOSE_PERF_DEBUG_LOGS || typeof window === "undefined" || typeof performance === "undefined") {
    return;
  }
  const landedAt = performance.now();
  window.requestAnimationFrame(() => {
    const firstFrameAt = performance.now();
    perfDebugLog(`${tag}.first_frame`, () => ({
      ...data,
      first_frame_ms: Math.round(firstFrameAt - landedAt),
      elapsed_ms: Math.round(firstFrameAt - startedAt),
    }));
    window.requestAnimationFrame(() => {
      const afterPaintAt = performance.now();
      perfDebugLog(tag, () => ({
        ...data,
        first_frame_ms: Math.round(firstFrameAt - landedAt),
        frame_gap_ms: Math.round(afterPaintAt - firstFrameAt),
        after_paint_ms: Math.round(afterPaintAt - landedAt),
        elapsed_ms: Math.round(afterPaintAt - startedAt),
      }));
    });
  });
}

const reactProfilerActualDurationLogMs = 1;
const reactProfilerCommitDelayLogMs = 8;
const reactProfilerCommitDelayIds = new Set([
  "FlightDataBanner",
  "MapChartControls",
  "MapControls",
  "MapSurface",
  "PrimaryNavigation",
  "RasterLayer",
  "SituationLayer",
  "StatusControls",
  "TerrainLayer",
  "VectorLayer",
  "ZoomControl",
]);

type ReactProfilerTotals = {
  commits: number;
  actualDurationMs: number;
  maxActualDurationMs: number;
};

const webReactProfilerTotals: Record<string, ReactProfilerTotals> = {};

const logReactProfilerRender: ProfilerOnRenderCallback = (
  id,
  phase,
  actualDuration,
  baseDuration,
  startTime,
  commitTime,
) => {
  const totals = webReactProfilerTotals[id] ?? {
    commits: 0,
    actualDurationMs: 0,
    maxActualDurationMs: 0,
  };
  totals.commits += 1;
  totals.actualDurationMs += actualDuration;
  totals.maxActualDurationMs = Math.max(totals.maxActualDurationMs, actualDuration);
  webReactProfilerTotals[id] = totals;
  if (!VERBOSE_PERF_DEBUG_LOGS) {
    return;
  }
  const commitDelayMs = commitTime - startTime;
  const shouldLogActual = phase === "mount" || actualDuration >= reactProfilerActualDurationLogMs;
  const shouldLogCommitDelay = reactProfilerCommitDelayIds.has(id) && commitDelayMs >= reactProfilerCommitDelayLogMs;
  if (
    shouldLogActual
    || shouldLogCommitDelay
  ) {
    perfDebugLog("react.profiler.render", () => ({
      id,
      phase,
      actual_duration_ms: Math.round(actualDuration),
      base_duration_ms: Math.round(baseDuration),
      commit_delay_ms: Math.round(commitDelayMs),
      start_time_ms: Math.round(startTime),
      commit_time_ms: Math.round(commitTime),
    }));
  }
};

function sleepMs(delayMs: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, delayMs));
}

function createLiveDragPerfRunId(): string {
  const suffix = Math.random().toString(36).slice(2, 10);
  return `live-drag-${Date.now().toString(36)}-${suffix}`;
}

function initialStartupProgress(): StartupProgress {
  return {
    phase: "app_shell",
    detail: "Starting web UI",
    updatedAtMs: Date.now(),
  };
}

function startupErrorFromUnknown(
  source: string,
  error: unknown,
  progress: StartupProgress,
  startedAtMs: number,
): StartupFatalError {
  return {
    source,
    message: errorMessage(error),
    phase: progress.phase,
    detail: progress.detail,
    elapsedMs: Math.max(0, Date.now() - startedAtMs),
    stack: error instanceof Error ? error.stack : undefined,
  };
}

function StartupFatalErrorModal({ error }: { error: StartupFatalError }) {
  return (
    <div
      className="startupErrorScrim"
      data-testid="startup-fatal-error"
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="startup-error-title"
    >
      <section className="startupErrorModal">
        <h1 id="startup-error-title">Startup failed</h1>
        <p>{error.message}</p>
        <dl>
          <div>
            <dt>Phase</dt>
            <dd>{error.phase}</dd>
          </div>
          {error.detail ? (
            <div>
              <dt>Detail</dt>
              <dd>{error.detail}</dd>
            </div>
          ) : null}
          <div>
            <dt>Source</dt>
            <dd>{error.source}</dd>
          </div>
          <div>
            <dt>Elapsed</dt>
            <dd>{Math.round(error.elapsedMs / 1000)}s</dd>
          </div>
        </dl>
      </section>
    </div>
  );
}

function DisclaimerModal(props: {
  state: UiDisclaimerState;
  acceptingDisabled: boolean;
  onAccept: () => void;
}) {
  return (
    <div className="disclaimerModalScrim" role="alertdialog" aria-modal="true" aria-labelledby="disclaimer-modal-title">
      <section className="disclaimerModal">
        <h1 id="disclaimer-modal-title">Before You Use Aerobag</h1>
        <div
          className="disclaimerModalText"
          dangerouslySetInnerHTML={{ __html: props.state.html }}
        />
        <button
          type="button"
          className="disclaimerAcceptButton"
          data-testid="parity:disclaimer-accept-button"
          disabled={props.acceptingDisabled}
          onClick={props.onAccept}
        >
          {props.state.accept_label}
        </button>
      </section>
    </div>
  );
}

function airspaceSvgPathD(path: AirspaceDisplayPath["paths"][number]): string {
  if (path.points.length === 0) {
    return "";
  }
  const [first, ...rest] = path.points;
  const segments = [`M ${first.x} ${first.y}`];
  for (const point of rest) {
    segments.push(`L ${point.x} ${point.y}`);
  }
  if (path.closed) {
    segments.push("Z");
  }
  return segments.join(" ");
}

function airspaceSvgPathListD(paths: AirspaceDisplayPath["paths"]): string {
  return paths.map(airspaceSvgPathD).filter(Boolean).join(" ");
}

function airspaceSegmentListD(segments: NonNullable<AirspaceDisplayPath["decorations"][number]["segments"]>): string {
  return segments.map(([x1, y1, x2, y2]) => `M ${x1} ${y1} L ${x2} ${y2}`).join(" ");
}

function airspaceDecorationD(decoration: AirspaceDisplayPath["decorations"][number]): string {
  return [
    airspaceSvgPathListD(decoration.paths ?? []),
    airspaceSegmentListD(decoration.segments ?? []),
  ].filter(Boolean).join(" ");
}

function airspaceDashArray(dashPx: number[]): string | undefined {
  return dashPx.length > 0 ? dashPx.join(" ") : undefined;
}

function svgStrokeLinecap(lineCap: string): "butt" | "round" | "square" {
  return lineCap === "butt" || lineCap === "square" ? lineCap : "round";
}

function svgStrokeLinejoin(lineJoin: string): "miter" | "round" | "bevel" {
  return lineJoin === "miter" || lineJoin === "bevel" ? lineJoin : "round";
}

function airspaceLabelDividerWidth(parts: { upper: string; lower: string }): number {
  return Math.max(parts.upper.length, parts.lower.length, 2) * 7.2 + 6;
}

function colorWithOpacity(color: string, opacity: number): string {
  return `color-mix(in srgb, ${color} ${Math.round(opacity * 100)}%, transparent)`;
}

function aviationThemeColor(colorKey: string): string {
  return loadedUiTheme.aviation[colorKey as AviationThemeColorKey] ?? loadedUiTheme.aviation.dark_gray;
}

function AirspaceDisplayPathGroup(props: { feature: AirspaceDisplayPath }) {
  const { feature } = props;
  return (
    <g>
      {feature.paths.map((path, index) => (
        <Fragment key={`${feature.id}:${index}`}>
          <path
            d={airspaceSvgPathD(path)}
            fill={path.closed ? colorWithOpacity(aviationThemeColor(feature.style.fill_color_key), feature.style.fill_opacity) : "none"}
            stroke="none"
          />
          {feature.style.strokes.map((stroke, strokeIndex) => (
            <path
              key={strokeIndex}
              d={airspaceSvgPathD(path)}
              fill="none"
              stroke={aviationThemeColor(stroke.color_key)}
              strokeWidth={stroke.width_px}
              strokeDasharray={airspaceDashArray(stroke.dash_px)}
              strokeLinecap={svgStrokeLinecap(stroke.line_cap)}
              strokeLinejoin="round"
              vectorEffect="non-scaling-stroke"
            />
          ))}
        </Fragment>
      ))}
      {feature.decorations.map((decoration, index) => (
        <path
          key={`${feature.id}:decoration:${index}`}
          d={airspaceDecorationD(decoration)}
          fill="none"
          stroke={aviationThemeColor(decoration.color_key)}
          strokeWidth={decoration.width_px}
          strokeLinecap={svgStrokeLinecap(decoration.line_cap)}
          strokeLinejoin="round"
          vectorEffect="non-scaling-stroke"
        />
      ))}
    </g>
  );
}

function offlineRegionPoints(points: Array<{ x: number; y: number }>): string {
  return points.map((point) => `${point.x},${point.y}`).join(" ");
}

function offlineRegionSummaryLines(region: MapOverlayQueryResult["offline_regions"][number]): string[] {
  if (!region.summary.length) {
    return [];
  }
  const entries = region.summary.map((entry) => {
    const suffix = entry.count > 1 ? ` (${entry.count})` : "";
    return `${offlineRegionSummaryIcon(entry.action)} ${entry.cycle}${suffix}`;
  });
  const lines: string[] = [];
  for (let index = 0; index < entries.length; index += 2) {
    lines.push(entries.slice(index, index + 2).join("  "));
  }
  return lines;
}

function offlineRegionSummaryIcon(action: string): string {
  switch (action) {
    case "fetch":
      return "▶";
    case "pause":
      return "‖";
    case "delete":
      return "×";
    default:
      return "●";
  }
}

type AppPage = "map" | "plan" | "altitude" | "charts" | "home" | "data" | "settings" | "cloud" | "about";

type WebPageTilePaintTiming = {
  id: number;
  fromPage: AppPage;
  startedAt: number;
};

type ChartAsset = NonNullable<ChartPageData["airports"][number]>["charts"][number];
type ResolvedChartUrls = {
  assetUrl?: string | null;
  thumbnailUrl?: string | null;
};
type TrayOption =
  | {
    kind?: "option";
    id: string;
    label: string;
    iconSrc?: string;
    aircraftSymbol?: UiAircraftSymbol | null;
    toggleState?: UiMapLayerToggleState;
    active?: boolean;
    disabled?: boolean;
    disabledReason?: string | null;
    accentColor?: string;
    accessory?: {
      iconSrc: string;
      ariaLabel: string;
      onSelect: () => void;
    };
    dismissTrayOnSelect?: boolean;
    onSelect: () => void;
  }
  | {
    kind: "separator";
    id: string;
    label: string;
  };

function disabledReasonText(reason: string | null | undefined): string | null {
  const trimmed = reason?.trim();
  return trimmed ? trimmed : null;
}

function useDisabledActionToast() {
  const [toast, setToast] = useState<{ id: number; message: string } | null>(null);
  const show = useCallback((message: string) => {
    setToast({ id: Date.now(), message });
  }, []);

  useEffect(() => {
    if (!toast) {
      return;
    }
    const toastId = toast.id;
    const timeout = window.setTimeout(() => {
      setToast((current) => current?.id === toastId ? null : current);
    }, 2600);
    return () => window.clearTimeout(timeout);
  }, [toast]);

  return { toast, show };
}

type UiThemeJson = {
  controls: {
    button_checked: string;
    button_unchecked: string;
    header_button: string;
    button_disabled: string;
    button_disabled_icon_saturation: number;
    button_disabled_icon_opacity: number;
    button_fg: string;
    control_group_bg: string;
    text_input_bg: string;
    button_icon_secondary: string;
    panel_bg: string;
    panel_border: string;
    panel_fg: string;
    panel_muted: string;
    map_selection_display_bg: string;
    map_selection_display_fg: string;
    situation_status_bg: string;
    situation_status_fg: string;
    situation_status_unavailable_fg: string;
    data_status_ok_bg: string;
    data_status_ok_stroke: string;
    data_status_info_bg: string;
    data_status_info_stroke: string;
    data_status_caution_bg: string;
    data_status_caution_stroke: string;
    data_status_warning_bg: string;
    data_status_warning_stroke: string;
    data_status_unavailable_bg: string;
    data_status_unavailable_stroke: string;
    data_status_quiet_bg: string;
    data_status_quiet_stroke: string;
    chart_surface_bg: string;
    flight_data_bg: string;
    flight_data_border: string;
    flight_data_label: string;
    flight_data_value: string;
    flight_data_missing_value: string;
    flight_data_passed_value: string;
    flight_data_active_value: string;
    flight_data_modeled_value: string;
    cdi_pointer: string;
    compass_north: string;
    compass_south: string;
  };
  aviation: {
    class_b_d_blue: string;
    class_c_magenta: string;
    tfr_active: string;
    tfr_upcoming: string;
    intersection_cyan: string;
    traffic: string;
    traffic_contrast: string;
    traffic_label: string;
    dark_gray: string;
    obstacle_danger: string;
    obstacle_caution: string;
    obstacle_muted: string;
    obstacle_under: string;
    airport_runway_paved: string;
    airport_runway_turf: string;
    airport_runway_unpaved: string;
    airport_runway_water: string;
    airport_runway_inactive: string;
    airport_runway_pattern: string;
  };
  flight_plan_route: {
    contrast: string;
    completed: string;
    active: string;
    active_leg_remaining: string;
    guidance_arrow: string;
    remaining: string;
    distance_pill_bg: string;
    distance_pill_fg: string;
  };
  plate_folder: {
    thumbnail_bg: string;
    notam_badge_bg: string;
    notam_badge_fg: string;
    notam_badge_stroke: string;
    disabled_accent_percent: number;
    label_colors: Record<string, string>;
  };
};

type AviationThemeColorKey = keyof UiThemeJson["aviation"];

type TrayDockStyle = "compact" | "plate_narrow" | "plate_wide" | "wide" | "situation";
type PlateFolderCategory = ChartAsset["folder_category"];

const emptyChartPage: ChartPageData = { airports: [] };
const PAGE_CHART_ICON_SRC = "/icons/icons/page-chart-icon.png?v=20260424b";
const PAGE_HOME_ICON_SRC = "/icons/icons/page-home-icon.png?v=20260617a";
const PAGE_PLAN_ICON_SRC = "/icons/icons/page-plan1-icon.png?v=20260424b";
const PAGE_PLATE_ICON_SRC = "/icons/icons/page-plate-icon.png?v=20260424b";
const HOME_ABOUT_ICON_SRC = "/icons/icons/home-about-icon.png?v=20260802a";
const HOME_ALTITUDE_PLANNER_ICON_SRC = "/icons/icons/home-altitude-planner-icon.png?v=20260806a";
const HOME_CLOUD_ICON_SRC = "/icons/icons/home-cloud-icon.png?v=20260803a";
const HOME_FLIGHT_PLAN_ICON_SRC = "/icons/icons/home-flight-plan-icon.png?v=20260802a";
const HOME_OFFLINE_PACKAGES_ICON_SRC = "/icons/icons/home-offline-packages-icon.png?v=20260802a";
const HOME_SETTINGS_ICON_SRC = "/icons/icons/home-settings-icon.png?v=20260802a";
const HOME_STATUS_ICON_SRC = "/icons/icons/home-status-icon.png?v=20260802a";
const HOME_PAGE_BACKDROP_SRC = "/icons/backdrops/home-page-backdrop.jpg?v=20260617a";
const LAYER_VECTORS_ICON_SRC = "/icons/icons/layer-vectors-icon.png?v=20260424b";
const LAYER_NEXRAD_ICON_SRC = "/icons/icons/layer-nexrad-icon.png?v=20260424b";
const LAYER_TERRAIN_WARNING_ICON_SRC = "/icons/icons/layer-terrain-warning-icon.png?v=20260424b";
const LAYER_OBSERVATIONS_ICON_SRC = "/icons/icons/layer-observations-icon.png?v=20260805a";
const LAYER_ADSB_ICON_SRC = "/icons/icons/layer-adsb-icon.png?v=20260805a";
const LAYER_OFFLINE_REGIONS_ICON_SRC = "/icons/icons/layer-offline-regions-icon.png?v=20260805a";
const LAYER_WORLD_BASEMAP_ICON_SRC = "/icons/icons/layer-world-basemap-icon.png?v=20260805a";
const CHART_REFERENCE_ICON_SRC = "/icons/icons/chart-reference-icon.png?v=20260713a";
const NEXRAD_VIEWPORT_REFRESH_THROTTLE_MS = 1_000;
const HOME_GRID_COLUMN_COUNT = 3;

function webHomeButtonPresentation(destination: UiHomeDestination): { page: AppPage | null; iconSrc?: string } {
  switch (destination) {
    case "chart":
      return { page: "map", iconSrc: PAGE_CHART_ICON_SRC };
    case "plate":
      return { page: "charts", iconSrc: PAGE_PLATE_ICON_SRC };
    case "flight_plan":
      return { page: "plan", iconSrc: HOME_FLIGHT_PLAN_ICON_SRC };
    case "altitude_planner":
      return { page: "altitude", iconSrc: HOME_ALTITUDE_PLANNER_ICON_SRC };
    case "data_status":
      return { page: "data", iconSrc: HOME_STATUS_ICON_SRC };
    case "settings":
      return { page: "settings", iconSrc: HOME_SETTINGS_ICON_SRC };
    case "cloud":
      return { page: "cloud", iconSrc: HOME_CLOUD_ICON_SRC };
    case "offline_packages":
      return { page: null, iconSrc: HOME_OFFLINE_PACKAGES_ICON_SRC };
    case "about":
      return { page: "about", iconSrc: HOME_ABOUT_ICON_SRC };
  }
}

function chartFamilyIconSrc(familyId: ChartFamilyId | null | undefined): string | undefined {
  switch (familyId) {
    case "sec":
      return "/icons/icons/sectional-icon.png?v=20260424b";
    case "tac":
      return "/icons/icons/tac-icon.png?v=20260424b";
    case "flyway":
      return "/icons/icons/flyway-icon.png?v=20260717a";
    case "enr-l":
      return "/icons/icons/ifr-l-icon.png?v=20260424b";
    case "enr-h":
      return "/icons/icons/ifr-h-icon.png?v=20260424b";
    case "shaded-relief":
      return "/icons/icons/shaded-relief-icon.png?v=20260424b";
    case "world-basemap":
      return LAYER_WORLD_BASEMAP_ICON_SRC;
    default:
      return undefined;
  }
}

function layerIconSrc(layerId: MapLayerId): string {
  switch (layerId) {
    case "world_basemap":
      return LAYER_WORLD_BASEMAP_ICON_SRC;
    case "vectors":
      return LAYER_VECTORS_ICON_SRC;
    case "metars":
      return LAYER_OBSERVATIONS_ICON_SRC;
    case "nexrad":
      return LAYER_NEXRAD_ICON_SRC;
    case "traffic":
      return LAYER_ADSB_ICON_SRC;
    case "terrain_warning":
      return LAYER_TERRAIN_WARNING_ICON_SRC;
    case "offline_regions":
      return LAYER_OFFLINE_REGIONS_ICON_SRC;
  }
}

type TerrainOverlayImage = TerrainOverlayTileRequest & {
  rgba: Uint8ClampedArray;
  imageWidth: number;
  imageHeight: number;
};

type TerrainOverlayUiState = {
  query: TerrainOverlayQueryResult | null;
  images: TerrainOverlayImage[];
};

type TerrainPendingFrame = {
  requestId: number;
  query: TerrainOverlayQueryResult;
  frameKey: string;
};

type TerrainScheduleRequest = {
  id: number;
  session: UiSession;
  viewport: MapViewportState;
  width: number;
  height: number;
  navDataEpoch: number;
  altitudeBucket: number | null;
};

type TerrainTileCacheEntry = {
  rgba: Uint8ClampedArray;
  imageWidth: number;
  imageHeight: number;
};

function terrainCacheKey(request: TerrainOverlayTileRequest) {
  return request.cache_key;
}

function requireTerrainFrameKey(query: TerrainOverlayQueryResult) {
  if (!query.frame_key) {
    throw new Error("ready terrain overlay query is missing core frame_key");
  }
  return query.frame_key;
}

function pruneTerrainFrameStarts(starts: Map<string, number>) {
  while (starts.size > 32) {
    const firstKey = starts.keys().next().value;
    if (!firstKey) {
      return;
    }
    starts.delete(firstKey);
  }
}

function wrappedFeatureRenderKey(id: string, screenX: number, screenY: number): string {
  return `${id}:${Math.round(screenX * 10)}:${Math.round(screenY * 10)}`;
}

function parseTerrainRawRgba(bytes: Uint8Array): TerrainTileCacheEntry {
  if (bytes.byteLength < 4) {
    throw new Error("terrain raw RGBA payload is missing header");
  }
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const imageWidth = view.getUint16(0, true);
  const imageHeight = view.getUint16(2, true);
  const expectedBytes = 4 + imageWidth * imageHeight * 4;
  if (bytes.byteLength < expectedBytes) {
    throw new Error(`terrain raw RGBA payload truncated: expected ${expectedBytes}, got ${bytes.byteLength}`);
  }
  return {
    imageWidth,
    imageHeight,
    rgba: new Uint8ClampedArray(bytes.buffer, bytes.byteOffset + 4, expectedBytes - 4),
  };
}

function terrainRequestSummary(requests: TerrainOverlayTileRequest[]) {
  const zoomCounts = new Map<number, number>();
  for (const request of requests) {
    zoomCounts.set(request.z, (zoomCounts.get(request.z) ?? 0) + 1);
  }
  const summarize = <T extends string | number>(counts: Map<T, number>) =>
    Array.from(counts.entries())
      .sort(([left], [right]) => (left < right ? -1 : left > right ? 1 : 0))
      .map(([key, count]) => `${key}:${count}`)
      .join(",");
  return {
    zooms: summarize(zoomCounts),
  };
}

function cachedTerrainImageForDisplay(
  cache: Map<string, TerrainTileCacheEntry>,
  request: TerrainOverlayTileRequest,
) {
  const exact = cache.get(terrainCacheKey(request));
  return exact ? ({ ...request, ...exact } satisfies TerrainOverlayImage) : null;
}

function terrainImageForViewport(
  image: TerrainOverlayImage,
  viewport: MapViewportState,
  widthPx: number,
  heightPx: number,
): TerrainOverlayImage {
  const scale = scaleForZoom(viewport.zoom);
  const tilesAtZoom = 2 ** image.z;
  const tileWorldSize = WEB_MERCATOR_WORLD_SIZE / tilesAtZoom;
  const yXyz = (tilesAtZoom - 1) - image.y_tms;
  return {
    ...image,
    left: (image.x * tileWorldSize - viewport.centerWorldX) * scale + widthPx / 2,
    top: (yXyz * tileWorldSize - viewport.centerWorldY) * scale + heightPx / 2,
    size: tileWorldSize * scale,
  };
}

function terrainImagesForCompleteQuery(
  cache: Map<string, TerrainTileCacheEntry>,
  query: TerrainOverlayQueryResult,
) {
  if (query.status.state !== "ready") {
    return null;
  }
  const images: TerrainOverlayImage[] = [];
  for (const request of query.tile_requests) {
    const cached = cachedTerrainImageForDisplay(cache, request);
    if (!cached) {
      return null;
    }
    images.push(cached);
  }
  return images;
}

const WEB_MERCATOR_WORLD_SIZE = 256;
const RASTER_TILE_OVERDRAW_PX = 1;

function TerrainOverlayCanvasTile({ tile }: { tile: TerrainOverlayImage }) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  // Deliberately key this scroll effect only on active row identity. Row data
  // changes during passive ownship/replay updates must not yank the user's
  // manual FP scroll position.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) {
      return;
    }
    canvas.width = tile.imageWidth;
    canvas.height = tile.imageHeight;
    const context = canvas.getContext("2d");
    if (!context) {
      return;
    }
    context.putImageData(new ImageData(tile.rgba as ImageDataArray, tile.imageWidth, tile.imageHeight), 0, 0);
  }, [tile.imageHeight, tile.imageWidth, tile.rgba]);

  return (
    <canvas
      ref={canvasRef}
      className="terrainOverlayTile"
      width={tile.imageWidth}
      height={tile.imageHeight}
      style={{
        left: `${tile.left}px`,
        top: `${tile.top}px`,
        width: `${tile.size}px`,
        height: `${tile.size}px`,
      }}
    />
  );
}

function nexradTileBounds(tile: NexradOverlayTile) {
  const xs = [tile.corners.nw.x, tile.corners.ne.x, tile.corners.se.x, tile.corners.sw.x];
  const ys = [tile.corners.nw.y, tile.corners.ne.y, tile.corners.se.y, tile.corners.sw.y];
  const left = Math.min(...xs);
  const top = Math.min(...ys);
  return {
    left,
    top,
    width: Math.max(...xs) - left,
    height: Math.max(...ys) - top,
  };
}

async function preloadNexradOverlayImages(
  query: NexradOverlayQueryResult,
  cache: NexradFrameImageCache,
): Promise<{ loaded: number; failed: number }> {
  if (!query.cache_plan) {
    return { loaded: 0, failed: 0 };
  }
  return cache.applyPlan({
    retained_frame_versions: query.cache_plan.retained_frame_versions,
    fetch_resources: query.cache_plan.fetch_resources.map((resource) => ({
      frame_version: resource.frame_version,
      src: resolveLiveFeedResourceUrl(resource.src),
    })),
  });
}

type NavigationPageOption = { id: AppPage; label: string; launcherLabel: string; iconSrc?: string };
type NavigationPagePolicy = {
  options: NavigationPageOption[];
  maxHistoryDepth: number;
  chartOrPlateReturnPages: ReadonlySet<AppPage>;
  defaultChartOrPlateReturnPage: AppPage;
};

const NavigationPageOptionsContext = createContext<NavigationPagePolicy | null>(null);

function navigationPageOptionsFromCore(state: UiSessionSnapshot["navigation_page_state"]): NavigationPagePolicy {
  const pagePairs = state.options.flatMap((option) => {
    const id = appPageFromNavigationPageId(option.id);
    if (!id) return [];
    const iconSrc = id === "map"
      ? PAGE_CHART_ICON_SRC
      : id === "charts"
        ? PAGE_PLATE_ICON_SRC
        : id === "plan"
          ? PAGE_PLAN_ICON_SRC
          : id === "home"
            ? PAGE_HOME_ICON_SRC
            : undefined;
    return [{ option: { id, label: option.label, launcherLabel: option.launcher_label, iconSrc }, returnTarget: option.chart_or_plate_return_target }];
  });
  const defaultChartOrPlateReturnPage = appPageFromNavigationPageId(
    state.default_chart_or_plate_return_target,
  );
  if (!defaultChartOrPlateReturnPage) {
    throw new Error("core supplied an unsupported default chart/plate return page");
  }
  return {
    options: pagePairs.map((entry) => entry.option),
    maxHistoryDepth: state.max_history_depth,
    chartOrPlateReturnPages: new Set(pagePairs.filter((entry) => entry.returnTarget).map((entry) => entry.option.id)),
    defaultChartOrPlateReturnPage,
  };
}

function appPageFromNavigationPageId(id: UiSessionSnapshot["navigation_page_state"]["options"][number]["id"]): AppPage | null {
  switch (id) {
    case "map": return "map";
    case "charts": return "charts";
    case "flight_plan": return "plan";
    case "altitude_planner": return "altitude";
    case "data_status": return "data";
    case "settings": return "settings";
    case "home": return "home";
    default: return null;
  }
}

function useNavigationPageOptions() {
  return useNavigationPagePolicy().options;
}

function useNavigationPagePolicy(): NavigationPagePolicy {
  const policy = useContext(NavigationPageOptionsContext);
  if (!policy) {
    throw new Error("navigation page policy is unavailable outside the session provider");
  }
  return policy;
}

const webUiStateStorageKey = "aerobag.web.uiState.v1";
declare const __AEROBAG_DOWNLOADS_BASE_URL__: string | null;

const androidApkMetadataPath = `${
  __AEROBAG_DOWNLOADS_BASE_URL__?.replace(/\/+$/, "") || "/downloads"
}/android-apk.json`;
const loadedUiTheme = uiTheme as UiThemeJson;
const controlTheme = loadedUiTheme.controls;
const plateFolderTheme = loadedUiTheme.plate_folder;
const defaultPlaybackTracePath = "/gps-captures/black-tablet-20260727-drive.jsonl";
const startupHighLatencyWarningGraceMs = 10_000;
const browserGeolocationSourceId = "browser-geolocation";
const metersPerSecondToKnots = 1.9438444924406;
const metersToFeet = 3.280839895;
const flightDataBannerEdge: FlightDataBannerEdge = "right";

type PersistedWebUiState = {
  page?: AppPage;
  mapOrientationMode?: MapOrientationMode;
  selectedAirportId?: string;
  selectedChartId?: string;
  recentAirportIds?: string[];
};

type AndroidApkDownloadMetadata = {
  apk_url: string;
  filename: string;
  apk_size_bytes: number;
  git_commit: string;
  version_code: number;
  version_name: string;
  built_at_utc: string;
};

type FlightDataBannerEdge = "left" | "right";

type AppViewSnapshot = {
  page: AppPage;
  selectedMapId: string;
  mapViewport: MapViewportState;
  plateTargetAirportId: string | null;
  selectedAirportId: string;
  selectedReferenceFamilyId: string | null;
  selectedChartId: string;
  selectedChartLabel: string;
  suggestedChartIds: string[];
  recentAirportIds: string[];
  chartViewport: ImageViewportState | null;
  chartFolderOpen: boolean;
};

type WebHistoryState = {
  __aerobag?: true;
  current?: AppViewSnapshot;
  stack?: AppViewSnapshot[];
};

function appPageForCurrentPath(): AppPage | null {
  if (typeof window === "undefined") {
    return null;
  }
  return appPageForPath(window.location.pathname);
}

function urlForAppPage(page: AppPage): string {
  return appPageUrl(
    page,
    typeof window === "undefined" ? "/" : window.location.pathname,
  );
}

type RasterRenderTile = {
  drawKey: string;
  x: number;
  yTms: number;
  left: number;
  top: number;
  size: number;
  zoom: number;
  zIndex: number;
  src: string;
  mapViewId: string;
  packageName?: string | null;
  chartFamily: ChartFamilyId;
  fallbacks: RasterTileDraw["fallbacks"];
};

function renderTileFromCore(
  tile: RasterTileDraw,
  cssScale = 1,
): RasterRenderTile {
  // TASK-25 raster exception: raster tiles deliberately arrive as resolved image
  // URLs so the browser can stream/cache many tiles without generic resource
  // ingestion. Do not copy this for new resources; use core-owned resource
  // operations such as resolveChartAssetUrl or the terrain/NEXRAD overlays.
  const packageName = tile.primary.package_name;
  if (!packageName) {
    throw new Error(`raster tile ${tile.draw_key} missing package_name`);
  }
  if (tile.primary.resource.kind !== "resolved_public_url") {
    throw new Error(`raster tile ${tile.draw_key} is not a public unpacked web resource`);
  }
  const src = tile.primary.resource.url;
  return {
    drawKey: tile.draw_key,
    x: tile.x,
    yTms: tile.y_tms,
    left: tile.left_px * cssScale,
    top: tile.top_px * cssScale,
    size: tile.size_px * cssScale,
    zoom: tile.source_zoom,
    zIndex: tile.z_order,
    src,
    mapViewId: tile.primary.map_view_id,
    packageName,
    chartFamily: tile.family,
    fallbacks: tile.fallbacks,
  };
}

function thumbPixels(multiplier = 1) {
  if (typeof window === "undefined") {
    return 0;
  }
  const raw = getComputedStyle(document.documentElement).getPropertyValue("--thumb").trim();
  const parsed = Number.parseFloat(raw);
  if (!Number.isFinite(parsed)) {
    return 0;
  }
  if (raw.endsWith("rem")) {
    const rootFontSize = Number.parseFloat(getComputedStyle(document.documentElement).fontSize);
    return parsed * (Number.isFinite(rootFontSize) ? rootFontSize : 16) * multiplier;
  }
  return parsed * multiplier;
}

function shouldLowerStatusControlDock(surfaceWidthPx: number, dataStatusCount: number) {
  const leftControlWidthThumbs = 6.4;
  const ownshipPillWidthThumbs = 2;
  const dataStatusWidthThumbs = Math.max(0, dataStatusCount) * 0.6;
  const outerGutterThumbs = 1;
  return surfaceWidthPx > 0 && surfaceWidthPx < thumbPixels(leftControlWidthThumbs + ownshipPillWidthThumbs + dataStatusWidthThumbs + outerGutterThumbs);
}

function shouldRaiseBottomCornerControls(surfaceWidthPx: number) {
  const primaryNavigationWidthThumbs = 5.2;
  const widestCornerControlWidthThumbs = 3;
  const cornerOuterAndSeparationGuttersThumbs = 0.2;
  const collisionWidthThumbs =
    primaryNavigationWidthThumbs
    + 2 * (widestCornerControlWidthThumbs + cornerOuterAndSeparationGuttersThumbs);
  return surfaceWidthPx > 0 && surfaceWidthPx < thumbPixels(collisionWidthThumbs);
}

function flightDataEdgeColumnCount(
  surfaceSize: SurfaceSize,
  cellCount: number,
  situationDockLowered: boolean,
) {
  if (cellCount <= 0 || surfaceSize.height <= 0) {
    return 1;
  }
  const thumb = thumbPixels();
  const gap = thumbPixels(0.06);
  const topReserve = thumbPixels(situationDockLowered ? 2.15 : 0.72);
  const bottomReserve = thumbPixels(1.25);
  const availableHeight = Math.max(thumb, surfaceSize.height - topReserve - bottomReserve);
  const readableCellHeight = thumbPixels(0.64);
  const rowsPerColumn = Math.max(1, Math.floor((availableHeight + gap) / (readableCellHeight + gap)));
  return Math.min(3, Math.max(1, Math.ceil(cellCount / rowsPerColumn)));
}

const airportLabelY = -24;
const vorLabelY = -24;
const fixLabelY = -15;
const obstacleLabelY = -14;
const weatherCameraLabelY = -24;

type VectorPointSymbolFeature = NavSymbolFeature & {
  label_style?: VectorIdentLabelStyle;
};

type VectorIdentLabelStyle = "default" | "flight_plan" | "active_flight_plan";

function VectorIdentLabel(props: {
  label: string;
  y: number;
  className: string;
  labelStyle?: VectorIdentLabelStyle;
}) {
  const { label, y, className, labelStyle = "default" } = props;
  if (!label) return null;
  if (labelStyle === "default") {
    return (
      <text x="0" y={y} textAnchor="middle" className={className}>
        {label}
      </text>
    );
  }
  const width = Math.max(26, label.length * 9.5 + 14);
  const height = 15;
  const styleClass = labelStyle === "active_flight_plan"
    ? "vectorIdent vectorIdentActiveFlightPlan"
    : "vectorIdent vectorIdentFlightPlan";
  return (
    <g className={styleClass}>
      <rect
        x={-width / 2}
        y={y - height + 2}
        width={width}
        height={height}
        rx="2"
        ry="2"
        className="vectorIdentBox"
      />
      <text x="0" y={y} textAnchor="middle" className="vectorIdentText">
        {label}
      </text>
    </g>
  );
}

function VectorPointSymbol(props: { feature: VectorPointSymbolFeature; showLabel?: boolean }) {
  const { feature, showLabel = true } = props;
  const isAirport = feature.symbol_kind === "airport";
  const isVor = feature.symbol_kind === "nav";
  const isObstacle = feature.symbol_kind === "obstacle";
  const isWeatherCamera = feature.symbol_kind === "weather_camera";
  const airportClass = feature.towered ? "airportMarker airportTowered" : "airportMarker airportUntowered";
  const airportLabelClass = feature.towered ? "airportLabel airportToweredLabel" : "airportLabel airportUntoweredLabel";
  if (isAirport) {
    const isHeliport = feature.heliport === true;
    const isSeaplaneBase = feature.has_water_runway === true;
    const usesOpenAirportCircle = isHeliport || isSeaplaneBase || feature.has_paved_runway === false;
    const halfLength = 8 * Math.max(feature.runway_length_ratio, 0.2);
    return (
      <>
        <g className="mapUpright">
          {usesOpenAirportCircle ? (
            <>
              {airportOpenMarkerSymbol.map((layer) => (
                <path
                  key={layer.paint}
                  d={layer.path}
                  style={navSymbolLayerStyle(layer)}
                />
              ))}
            </>
          ) : feature.fuel_available ? (
            <path d={airportFuelMarkerPath} className={airportClass} />
          ) : (
            <path d={airportCircleMarkerPath} className={airportClass} />
          )}
          {isHeliport ? (
            <path d={heliportHPath} className="airportSpecialGlyph airportHeliportGlyph" />
          ) : isSeaplaneBase ? (
            <path
              d={seaplaneAnchorPath}
              transform="rotate(15)"
              className="airportSpecialGlyph airportAnchorGlyph"
            />
          ) : null}
        </g>
        {!usesOpenAirportCircle && feature.longest_runway_heading_true_deg != null ? (
          <>
            <line
              x1="0"
              y1={-halfLength}
              x2="0"
              y2={halfLength}
              className="airportRunwayBarUnder"
              transform={`rotate(${feature.longest_runway_heading_true_deg})`}
            />
            <line
              x1="0"
              y1={-halfLength}
              x2="0"
              y2={halfLength}
              className="airportRunwayBar"
              transform={`rotate(${feature.longest_runway_heading_true_deg})`}
            />
          </>
        ) : null}
        {showLabel ? (
          <g className="mapUpright">
            <VectorIdentLabel
              label={feature.label}
              y={airportLabelY}
              className={airportLabelClass}
              labelStyle={feature.label_style}
            />
          </g>
        ) : null}
      </>
    );
  }
  if (isVor) {
    return (
      <g className="mapUpright">
        <path d={vorBandPath} className="vorBand" fillRule="evenodd" />
        <path d={vorOuterHexPath} className="vorBorder" />
        {showLabel ? (
          <VectorIdentLabel label={feature.label} y={vorLabelY} className="vorLabel" labelStyle={feature.label_style} />
        ) : null}
      </g>
    );
  }
  if (isWeatherCamera) {
    return (
      <g className="mapUpright">
        <RenderNavSymbolLayers layers={weatherCameraSymbol} />
        {showLabel ? (
          <VectorIdentLabel
            label={feature.label}
            y={weatherCameraLabelY}
            className="fixLabel"
            labelStyle={feature.label_style}
          />
        ) : null}
      </g>
    );
  }
  if (isObstacle) {
    const obstacleClass = feature.obstacle_tone === "danger"
      ? "obstacleMarker obstacleDanger"
      : feature.obstacle_tone === "muted"
        ? "obstacleMarker obstacleMuted"
        : "obstacleMarker obstacleCaution";
    const obstacleDotClass = feature.obstacle_tone === "danger"
      ? "obstacleDot obstacleDangerFill"
      : feature.obstacle_tone === "muted"
        ? "obstacleDot obstacleMutedFill"
        : "obstacleDot obstacleCautionFill";
    const isTallObstacle = feature.obstacle_variant === "tall";
    const obstaclePath = isTallObstacle ? obstacleTallPath : obstacleShortPath;
    const obstacleDotY = isTallObstacle ? obstacleTallDotY : obstacleShortDotY;
    return (
      <g className="mapUpright">
        <path d={obstaclePath} className={`${obstacleClass} obstacleMarkerUnder`} />
        <path d={obstaclePath} className={obstacleClass} />
        <circle cx="0" cy={obstacleDotY} r={obstacleDotRadius} className="obstacleDotUnder" />
        <circle cx="0" cy={obstacleDotY} r={obstacleDotRadius} className={obstacleDotClass} />
        {showLabel && feature.label ? (
          <VectorIdentLabel label={feature.label} y={obstacleLabelY} className="obstacleLabel" labelStyle={feature.label_style} />
        ) : null}
      </g>
    );
  }
  return (
    <g className="mapUpright">
      <path d="M 0 -8 L 7 6 L -7 6 Z" className="fixMarker" />
      {showLabel ? (
        <VectorIdentLabel label={feature.label} y={fixLabelY} className="fixLabel" labelStyle={feature.label_style} />
      ) : null}
    </g>
  );
}

function spotSymbolClassName(paint: string): string {
  switch (paint) {
    case "map_selection_spot_under":
      return "mapSelectionSpotPegUnder";
    case "map_selection_spot_dot":
      return "mapSelectionSpotPegDot";
    default:
      return "mapSelectionSpotPeg";
  }
}

function navSymbolColor(token: string | null | undefined): string | undefined {
  switch (token) {
    case "none":
      return "none";
    case "white":
      return "white";
    case "ink_70":
      return "rgba(8, 18, 24, 0.7)";
    case "ink_75":
      return "rgba(8, 18, 24, 0.75)";
    case "class_c_magenta":
      return "var(--theme-class-c-magenta)";
    case "button_unchecked":
      return "var(--theme-button-unchecked)";
    case "button_icon":
      return "var(--theme-button-fg)";
    case "button_icon_secondary":
      return "var(--theme-button-icon-secondary)";
    case "flight_plan_guidance":
      return "var(--theme-flight-plan-guidance-arrow)";
    case "compass_north":
      return "var(--theme-compass-north)";
    case "compass_south":
      return "var(--theme-compass-south)";
    case "white_90":
      return "rgba(255, 255, 255, 0.9)";
    case "white_68":
      return "rgba(255, 255, 255, 0.68)";
    case "paper":
      return "#fffef8";
    case "pirep_ink":
      return "#071015";
    case "metar_category":
      return "var(--metar-color)";
    case "pirep_symbol":
      return "var(--pirep-color)";
    case "data_status_symbol_ink":
      return "var(--data-status-symbol-ink)";
    default:
      return undefined;
  }
}

function navSymbolLayerStyle(layer: { fill?: string | null; stroke?: string | null; stroke_width?: number | null; line_cap?: string | null; line_join?: string | null }) {
  return {
    fill: navSymbolColor(layer.fill),
    stroke: navSymbolColor(layer.stroke),
    strokeWidth: layer.stroke_width ?? undefined,
    strokeLinecap: svgStrokeLinecap(layer.line_cap ?? "butt"),
    strokeLinejoin: svgStrokeLinejoin(layer.line_join ?? "miter"),
  };
}

function MapSelectionSpotSymbol() {
  return (
    <g className="mapUpright">
      {mapSelectionSpotSymbol.map((layer) => (
        <path
          key={layer.paint}
          className={spotSymbolClassName(layer.paint)}
          style={navSymbolLayerStyle(layer)}
          d={layer.path}
          transform={layer.transform_degrees != null ? `rotate(${layer.transform_degrees})` : undefined}
        />
      ))}
    </g>
  );
}

function RenderNavSymbolLayers(props: { layers: readonly { path: string; paint: string; fill?: string | null; stroke?: string | null; stroke_width?: number | null; line_cap?: string | null; line_join?: string | null; transform_degrees?: number | null }[] }) {
  return (
    <>
      {props.layers.map((layer, index) => (
        <path
          key={`${layer.paint}:${index}`}
          style={navSymbolLayerStyle(layer)}
          d={layer.path}
          transform={layer.transform_degrees != null ? `rotate(${layer.transform_degrees})` : undefined}
        />
      ))}
    </>
  );
}

function ActionIcon(props: { layers: NonNullable<ReturnType<typeof actionSymbol>> }) {
  return (
    <svg
      className="actionIcon"
      viewBox="-24 -24 48 48"
      aria-hidden="true"
      focusable="false"
    >
      <RenderNavSymbolLayers layers={props.layers} />
    </svg>
  );
}

function metarCategoryClass(category: string): string {
  switch (category) {
    case "vfr":
      return "metarVfr";
    case "mvfr":
      return "metarMvfr";
    case "ifr":
      return "metarIfr";
    case "lifr":
      return "metarLifr";
    default:
      return "metarMissing";
  }
}

function MetarGlyph(props: {
  flightCategory: string;
  ceilingAmount: string;
}) {
  const categoryClass = metarCategoryClass(props.flightCategory);
  const layers = props.ceilingAmount === "few"
    ? metarFewSymbol
    : props.ceilingAmount === "sct"
      ? metarSctSymbol
      : props.ceilingAmount === "bkn"
        ? metarBknSymbol
        : props.ceilingAmount === "ovc"
          ? metarOvcSymbol
          : props.ceilingAmount === "missing"
            ? metarMissingSymbol
            : metarClearSymbol;
  return (
    <g className={`metarSymbol ${categoryClass}`} aria-hidden="true">
      <RenderNavSymbolLayers layers={layers} />
    </g>
  );
}

function MetarSymbol(props: { feature: VisibleMetarFeature }) {
  const { feature } = props;
  return (
    <g className="mapUpright">
      <MetarGlyph
        flightCategory={feature.flight_category}
        ceilingAmount={feature.ceiling_amount}
      />
    </g>
  );
}

function pirepStrokeColor(symbol: string): string {
  switch (symbol) {
    case "light-turbulence":
      return "#e9be5e";
    case "moderate-turbulence":
      return "#e79347";
    case "severe-turbulence":
      return "#d24700";
    case "light-icing":
      return "#64c6e9";
    case "moderate-icing":
      return "#3c7ee0";
    case "severe-icing":
      return "#0018e0";
    default:
      return "#071015";
  }
}

function PirepSymbol(props: { feature: VisiblePirepFeature; scale?: number }) {
  const { feature, scale = 1 } = props;
  const layers = feature.symbol === "light-turbulence"
    ? pirepLightTurbulenceSymbol
    : feature.symbol === "moderate-turbulence"
      ? pirepModerateTurbulenceSymbol
      : feature.symbol === "severe-turbulence"
        ? pirepSevereTurbulenceSymbol
        : feature.symbol === "light-icing"
          ? pirepLightIcingSymbol
          : feature.symbol === "moderate-icing"
            ? pirepModerateIcingSymbol
            : feature.symbol === "severe-icing"
              ? pirepSevereIcingSymbol
              : pirepGenericSymbol;
  return (
    <g className="mapUpright">
      <g
        className="pirepSymbol"
        transform={scale === 1 ? undefined : `scale(${scale})`}
        style={{ "--pirep-color": pirepStrokeColor(feature.symbol) } as CSSProperties}
        aria-hidden="true"
      >
        <RenderNavSymbolLayers layers={layers} />
      </g>
    </g>
  );
}

function AdsbTrafficSymbol(props: { feature: MapOverlayQueryResult["visible_traffic"][number] }) {
  const { feature } = props;
  return (
    <g aria-hidden="true">
      <g transform={`rotate(${feature.track_deg_true ?? 0})`}>
        <path
          d="M 0 -11 L 8 9 L 0 5 L -8 9 Z"
          fill="none"
          stroke={loadedUiTheme.aviation.traffic_contrast}
          strokeWidth="5"
          strokeLinejoin="round"
        />
        <path
          d="M 0 -11 L 8 9 L 0 5 L -8 9 Z"
          fill={loadedUiTheme.aviation.traffic}
          stroke={loadedUiTheme.aviation.traffic_contrast}
          strokeWidth="1.25"
          strokeLinejoin="round"
        />
      </g>
      <g transform="translate(13 0)">
        <g className="mapUpright">
          <text
            x="0"
            y="-2"
            dominantBaseline="auto"
            fill={loadedUiTheme.aviation.traffic_label}
            stroke={loadedUiTheme.aviation.traffic_contrast}
            strokeWidth="3"
            paintOrder="stroke"
            fontSize="12"
            fontWeight="800"
          >
            <tspan x="0">{feature.label}</tspan>
            <tspan x="0" dy="1.1em">{feature.detail_label}</tspan>
          </text>
        </g>
      </g>
    </g>
  );
}

function PlanWaypointSymbol(props: {
  feature: NavSymbolFeature | null;
  weatherBadge?: FlightPlanWeatherBadgeUiView | null;
}) {
  const { feature, weatherBadge } = props;
  if (!feature) {
    return null;
  }
  return (
    <svg
      className="planWaypointSymbol"
      viewBox="-20 -20 40 40"
      aria-hidden="true"
      data-testid={weatherBadge ? `parity:plan-weather-badge:${weatherBadge.flight_category}` : undefined}
    >
      <VectorPointSymbol feature={feature} showLabel={false} />
      {weatherBadge ? (
        <g className="planWaypointWeatherBadge" transform="translate(10 10) scale(1)">
          <MetarGlyph
            flightCategory={weatherBadge.flight_category}
            ceilingAmount={weatherBadge.ceiling_amount}
          />
        </g>
      ) : null}
    </svg>
  );
}

function waypointSuggestionName(suggestion: WaypointIdentifierSuggestion): string | null {
  const displayName = suggestion.display_name.replace(/\s+/g, " ").trim();
  if (
    !displayName ||
    displayName.toUpperCase() === suggestion.kind.toUpperCase() ||
    displayName.toUpperCase() === suggestion.identifier.toUpperCase()
  ) {
    return null;
  }
  return displayName;
}

function waypointSuggestionDistance(suggestion: WaypointIdentifierSuggestion): string {
  return suggestion.distance_text;
}

function navRefFromWaypointSuggestion(navRef: WaypointSuggestionNavRef): NavRef {
  switch (navRef.kind) {
    case "airport": return { Airport: navRef.code };
    case "navaid": return { Navaid: navRef.code };
    case "arinc_navaid": return { ArincNavaid: {
      identifier: navRef.identifier,
      icao_code: navRef.icao_code,
      section_code: navRef.section_code,
      subsection_code: navRef.subsection_code,
    } };
    case "terminal_navaid": return { TerminalNavaid: {
      airport_id: navRef.airport_id,
      identifier: navRef.identifier,
      icao_code: navRef.icao_code,
      section_code: navRef.section_code,
      subsection_code: navRef.subsection_code,
    } };
    case "fix": return { Fix: navRef.code };
    case "lat_lon": return { LatLon: navRef.position };
    case "spot": return { Spot: navRef.position };
  }
}

function WaypointButtonContent(props: {
  label: string;
  symbolFeature: NavSymbolFeature | null | undefined;
  weatherBadge?: FlightPlanWeatherBadgeUiView | null;
  details?: Array<string | null | undefined>;
  indented?: boolean;
  fullWidthLabel?: boolean;
}) {
  const details = (props.details ?? []).filter((detail): detail is string => Boolean(detail?.trim()));
  const fullWidthLabel = flightPlanWaypointUsesFullWidthLabel(
    Boolean(props.fullWidthLabel),
    Boolean(props.symbolFeature),
  );
  return (
    <>
      <span
        className={`planStructuredLabel${props.indented ? " isIndented" : ""}${details.length > 0 ? " hasDetails" : ""}${fullWidthLabel ? " isFullWidth" : ""}`}
      >
        <span className="waypointButtonTitle">{props.label}</span>
        {details.map((detail, index) => (
          <span key={`${index}:${detail}`} className="waypointButtonDetail">{detail}</span>
        ))}
      </span>
      {fullWidthLabel ? null : (
        <PlanWaypointSymbol
          feature={props.symbolFeature ?? null}
          weatherBadge={props.weatherBadge}
        />
      )}
    </>
  );
}

function emptyPlaybackUiState(): PlaybackUiState {
  return {
    status: "empty",
    source_path: null,
    title_label: "Playback",
    registration: null,
    icao: null,
    aircraft_type: null,
    point_count: 0,
    duration_seconds: 0,
    cursor_seconds: 0,
    cursor_label: "0:00",
    duration_label: "0:00",
    rate: 1,
    tick_interval_ms: 100,
    speed_profile_norm: [],
    altitude_profile_norm: [],
    gap_spans: [],
  };
}

function emptyMapFollowUiState(): MapFollowUiState {
  return {
    can_center_here: false,
    following: true,
  };
}

function defaultUiMapLayerState(): UiMapLayerState {
  return {
    options: [],
    world_basemap: { visible: true, enabled: true },
    vectors: { visible: true, enabled: true },
    metars: { visible: true, enabled: true },
    nexrad: { visible: false, enabled: true },
    traffic: { visible: false, enabled: true },
    terrain_warning: { visible: true, enabled: true },
    offline_regions: { visible: false, enabled: true },
  };
}

function mapLayerToggleState(state: UiMapLayerState, layerId: MapLayerId): UiMapLayerToggleState {
  switch (layerId) {
    case "world_basemap": return state.world_basemap;
    case "vectors": return state.vectors;
    case "metars": return state.metars;
    case "nexrad": return state.nexrad;
    case "traffic": return state.traffic;
    case "terrain_warning": return state.terrain_warning;
    case "offline_regions": return state.offline_regions;
  }
}

function normalizeUiMapLayerState(state: Partial<UiMapLayerState> | null | undefined): UiMapLayerState {
  return {
    ...defaultUiMapLayerState(),
    ...state,
  };
}

function defaultUiDebugState(): UiDebugState {
  const debugTiles = typeof window !== "undefined" && new URLSearchParams(window.location.search).has("debugTiles");
  return {
    tile_labels: debugTiles,
    nexrad_tile_labels: false,
    fast_tiles: false,
    offline_simulated_clock_buttons: false,
    sequencing_finish_lines: false,
    plate_flight_plan: false,
    bad_autopilot: false,
    internet_adsb: false,
    gps_capture: false,
    debug_log_to_developer_server: readPersistedDebugLogDeveloperServerUploadEnabled(),
  };
}

function defaultUiPlaybackPanelState(): UiPlaybackPanelState {
  return {
    visible: false,
  };
}

function emptyNexradOverlayStats(): NexradOverlayQueryResult["stats"] {
  return {
    source_tile_count: 0,
    render_piece_count: 0,
    split_count: 0,
    max_affine_error_px: 0,
    level_pixel_span_px: 0,
    max_level_pixel_stretch_px: 0,
    max_stack_depth: 0,
    res: null,
    observed_at_utc: null,
  };
}

function emptyNexradOverlayAnimation(): NexradOverlayQueryResult["animation"] {
  return {
    phase: "idle",
    selected_frame_index: null,
    frame_count: 0,
    age_labels: [],
    age_summary: "---",
    next_update_delay_ms: null,
    next_update_epoch_ms: null,
  };
}

const webSessionRenderCounts = {
  app: 0,
  map: 0,
  charts: 0,
  mapCommits: 0,
  mapCommitSources: {} as Record<string, number>,
};

function recordSessionRender(scope: "app" | "map" | "charts") {
  webSessionRenderCounts[scope] += 1;
}

function useMapCommitProbe(values: Record<string, unknown>) {
  const previousRef = useRef<Record<string, unknown> | null>(null);
  useLayoutEffect(() => {
    const previous = previousRef.current;
    previousRef.current = values;
    webSessionRenderCounts.mapCommits += 1;
    if (previous === null) return;
    const changed = Object.keys(values).filter((key) => !Object.is(previous[key], values[key]));
    const sources = changed.length > 0 ? changed : ["unattributed"];
    for (const source of sources) {
      webSessionRenderCounts.mapCommitSources[source] =
        (webSessionRenderCounts.mapCommitSources[source] ?? 0) + 1;
    }
  });
}

function useSessionSnapshotGroups(
  store: SessionRenderStore,
  groups: readonly UiSessionUpdateGroup[],
): UiSessionSnapshot {
  const subscribe = useCallback(
    (listener: () => void) => store.subscribe(groups, listener),
    [groups, store],
  );
  const getSnapshot = useCallback(() => store.snapshot, [store]);
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}

function useRenderValue<T>(store: RenderValueStore<T>, enabled: boolean): T {
  const subscribe = useCallback(
    (listener: () => void) => enabled ? store.subscribe(listener) : () => {},
    [enabled, store],
  );
  const getSnapshot = useCallback(() => store.value, [store]);
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}

function requireMapViewport(viewport: MapViewportState | null): MapViewportState {
  if (viewport === null) throw new Error("map page rendered before its viewport was initialized");
  return viewport;
}

const PageLayer = memo(
  function PageLayer(props: { active: boolean; children: ReactNode }) {
    return (
      <div className={`pageLayer${props.active ? " isActive" : ""}`} aria-hidden={!props.active}>
        {props.children}
      </div>
    );
  },
  (previous, next) => !previous.active && !next.active,
);

const RenderDependencyBoundary = memo(
  function RenderDependencyBoundary(props: {
    dependencies: readonly unknown[];
    render: () => ReactNode;
  }) {
    return props.render();
  },
  (previous, next) =>
    previous.dependencies.length === next.dependencies.length
    && previous.dependencies.every((value, index) => Object.is(value, next.dependencies[index])),
);

const appThemeVars = {
  "--theme-button-checked": controlTheme.button_checked,
  "--theme-button-unchecked": controlTheme.button_unchecked,
  "--theme-header-button": controlTheme.header_button,
  "--theme-button-disabled": controlTheme.button_disabled,
  "--theme-button-disabled-icon-saturation": String(controlTheme.button_disabled_icon_saturation),
  "--theme-button-disabled-icon-opacity": String(controlTheme.button_disabled_icon_opacity),
  "--theme-disabled-accent-percent": `${plateFolderTheme.disabled_accent_percent}%`,
  "--theme-plate-notam-badge-bg": plateFolderTheme.notam_badge_bg,
  "--theme-plate-notam-badge-fg": plateFolderTheme.notam_badge_fg,
  "--theme-plate-notam-badge-stroke": plateFolderTheme.notam_badge_stroke,
  "--theme-button-fg": controlTheme.button_fg,
  "--theme-control-group-bg": controlTheme.control_group_bg,
  "--theme-text-input-bg": controlTheme.text_input_bg,
  "--theme-button-icon-secondary": controlTheme.button_icon_secondary,
  "--theme-flight-plan-active": loadedUiTheme.flight_plan_route.active,
  "--theme-flight-plan-guidance-arrow": loadedUiTheme.flight_plan_route.guidance_arrow,
  "--theme-panel-bg": controlTheme.panel_bg,
  "--theme-panel-border": controlTheme.panel_border,
  "--theme-panel-fg": controlTheme.panel_fg,
  "--theme-panel-muted": controlTheme.panel_muted,
  "--theme-map-selection-display-bg": controlTheme.map_selection_display_bg,
  "--theme-map-selection-display-fg": controlTheme.map_selection_display_fg,
  "--theme-situation-status-bg": controlTheme.situation_status_bg,
  "--theme-situation-status-fg": controlTheme.situation_status_fg,
  "--theme-situation-status-unavailable-fg": controlTheme.situation_status_unavailable_fg,
  "--theme-data-status-ok-bg": controlTheme.data_status_ok_bg,
  "--theme-data-status-ok-stroke": controlTheme.data_status_ok_stroke,
  "--theme-data-status-info-bg": controlTheme.data_status_info_bg,
  "--theme-data-status-info-stroke": controlTheme.data_status_info_stroke,
  "--theme-data-status-caution-bg": controlTheme.data_status_caution_bg,
  "--theme-data-status-caution-stroke": controlTheme.data_status_caution_stroke,
  "--theme-data-status-warning-bg": controlTheme.data_status_warning_bg,
  "--theme-data-status-warning-stroke": controlTheme.data_status_warning_stroke,
  "--theme-data-status-unavailable-bg": controlTheme.data_status_unavailable_bg,
  "--theme-data-status-unavailable-stroke": controlTheme.data_status_unavailable_stroke,
  "--theme-data-status-quiet-bg": controlTheme.data_status_quiet_bg,
  "--theme-data-status-quiet-stroke": controlTheme.data_status_quiet_stroke,
  "--theme-chart-surface-bg": controlTheme.chart_surface_bg,
  "--theme-flight-data-bg": controlTheme.flight_data_bg,
  "--theme-flight-data-border": controlTheme.flight_data_border,
  "--theme-flight-data-label": controlTheme.flight_data_label,
  "--theme-flight-data-value": controlTheme.flight_data_value,
  "--theme-flight-data-missing-value": controlTheme.flight_data_missing_value,
  "--theme-flight-data-passed-value": controlTheme.flight_data_passed_value,
  "--theme-flight-data-active-value": controlTheme.flight_data_active_value,
  "--theme-flight-data-modeled-value": controlTheme.flight_data_modeled_value,
  "--theme-cdi-pointer": controlTheme.cdi_pointer,
  "--theme-compass-north": controlTheme.compass_north,
  "--theme-compass-south": controlTheme.compass_south,
  "--theme-class-b-d-blue": loadedUiTheme.aviation.class_b_d_blue,
  "--theme-class-c-magenta": loadedUiTheme.aviation.class_c_magenta,
  "--theme-tfr-active": loadedUiTheme.aviation.tfr_active,
  "--theme-tfr-upcoming": loadedUiTheme.aviation.tfr_upcoming,
  "--theme-intersection-cyan": loadedUiTheme.aviation.intersection_cyan,
  "--theme-aviation-dark-gray": loadedUiTheme.aviation.dark_gray,
  "--theme-obstacle-danger": loadedUiTheme.aviation.obstacle_danger,
  "--theme-obstacle-caution": loadedUiTheme.aviation.obstacle_caution,
  "--theme-obstacle-muted": loadedUiTheme.aviation.obstacle_muted,
  "--theme-obstacle-under": loadedUiTheme.aviation.obstacle_under,
  "--theme-airport-runway-paved": loadedUiTheme.aviation.airport_runway_paved,
  "--theme-airport-runway-turf": loadedUiTheme.aviation.airport_runway_turf,
  "--theme-airport-runway-unpaved": loadedUiTheme.aviation.airport_runway_unpaved,
  "--theme-airport-runway-water": loadedUiTheme.aviation.airport_runway_water,
  "--theme-airport-runway-inactive": loadedUiTheme.aviation.airport_runway_inactive,
  "--theme-airport-runway-pattern": loadedUiTheme.aviation.airport_runway_pattern,
} as CSSProperties;

export default function App() {
  if (appPageForCurrentPath() === "about") {
    return (
      <main className="appShell" style={appThemeVars}>
        <AboutPage />
      </main>
    );
  }
  return <OperationalApp />;
}

function OperationalApp() {
  recordSessionRender("app");
  const [sessionStartMs] = useState(() => Date.now());
  const initialDebugState = useMemo(defaultUiDebugState, []);
  const persistedUiState = useMemo(readPersistedWebUiState, []);
  const initialPage = useMemo(() => appPageForCurrentPath() ?? persistedUiState.page ?? "map", [persistedUiState.page]);
  const [page, setPage] = useState<AppPage>(initialPage);
  const [persistedPage, setPersistedPage] = useState<AppPage>(initialPage);
  const [mapOrientationMode, setMapOrientationMode] = useState<MapOrientationMode>(
    persistedUiState.mapOrientationMode ?? "north",
  );
  const [pageHistory, setPageHistory] = useState<AppViewSnapshot[]>([]);
  const [flightPlanWeatherModal, setFlightPlanWeatherModal] = useState<WeatherDetailUiView | null>(null);
  const [appCoreAdapter, setAppCoreAdapter] = useState<AppCoreAdapter | null>(null);
  const [adapterBackend, setAdapterBackend] = useState<AdapterBackendKind>("wasm");
  const [sessionInitError, setSessionInitError] = useState<string | null>(null);
  const [startupProgress, setStartupProgress] = useState<StartupProgress>(initialStartupProgress);
  const [startupFatalError, setStartupFatalError] = useState<StartupFatalError | null>(null);
  const startupProgressRef = useRef<StartupProgress>(startupProgress);
  const startupFatalErrorRef = useRef<StartupFatalError | null>(null);
  const startupResolvedRef = useRef(false);
  const startupVisualReadyRef = useRef(false);
  const pageTilePaintTimingRef = useRef<WebPageTilePaintTiming | null>(null);
  const nextPageTilePaintTimingIdRef = useRef(1);
  const highLatencyWarningsSuppressedRef = useRef(true);
  const highLatencyWarningTimerRef = useRef<number | null>(null);
  const [rasterMapState, setRasterMapState] = useState<RasterMapUiState | null>(null);
  const [mapSelectorLoadError, setMapSelectorLoadError] = useState<string | null>(null);
  const initialRecentAirportIds = useMemo(
    () => persistedUiState.recentAirportIds ?? [],
    [persistedUiState],
  );
  const initialChartPageState = useMemo<DerivedChartPageState>(
    () => ({
      airports: emptyChartPage.airports,
      reference_families: [],
      airport_menu_entries: [],
      recent_airport_ids: initialRecentAirportIds,
      selected_airport_id: persistedUiState.selectedAirportId ?? initialRecentAirportIds[0] ?? "",
      selected_reference_family_id: null,
      selected_chart_id: persistedUiState.selectedChartId ?? "",
      suggested_chart_ids: [],
      collection_control: { launcher_label: "", enabled: false },
      chart_control: { launcher_label: "", enabled: false },
      procedure_load_menu: {
        procedure_kind: null,
        launcher_label: "",
        header: "",
        header_tone: "normal",
        enabled: false,
        options: [],
      },
      procedure_geometry_status: {
        boxes: [],
        launcher_count: null,
        launcher_severity: "ok",
      },
      status_controls: { controls: [] },
    }),
    [initialRecentAirportIds, persistedUiState.selectedAirportId, persistedUiState.selectedChartId],
  );
  const [uiSession, setUiSession] = useState<UiSession | null>(null);
  const cloudPumpWorkRef = useRef<() => Promise<void>>(async () => {});
  const cloudPumpRunnerRef = useRef<CoalescedAsyncRunner | null>(null);
  const cloudPumpRunner = cloudPumpRunnerRef.current
    ?? (cloudPumpRunnerRef.current = new CoalescedAsyncRunner(
      () => cloudPumpWorkRef.current(),
    ));
  const cloudEventStreamRef = useRef<{
    streamId: number;
    source: EventSource;
    connectTimer: number | null;
    idleTimer: number | null;
  } | null>(null);
  const cloudEventReportQueueRef = useRef<Promise<void>>(Promise.resolve());
  const webIdleState = useWebIdleState();
  const uiInvalidationStoreRef = useRef<RenderValueStore<UiInvalidationRevisions> | null>(null);
  const uiInvalidationStore = uiInvalidationStoreRef.current
    ?? (uiInvalidationStoreRef.current = new RenderValueStore(initialUiInvalidationRevisions()));
  const sessionSnapshotRefreshInFlightRef = useRef(false);
  const sessionSnapshotRefreshTimerRef = useRef<number | null>(null);
  const appliedSessionRevisionRef = useRef(0);
  const cycleProductFreshnessTimerRef = useRef<number | null>(null);
  const navDbMaintenanceTimerRef = useRef<number | null>(null);
  const cloudRefreshTimerRef = useRef<number | null>(null);
  const [sessionSnapshot, setSessionSnapshot] = useState<UiSessionSnapshot>({
    ui_contract_version: UI_SESSION_PAGE_CONTRACTS_WIRE_VERSION,
    session_revision: 0,
    flight_plan_route_revision: 0,
    nav_data_epoch: 0,
    active_nav_db: null,
    next_nav_db_maintenance_epoch_ms: null,
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
          magnetic_variation_deg: null,
          speed_kt: null,
          altitude_msl_ft: null,
          pressure_altitude_ft: null,
          terrain_altitude_bucket_ft: null,
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
    playback_ui_state: emptyPlaybackUiState(),
    playback_panel_state: defaultUiPlaybackPanelState(),
    map_follow_ui_state: emptyMapFollowUiState(),
    map_follow_target_viewport: null,
    chart_page_state: {
      ordered_airport_ids: initialChartPageState.airports.map((airport) => airport.id),
      recent_airport_ids: initialChartPageState.recent_airport_ids,
      plate_target_airport_id: null,
      selected_airport_id: initialChartPageState.selected_airport_id,
      selected_reference_family_id: null,
      selected_chart_id: initialChartPageState.selected_chart_id,
      suggested_chart_ids: [],
    },
    map_layer_state: defaultUiMapLayerState(),
    data_status_state: {
      boxes: [],
      launcher_count: null,
      launcher_severity: "info",
    },
    map_status_controls: { controls: [] },
    data_status_page_state: {
      title: "Status",
      summary: "Status will appear after core session data loads.",
      rows: [],
    },
    settings_page_state: {
      title: "Settings",
      summary: "No platform settings are available.",
      rows: [],
      sections: [],
    },
    cloud_page_state: {
      action_revision: 0,
      title: "",
      summary: "",
      sync_account_heading: "",
      provider_heading: "",
      overall_status_label: "",
      sync_account_panels: [],
      overall_status: {
        id: "",
        title: "",
        state: "informational",
        time_facts: [],
        actions: [],
      },
      next_refresh_epoch_ms: null,
    },
    offline_package_preferences_json: "{\"regions\":{},\"products\":{}}",
    home_page_state: {
      buttons: [],
    },
    navigation_page_state: {
      options: [],
      max_history_depth: 0,
      default_chart_or_plate_return_target: "map",
    },
    display_policy: null,
    disclaimer_state: {
      agreement_id: "no-warranty-v1",
      required: true,
      html: noWarrantyHtml,
      text: defaultDisclaimerText,
      accept_label: "I understand and agree",
    },
    debug_state: initialDebugState,
    raster_map: null,
    next_session_snapshot_refresh_epoch_ms: 0,
    next_cycle_product_freshness_check_epoch_ms: null,
  });
  const sessionRenderStoreRef = useRef<SessionRenderStore | null>(null);
  const sessionRenderStore = sessionRenderStoreRef.current
    ?? (sessionRenderStoreRef.current = new SessionRenderStore(sessionSnapshot));
  const applySessionSnapshot = useCallback((nextSnapshot: UiSessionSnapshot, source: string) => {
    const nextRevision = nextSnapshot.session_revision;
    const currentRevision = appliedSessionRevisionRef.current;
    if (nextRevision < currentRevision) {
      debugLog("session.snapshot.stale_skip", {
        source,
        next_revision: nextRevision,
        current_revision: currentRevision,
      });
      return false;
    }
    if (nextRevision === currentRevision && sessionRenderStore.snapshot === nextSnapshot) {
      return true;
    }
    sessionRenderStore.replaceUnannouncedSnapshot(nextSnapshot);
    appliedSessionRevisionRef.current = nextRevision;
    debugLog("session.snapshot.apply", {
      source,
      revision: nextRevision,
    });
    setSessionSnapshot(nextSnapshot);
    return true;
  }, [sessionRenderStore]);
  const applySessionSnapshotDispatch = useCallback((nextSnapshot: SetStateAction<UiSessionSnapshot>) => {
    if (typeof nextSnapshot === "function") {
      applySessionSnapshot(nextSnapshot(sessionRenderStore.snapshot), "session_callback");
      return;
    }
    applySessionSnapshot(nextSnapshot, "session_callback");
  }, [applySessionSnapshot, sessionRenderStore]);
  const performStatusAction = useCallback(async (actionId: string) => {
    if (!uiSession) {
      return;
    }
    const decision = await uiSession.statusActionDecision(actionId);
    if (decision.perform_session_mutation) {
      const nextSnapshot = await uiSession.performStatusAction(actionId);
      applySessionSnapshot(nextSnapshot, "status_action");
    }
    if (decision.platform_effect?.kind === "reload_application") {
      window.location.reload();
    }
  }, [applySessionSnapshot, uiSession]);

  useEffect(() => {
    const render = () => ({
      ...webSessionRenderCounts,
      profilers: webReactProfilerTotals,
      store: sessionRenderStore.stats,
      session_revision: sessionRenderStore.snapshot.session_revision,
    });
    window.__aerobagE2e = { ...(window.__aerobagE2e ?? {}), render };
    return () => {
      if (window.__aerobagE2e?.render === render) delete window.__aerobagE2e.render;
    };
  }, [sessionRenderStore]);

  cloudPumpWorkRef.current = async () => {
    if (!uiSession) {
      return;
    }
    try {
      for (let step = 0; step < 32; step += 1) {
        const request = await uiSession.takeCloudProviderRequest(Date.now());
        if (!request) {
          return;
        }
        const response = await executeCloudHttpRequest(request);
        const nextSnapshot = await uiSession.completeCloudProviderRequest(
          request.request_id,
          response,
          Date.now(),
        );
        applySessionSnapshot(nextSnapshot, "cloud_provider_completion");
      }
      throw new Error("cloud provider pump exceeded its bounded work batch");
    } catch (error) {
      debugLog("cloud.provider_pump.failed", { error: errorMessage(error) });
    }
  };
  const pumpCloudProvider = useCallback(
    () => cloudPumpRunner.request(),
    [cloudPumpRunner],
  );

  const reconcileCloudEventStream = useCallback(async () => {
    if (!uiSession) {
      return;
    }
    const plan = await uiSession.cloudEventStreamPlan();
    const current = cloudEventStreamRef.current;
    if (current?.streamId === plan?.stream_id) {
      return;
    }
    if (current) {
      current.source.close();
      if (current.connectTimer !== null) window.clearTimeout(current.connectTimer);
      if (current.idleTimer !== null) window.clearTimeout(current.idleTimer);
      cloudEventStreamRef.current = null;
    }
    if (!plan) {
      return;
    }

    const source = new EventSource(plan.url);
    const runtime = {
      streamId: plan.stream_id,
      source,
      connectTimer: null as number | null,
      idleTimer: null as number | null,
    };
    cloudEventStreamRef.current = runtime;
    const closeRuntime = () => {
      source.close();
      if (runtime.connectTimer !== null) window.clearTimeout(runtime.connectTimer);
      if (runtime.idleTimer !== null) window.clearTimeout(runtime.idleTimer);
      if (cloudEventStreamRef.current === runtime) {
        cloudEventStreamRef.current = null;
      }
    };
    const report = (kind: "connecting" | "connected" | "message" | "error" | "closed" | "idle_timeout", data?: string, detail?: string) => {
      cloudEventReportQueueRef.current = cloudEventReportQueueRef.current.then(async () => {
        const nextSnapshot = await uiSession.reportCloudEventStreamEvent({
          stream_id: plan.stream_id,
          kind,
          data: data ?? null,
          detail: detail ?? null,
        }, Date.now());
        applySessionSnapshot(nextSnapshot, `cloud_event_stream_${kind}`);
      }).catch((error) => {
        debugLog("cloud.event_stream.report_failed", { error: errorMessage(error), kind });
      });
    };
    const armIdleTimer = () => {
      if (runtime.idleTimer !== null) window.clearTimeout(runtime.idleTimer);
      runtime.idleTimer = window.setTimeout(() => {
        closeRuntime();
        report("idle_timeout", undefined, "Aerobag Cloud notification stream went quiet.");
      }, plan.idle_timeout_ms);
    };
    report("connecting");
    runtime.connectTimer = window.setTimeout(() => {
      closeRuntime();
      report("error", undefined, "Aerobag Cloud notification stream did not connect in time.");
    }, plan.connect_timeout_ms);
    source.onopen = () => {
      if (runtime.connectTimer !== null) {
        window.clearTimeout(runtime.connectTimer);
        runtime.connectTimer = null;
      }
      armIdleTimer();
      report("connected");
    };
    const onMessage = (event: MessageEvent<string>) => {
      armIdleTimer();
      report("message", event.data);
    };
    for (const eventName of ["ready", "root-changed", "reset", "heartbeat"]) {
      source.addEventListener(eventName, onMessage as EventListener);
    }
    source.onerror = () => {
      closeRuntime();
      report("error", undefined, "Aerobag Cloud notification stream disconnected.");
    };
  }, [applySessionSnapshot, uiSession]);

  useEffect(() => {
    if (!__AEROBAG_E2E_ENABLED__ || !uiSession) {
      return;
    }
    const cloud = {
      state: () => ({
        offline_package_preferences: JSON.parse(sessionSnapshot.offline_package_preferences_json),
        overall_status: sessionSnapshot.cloud_page_state.overall_status,
        provider_status: sessionSnapshot.data_status_page_state.rows.find(
          (row) => row.id === "cloud:provider",
        ) ?? null,
        event_stream_id: cloudEventStreamRef.current?.streamId ?? null,
        flight_plan_rows: sessionSnapshot.app_ui_state.active_plan?.display_rows.map((row) => row.label) ?? [],
      }),
      setOfflinePackagePreferences: async (preferences: unknown) => {
        const nextSnapshot = await uiSession.recordOfflinePackagePreferences(
          JSON.stringify(preferences),
          Date.now(),
        );
        applySessionSnapshot(nextSnapshot, "e2e_offline_package_preferences");
        await pumpCloudProvider();
      },
      dropEventStream: async () => {
        const current = cloudEventStreamRef.current;
        if (!current) {
          throw new Error("Aerobag Cloud event stream is not connected");
        }
        current.source.close();
        if (current.connectTimer !== null) window.clearTimeout(current.connectTimer);
        if (current.idleTimer !== null) window.clearTimeout(current.idleTimer);
        cloudEventStreamRef.current = null;
        const nextSnapshot = await uiSession.reportCloudEventStreamEvent({
          stream_id: current.streamId,
          kind: "error",
          data: null,
          detail: "Event stream dropped by browser regression harness.",
        }, Date.now());
        applySessionSnapshot(nextSnapshot, "e2e_cloud_event_stream_drop");
      },
    };
    window.__aerobagE2e = { ...(window.__aerobagE2e ?? {}), cloud };
    return () => {
      if (window.__aerobagE2e?.cloud === cloud) {
        delete window.__aerobagE2e.cloud;
      }
    };
  }, [
    applySessionSnapshot,
    pumpCloudProvider,
    sessionSnapshot.cloud_page_state.overall_status,
    sessionSnapshot.app_ui_state.active_plan,
    sessionSnapshot.data_status_page_state,
    sessionSnapshot.offline_package_preferences_json,
    uiSession,
  ]);

  useEffect(() => {
    if (!uiSession) {
      return;
    }
    void pumpCloudProvider();
    void reconcileCloudEventStream();
    const timer = window.setInterval(() => {
      void pumpCloudProvider();
      void reconcileCloudEventStream();
    }, 1_000);
    return () => {
      window.clearInterval(timer);
      const current = cloudEventStreamRef.current;
      if (current) {
        current.source.close();
        if (current.connectTimer !== null) window.clearTimeout(current.connectTimer);
        if (current.idleTimer !== null) window.clearTimeout(current.idleTimer);
        cloudEventStreamRef.current = null;
      }
    };
  }, [pumpCloudProvider, reconcileCloudEventStream, uiSession]);

  // Core's cloud publication is the wakeup edge for provider effects. Do not
  // depend on a React render or a throttled background-page timer to publish
  // a user mutation.
  useEffect(() => sessionRenderStore.subscribe(
    CLOUD_EFFECT_SESSION_UPDATE_GROUPS,
    () => { void pumpCloudProvider(); },
  ), [pumpCloudProvider, sessionRenderStore]);

  // A core mutation can enqueue cloud work from any feature. Start that work
  // from the committed session revision instead of waiting for the polling
  // backstop, which browsers may throttle when the page is not foregrounded.
  useEffect(() => {
    void pumpCloudProvider();
  }, [pumpCloudProvider, sessionSnapshot.session_revision]);

  const performCloudPageAction = useCallback(async (
    actionId: CloudUiActionId,
    fields: CloudUiFieldValue[],
    platformEffect: CloudPlatformEffect | null,
  ): Promise<string | null> => {
    if (!uiSession) {
      return null;
    }
    let nextSnapshot: UiSessionSnapshot;
    nextSnapshot = await uiSession.performCloudUiAction(actionId, fields, Date.now());
    applySessionSnapshot(nextSnapshot, `cloud_action_${actionId}`);
    if (platformEffect?.kind === "copy_text") {
      await navigator.clipboard.writeText(platformEffect.text);
      return platformEffect.completion_label;
    }
    await pumpCloudProvider();
    return null;
  }, [applySessionSnapshot, pumpCloudProvider, uiSession]);
  useEffect(() => {
    if (typeof window === "undefined") {
      return;
    }
    const liveFeeds = () => webE2eLiveFeedStatus(sessionSnapshot.data_status_page_state);
    window.__aerobagE2e = {
      ...(window.__aerobagE2e ?? {}),
      liveFeeds,
    };
    return () => {
      if (window.__aerobagE2e?.liveFeeds === liveFeeds) {
        delete window.__aerobagE2e.liveFeeds;
      }
    };
  }, [sessionSnapshot.data_status_page_state]);
  const [playbackSourcePath, setPlaybackSourcePath] = useState(defaultPlaybackTracePath);
  const [derivedChartPageState, setDerivedChartPageState] = useState<DerivedChartPageState>(initialChartPageState);
  const [chartPageStateLoadError, setChartPageStateLoadError] = useState<string | null>(null);
  const logDebugWarning = useCallback((tag: string, data?: unknown) => {
    debugLog(tag, data);
    debugLog("debug.warn.latched", { tag, data });
  }, []);
  const logHighLatencyWarning = useCallback((tag: string, data?: unknown) => {
    if (highLatencyWarningsSuppressedRef.current) {
      debugLog(`${tag}.startup_suppressed`, data);
      return;
    }
    logDebugWarning(tag, data);
  }, [logDebugWarning]);
  const markStartupProgress = useCallback((phase: string, detail?: string) => {
    const progress = { phase, detail, updatedAtMs: Date.now() };
    startupProgressRef.current = progress;
    debugLog("startup.progress", { phase, detail });
    setStartupProgress(progress);
  }, []);
  const reportStartupFatalError = useCallback((source: string, error: unknown) => {
    if (startupResolvedRef.current || startupFatalErrorRef.current) {
      return;
    }
    const fatal = startupErrorFromUnknown(source, error, startupProgressRef.current, sessionStartMs);
    startupFatalErrorRef.current = fatal;
    debugLog("startup.fatal", fatal);
    setSessionInitError(`${fatal.source}: ${fatal.message}`);
    setStartupFatalError(fatal);
  }, [sessionStartMs]);
  const applySituationControlInput = useCallback(async (input: SituationControlInput) => {
    if (!uiSession) {
      return;
    }
    applySessionSnapshot(await uiSession.applySituationControlInput(input, Date.now()), "situation_control_input");
  }, [applySessionSnapshot, uiSession]);
  const acceptDisclaimer = useCallback(async () => {
    if (!uiSession) {
      return;
    }
    applySessionSnapshot(
      await uiSession.acceptDisclaimer(sessionSnapshot.disclaimer_state.agreement_id),
      "accept_disclaimer",
    );
  }, [applySessionSnapshot, sessionSnapshot.disclaimer_state.agreement_id, uiSession]);
  const reportStartupVisualReady = useCallback(() => {
    if (startupVisualReadyRef.current) {
      return;
    }
    startupVisualReadyRef.current = true;
    if (typeof window === "undefined") {
      highLatencyWarningsSuppressedRef.current = false;
      return;
    }
    highLatencyWarningTimerRef.current = window.setTimeout(() => {
      highLatencyWarningsSuppressedRef.current = false;
      highLatencyWarningTimerRef.current = null;
    }, startupHighLatencyWarningGraceMs);
  }, []);
  const mapLayerState = useMemo(
    () => normalizeUiMapLayerState(sessionSnapshot.map_layer_state),
    [sessionSnapshot.map_layer_state],
  );
  const chartPageData: ChartPageData = useMemo(
    () => ({ airports: derivedChartPageState.airports }),
    [derivedChartPageState.airports],
  );
  const airportMenuEntries = derivedChartPageState.airport_menu_entries;

  useEffect(() => installMainThreadResponsivenessInstrumentation(), []);

  useEffect(() => () => {
    if (highLatencyWarningTimerRef.current !== null) {
      window.clearTimeout(highLatencyWarningTimerRef.current);
    }
  }, []);

  useEffect(() => {
    if (typeof window === "undefined") {
      highLatencyWarningsSuppressedRef.current = false;
      return;
    }
    const timer = window.setTimeout(() => {
      if (highLatencyWarningTimerRef.current === null) {
        highLatencyWarningsSuppressedRef.current = false;
      }
    }, startupHighLatencyWarningGraceMs);
    return () => window.clearTimeout(timer);
  }, []);

  const clearSessionSnapshotRefreshTimer = useCallback(() => {
    if (sessionSnapshotRefreshTimerRef.current === null) {
      return;
    }
    window.clearTimeout(sessionSnapshotRefreshTimerRef.current);
    sessionSnapshotRefreshTimerRef.current = null;
  }, []);

  const handleSessionSnapshotRefreshDecisionRef = useRef<(
    decision: SessionSnapshotRefreshDecision,
    source: string,
  ) => void>(() => {});

  const armSessionSnapshotRefreshTimer = useCallback((delayMs: number, reason: string, source: string) => {
    clearSessionSnapshotRefreshTimer();
    const roundedDelayMs = Math.max(0, Math.round(delayMs));
    sessionSnapshotRefreshTimerRef.current = window.setTimeout(() => {
      sessionSnapshotRefreshTimerRef.current = null;
      if (!uiSession) {
        return;
      }
      void uiSession.pollSessionSnapshotRefresh().then((decision) => {
        handleSessionSnapshotRefreshDecisionRef.current(decision, "timer");
      }).catch((error: unknown) => {
        debugLog("session.snapshot.refresh.scheduler_error", {
          source: "timer",
          error: errorMessage(error),
        });
      });
    }, roundedDelayMs);
    debugLog("session.snapshot.refresh.scheduled", {
      reason,
      source,
      delay_ms: roundedDelayMs,
      in_flight: sessionSnapshotRefreshInFlightRef.current,
    });
  }, [clearSessionSnapshotRefreshTimer, uiSession]);

  const startSessionSnapshotRefresh = useCallback((reason: string) => {
    if (!uiSession) {
      return;
    }
    if (sessionSnapshotRefreshInFlightRef.current) {
      debugLog("session.snapshot.refresh.start_while_in_flight", { reason });
      void uiSession.requestSessionSnapshotRefresh("timely", reason).then((decision) => {
        handleSessionSnapshotRefreshDecisionRef.current(decision, "start_while_in_flight");
      }).catch((error: unknown) => {
        debugLog("session.snapshot.refresh.scheduler_error", {
          source: "start_while_in_flight",
          error: errorMessage(error),
        });
      });
      return;
    }
    sessionSnapshotRefreshInFlightRef.current = true;
    debugLog("session.snapshot.refresh.start", { reason });
    void uiSession.snapshot().then((nextSnapshot) => {
      applySessionSnapshot(nextSnapshot, `refresh:${reason}`);
    }).catch((error: unknown) => {
      debugLog("session.snapshot.refresh.error", { reason, error: errorMessage(error) });
    }).finally(() => {
      sessionSnapshotRefreshInFlightRef.current = false;
      void uiSession.sessionSnapshotRefreshCompleted().then((decision) => {
        handleSessionSnapshotRefreshDecisionRef.current(decision, "after_in_flight");
      }).catch((error: unknown) => {
        debugLog("session.snapshot.refresh.scheduler_error", {
          source: "after_in_flight",
          error: errorMessage(error),
        });
      });
    });
  }, [applySessionSnapshot, uiSession]);

  const handleSessionSnapshotRefreshDecision = useCallback((
    decision: SessionSnapshotRefreshDecision,
    source: string,
  ) => {
    if (decision.kind === "idle") {
      clearSessionSnapshotRefreshTimer();
      return;
    }
    if (decision.kind === "schedule") {
      armSessionSnapshotRefreshTimer(decision.delay_ms, decision.reason, source);
      return;
    }
    clearSessionSnapshotRefreshTimer();
    startSessionSnapshotRefresh(`${source}:${decision.reason}`);
  }, [armSessionSnapshotRefreshTimer, clearSessionSnapshotRefreshTimer, startSessionSnapshotRefresh, uiSession]);

  useEffect(() => {
    handleSessionSnapshotRefreshDecisionRef.current = handleSessionSnapshotRefreshDecision;
  }, [handleSessionSnapshotRefreshDecision]);

  useEffect(() => clearSessionSnapshotRefreshTimer, [clearSessionSnapshotRefreshTimer]);

  const requestSessionSnapshotRefresh = useCallback((priority: SessionSnapshotRefreshPriority, reason: string) => {
    if (!uiSession) {
      return;
    }
    void uiSession.requestSessionSnapshotRefresh(priority, reason).then((decision) => {
      handleSessionSnapshotRefreshDecisionRef.current(decision, "request");
    }).catch((error: unknown) => {
      debugLog("session.snapshot.refresh.scheduler_error", {
        source: "request",
        error: errorMessage(error),
      });
    });
  }, [applySessionSnapshot, uiSession]);

  const handleMapViewportGestureActiveChange = useCallback((active: boolean) => {
    if (!uiSession) {
      return;
    }
    void uiSession.sessionSnapshotViewportGestureActiveChanged(active).then((decision) => {
      handleSessionSnapshotRefreshDecisionRef.current(decision, active ? "gesture_active" : "gesture_end");
    }).catch((error: unknown) => {
      debugLog("session.snapshot.refresh.scheduler_error", {
        source: active ? "gesture_active" : "gesture_end",
        error: errorMessage(error),
      });
    });
  }, [uiSession]);

  const handleMapViewportGestureActivity = useCallback(() => {
    if (!uiSession) {
      return;
    }
    void uiSession.sessionSnapshotViewportActivity().then((decision) => {
      handleSessionSnapshotRefreshDecisionRef.current(decision, "viewport_activity");
    }).catch((error: unknown) => {
      debugLog("session.snapshot.refresh.scheduler_error", {
        source: "viewport_activity",
        error: errorMessage(error),
      });
    });
  }, [uiSession]);

  useEffect(() => {
    if (!uiSession) {
      return;
    }
    uiSession.setInvalidationListener((invalidations) => {
      const next = { ...uiInvalidationStore.value };
      for (const invalidation of invalidations) {
        next[invalidation] = (next[invalidation] ?? 0) + 1;
      }
      uiInvalidationStore.publish(next);
      if (invalidations.includes("session_snapshot")) {
        const priority = invalidations.includes("flight_plan_route") ? "timely" : "low_priority";
        requestSessionSnapshotRefresh(priority, "invalidation");
      }
    });
    return () => {
      uiSession.setInvalidationListener(null);
    };
  }, [requestSessionSnapshotRefresh, uiInvalidationStore, uiSession]);

  useEffect(() => {
    if (!uiSession) return;
    uiSession.setProjectionListener((publication) => {
      const nextRevision = publication.snapshot.session_revision;
      if (nextRevision < appliedSessionRevisionRef.current) {
        debugLog("session.projection.stale_skip", {
          next_revision: nextRevision,
          current_revision: appliedSessionRevisionRef.current,
        });
        return;
      }
      if (!sessionRenderStore.publish(publication)) return;
      appliedSessionRevisionRef.current = nextRevision;
      if (publicationAffectsGroups(publication, SHELL_SESSION_UPDATE_GROUPS)) {
        setSessionSnapshot(publication.snapshot);
      }
    });
    return () => uiSession.setProjectionListener(null);
  }, [sessionRenderStore, uiSession]);

  useEffect(() => {
    const deadline = sessionSnapshot.next_session_snapshot_refresh_epoch_ms;
    const timer = window.setTimeout(
      () => requestSessionSnapshotRefresh("low_priority", "core_deadline"),
      Math.max(0, Math.min(deadline - Date.now(), 2_147_000_000)),
    );
    return () => window.clearTimeout(timer);
  }, [
    requestSessionSnapshotRefresh,
    sessionSnapshot.next_session_snapshot_refresh_epoch_ms,
  ]);

  useEffect(() => {
    if (cycleProductFreshnessTimerRef.current !== null) {
      window.clearTimeout(cycleProductFreshnessTimerRef.current);
      cycleProductFreshnessTimerRef.current = null;
    }
    const nextCheckEpochMs = sessionSnapshot.next_cycle_product_freshness_check_epoch_ms;
    if (nextCheckEpochMs === null || nextCheckEpochMs === undefined) {
      return;
    }
    const maxTimerDelayMs = 2_147_000_000;
    const armTimer = () => {
      const delayMs = Math.min(
        Math.max(0, nextCheckEpochMs - Date.now()),
        maxTimerDelayMs,
      );
      cycleProductFreshnessTimerRef.current = window.setTimeout(() => {
        cycleProductFreshnessTimerRef.current = null;
        if (Date.now() >= nextCheckEpochMs) {
          requestSessionSnapshotRefresh("low_priority", "cycle_product_freshness_deadline");
          return;
        }
        armTimer();
      }, delayMs);
      debugLog("cycle_product_freshness.deadline.scheduled", {
        next_check_epoch_ms: nextCheckEpochMs,
        delay_ms: Math.round(delayMs),
      });
    };
    armTimer();
    return () => {
      if (cycleProductFreshnessTimerRef.current !== null) {
        window.clearTimeout(cycleProductFreshnessTimerRef.current);
        cycleProductFreshnessTimerRef.current = null;
      }
    };
  }, [requestSessionSnapshotRefresh, sessionSnapshot.next_cycle_product_freshness_check_epoch_ms]);

  useEffect(() => {
    if (cloudRefreshTimerRef.current !== null) {
      window.clearTimeout(cloudRefreshTimerRef.current);
      cloudRefreshTimerRef.current = null;
    }
    const deadline = sessionSnapshot.cloud_page_state.next_refresh_epoch_ms;
    if (deadline == null) {
      return;
    }
    const delayMs = Math.max(0, Math.min(deadline - Date.now(), 2_147_000_000));
    cloudRefreshTimerRef.current = window.setTimeout(() => {
      cloudRefreshTimerRef.current = null;
      requestSessionSnapshotRefresh("low_priority", "cloud_state_deadline");
    }, delayMs);
    return () => {
      if (cloudRefreshTimerRef.current !== null) {
        window.clearTimeout(cloudRefreshTimerRef.current);
        cloudRefreshTimerRef.current = null;
      }
    };
  }, [requestSessionSnapshotRefresh, sessionSnapshot.cloud_page_state.next_refresh_epoch_ms]);

  useEffect(() => {
    if (navDbMaintenanceTimerRef.current !== null) {
      window.clearTimeout(navDbMaintenanceTimerRef.current);
      navDbMaintenanceTimerRef.current = null;
    }
    if (!uiSession) {
      return;
    }
    const nextCheckEpochMs = sessionSnapshot.next_nav_db_maintenance_epoch_ms;
    if (nextCheckEpochMs === null || nextCheckEpochMs === undefined) {
      return;
    }
    let cancelled = false;
    const runMaintenance = async () => {
      try {
        const nextSnapshot = await uiSession.maintainNavDb(Date.now());
        if (!cancelled) {
          applySessionSnapshot(nextSnapshot, "nav_db_maintenance");
          const nextDeadline = nextSnapshot.next_nav_db_maintenance_epoch_ms;
          if (nextDeadline !== null && nextDeadline <= Date.now()) {
            navDbMaintenanceTimerRef.current = window.setTimeout(runMaintenance, 60_000);
          }
        }
      } catch (error) {
        debugLog("nav_db.maintenance.failed", {
          error: errorMessage(error),
        });
        if (!cancelled) {
          navDbMaintenanceTimerRef.current = window.setTimeout(runMaintenance, 60_000);
        }
      }
    };
    const schedule = () => {
      const delayMs = Math.min(
        Math.max(0, nextCheckEpochMs - Date.now()),
        2_147_000_000,
      );
      navDbMaintenanceTimerRef.current = window.setTimeout(runMaintenance, delayMs);
    };
    const handleVisibilityChange = () => {
      if (document.visibilityState === "visible" && Date.now() >= nextCheckEpochMs) {
        if (navDbMaintenanceTimerRef.current !== null) {
          window.clearTimeout(navDbMaintenanceTimerRef.current);
        }
        void runMaintenance();
      }
    };
    schedule();
    document.addEventListener("visibilitychange", handleVisibilityChange);
    return () => {
      cancelled = true;
      document.removeEventListener("visibilitychange", handleVisibilityChange);
      if (navDbMaintenanceTimerRef.current !== null) {
        window.clearTimeout(navDbMaintenanceTimerRef.current);
        navDbMaintenanceTimerRef.current = null;
      }
    };
  }, [applySessionSnapshot, sessionSnapshot.next_nav_db_maintenance_epoch_ms, uiSession]);

  useEffect(() => {
    const handleVisibilityChange = () => {
      if (document.visibilityState !== "visible") {
        return;
      }
      const nextCheckEpochMs = sessionSnapshot.next_cycle_product_freshness_check_epoch_ms;
      if (nextCheckEpochMs !== null && nextCheckEpochMs !== undefined && Date.now() >= nextCheckEpochMs) {
        requestSessionSnapshotRefresh("low_priority", "cycle_product_freshness_resume");
      }
    };
    document.addEventListener("visibilitychange", handleVisibilityChange);
    return () => document.removeEventListener("visibilitychange", handleVisibilityChange);
  }, [requestSessionSnapshotRefresh, sessionSnapshot.next_cycle_product_freshness_check_epoch_ms]);

  useEffect(() => {
    if (!uiSession) {
      return;
    }
    const handler = (event: KeyboardEvent) => {
      if (event.defaultPrevented || isEditableTarget(event.target)) {
        return;
      }
      const input = situationInputForKey(event.key);
      if (!input) {
        return;
      }
      event.preventDefault();
      void applySituationControlInput(input);
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [applySituationControlInput, uiSession]);

  useEffect(() => {
    if (!uiSession) {
      return;
    }
    let cancelled = false;
    let watchId: number | null = null;
    let selectedAutoAfterFirstFix = false;

    const updateStatus = (connectionState: "unavailable" | "searching" | "connected" | "stale" | "failed", enabled: boolean, statusLabel: string) => {
      void uiSession.updateOwnshipSourceStatus({
        source_id: browserGeolocationSourceId,
        connection_state: connectionState,
        enabled,
        status_label: statusLabel,
      }).then((nextSnapshot) => {
        if (!cancelled) {
          applySessionSnapshot(nextSnapshot, "geolocation_status");
        }
      }).catch((error) => {
        debugLog("geolocation.status_update_failed", { error: errorMessage(error) });
      });
    };

    void (async () => {
      try {
        let nextSnapshot = await uiSession.registerOwnshipSource({
          source_id: browserGeolocationSourceId,
          source_kind: "device_gps",
          display_name: "Browser Location",
          selectable: true,
          auto_eligible: true,
        });
        if (cancelled) {
          return;
        }
        applySessionSnapshot(nextSnapshot, "geolocation_register");

        if (typeof navigator === "undefined" || !navigator.geolocation) {
          updateStatus("unavailable", false, "Browser geolocation unavailable");
          return;
        }

        nextSnapshot = await uiSession.updateOwnshipSourceStatus({
          source_id: browserGeolocationSourceId,
          connection_state: "searching",
          enabled: true,
          status_label: "Waiting for browser location",
        });
        if (cancelled) {
          return;
        }
        applySessionSnapshot(nextSnapshot, "geolocation_searching");

        watchId = navigator.geolocation.watchPosition(
          (position) => {
            const coords = position.coords;
            const eventTimeEpochMs = Number.isFinite(position.timestamp) ? Math.trunc(position.timestamp) : Date.now();
            const receivedTimeEpochMs = Date.now();
            const speedMps = finiteOrNull(coords.speed);
            const altitudeM = finiteOrNull(coords.altitude);
            const sample: SituationSample = {
              source_id: browserGeolocationSourceId,
              source_kind: "device_gps",
              event_time_epoch_ms: eventTimeEpochMs,
              received_time_epoch_ms: receivedTimeEpochMs,
              position: { lat: coords.latitude, lon: coords.longitude },
              horizontal_accuracy_m: finiteOrNull(coords.accuracy),
              vertical_accuracy_m: finiteOrNull(coords.altitudeAccuracy),
              track_deg_true: finiteOrNull(coords.heading),
              heading_deg_true: null,
              ground_speed_kt: speedMps == null ? null : speedMps * metersPerSecondToKnots,
              altitude_msl_ft: altitudeM == null ? null : altitudeM * metersToFeet,
              pressure_altitude_ft: null,
              vertical_speed_fpm: null,
            };
            void uiSession.pushSituationSample(sample).then(async (pushedSnapshot) => {
              if (cancelled) {
                return;
              }
              applySessionSnapshot(pushedSnapshot, "geolocation_sample");
              if (selectedAutoAfterFirstFix) {
                return;
              }
              selectedAutoAfterFirstFix = true;
              const selectedSnapshot = await uiSession.selectOwnshipSource({ kind: "auto" });
              if (!cancelled) {
                applySessionSnapshot(selectedSnapshot, "geolocation_select_auto");
              }
            }).catch((error) => {
              debugLog("geolocation.sample_failed", { error: errorMessage(error) });
            });
          },
          (error) => {
            const permissionDenied = error.code === error.PERMISSION_DENIED;
            updateStatus(
              permissionDenied ? "unavailable" : "failed",
              !permissionDenied,
              permissionDenied ? "Browser location permission denied" : error.message || "Browser location failed",
            );
          },
          {
            enableHighAccuracy: true,
            maximumAge: 1_000,
            timeout: 15_000,
          },
        );
      } catch (error) {
        updateStatus("failed", false, errorMessage(error));
      }
    })();

    return () => {
      cancelled = true;
      if (watchId !== null && typeof navigator !== "undefined" && navigator.geolocation) {
        navigator.geolocation.clearWatch(watchId);
      }
    };
  }, [uiSession]);
  const planUiState = sessionSnapshot.app_ui_state.active_plan;
  const chartPageStateRequestKey = JSON.stringify([
    planUiState?.plan_id ?? null,
    planUiState?.plan_version ?? null,
    sessionSnapshot.chart_page_state,
    sessionSnapshot.notam_display_state_id ?? null,
  ]);
  useEffect(() => {
    if (typeof window === "undefined" || !__AEROBAG_E2E_ENABLED__ || !uiSession) {
      return;
    }
    const navDb = () => ({
      active_nav_db: sessionSnapshot.active_nav_db,
      nav_data_epoch: sessionSnapshot.nav_data_epoch,
      next_nav_db_maintenance_epoch_ms: sessionSnapshot.next_nav_db_maintenance_epoch_ms,
      advance_warning:
        sessionSnapshot.data_status_state.boxes.find((box) => box.id === "nav_db:advance")
        ?? null,
      active_plan: planUiState
        ? {
          plan_id: planUiState.plan_id,
          plan_version: planUiState.plan_version,
        }
        : null,
      plan_ui_state: planUiState
        ? {
          display_rows: planUiState.display_rows.map((row) => ({
            uid: row.uid,
            label: row.label,
            row_kind: row.row_kind,
            component_kind: row.component_kind,
            component_uid: row.component_uid,
            procedure_id: row.procedure_id,
          })),
          guidance: planUiState.guidance,
        }
        : null,
    });
    const navDbMaintainAt = async (nowEpochMs: number) => {
      const nextSnapshot = await uiSession.maintainNavDb(nowEpochMs);
      applySessionSnapshot(nextSnapshot, "e2e_nav_db_maintenance");
    };
    window.__aerobagE2e = {
      ...(window.__aerobagE2e ?? {}),
      navDb,
      navDbMaintainAt,
    };
    return () => {
      if (window.__aerobagE2e?.navDb === navDb) {
        delete window.__aerobagE2e.navDb;
      }
      if (window.__aerobagE2e?.navDbMaintainAt === navDbMaintainAt) {
        delete window.__aerobagE2e.navDbMaintainAt;
      }
    };
  }, [
    applySessionSnapshot,
    planUiState,
    sessionSnapshot.active_nav_db,
    sessionSnapshot.data_status_state.boxes,
    sessionSnapshot.nav_data_epoch,
    sessionSnapshot.next_nav_db_maintenance_epoch_ms,
    uiSession,
  ]);
  const recentAirportIds = derivedChartPageState.recent_airport_ids;
  const selectedAirportId = derivedChartPageState.selected_airport_id;
  const selectedReferenceFamilyId = derivedChartPageState.selected_reference_family_id ?? null;
  const selectedChartId = derivedChartPageState.selected_chart_id;

  const selectedMap = rasterMapState;
  const mapViewportStoreRef = useRef<RenderValueStore<MapViewportState | null> | null>(null);
  const mapViewportStore = mapViewportStoreRef.current
    ?? (mapViewportStoreRef.current = new RenderValueStore<MapViewportState | null>(null));
  const [mapViewportReady, setMapViewportReady] = useState(false);
  const setMapViewport = useCallback((next: SetStateAction<MapViewportState | null>) => {
    const current = mapViewportStore.value;
    const resolved = typeof next === "function" ? next(current) : next;
    mapViewportStore.publish(resolved);
    if (current === null && resolved !== null) setMapViewportReady(true);
  }, [mapViewportStore]);
  const mapViewport = mapViewportStore.value;
  const [chartViewport, setChartViewport] = useState<ImageViewportState | null>(null);
  const [chartFolderOpen, setChartFolderOpen] = useState(false);
  const selectedFamily = useMemo(
    () => rasterMapState?.family_options.find((family) => family.active) ?? null,
    [rasterMapState],
  );
  const selectedAirport = useMemo(
    () => chartPageData.airports.find((airport) => airport.id === selectedAirportId) ?? chartPageData.airports[0] ?? null,
    [chartPageData, selectedAirportId],
  );
  const selectedReferenceFamily = useMemo(
    () => selectedReferenceFamilyId
      ? derivedChartPageState.reference_families.find((family) => family.id === selectedReferenceFamilyId) ?? null
      : null,
    [derivedChartPageState.reference_families, selectedReferenceFamilyId],
  );
  const selectedChartCollection = selectedReferenceFamily ?? selectedAirport;
  const selectedChart = useMemo(
    () => selectedChartCollection?.charts.find((chart) => chart.id === selectedChartId) ?? selectedChartCollection?.charts[0] ?? null,
    [selectedChartCollection, selectedChartId],
  );

  useEffect(() => {
    debugLog("charts.selection.render", {
      selected_airport_id: selectedAirportId,
      selected_chart_id: selectedChartId,
      selected_chart_label: selectedChart?.label ?? null,
    });
  }, [selectedAirportId, selectedChartId, selectedChart?.label]);
  useEffect(() => {
    let cancelled = false;
    markStartupProgress("adapter.load", "Loading app-core adapter");
    loadBestAvailableAdapter().then((loaded) => {
      if (!cancelled) {
        markStartupProgress("adapter.ready", loaded.detail);
        setAppCoreAdapter(loaded.adapter);
        setAdapterBackend(loaded.backend);
        setSessionInitError(null);
      }
    }).catch((error) => {
      if (!cancelled) {
        const message = error instanceof Error ? error.message : String(error);
        setSessionInitError(`WASM adapter init failed: ${message}`);
        reportStartupFatalError("adapter.load", error);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [markStartupProgress, reportStartupFatalError]);

  useEffect(() => {
    installGlobalErrorLogging();
    const handleError = (event: ErrorEvent) => {
      reportStartupFatalError("window.error", event.error ?? new Error(event.message));
    };
    const handleUnhandledRejection = (event: PromiseRejectionEvent) => {
      reportStartupFatalError("window.unhandledrejection", event.reason);
    };
    window.addEventListener("error", handleError);
    window.addEventListener("unhandledrejection", handleUnhandledRejection);
    return () => {
      window.removeEventListener("error", handleError);
      window.removeEventListener("unhandledrejection", handleUnhandledRejection);
    };
  }, [reportStartupFatalError]);

  useEffect(() => {
    setDebugLogDeveloperServerUploadEnabled(sessionSnapshot.debug_state.debug_log_to_developer_server);
  }, [sessionSnapshot.debug_state.debug_log_to_developer_server]);

  useEffect(() => {
    let cancelled = false;
    void appCoreAdapter?.prewarm().catch((error) => {
      if (!cancelled) {
        console.error("failed to prewarm web adapter", error);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [appCoreAdapter]);

  useEffect(() => {
    let cancelled = false;
    let nextSession: UiSession | null = null;
    const destroyNextSession = () => {
      const session = nextSession;
      nextSession = null;
      void session?.destroy();
    };
    if (!appCoreAdapter) {
      return;
    }
    debugTiming("startup.session.create", async () => {
      markStartupProgress("session.create", "Creating core UI session");
      const created = await debugTiming("startup.session.create.core", () => appCoreAdapter.createUiSession(
        initialRecentAirportIds,
        initialChartPageState.selected_airport_id,
        initialChartPageState.selected_chart_id,
      ));
      nextSession = created;
      if (cancelled) {
        destroyNextSession();
        return;
      }
      markStartupProgress("session.initial_snapshot", "Using initial session snapshot");
      const createdSnapshot = debugTiming("startup.session.initial_snapshot", () => created.initialSnapshot());
      debugLog("session.create.snapshot", {
        plan_id: createdSnapshot.app_ui_state.active_plan?.plan_id ?? null,
        app_ui_state_nav_element: createdSnapshot.app_ui_state.active_plan?.guidance?.nav_element ?? null,
      });
      if (cancelled) {
        return;
      }
      markStartupProgress("session.ready", "Initial session ready");
      setUiSession(created);
      applySessionSnapshot(createdSnapshot, "session_create");
    }).catch((error) => {
      if (!cancelled) {
        console.error("failed to initialize web ui session", error);
        reportStartupFatalError("session.create", error);
      }
    });
    return () => {
      cancelled = true;
      destroyNextSession();
    };
  }, [adapterBackend, appCoreAdapter, applySessionSnapshot, initialChartPageState.selected_airport_id, initialChartPageState.selected_chart_id, initialDebugState, initialRecentAirportIds, markStartupProgress, reportStartupFatalError]);

  useEffect(() => {
    if (!uiSession) {
      return;
    }
    const handleOnline = () => {
      uiSession.notifyLiveFeedOnline();
    };
    window.addEventListener("online", handleOnline);
    return () => {
      window.removeEventListener("online", handleOnline);
    };
  }, [uiSession]);

  useEffect(() => {
    if (!uiSession) {
      return;
    }
    if (webIdleState.idle) {
      debugLog("live_feeds.subscription.idle", { reason: webIdleState.reason });
      return;
    }
    let cancelled = false;
    void uiSession.startLiveFeedSubscription().catch((error) => {
      debugLog("live_feeds.subscription.failed", { message: error instanceof Error ? error.message : String(error) });
    });
    return () => {
      cancelled = true;
      void uiSession.stopLiveFeedSubscription().catch((error) => {
        if (!cancelled) {
          debugLog("live_feeds.subscription_stop.failed", { message: error instanceof Error ? error.message : String(error) });
        }
      });
    };
  }, [uiSession, webIdleState.idle, webIdleState.reason]);

  useEffect(() => {
    if (!sessionSnapshot.raster_map) {
      return;
    }
    const nextRasterMap = sessionSnapshot.raster_map;
    setRasterMapState(nextRasterMap);
    setMapViewport((current) => current ?? createInitialViewport(nextRasterMap));
    setMapSelectorLoadError(null);
  }, [sessionSnapshot.raster_map]);

  useEffect(() => {
    let cancelled = false;
    if (!uiSession || !planUiState) {
      return;
    }
    debugTiming(
      "charts.page_state.load",
      () => uiSession.deriveChartPageState(),
    ).then((state) => {
      if (cancelled) {
        return;
      }
      setDerivedChartPageState(state);
      setChartPageStateLoadError(null);
    }).catch((error) => {
      if (!cancelled) {
        setChartPageStateLoadError(`failed to derive chart page state: ${errorMessage(error)}`);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [
    chartPageStateRequestKey,
    uiSession,
  ]);

  useEffect(() => {
    if (!selectedMap) {
      return;
    }
    setMapViewport((current) => (
      current === null ? createInitialViewport(selectedMap) : preserveViewportForMap(current, selectedMap)
    ));
  }, [selectedMap]);

  const appReady =
    appCoreAdapter !== null &&
    uiSession !== null &&
    selectedMap !== null &&
    mapViewportReady &&
    planUiState !== null;

  useEffect(() => {
    if (appReady) {
      startupResolvedRef.current = true;
      markStartupProgress("ready", "App ready");
    }
  }, [appReady, markStartupProgress]);

  useEffect(() => {
    if (appReady || startupFatalError) {
      return;
    }
    const intervalId = window.setInterval(() => {
      if (startupResolvedRef.current || startupFatalErrorRef.current) {
        return;
      }
      const idleMs = Date.now() - startupProgressRef.current.updatedAtMs;
      if (idleMs >= startupFatalStalledMs) {
        reportStartupFatalError(
          "startup.watchdog",
          new Error(`startup made no progress for ${Math.round(idleMs / 1000)}s`),
        );
      } else if (idleMs >= startupStalledWarningMs) {
        debugLog("startup.stalled", {
          phase: startupProgressRef.current.phase,
          detail: startupProgressRef.current.detail,
          idle_ms: idleMs,
        });
      }
    }, 1_000);
    return () => window.clearInterval(intervalId);
  }, [appReady, reportStartupFatalError, startupFatalError]);

  useEffect(() => {
    writePersistedWebUiState({
      page: page === "about" ? undefined : page,
      mapOrientationMode,
      selectedAirportId,
      selectedChartId,
      recentAirportIds,
    });
    if (page !== "about") setPersistedPage(page);
  }, [mapOrientationMode, page, recentAirportIds, selectedAirportId, selectedChartId]);

  const navigationPageOptions = useMemo(
    () => navigationPageOptionsFromCore(sessionSnapshot.navigation_page_state),
    [sessionSnapshot.navigation_page_state],
  );

  function currentSnapshot(): AppViewSnapshot {
    const currentMapViewport = mapViewportStore.value;
    if (currentMapViewport === null) {
      throw new Error("cannot snapshot map view before core supplies an initial viewport");
    }
    return {
      page,
      selectedMapId: rasterMapState?.selected_map_id ?? "",
      mapViewport: currentMapViewport,
      plateTargetAirportId: sessionSnapshot.chart_page_state.plate_target_airport_id ?? null,
      selectedAirportId,
      selectedReferenceFamilyId,
      selectedChartId,
      selectedChartLabel: selectedChart?.label ?? "",
      suggestedChartIds: derivedChartPageState.suggested_chart_ids,
      recentAirportIds,
      chartViewport,
      chartFolderOpen,
    };
  }

  function applySnapshotLocally(snapshot: AppViewSnapshot, history: AppViewSnapshot[]) {
    setPageHistory(history);
    setPage(snapshot.page);
    setMapViewport(snapshot.mapViewport);
    setChartViewport(snapshot.chartViewport);
    setChartFolderOpen(snapshot.chartFolderOpen);
  }

  function restoreSnapshot(snapshot: AppViewSnapshot, history: AppViewSnapshot[]) {
    applySnapshotLocally(snapshot, history);
    if (snapshot.selectedMapId && uiSession) {
      void uiSession.selectRasterMap(snapshot.selectedMapId).then((nextSnapshot) => {
        applySessionSnapshot(nextSnapshot, "restore_view_map");
      }).catch((error) => {
        setMapSelectorLoadError(`failed to restore map selector state: ${errorMessage(error)}`);
      });
    }
    if (uiSession) {
      void uiSession.restoreChartPageState(
        snapshot.recentAirportIds,
        snapshot.plateTargetAirportId ?? null,
        snapshot.selectedAirportId || undefined,
        snapshot.selectedReferenceFamilyId ?? null,
        snapshot.selectedChartId || undefined,
        snapshot.suggestedChartIds ?? [],
      ).then((nextSnapshot) => {
        applySessionSnapshot(nextSnapshot, "restore_view_chart");
      }).catch(() => {});
    }
  }

  function boundedHistory(history: AppViewSnapshot[]) {
    const maxDepth = navigationPageOptions.maxHistoryDepth;
    return maxDepth <= 0 || history.length <= maxDepth ? history : history.slice(history.length - maxDepth);
  }

  useEffect(() => {
    if (typeof window === "undefined") {
      return;
    }
    if (!appReady) {
      return;
    }
    const timeoutId = window.setTimeout(() => {
      const state: WebHistoryState = {
        __aerobag: true,
        current: currentSnapshot(),
        stack: pageHistory,
      };
      window.history.replaceState(state, "", urlForAppPage(page));
    }, 120);
    return () => window.clearTimeout(timeoutId);
  }, [appReady, page, pageHistory, rasterMapState?.selected_map_id, selectedAirportId, selectedReferenceFamilyId, selectedChartId, recentAirportIds, derivedChartPageState.suggested_chart_ids, chartViewport, chartFolderOpen]);

  useEffect(() => {
    if (typeof window === "undefined") {
      return;
    }
    function handlePopState(event: PopStateEvent) {
      const state = (event.state ?? {}) as WebHistoryState;
      if (state.__aerobag && state.current) {
        restoreSnapshot(state.current, Array.isArray(state.stack) ? state.stack : []);
      }
    }
    window.addEventListener("popstate", handlePopState);
    return () => window.removeEventListener("popstate", handlePopState);
  }, [applySessionSnapshot, uiSession]);

  function navigateToPage(nextPage: AppPage) {
    if (nextPage === page) {
      return;
    }
    if (nextPage === "map") {
      const timing = {
        id: nextPageTilePaintTimingIdRef.current++,
        fromPage: page,
        startedAt: performance.now(),
      };
      pageTilePaintTimingRef.current = timing;
      perfDebugLog("web.page-to-map.start", () => ({ id: timing.id, from_page: timing.fromPage }));
      requestAnimationFrame(() => {
        perfDebugLog("web.page-to-map.visible_frame", () => ({
          id: timing.id,
          from_page: timing.fromPage,
          elapsed_ms: Math.round(performance.now() - timing.startedAt),
        }));
      });
    }
    const nextHistory = boundedHistory([...pageHistory, currentSnapshot()]);
    setPageHistory(nextHistory);
    setPage(nextPage);
    if (typeof window !== "undefined") {
      const nextCurrent: AppViewSnapshot = {
        ...currentSnapshot(),
        page: nextPage,
      };
      const state: WebHistoryState = {
        __aerobag: true,
        current: nextCurrent,
        stack: nextHistory,
      };
      window.history.pushState(state, "", urlForAppPage(nextPage));
    }
  }

  function pushViewSnapshot(
    next: Partial<AppViewSnapshot> & Pick<AppViewSnapshot, "page">,
    restoreCore = false,
  ) {
    const nextHistory = boundedHistory([...pageHistory, currentSnapshot()]);
    const nextCurrent: AppViewSnapshot = {
      ...currentSnapshot(),
      ...next,
    };
    if (restoreCore) {
      restoreSnapshot(nextCurrent, nextHistory);
    } else {
      applySnapshotLocally(nextCurrent, nextHistory);
    }
    if (typeof window !== "undefined") {
      window.history.pushState(
        {
          __aerobag: true,
          current: nextCurrent,
          stack: nextHistory,
        } satisfies WebHistoryState,
        "",
        urlForAppPage(nextCurrent.page),
      );
    }
  }

  function navigateToMostRecentChartOrPlate() {
    const target = pageHistory
      .slice()
      .reverse()
      .find((snapshot) => navigationPageOptions.chartOrPlateReturnPages.has(snapshot.page));
    if (target) {
      pushViewSnapshot(target, true);
      return;
    }
    navigateToPage(navigationPageOptions.defaultChartOrPlateReturnPage);
  }
  const mostRecentChartOrPlatePage = mostRecentChartOrPlatePageFromHistory(
    pageHistory,
    navigationPageOptions.chartOrPlateReturnPages,
    navigationPageOptions.defaultChartOrPlateReturnPage,
  );

  function openPlateTarget(airportId: string, target: "Folder" | "CSup") {
    if (!uiSession) {
      return;
    }
    const targetChartId = `Plate:${airportId}:${target}`;
    void uiSession.openChartAirport(airportId, targetChartId)
      .then((nextSnapshot) => {
        applySessionSnapshot(nextSnapshot, "open_plate_target");
        const chartState = nextSnapshot.chart_page_state;
        pushViewSnapshot({
          page: "charts",
          plateTargetAirportId: airportId,
          selectedAirportId: chartState.selected_airport_id,
          selectedReferenceFamilyId: null,
          selectedChartId: chartState.selected_chart_id,
          selectedChartLabel: "",
          recentAirportIds: chartState.recent_airport_ids,
          suggestedChartIds: [],
          chartViewport: null,
          chartFolderOpen: target === "Folder",
        });
      })
      .catch((error) => {
        debugLog("plates.open.target.failed", {
          airport_id: airportId,
          target,
          error: errorMessage(error),
        });
      });
  }

  const themeVars = appThemeVars;

  useEffect(() => {
    if (typeof window === "undefined") {
      return;
    }
    const shouldHideStartupShell =
      page === "about" ||
      startupFatalError !== null ||
      sessionInitError !== null ||
      mapSelectorLoadError !== null ||
      chartPageStateLoadError !== null ||
      (appReady && planUiState !== null);
    if (shouldHideStartupShell) {
      const reason = page === "about"
        ? "about_page"
        : startupFatalError !== null
        ? "startup_fatal_error"
        : sessionInitError !== null
          ? "session_init_error"
          : mapSelectorLoadError !== null
            ? "map_selector_load_error"
            : chartPageStateLoadError !== null
              ? "chart_page_state_load_error"
              : "app_ready";
      window.__aerobag_hide_startup_shell?.(reason);
    }
  }, [appReady, chartPageStateLoadError, mapSelectorLoadError, page, planUiState, sessionInitError, startupFatalError]);

  if (page === "about") {
    return (
      <main className="appShell" style={themeVars}>
        <AboutPage />
      </main>
    );
  }

  if (startupFatalError) {
    return (
      <main className="appShell" style={themeVars}>
        <StartupFatalErrorModal error={startupFatalError} />
      </main>
    );
  }

  if (sessionInitError || mapSelectorLoadError || chartPageStateLoadError) {
    return (
      <main className="appFrame">
        <section className="appPage planPage">
          <div className="planGuidanceSummary">{sessionInitError ?? mapSelectorLoadError ?? chartPageStateLoadError}</div>
        </section>
      </main>
    );
  }

  if (!appReady || !planUiState || !selectedMap || !rasterMapState || !mapViewport) {
    return (
      <main className="startupProgressHost" aria-live="polite">
        <span>{startupProgress.detail ?? startupProgress.phase}</span>
      </main>
    );
  }

  return (
    <main
      className="appShell"
      style={themeVars}
      data-testid={`parity:startup-state:ready:true:disclaimer_required:${sessionSnapshot.disclaimer_state.required}:persisted_page:${persistedPage}:session_revision:${sessionSnapshot.session_revision}`}
    >
      <NavigationPageOptionsContext.Provider value={navigationPageOptions}>
      <HighRateSessionEffects
        sessionRenderStore={sessionRenderStore}
        uiSession={uiSession}
        requestSessionSnapshotRefresh={requestSessionSnapshotRefresh}
        applySessionSnapshot={applySessionSnapshot}
        onPlaybackSourcePathChange={setPlaybackSourcePath}
      />
      <PageLayer active={page === "map"}>
        <MapPage
          sessionRenderStore={sessionRenderStore}
          mapViewportStore={mapViewportStore}
          uiInvalidationStore={uiInvalidationStore}
          key={sessionSnapshot.nav_data_epoch}
          appCoreAdapter={appCoreAdapter}
          navDataEpoch={sessionSnapshot.nav_data_epoch}
          flightPlanRouteRevision={sessionSnapshot.flight_plan_route_revision}
          page={page}
          debugState={sessionSnapshot.debug_state}
          mapLayerState={mapLayerState}
          selectedMap={selectedMap}
          selectedFamily={selectedFamily}
          familyOptions={rasterMapState.family_options}
          mapOrientationMode={mapOrientationMode}
          onMapOrientationModeChange={setMapOrientationMode}
          pageTilePaintTiming={pageTilePaintTimingRef.current}
          onPageTilePaintTimingComplete={(id) => {
            if (pageTilePaintTimingRef.current?.id === id) {
              pageTilePaintTimingRef.current = null;
            }
          }}
          onViewportGestureActiveChange={handleMapViewportGestureActiveChange}
          onViewportGestureActivity={handleMapViewportGestureActivity}
          onSelectMapFamily={(familyId) => {
            if (!uiSession) {
              return;
            }
            void uiSession.selectMapFamily(familyId).then((nextSnapshot) => {
              applySessionSnapshot(nextSnapshot, "select_map_family");
            }).catch((error) => {
              setMapSelectorLoadError(`failed to select map family: ${errorMessage(error)}`);
            });
          }}
          onSelectPage={navigateToPage}
          onOpenPlan={() => navigateToPage("plan")}
          onOpenPlateTarget={openPlateTarget}
          onOpenChartReference={(action) => {
            if (!uiSession) {
              return;
            }
            void uiSession.selectChartReference(action.family_id, action.suggested_chart_ids)
              .then((nextSnapshot) => {
                applySessionSnapshot(nextSnapshot, "select_chart_reference");
                setChartFolderOpen(true);
                navigateToPage("charts");
              })
              .catch((error) => {
                setChartPageStateLoadError(`failed to open chart references: ${errorMessage(error)}`);
              });
          }}
          statusControls={sessionSnapshot.map_status_controls}
          onStatusAction={performStatusAction}
          planUiState={planUiState}
          playbackSourcePath={playbackSourcePath}
          onPlaybackSourcePathChange={setPlaybackSourcePath}
          onPlaybackSnapshotChange={applySessionSnapshotDispatch}
          onSituationControlInput={applySituationControlInput}
          uiSession={uiSession}
          onSessionSnapshot={applySessionSnapshot}
          onDebugWarning={logDebugWarning}
          onHighLatencyWarning={logHighLatencyWarning}
          onFirstVisualReady={reportStartupVisualReady}
        />
      </PageLayer>

      <PageLayer active={page === "plan"}>
        <FlightPlanPage
          appCoreAdapter={appCoreAdapter}
          uiSession={uiSession}
          page={page}
          pageHistory={pageHistory}
          planUiState={planUiState}
          mostRecentChartOrPlatePage={mostRecentChartOrPlatePage}
          onOpenRecentChartOrPlate={navigateToMostRecentChartOrPlate}
          onSelectPage={navigateToPage}
          onOpenWeatherDetail={setFlightPlanWeatherModal}
          onOpenCharts={(airportId, chartId) => {
            if (!airportId || !uiSession) {
              return;
            }
            const airport = chartPageData.airports.find((entry) => entry.id === airportId);
            debugLog("charts.open.request", {
              airport_id: airportId,
              chart_id: chartId,
            });
            void uiSession.openChartAirport(airportId, chartId || undefined)
              .then((nextSnapshot) => {
                const chartState = nextSnapshot.chart_page_state;
                const selectedChartLabel = airport?.charts.find(
                  (chart) => chart.id === chartState.selected_chart_id,
                )?.label ?? "";
                debugLog("charts.open.snapshot", {
                  requested_airport_id: airportId,
                  requested_chart_id: chartId,
                  selected_airport_id: chartState.selected_airport_id,
                  selected_chart_id: chartState.selected_chart_id,
                });
                applySessionSnapshot(nextSnapshot, "open_charts");
                pushViewSnapshot({
                  page: "charts",
                  plateTargetAirportId: airportId,
                  selectedAirportId: chartState.selected_airport_id,
                  selectedReferenceFamilyId: null,
                  selectedChartId: chartState.selected_chart_id,
                  selectedChartLabel,
                  recentAirportIds: chartState.recent_airport_ids,
                  suggestedChartIds: [],
                  chartViewport: null,
                  chartFolderOpen: !chartId,
                });
              })
              .catch((error) => {
                debugLog("charts.open.failed", {
                  airport_id: airportId,
                  chart_id: chartId,
                  error: errorMessage(error),
                });
              });
          }}
          onInsertAirportWaypointAtRow={async (rowUid, before, airportId) => {
            if (!appCoreAdapter) return;
            const waypoint = await appCoreAdapter.resolveWaypointIdentifier(airportId);
            if (!waypoint) {
              throw new Error(`Unknown waypoint ${airportId}`);
            }
            applySessionSnapshot(
              await uiSession.insertWaypointAtFlightPlanRow(rowUid, before, waypoint),
              "insert_waypoint_at_row",
            );
          }}
          onPreviewFlightPlanEntry={async (input) => {
            if (!uiSession) throw new Error("flight plan preview requires live core session");
            return uiSession.previewFlightPlanEntry(input);
          }}
          onAppendFlightPlanEntry={async (input) => {
            if (!uiSession) return;
            applySessionSnapshot(await uiSession.appendFlightPlanEntry(input), "append_flight_plan_entry");
          }}
          onPerformFlightPlanControl={async (controlId) => {
            if (!uiSession) return;
            applySessionSnapshot(
              await uiSession.performFlightPlanControl(controlId),
              "flight_plan_control",
            );
          }}
          onPerformFlightPlanRowAction={async (rowUid, actionUid) => {
            if (!uiSession) return;
            applySessionSnapshot(
              await uiSession.performFlightPlanRowAction(rowUid, actionUid),
              "flight_plan_row_action",
            );
          }}
          onInsertAirwayAtRow={async (rowUid, entryPointUid, exitPointUid, presentation) => {
            if (!uiSession) return;
            applySessionSnapshot(
              await uiSession.insertAirwayAtFlightPlanRow(rowUid, presentation, entryPointUid, exitPointUid),
              "insert_airway_at_row",
            );
          }}
          onSelectProcedureAtRow={async (rowUid, airportId, procedureId, kind, runwayTransition, enrouteTransition) => {
            if (!uiSession) return;
            applySessionSnapshot(
              await uiSession.selectProcedureAtFlightPlanRow(
                rowUid,
                airportId,
                procedureId,
                kind,
                runwayTransition,
                enrouteTransition,
              ),
              "select_procedure_at_row",
            );
          }}
          onTimeDisplayAction={async (actionId) => {
            if (!uiSession) return;
            applySessionSnapshot(
              await uiSession.performTimeDisplayAction(actionId),
              "time_display_mode",
            );
          }}
          onFlightPlanColumnAction={async (actionId) => {
            if (!uiSession) return;
            applySessionSnapshot(
              await uiSession.performFlightPlanColumnAction(actionId),
              "flight_plan_column_action",
            );
          }}
        />
      </PageLayer>

      <PageLayer active={page === "altitude"}>
        <AltitudePlannerPage
          page={page}
          planUiState={planUiState}
          mostRecentChartOrPlatePage={mostRecentChartOrPlatePage}
          onOpenRecentChartOrPlate={navigateToMostRecentChartOrPlate}
          onSelectPage={navigateToPage}
          onQueryAltitudeComparisons={async () => {
            if (!uiSession) throw new Error("altitude comparison requires live core session");
            return uiSession.altitudeComparisons();
          }}
          onPerformAltitudePlannerAction={async (actionUid) => {
            if (!uiSession) return;
            applySessionSnapshot(
              await uiSession.performAltitudePlannerAction(actionUid),
              "altitude_planner_action",
            );
          }}
          onSetDepartureInput={async (field, input) => {
            if (!uiSession) return;
            applySessionSnapshot(
              await uiSession.setAltitudePlannerDepartureInput(field, input),
              "altitude_planner_departure_input",
            );
          }}
          onToggleDepartureTimeBasis={async () => {
            if (!uiSession) return;
            applySessionSnapshot(
              await uiSession.performTimeDisplayAction(planUiState.altitude_planner.departure.time_display_action_id),
              "time_display_mode",
            );
          }}
        />
      </PageLayer>

      <PageLayer active={page === "charts"}>
        <ChartsPage
          sessionRenderStore={sessionRenderStore}
          appCoreAdapter={appCoreAdapter}
          page={page}
          planUiState={planUiState}
          flightPlanRouteRevision={sessionSnapshot.flight_plan_route_revision}
          navDataEpoch={sessionSnapshot.nav_data_epoch}
          airportMenuEntries={airportMenuEntries}
          selectedCollection={selectedChartCollection}
          selectedChart={selectedChart}
          suggestedChartIds={derivedChartPageState.suggested_chart_ids}
          collectionControl={derivedChartPageState.collection_control}
          chartControl={derivedChartPageState.chart_control}
          projectedProcedureLoadMenu={derivedChartPageState.procedure_load_menu}
          statusControls={derivedChartPageState.status_controls}
          folderOpen={chartFolderOpen}
          viewport={chartViewport}
          onViewportChange={setChartViewport}
          onFolderOpenChange={(next) => {
            pushViewSnapshot({
              page: "charts",
              chartFolderOpen: next,
            });
          }}
          onSelectPage={navigateToPage}
          onOpenPlan={() => navigateToPage("plan")}
          onSelectAirport={(airportId) => {
            if (!uiSession) {
              return;
            }
            const airport = chartPageData.airports.find((entry) => entry.id === airportId);
            void uiSession.openChartAirport(airportId)
              .then((nextSnapshot) => {
                applySessionSnapshot(nextSnapshot, "select_airport");
                const chartState = nextSnapshot.chart_page_state;
                pushViewSnapshot({
                  page: "charts",
                  selectedAirportId: chartState.selected_airport_id,
                  selectedReferenceFamilyId: null,
                  selectedChartId: chartState.selected_chart_id,
                  selectedChartLabel: airport?.charts[0]?.label ?? "",
                  recentAirportIds: chartState.recent_airport_ids,
                  suggestedChartIds: [],
                  chartViewport: null,
                  chartFolderOpen: false,
                });
              })
              .catch((error) => {
                debugLog("charts.select.airport.failed", {
                  airport_id: airportId,
                  error: errorMessage(error),
                });
              });
          }}
          onSelectReference={(familyId: ChartFamilyId) => {
            if (!uiSession) {
              return;
            }
            void uiSession.selectChartReference(familyId, []).then((nextSnapshot) => {
              applySessionSnapshot(nextSnapshot, "select_chart_reference_menu");
            }).catch(() => {});
            setChartViewport(null);
            setChartFolderOpen(true);
          }}
          onSelectChart={(chartId) => {
            const nextChart = selectedChartCollection?.charts.find((chart) => chart.id === chartId);
            debugLog("charts.select.request", {
              requested_airport_id: selectedReferenceFamily ? null : selectedAirport?.id ?? null,
              requested_chart_id: chartId,
              requested_chart_label: nextChart?.label ?? null,
            });
            if (!uiSession) {
              return;
            }
            void uiSession.selectChart(chartId).then((nextSnapshot) => {
              debugLog("charts.select.snapshot", {
                requested_chart_id: chartId,
                selected_airport_id: nextSnapshot.chart_page_state.selected_airport_id,
                selected_chart_id: nextSnapshot.chart_page_state.selected_chart_id,
              });
              applySessionSnapshot(nextSnapshot, "select_chart");
              pushViewSnapshot({
                page: "charts",
                selectedReferenceFamilyId,
                selectedChartId: chartId,
                selectedChartLabel: nextChart?.label ?? "",
                suggestedChartIds: derivedChartPageState.suggested_chart_ids,
                chartViewport: null,
                chartFolderOpen: false,
              });
            }).catch((error) => {
              debugLog("charts.select.failed", {
                requested_chart_id: chartId,
                error: errorMessage(error),
              });
              setChartPageStateLoadError(`failed to select chart: ${errorMessage(error)}`);
            });
          }}
          playbackSourcePath={playbackSourcePath}
          onPlaybackSourcePathChange={setPlaybackSourcePath}
          onPlaybackSnapshotChange={applySessionSnapshotDispatch}
          onSituationControlInput={applySituationControlInput}
          onStatusAction={performStatusAction}
          debugState={sessionSnapshot.debug_state}
          uiSession={uiSession}
          onFirstVisualReady={reportStartupVisualReady}
        />
      </PageLayer>

      <PageLayer active={page === "home"}>
        <HomePage
          page={page}
          state={sessionSnapshot.home_page_state}
          planUiState={planUiState}
          mostRecentChartOrPlatePage={mostRecentChartOrPlatePage}
          onOpenRecentChartOrPlate={navigateToMostRecentChartOrPlate}
          onSelectPage={navigateToPage}
          onOpenPlan={() => navigateToPage("plan")}
        />
      </PageLayer>

      <PageLayer active={page === "data"}>
        <DataStatusPage
          page={page}
          state={sessionSnapshot.data_status_page_state}
          navElement={planUiState?.guidance?.nav_element}
          mostRecentChartOrPlatePage={mostRecentChartOrPlatePage}
          onOpenPlan={() => navigateToPage("plan")}
          onOpenRecentChartOrPlate={navigateToMostRecentChartOrPlate}
          onSelectPage={navigateToPage}
          onTimeDisplayAction={async (actionId) => {
            if (!uiSession) return;
            applySessionSnapshot(
              await uiSession.performTimeDisplayAction(actionId),
              "time_display_mode",
            );
          }}
        />
      </PageLayer>
      <PageLayer active={page === "settings"}>
        <SettingsPage
          page={page}
          state={sessionSnapshot.settings_page_state}
          navElement={planUiState?.guidance?.nav_element}
          mostRecentChartOrPlatePage={mostRecentChartOrPlatePage}
          onOpenPlan={() => navigateToPage("plan")}
          onOpenRecentChartOrPlate={navigateToMostRecentChartOrPlate}
          onSelectPage={navigateToPage}
          onSettingsAction={(actionId, valueId) => {
            if (!uiSession) {
              return;
            }
            void uiSession.performSettingsAction(actionId, valueId).then((nextSnapshot) => {
              writePersistedDebugLogDeveloperServerUploadEnabled(
                nextSnapshot.debug_state.debug_log_to_developer_server,
              );
              applySessionSnapshot(nextSnapshot, "settings_action");
            });
          }}
          onAircraftLibraryAction={(actionId, sourceJson) => {
            if (!uiSession) return;
            void uiSession
              .performAircraftLibraryAction(actionId, sourceJson)
              .then((nextSnapshot) => {
                applySessionSnapshot(nextSnapshot, "aircraft_library_action");
              });
          }}
        />
      </PageLayer>
      <PageLayer active={page === "cloud"}>
        <CloudPage
          page={page}
          state={sessionSnapshot.cloud_page_state}
          navElement={planUiState?.guidance?.nav_element}
          mostRecentChartOrPlatePage={mostRecentChartOrPlatePage}
          onOpenPlan={() => navigateToPage("plan")}
          onOpenRecentChartOrPlate={navigateToMostRecentChartOrPlate}
          onSelectPage={navigateToPage}
          onAction={performCloudPageAction}
        />
      </PageLayer>
      {flightPlanWeatherModal ? (
        <>
          <TrayScrim ariaLabel="Close weather" onClose={() => setFlightPlanWeatherModal(null)} />
          <WeatherDetailModal detail={flightPlanWeatherModal} />
        </>
      ) : null}
      {sessionSnapshot.disclaimer_state.required ? (
        <DisclaimerModal
          state={sessionSnapshot.disclaimer_state}
          acceptingDisabled={!uiSession}
          onAccept={() => void acceptDisclaimer()}
        />
      ) : null}
      </NavigationPageOptionsContext.Provider>
    </main>
  );
}

function HighRateSessionEffects(props: {
  sessionRenderStore: SessionRenderStore;
  uiSession: UiSession | null;
  requestSessionSnapshotRefresh: (priority: SessionSnapshotRefreshPriority, reason: string) => void;
  applySessionSnapshot: (snapshot: UiSessionSnapshot, source: string) => boolean;
  onPlaybackSourcePathChange: Dispatch<SetStateAction<string>>;
}) {
  const snapshot = useSessionSnapshotGroups(
    props.sessionRenderStore,
    HIGH_RATE_SESSION_UPDATE_GROUPS,
  );
  const playback = snapshot.playback_ui_state;
  const ownship = snapshot.app_ui_state.ownship;

  useEffect(() => {
    if (playback.source_path) props.onPlaybackSourcePathChange(playback.source_path);
  }, [playback.source_path, props.onPlaybackSourcePathChange]);

  useEffect(() => {
    const deadline = ownship.controls.next_refresh_epoch_ms;
    if (deadline == null) return;
    const timer = window.setTimeout(
      () => props.requestSessionSnapshotRefresh("timely", "ownship_source_deadline"),
      Math.max(0, Math.min(deadline - Date.now(), 2_147_000_000)),
    );
    return () => window.clearTimeout(timer);
  }, [ownship.controls.next_refresh_epoch_ms, props.requestSessionSnapshotRefresh]);

  useEffect(() => {
    if (!props.uiSession || playback.status !== "playing") return;
    let cancelled = false;
    let timer: number | null = null;
    let inFlight = false;
    const intervalMs = Math.max(16, Math.min(1000, playback.tick_interval_ms));
    const schedule = (delayMs: number) => {
      if (!cancelled) timer = window.setTimeout(tick, delayMs);
    };
    const tick = () => {
      if (cancelled || inFlight || !props.uiSession) return;
      inFlight = true;
      const startedAt = performance.now();
      void props.uiSession.tickPlayback(Date.now()).then((nextSnapshot) => {
        if (!cancelled) props.applySessionSnapshot(nextSnapshot, "playback_tick");
      }).catch(() => {}).finally(() => {
        inFlight = false;
        schedule(Math.max(0, intervalMs - (performance.now() - startedAt)));
      });
    };
    tick();
    return () => {
      cancelled = true;
      if (timer !== null) window.clearTimeout(timer);
    };
  }, [playback.status, playback.tick_interval_ms, props.applySessionSnapshot, props.uiSession]);

  const badAutopilotActive = ownship.controls.sources.some(
    (source) => source.source_kind === "bad_autopilot" && source.active,
  );
  useEffect(() => {
    if (!props.uiSession || !badAutopilotActive) return;
    let cancelled = false;
    let inFlight = false;
    const tick = () => {
      if (inFlight || !props.uiSession) return;
      inFlight = true;
      void props.uiSession.tickBadAutopilot(Date.now()).then((nextSnapshot) => {
        if (!cancelled) props.applySessionSnapshot(nextSnapshot, "bad_autopilot_tick");
      }).catch(() => {}).finally(() => {
        inFlight = false;
      });
    };
    tick();
    const timer = window.setInterval(tick, 250);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [badAutopilotActive, props.applySessionSnapshot, props.uiSession]);

  return null;
}

function MapPage(props: {
  sessionRenderStore: SessionRenderStore;
  mapViewportStore: RenderValueStore<MapViewportState | null>;
  uiInvalidationStore: RenderValueStore<UiInvalidationRevisions>;
  appCoreAdapter: AppCoreAdapter;
  navDataEpoch: number;
  flightPlanRouteRevision: number;
  page: AppPage;
  debugState: UiDebugState;
  mapLayerState: UiMapLayerState;
  selectedMap: RasterMapUiState;
  selectedFamily: RasterMapUiState["family_options"][number] | null;
  familyOptions: RasterMapUiState["family_options"];
  mapOrientationMode: MapOrientationMode;
  onMapOrientationModeChange: (mode: MapOrientationMode) => void;
  pageTilePaintTiming: WebPageTilePaintTiming | null;
  onPageTilePaintTimingComplete: (id: number) => void;
  onViewportGestureActiveChange: (active: boolean) => void;
  onViewportGestureActivity: () => void;
  onSelectMapFamily: (familyId: ChartFamilyId) => void;
  onSelectPage: (page: AppPage) => void;
  onOpenPlan: () => void;
  onOpenPlateTarget: (airportId: string, target: "Folder" | "CSup") => void;
  onOpenChartReference: (action: NonNullable<RasterTilePlan["chart_reference_action"]>) => void;
  statusControls: UiSurfaceStatusState;
  onStatusAction: (actionId: string) => void | Promise<void>;
  planUiState: FlightPlanUiState | null;
  playbackSourcePath: string;
  onPlaybackSourcePathChange: Dispatch<SetStateAction<string>>;
  onPlaybackSnapshotChange: Dispatch<SetStateAction<UiSessionSnapshot>>;
  onSituationControlInput: (input: SituationControlInput) => void;
  uiSession: UiSession | null;
  onSessionSnapshot: (nextSnapshot: UiSessionSnapshot, source: string) => void;
  onDebugWarning: (tag: string, data?: unknown) => void;
  onHighLatencyWarning: (tag: string, data?: unknown) => void;
  onFirstVisualReady: () => void;
}) {
  recordSessionRender("map");
  const highRateSnapshot = useSessionSnapshotGroups(
    props.sessionRenderStore,
    props.page === "map" ? HIGH_RATE_SESSION_UPDATE_GROUPS : NO_SESSION_UPDATE_GROUPS,
  );
  const flightPlanSnapshot = useSessionSnapshotGroups(
    props.sessionRenderStore,
    props.page === "map" ? FLIGHT_PLAN_SESSION_UPDATE_GROUPS : NO_SESSION_UPDATE_GROUPS,
  );
  const ownship = highRateSnapshot.app_ui_state.ownship.render;
  const aircraftPlanViewPath = flightPlanSnapshot.app_ui_state.aircraft_plan_view_path;
  const ownshipControls = highRateSnapshot.app_ui_state.ownship.controls;
  const activeOwnshipSourceId = ownshipControls.sources.find((source) => source.active)?.source_id;
  const activeOwnshipSource = typeof activeOwnshipSourceId === "string"
    ? activeOwnshipSourceId
    : activeOwnshipSourceId?.[0] ?? "none";
  const flightDataBanner = highRateSnapshot.app_ui_state.flight_data_banner;
  const playbackUiState = highRateSnapshot.playback_ui_state;
  const playbackPanelState = highRateSnapshot.playback_panel_state;
  const mapFollowUiState = highRateSnapshot.map_follow_ui_state;
  const mapFollowTargetViewport = highRateSnapshot.map_follow_target_viewport;
  const uiInvalidationRevisions = useRenderValue(
    props.uiInvalidationStore,
    props.page === "map",
  );
  const viewport = requireMapViewport(
    useRenderValue(props.mapViewportStore, props.page === "map"),
  );
  const onViewportChange = useCallback(
    (next: MapViewportState) => props.mapViewportStore.publish(next),
    [props.mapViewportStore],
  );
  const {
    appCoreAdapter,
    navDataEpoch,
    flightPlanRouteRevision,
    debugState,
    mapLayerState,
    page,
    selectedMap,
    selectedFamily,
    familyOptions,
    mapOrientationMode,
    onMapOrientationModeChange,
    pageTilePaintTiming,
    onPageTilePaintTimingComplete,
    onViewportGestureActiveChange,
    onViewportGestureActivity,
    onSelectMapFamily,
    onSelectPage,
    onOpenPlan,
    onOpenPlateTarget,
    onOpenChartReference,
    statusControls,
    onStatusAction,
    planUiState,
    uiSession,
    onPlaybackSnapshotChange,
    onSituationControlInput,
    onDebugWarning,
    onHighLatencyWarning,
    onFirstVisualReady,
  } = props;
  const containerRef = useRef<HTMLDivElement | null>(null);
  const mapBearingTransformRef = useRef<HTMLDivElement | null>(null);
  const mapContentTransformRef = useRef<HTMLDivElement | null>(null);
  const trayGroup = useModalTrayGroup(["family", "layers", "procedureWarning", "status", "ownship"] as const);
  const layerToggleBusyRef = useRef(false);
  const [chartSearch, setChartSearch] = useState<{
    query: string;
    open: boolean;
    loading: boolean;
    error: string | null;
    suggestions: WaypointIdentifierSuggestion[];
  }>({
    query: "",
    open: false,
    loading: false,
    error: null,
    suggestions: [],
  });
  const [mapOverlay, setMapOverlay] = useState<MapOverlayQueryResult>({
    visible_features: [],
    visible_metars: [],
    visible_pireps: [],
    visible_traffic: [],
    traffic_next_refresh_epoch_ms: null,
    airspace_paths: [],
    tfr_paths: [],
    airspace_labels: [],
    offline_regions: [],
  });
  const [trafficRefreshTick, setTrafficRefreshTick] = useState(0);
  const [nexradOverlay, setNexradOverlay] = useState<NexradOverlayQueryResult>({
    status: { state: "hidden" },
    tiles: [],
    stats: emptyNexradOverlayStats(),
    animation: emptyNexradOverlayAnimation(),
  });
  const [nexradAnimationTick, setNexradAnimationTick] = useState(0);
  const [nexradViewportRefreshTick, setNexradViewportRefreshTick] = useState(0);
  const [nexradOverlayFrame, setNexradOverlayFrame] = useState<MapDisplayFrame | null>(null);
  const nexradFrameCacheRef = useRef<NexradFrameImageCache | null>(null);
  if (!nexradFrameCacheRef.current) {
    nexradFrameCacheRef.current = new NexradFrameImageCache();
  }
  const nexradQueryRequestRef = useRef<{
    id: number;
    session: UiSession;
    viewport: MapViewportState;
    width: number;
    height: number;
    debugTileLabels: boolean;
  } | null>(null);
  const nexradQueryRequestIdRef = useRef(0);
  const nexradQueryPendingRef = useRef(false);
  const nexradQueryPumpActiveRef = useRef(false);
  const nexradLastPaintTimingRef = useRef<NexradPaintTiming | null>(null);
  const nexradTimerSeqRef = useRef(0);
  const nexradViewportRefreshTimerRef = useRef<number | null>(null);
  const [terrainOverlay, setTerrainOverlay] = useState<TerrainOverlayUiState>({ query: null, images: [] });
  const terrainTileCacheRef = useRef<Map<string, TerrainTileCacheEntry>>(new Map());
  const terrainTileInFlightRef = useRef<Set<string>>(new Set());
  const terrainScheduleRequestRef = useRef<TerrainScheduleRequest | null>(null);
  const terrainScheduleRequestIdRef = useRef(0);
  const landedTerrainScheduleRequestIdRef = useRef(0);
  const terrainSchedulePendingRef = useRef(false);
  const terrainRenderPumpActiveRef = useRef(false);
  const terrainRendererRef = useRef<TerrainOverlayRenderer | null>(null);
  const terrainPendingFrameRef = useRef<TerrainPendingFrame | null>(null);
  const terrainFrameStartRef = useRef<Map<string, number>>(new Map());
  const lastTerrainRenderPlanKeyRef = useRef("");
  const [flightPlanRouteProjection, setFlightPlanRouteProjection] = useState<FlightPlanRouteProjection>({
    flight_plan_route_revision: -1,
    segments: [],
    distance_annotations: [],
  });
  const flightPlanRoute =
    flightPlanRouteProjection.flight_plan_route_revision === flightPlanRouteRevision
      ? flightPlanRouteProjection.segments
      : [];
  const flightPlanRouteDistanceAnnotations =
    flightPlanRouteProjection.flight_plan_route_revision === flightPlanRouteRevision
      ? flightPlanRouteProjection.distance_annotations
      : [];
  const [mapOverlayFrame, setMapOverlayFrame] = useState<MapDisplayFrame | null>(null);
  const mapOverlayQueryRequestRef = useRef<{
    id: number;
    requestedAt: number;
    session: UiSession;
    viewport: MapViewportState;
    center: LatLon;
    width: number;
    height: number;
    layerKey: string;
    navDataEpoch: number;
  } | null>(null);
  const mapOverlayQueryRequestIdRef = useRef(0);
  const landedMapOverlayQueryRequestIdRef = useRef(0);
  const mapOverlayQueryPendingRef = useRef(false);
  const mapOverlayQueryPumpActiveRef = useRef(false);
  const mapOverlayLandingTimingRef = useRef<{
    id: number;
    requestedAt: number;
    queryStartedAt: number;
    landStartedAt: number;
    landEndedAt: number;
    visibleFeatures: number;
    visibleMetars: number;
    visiblePireps: number;
    airspacePaths: number;
    tfrPaths: number;
    airspaceLabels: number;
    offlineRegions: number;
    flightPlanFeatures: number;
    committed: boolean;
  } | null>(null);
  const viewportRef = useRef<MapViewportState>(viewport);
  const committedViewportRef = useRef<MapViewportState>(viewport);
  const pendingReactViewportRef = useRef<MapViewportState | null>(null);
  const pendingReactViewportTimerRef = useRef<number | null>(null);
  const activePointersRef = useRef<Map<number, ScreenPoint>>(new Map());
  const dragRef = useRef<{ id: number; last: ScreenPoint } | null>(null);
  const pinchRef = useRef<ReturnType<typeof createPinchSnapshot> | null>(null);
  const clickCandidateRef = useRef<{ pointerId: number; start: ScreenPoint; latest: ScreenPoint } | null>(null);
  const mapSelectionRequestGenerationRef = useRef(0);
  const gestureActiveRef = useRef(false);
  const viewportGestureUntilRef = useRef(0);
  const followSyncSerialRef = useRef(0);
  const deferredFollowSyncViewportRef = useRef<MapViewportState | null>(null);
  const followTargetGateRef = useRef(new MapFollowTargetGate());
  const [followSyncPendingSerial, setFollowSyncPendingSerial] = useState(0);
  const [followTargetRetryToken, setFollowTargetRetryToken] = useState(0);
  const [surfaceSize, setSurfaceSize] = useState<SurfaceSize>({ width: 0, height: 0 });
  const mapSelectionLayoutBasisRef = useRef({
    width: 0,
    height: 0,
    uiSession,
  });
  const mapUpDegRef = useRef(0);
  const previousMapOrientationModeRef = useRef(mapOrientationMode);
  const plannedMapUpDeg = resolveMapUpDegrees(
    mapOrientationMode,
    ownship.track_deg_true,
    previousMapOrientationModeRef.current === "track" ? mapUpDegRef.current : 0,
  );
  const rasterPlanningViewport = useMemo(
    () => ({ ...viewport, rotationDeg: plannedMapUpDeg }),
    [plannedMapUpDeg, viewport],
  );
  const planningSurfaceSize = useMemo(() => {
    const envelope = rotatedViewportEnvelopeSize(surfaceSize.width, surfaceSize.height, plannedMapUpDeg);
    return { width: Math.ceil(envelope.width), height: Math.ceil(envelope.height) };
  }, [plannedMapUpDeg, surfaceSize.height, surfaceSize.width]);
  const [liveDragPerfRunning, setLiveDragPerfRunning] = useState(false);
  const [mapSelection, setMapSelection] = useState<{
    point: ScreenPoint;
    result: MapSelectionQueryResult;
    selectedItem: MapSelectionItem | null;
    detailModal: { kind: "text"; title: string; sourceActionId?: string | null; text: string; status?: { text: string; color_key: string; action_id?: string | null } | null } | { kind: "weather"; detail: WeatherDetailUiView } | { kind: "airport"; detail: AirportInfoUiView } | null;
  } | null>(null);
  const mapSelectionDistanceItemId = mapSelection?.detailModal === null
    ? mapSelection.selectedItem?.id ?? null
    : null;
  const mapSelectionDistanceTarget = mapSelection?.detailModal === null
    ? mapSelection.selectedItem?.distance_target ?? null
    : null;
  useEffect(() => {
    if (!uiSession || !mapSelectionDistanceItemId || !mapSelectionDistanceTarget) {
      return;
    }
    const itemId = mapSelectionDistanceItemId;
    const target = mapSelectionDistanceTarget;
    let cancelled = false;
    let timer: number | null = null;

    const refresh = async () => {
      try {
        const distance = await uiSession.queryMapSelectionDistance(target);
        if (cancelled) return;
        setMapSelection((current) => {
          const selected = current?.selectedItem;
          const currentTarget = selected?.distance_target;
          if (
            !current ||
            !selected ||
            selected.id !== itemId ||
            currentTarget?.lat !== target.lat ||
            currentTarget?.lon !== target.lon ||
            selected.distance === distance
          ) {
            return current;
          }
          const selectedItem = { ...selected, distance };
          return {
            ...current,
            selectedItem,
            result: {
              ...current.result,
              categories: current.result.categories.map((category) => ({
                ...category,
                items: category.items.map((item) =>
                  item.id === itemId &&
                  item.distance_target?.lat === target.lat &&
                  item.distance_target?.lon === target.lon
                    ? selectedItem
                    : item
                ),
              })),
            },
          };
        });
      } catch (error) {
        if (!cancelled) {
          debugLog("map.selection.distance_refresh_failed", { error: errorMessage(error) });
        }
      } finally {
        if (!cancelled) {
          timer = window.setTimeout(refresh, 1_000);
        }
      }
    };

    timer = window.setTimeout(refresh, 1_000);
    return () => {
      cancelled = true;
      if (timer !== null) window.clearTimeout(timer);
    };
  }, [
    uiSession,
    mapSelectionDistanceItemId,
    mapSelectionDistanceTarget?.lat,
    mapSelectionDistanceTarget?.lon,
  ]);
  const [hoverWeather, setHoverWeather] = useState<{
    stationId: string;
    point: ScreenPoint;
    detail: WeatherDetailUiView;
  } | null>(null);
  const hoverWeatherRequestSerialRef = useRef(0);
  const airportInfoRequestSerialRef = useRef(0);
  const { toast: disabledActionToast, show: showDisabledAction } = useDisabledActionToast();
  const firstVisualReadyRef = useRef(false);
  const statusControlDockLowered = shouldLowerStatusControlDock(
    surfaceSize.width,
    statusControls.controls.filter((control) => control.state.boxes.length > 0).length,
  );
  const bottomCornerControlsRaised = shouldRaiseBottomCornerControls(surfaceSize.width);
  const flightDataBannerEdgeLayout = surfaceSize.width > surfaceSize.height;
  const flightDataBannerEdgeColumnCount = flightDataBannerEdgeLayout
    ? flightDataEdgeColumnCount(surfaceSize, flightDataBanner.cells.length, statusControlDockLowered)
    : 1;

  function pumpTerrainRenderQueue() {
    if (terrainRenderPumpActiveRef.current) {
      return;
    }
    terrainRenderPumpActiveRef.current = true;
    void (async () => {
      try {
        while (terrainSchedulePendingRef.current) {
          terrainSchedulePendingRef.current = false;
          const renderer = terrainRendererRef.current;
          if (!renderer) {
            return;
          }
          const request = terrainScheduleRequestRef.current;
          if (!request) {
            return;
          }
          let query: TerrainOverlayQueryResult;
          try {
            query = await request.session.queryTerrainOverlay(
              request.viewport,
              request.width,
              request.height,
              Array.from(terrainTileCacheRef.current.keys()),
              Array.from(terrainTileInFlightRef.current),
            );
          } catch (error: unknown) {
            if (terrainScheduleRequestRef.current?.id !== request.id) {
              continue;
            }
            debugLog("terrain.overlay.query.error", {
              zoom: request.viewport.zoom,
              error: errorMessage(error),
            });
            console.warn("terrain overlay unavailable", error);
            continue;
          }
          const latestRequest = terrainScheduleRequestRef.current;
          const superseded = latestRequest?.id !== request.id;
          if (
            superseded
            && (
              query.status.state !== "ready"
              || !shouldLandCompletedCoalescedWork(
                request,
                latestRequest,
                landedTerrainScheduleRequestIdRef.current,
                terrainScheduleRequestsAreCompatible,
              )
            )
          ) {
            continue;
          }
          if (query.status.state !== "ready") {
            perfDebugLog("terrain.overlay.unavailable", () => ({
              status: query.status,
              request_count: query.tile_requests.length,
              zoom: request.viewport.zoom,
            }));
            terrainPendingFrameRef.current = null;
            setTerrainOverlay({ query, images: [] });
            continue;
          }
          const queryAltitudeBucket = query.altitude_bucket_ft;
          if (queryAltitudeBucket == null) {
            perfDebugLog("terrain.overlay.no_altitude_bucket", () => ({
              status: query.status,
              request_count: query.tile_requests.length,
              zoom: request.viewport.zoom,
            }));
            terrainPendingFrameRef.current = null;
            setTerrainOverlay({ query, images: [] });
            continue;
          }
          const frameKey = requireTerrainFrameKey(query);
          terrainPendingFrameRef.current = { requestId: request.id, query, frameKey };
          if (!terrainFrameStartRef.current.has(frameKey)) {
            terrainFrameStartRef.current.set(frameKey, performance.now());
            pruneTerrainFrameStarts(terrainFrameStartRef.current);
          }
          if (query.schedule.frame_complete) {
            commitTerrainFrameIfReady(frameKey, queryAltitudeBucket);
            continue;
          }
          const workBatch = query.schedule.work_batch;
          const renderPlanKey = [
            frameKey,
            query.schedule.cached_count,
            query.schedule.in_flight_count,
            query.schedule.missing_count,
            workBatch.length,
            workBatch[0]?.cache_key ?? "none",
          ].join(":");
          if (renderPlanKey !== lastTerrainRenderPlanKeyRef.current) {
            lastTerrainRenderPlanKeyRef.current = renderPlanKey;
            const requestSummary = terrainRequestSummary(query.tile_requests);
            perfDebugLog("terrain.overlay.render.plan", () => ({
              request_count: query.tile_requests.length,
              cached_count: query.schedule.cached_count,
              in_flight_count: query.schedule.in_flight_count,
              missing_count: query.schedule.missing_count,
              work_batch_count: workBatch.length,
              altitude_bucket: queryAltitudeBucket,
              request_zooms: requestSummary.zooms,
              next_tile: workBatch[0]?.key ?? null,
            }));
          }
          if (workBatch.length === 0) {
            continue;
          }
          const batch = workBatch
            .map((tileRequest) => ({ tileRequest, cacheKey: terrainCacheKey(tileRequest) }))
            .filter(({ cacheKey }) =>
              !terrainTileCacheRef.current.has(cacheKey) && !terrainTileInFlightRef.current.has(cacheKey),
            );
          if (batch.length === 0) {
            terrainSchedulePendingRef.current = true;
            continue;
          }
          for (const { cacheKey } of batch) {
            terrainTileInFlightRef.current.add(cacheKey);
          }
          try {
            const batchStartedAt = performance.now();
            let rawBytesTotal = 0;
            let tileCount = 0;
            await renderer.renderTiles(batch.map(({ tileRequest, cacheKey }) => ({
              generation: 0,
              cacheKey,
              tileKey: tileRequest.key,
              altitudeBucketFt: queryAltitudeBucket,
              sourceTiles: tileRequest.source_tiles,
            })), (result) => {
              const parsed = parseTerrainRawRgba(result.rawBytes);
              terrainTileCacheRef.current.set(result.cacheKey, parsed);
              rawBytesTotal += result.rawBytes.byteLength;
              tileCount += 1;
            });
            perfDebugLog("terrain.overlay.batch.done", () => ({
              altitude_bucket: queryAltitudeBucket,
              tile_count: tileCount,
              raw_bytes: rawBytesTotal,
              elapsed_ms: Math.round(performance.now() - batchStartedAt),
            }));
          } catch (error: unknown) {
            debugLog("terrain.overlay.batch.error", {
              altitude_bucket: queryAltitudeBucket,
              tile_count: batch.length,
              error: errorMessage(error),
            });
          } finally {
            for (const { cacheKey } of batch) {
              terrainTileInFlightRef.current.delete(cacheKey);
            }
          }
          for (const tileRequest of workBatch) {
            if (!terrainTileCacheRef.current.has(terrainCacheKey(tileRequest))) {
              perfDebugLog("terrain.overlay.tile.error", () => ({
                key: tileRequest.key,
                error: "batch render did not return tile",
              }));
            }
          }
          terrainSchedulePendingRef.current = true;
        }
      } finally {
        terrainRenderPumpActiveRef.current = false;
        if (terrainSchedulePendingRef.current) {
          pumpTerrainRenderQueue();
        }
      }
    })();
  }

  function terrainScheduleRequestsAreCompatible(
    completed: TerrainScheduleRequest,
    latest: TerrainScheduleRequest,
  ) {
    return (
      latest.session === completed.session
      && latest.navDataEpoch === completed.navDataEpoch
      && latest.altitudeBucket === completed.altitudeBucket
      && latest.width === completed.width
      && latest.height === completed.height
      && Math.abs(latest.viewport.zoom - completed.viewport.zoom) < 0.001
    );
  }

  function commitTerrainFrameIfReady(frameKey: string, altitudeBucket: number) {
    const pendingFrame = terrainPendingFrameRef.current;
    if (!pendingFrame || pendingFrame.frameKey !== frameKey) {
      return false;
    }
    const readyImages = terrainImagesForCompleteQuery(terrainTileCacheRef.current, pendingFrame.query);
    if (!readyImages) {
      return false;
    }
    const frameStartedAt = terrainFrameStartRef.current.get(frameKey);
    terrainFrameStartRef.current.delete(frameKey);
    terrainPendingFrameRef.current = null;
    landedTerrainScheduleRequestIdRef.current = pendingFrame.requestId;
    perfDebugLog("terrain.overlay.frame.ready", () => ({
      altitude_bucket: altitudeBucket,
      request_count: pendingFrame.query.tile_requests.length,
      image_count: readyImages.length,
      elapsed_ms: frameStartedAt == null ? null : Math.round(performance.now() - frameStartedAt),
    }));
    setTerrainOverlay({
      query: pendingFrame.query,
      images: readyImages,
    });
    logAfterNextPaint("terrain.overlay.frame.painted", performance.now(), {
      altitude_bucket: altitudeBucket,
      request_count: pendingFrame.query.tile_requests.length,
      image_count: readyImages.length,
    });
    return true;
  }

  function pumpNexradQueryQueue() {
    if (nexradQueryPumpActiveRef.current) {
      return;
    }
    nexradQueryPumpActiveRef.current = true;
    void (async () => {
      try {
        while (nexradQueryPendingRef.current) {
          nexradQueryPendingRef.current = false;
          const request = nexradQueryRequestRef.current;
          if (!request) {
            continue;
          }
          const startedAt = performance.now();
          try {
            const query = await request.session.queryNexradOverlay(request.viewport, request.width, request.height);
            const queryDoneAt = performance.now();
            if (nexradQueryRequestRef.current?.id !== request.id) {
              continue;
            }
            const preloadStartedAt = performance.now();
            const preload = await preloadNexradOverlayImages(query, nexradFrameCacheRef.current!);
            const preloadDoneAt = performance.now();
            if (nexradQueryRequestRef.current?.id !== request.id) {
              continue;
            }
            setNexradOverlay(query);
            setNexradOverlayFrame(query.status.state === "hidden"
              ? null
              : { viewport: request.viewport, width: request.width, height: request.height });
            const commitQueuedAt = performance.now();
            const previousPaint = nexradLastPaintTimingRef.current;
            window.requestAnimationFrame(() => {
              window.requestAnimationFrame(() => {
                const paintedAt = performance.now();
                const currentPaint: NexradPaintTiming = {
                  paintedAtMs: paintedAt,
                  requestId: request.id,
                  phase: query.animation.phase,
                  selectedFrameIndex: query.animation.selected_frame_index ?? null,
                  frameCount: query.animation.frame_count,
                  nextUpdateDelayMs: query.animation.next_update_delay_ms ?? null,
                  nextUpdateEpochMs: query.animation.next_update_epoch_ms ?? null,
                  status: query.status.state,
                };
                nexradLastPaintTimingRef.current = currentPaint;
                temporaryNexradTimingLog("frame_painted", {
                  request_id: request.id,
                  status: query.status.state,
                  phase: query.animation.phase,
                  selected_frame_index: query.animation.selected_frame_index,
                  frame_count: query.animation.frame_count,
                  next_update_delay_ms: query.animation.next_update_delay_ms,
                  next_update_epoch_ms: query.animation.next_update_epoch_ms,
                  previous_request_id: previousPaint?.requestId ?? null,
                  previous_status: previousPaint?.status ?? null,
                  previous_phase: previousPaint?.phase ?? null,
                  previous_selected_frame_index: previousPaint?.selectedFrameIndex ?? null,
                  previous_next_update_delay_ms: previousPaint?.nextUpdateDelayMs ?? null,
                  previous_next_update_epoch_ms: previousPaint?.nextUpdateEpochMs ?? null,
                  visible_dwell_ms: previousPaint ? Math.round(paintedAt - previousPaint.paintedAtMs) : null,
                  query_elapsed_ms: Math.round(queryDoneAt - startedAt),
                  preload_elapsed_ms: Math.round(preloadDoneAt - preloadStartedAt),
                  commit_to_paint_ms: Math.round(paintedAt - commitQueuedAt),
                  total_query_to_paint_ms: Math.round(paintedAt - startedAt),
                  tiles: query.tiles.length,
                  loaded_images: preload.loaded,
                  failed_images: preload.failed,
                });
              });
            });
            perfDebugLog("nexrad.overlay.frame.ready", () => ({
              status: query.status,
              tiles: query.tiles.length,
              loaded_images: preload.loaded,
              failed_images: preload.failed,
              elapsed_ms: Math.round(performance.now() - startedAt),
            }));
            if (query.status.state !== "ready") {
              perfDebugLog("nexrad.overlay.unavailable", () => ({ status: query.status }));
            } else if (request.debugTileLabels) {
              perfDebugLog("nexrad.overlay.mesh", () => query.stats);
            }
          } catch (error: unknown) {
            if (nexradQueryRequestRef.current?.id !== request.id) {
              continue;
            }
            setNexradOverlay({
              status: { state: "unavailable", reason: errorMessage(error) },
              tiles: [],
              stats: emptyNexradOverlayStats(),
              animation: emptyNexradOverlayAnimation(),
            });
            setNexradOverlayFrame(null);
          }
        }
      } finally {
        nexradQueryPumpActiveRef.current = false;
        if (nexradQueryPendingRef.current) {
          pumpNexradQueryQueue();
        }
      }
    })();
  }

  function clearNexradViewportRefreshTimer() {
    const timer = nexradViewportRefreshTimerRef.current;
    if (timer == null) {
      return;
    }
    window.clearTimeout(timer);
    nexradViewportRefreshTimerRef.current = null;
  }

  function requestThrottledNexradViewportRefresh() {
    if (nexradViewportRefreshTimerRef.current != null) {
      return;
    }
    nexradViewportRefreshTimerRef.current = window.setTimeout(() => {
      nexradViewportRefreshTimerRef.current = null;
      setNexradViewportRefreshTick((tick) => tick + 1);
    }, NEXRAD_VIEWPORT_REFRESH_THROTTLE_MS);
  }

  function pumpMapOverlayQueryQueue() {
    if (mapOverlayQueryPumpActiveRef.current) {
      return;
    }
    mapOverlayQueryPumpActiveRef.current = true;
    void (async () => {
      try {
        while (mapOverlayQueryPendingRef.current) {
          mapOverlayQueryPendingRef.current = false;
          const request = mapOverlayQueryRequestRef.current;
          if (!request) {
            continue;
          }
          const startedAt = performance.now();
          try {
            perfDebugLog("map.overlay.query.start", () => ({
              zoom: request.viewport.zoom,
              center: request.center,
              width: request.width,
              height: request.height,
              queue_wait_ms: Math.round(startedAt - request.requestedAt),
            }));
            const overlay = await request.session.queryMapOverlay(
              request.viewport,
              request.width,
              request.height,
            );
            const latestRequest = mapOverlayQueryRequestRef.current;
            const superseded = latestRequest?.id !== request.id;
            if (superseded && !shouldLandCompletedCoalescedWork(
              request,
              latestRequest,
              landedMapOverlayQueryRequestIdRef.current,
              mapOverlayRequestsAreCompatible,
            )) {
              perfDebugLog("map.overlay.query.stale_result", () => ({
                request_id: request.id,
                current_request_id: mapOverlayQueryRequestRef.current?.id ?? null,
                newer_pending: mapOverlayQueryPendingRef.current,
                zoom: request.viewport.zoom,
                elapsed_ms: Math.round(performance.now() - startedAt),
              }));
              continue;
            }
            landMapOverlayQuery(request, overlay, startedAt, superseded);
          } catch (error) {
            if (mapOverlayQueryRequestRef.current?.id !== request.id) {
              perfDebugLog("map.overlay.query.stale_error", () => ({
                zoom: request.viewport.zoom,
                elapsed_ms: Math.round(performance.now() - startedAt),
                error: errorMessage(error),
              }));
              continue;
            }
            if (isInvalidUiSessionHandleError(error)) {
              perfDebugLog("map.overlay.query.stale_session", () => ({
                zoom: request.viewport.zoom,
                elapsed_ms: Math.round(performance.now() - startedAt),
                error: errorMessage(error),
              }));
              continue;
            }
            debugLog("map.overlay.query.error", {
              zoom: request.viewport.zoom,
              elapsed_ms: Math.round(performance.now() - startedAt),
              error: errorMessage(error),
            });
            setMapOverlay({
              visible_features: [],
              visible_metars: [],
              visible_pireps: [],
              visible_traffic: [],
              traffic_next_refresh_epoch_ms: null,
              airspace_paths: [],
              tfr_paths: [],
              airspace_labels: [],
              offline_regions: [],
            });
            setMapOverlayFrame(null);
            console.error(error);
          }
        }
      } finally {
        mapOverlayQueryPumpActiveRef.current = false;
        if (mapOverlayQueryPendingRef.current) {
          pumpMapOverlayQueryQueue();
        }
      }
    })();
  }

  function mapOverlayRequestsAreCompatible(
    completed: NonNullable<typeof mapOverlayQueryRequestRef.current>,
    latest: NonNullable<typeof mapOverlayQueryRequestRef.current>,
  ) {
    return (
      latest.session === completed.session
      && latest.navDataEpoch === completed.navDataEpoch
      && latest.width === completed.width
      && latest.height === completed.height
      && latest.layerKey === completed.layerKey
      && Math.abs(latest.viewport.zoom - completed.viewport.zoom) < 0.001
    );
  }

  function landMapOverlayQuery(
    request: NonNullable<typeof mapOverlayQueryRequestRef.current>,
    overlay: MapOverlayQueryResult,
    startedAt: number,
    superseded: boolean,
  ) {
    const landStartedAt = performance.now();
    landedMapOverlayQueryRequestIdRef.current = request.id;
    setMapOverlay(overlay);
    const overlayStateQueuedAt = performance.now();
    setMapOverlayFrame({ viewport: request.viewport, width: request.width, height: request.height });
    const landEndedAt = performance.now();
    const overlayCounts = {
      visible_features: overlay.visible_features.length,
      visible_metars: overlay.visible_metars.length,
      visible_pireps: overlay.visible_pireps.length,
      airspace_paths: overlay.airspace_paths.length,
      tfr_paths: overlay.tfr_paths.length,
      airspace_labels: overlay.airspace_labels.length,
      offline_regions: overlay.offline_regions.length,
      flight_plan_features: overlay.flight_plan_features?.length ?? 0,
    };
    mapOverlayLandingTimingRef.current = {
      id: request.id,
      requestedAt: request.requestedAt,
      queryStartedAt: startedAt,
      landStartedAt,
      landEndedAt,
      visibleFeatures: overlayCounts.visible_features,
      visibleMetars: overlayCounts.visible_metars,
      visiblePireps: overlayCounts.visible_pireps,
      airspacePaths: overlayCounts.airspace_paths,
      tfrPaths: overlayCounts.tfr_paths,
      airspaceLabels: overlayCounts.airspace_labels,
      offlineRegions: overlayCounts.offline_regions,
      flightPlanFeatures: overlayCounts.flight_plan_features,
      committed: false,
    };
    perfDebugLog("map.overlay.query.land_steps", () => ({
      id: request.id,
      superseded,
      ...overlayCounts,
      overlay_state_queue_ms: Math.round(overlayStateQueuedAt - landStartedAt),
      viewport_state_queue_ms: Math.round(landEndedAt - overlayStateQueuedAt),
      land_sync_ms: Math.round(landEndedAt - landStartedAt),
      elapsed_ms: Math.round(landEndedAt - startedAt),
    }));
    perfDebugLog(superseded ? "map.overlay.query.superseded_result" : "map.overlay.query.done", () => ({
      id: request.id,
      zoom: request.viewport.zoom,
      center: request.center,
      elapsed_ms: Math.round(performance.now() - startedAt),
      ...overlayCounts,
    }));
    logAfterNextPaint("map.overlay.query.after_paint", startedAt, {
      id: request.id,
      superseded,
      ...overlayCounts,
    });
  }

  useLayoutEffect(() => {
    mapUpDegRef.current = plannedMapUpDeg;
    previousMapOrientationModeRef.current = mapOrientationMode;
  }, [mapOrientationMode, plannedMapUpDeg]);

  useLayoutEffect(() => {
    const timing = mapOverlayLandingTimingRef.current;
    if (!timing || timing.committed) {
      return;
    }
    timing.committed = true;
    const committedAt = performance.now();
    perfDebugLog("map.overlay.query.commit", () => ({
      id: timing.id,
      visible_features: timing.visibleFeatures,
      visible_metars: timing.visibleMetars,
      visible_pireps: timing.visiblePireps,
      airspace_paths: timing.airspacePaths,
      tfr_paths: timing.tfrPaths,
      airspace_labels: timing.airspaceLabels,
      offline_regions: timing.offlineRegions,
      flight_plan_features: timing.flightPlanFeatures,
      land_sync_ms: Math.round(timing.landEndedAt - timing.landStartedAt),
      land_to_commit_ms: Math.round(committedAt - timing.landStartedAt),
      request_to_commit_ms: Math.round(committedAt - timing.queryStartedAt),
      queue_to_commit_ms: Math.round(committedAt - timing.requestedAt),
    }));
  }, [mapOverlay, mapOverlayFrame]);

  useLayoutEffect(() => {
    const layoutBasis = mapSelectionLayoutBasisRef.current;
    const viewportInvalidatesSelection = committedViewportInvalidatesMapSelection(
      viewport,
      viewportRef.current,
      pendingReactViewportRef.current !== null,
      gestureActiveRef.current,
    );
    const layoutInvalidatesSelection = layoutBasis.width !== surfaceSize.width
      || layoutBasis.height !== surfaceSize.height
      || layoutBasis.uiSession !== uiSession;
    if (viewportInvalidatesSelection || layoutInvalidatesSelection) {
      mapSelectionRequestGenerationRef.current += 1;
    }
    mapSelectionLayoutBasisRef.current = {
      width: surfaceSize.width,
      height: surfaceSize.height,
      uiSession,
    };
    committedViewportRef.current = viewport;
    if (activePointersRef.current.size === 0 && !pendingReactViewportRef.current) {
      viewportRef.current = viewport;
    }
    applyImperativeMapContentTransform();
  }, [surfaceSize.height, surfaceSize.width, uiSession, viewport]);

  useEffect(() => () => {
    clearPendingReactViewportCommit();
  }, []);

  useEffect(() => {
    if (!containerRef.current) {
      return;
    }
    const observer = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (!entry) {
        return;
      }
      setSurfaceSize({
        width: entry.contentRect.width,
        height: entry.contentRect.height,
      });
    });
    observer.observe(containerRef.current);
    return () => observer.disconnect();
  }, []);

  const chartSearchAnchorWorldX = viewport.centerWorldX.toFixed(6);
  const chartSearchAnchorWorldY = viewport.centerWorldY.toFixed(6);
  const chartSearchAnchor = useMemo(
    () => viewportCenterLatLon(viewport),
    [chartSearchAnchorWorldX, chartSearchAnchorWorldY],
  );
  useEffect(() => {
    const query = chartSearch.query;
    if (!chartSearch.open || query.trim().length === 0) {
      setChartSearch((current) => ({ ...current, loading: false, error: null, suggestions: [] }));
      return;
    }
    let cancelled = false;
    setChartSearch((current) => ({ ...current, loading: true, error: null }));
    props.appCoreAdapter
      .suggestWaypointIdentifiersNear(chartSearchAnchor, query, 8)
      .then((suggestions) => {
        if (!cancelled) {
          setChartSearch((current) => ({ ...current, loading: false, error: null, suggestions }));
        }
      })
      .catch((error) => {
        if (!cancelled) {
          setChartSearch((current) => ({
            ...current,
            loading: false,
            error: `Search failed: ${errorMessage(error)}`,
            suggestions: [],
          }));
        }
      });
    return () => {
      cancelled = true;
    };
  }, [chartSearch.open, chartSearch.query, chartSearchAnchor, props.appCoreAdapter]);
  const [tiles, setTiles] = useState<RasterRenderTile[]>([]);
  const [chartReferenceAction, setChartReferenceAction] = useState<RasterTilePlan["chart_reference_action"]>(null);
  const [rasterTileViewport, setRasterTileViewport] = useState<MapViewportState | null>(null);
  const [rasterTileFrame, setRasterTileFrame] = useState<MapDisplayFrame | null>(null);
  const [failedRasterTileKeys, setFailedRasterTileKeys] = useState<Set<string>>(() => new Set());
  const [rasterTileLoadAttempts, setRasterTileLoadAttempts] = useState<Map<string, number>>(() => new Map());
  const rasterTileLoadAttemptsRef = useRef<Map<string, number>>(new Map());
  const rasterTileRecoveryCountRef = useRef(0);
  const [e2eRasterTileFaultKeys, setE2eRasterTileFaultKeys] = useState<Set<string>>(() => new Set());
  const e2eRasterTileFaultKeysRef = useRef<Set<string>>(new Set());
  const loadedRasterTileKeysRef = useRef<Set<string>>(new Set());
  const completedPageTilePaintTimingIdsRef = useRef<Set<number>>(new Set());
  const rasterTilePlanRequestRef = useRef<{
    id: number;
    key: string;
    requestedAt: number;
    session: UiSession;
    viewport: MapViewportState;
    width: number;
    height: number;
    devicePixelRatio: number;
    selectedMapId: string;
    navDataEpoch: number;
    pageTilePaintTiming: WebPageTilePaintTiming | null;
  } | null>(null);
  const rasterTilePlanRequestIdRef = useRef(0);
  const rasterTilePlanRequestKeyRef = useRef<string | null>(null);
  const rasterTilePlanPendingRef = useRef(false);
  const rasterTilePlanPumpActiveRef = useRef(false);
  const landedRasterTilePlanRequestIdRef = useRef(0);
  const rasterTileImageLoadStartedAtRef = useRef<number | null>(null);
  const knownLoadedRasterTileKeysRef = useRef<Set<string>>(new Set());
  const rasterTilePlanLandingTimingRef = useRef<{
    id: number;
    key: string;
    requestedAt: number;
    queryStartedAt: number;
    landStartedAt: number;
    landEndedAt: number;
    tiles: number;
    previousTiles: number;
    sameTileKeys: boolean;
    sameViewport: boolean;
    committed: boolean;
  } | null>(null);
  const rasterTileKey = useCallback((tile: RasterRenderTile) =>
    `${tile.chartFamily}-${tile.packageName ?? tile.mapViewId}-${tile.drawKey}`,
  []);
  const recoverRasterTileLoads = useCallback((
    tileList: RasterRenderTile[],
    trigger: "error" | "watchdog",
  ) => {
    const decision = classifyRasterTileLoadRecovery(
      tileList.map(rasterTileKey),
      loadedRasterTileKeysRef.current,
      failedRasterTileKeys,
      rasterTileLoadAttemptsRef.current,
    );
    if (decision.retry.length > 0) {
      const nextAttempts = new Map(rasterTileLoadAttemptsRef.current);
      for (const key of decision.retry) {
        nextAttempts.set(key, (nextAttempts.get(key) ?? 0) + 1);
      }
      rasterTileLoadAttemptsRef.current = nextAttempts;
      rasterTileRecoveryCountRef.current += 1;
      setRasterTileLoadAttempts(nextAttempts);
      debugLog("map.raster.tile.recovery", {
        trigger,
        selected_map_id: selectedMap.selected_map_id,
        retry_count: rasterTileRecoveryCountRef.current,
        tile_count: decision.retry.length,
        tile_keys: decision.retry,
      });
    }
    if (decision.exhausted.length > 0) {
      setFailedRasterTileKeys((current) => {
        const next = new Set(current);
        for (const key of decision.exhausted) {
          next.add(key);
        }
        return next.size === current.size ? current : next;
      });
      debugLog("map.raster.tile.recovery_exhausted", {
        trigger,
        selected_map_id: selectedMap.selected_map_id,
        tile_count: decision.exhausted.length,
        tile_keys: decision.exhausted,
      });
    }
    return decision;
  }, [failedRasterTileKeys, rasterTileKey, selectedMap.selected_map_id]);
  const rasterTilePlanKey = useCallback((
    nextViewport: MapViewportState,
    width: number,
    height: number,
    devicePixelRatio: number,
    selectedMapId: string,
    navDataEpoch: number,
  ) => [
    navDataEpoch,
    selectedMapId,
    width,
    height,
    devicePixelRatio,
    nextViewport.zoom.toFixed(6),
    nextViewport.centerWorldX.toFixed(3),
    nextViewport.centerWorldY.toFixed(3),
    (nextViewport.rotationDeg ?? 0).toFixed(3),
  ].join("|"), []);
  const reportRasterTilesReadyIfComplete = useCallback((tileList: RasterRenderTile[]) => {
    if (page !== "map" || tileList.length === 0) {
      return false;
    }
    const loadedKeys = loadedRasterTileKeysRef.current;
    const allLoaded = tileList.every((entry) => loadedKeys.has(rasterTileKey(entry)));
    if (!allLoaded) {
      return false;
    }
    const imageLoadStartedAt = rasterTileImageLoadStartedAtRef.current;
    if (imageLoadStartedAt !== null) {
      requestAnimationFrame(() => {
        perfDebugLog("map.raster.images.done", () => ({
          elapsed_ms: Math.round(performance.now() - imageLoadStartedAt),
          tiles: tileList.length,
        }));
      });
      rasterTileImageLoadStartedAtRef.current = null;
    }
    return true;
  }, [page, rasterTileKey]);
  const completePageTilePaintTiming = useCallback((timing: WebPageTilePaintTiming, phase: "frame" | "images") => {
    if (completedPageTilePaintTimingIdsRef.current.has(timing.id)) {
      return;
    }
    requestAnimationFrame(() => {
      completedPageTilePaintTimingIdsRef.current.add(timing.id);
      perfDebugLog("web.page-to-map.frame", () => ({
        id: timing.id,
        from_page: timing.fromPage,
        phase,
        elapsed_ms: Math.round(performance.now() - timing.startedAt),
        tiles: tiles.length,
      }));
      onPageTilePaintTimingComplete(timing.id);
    });
  }, [onPageTilePaintTimingComplete, tiles.length]);

  function rasterPlanRequestsAreCompatible(
    completed: NonNullable<typeof rasterTilePlanRequestRef.current>,
    latest: NonNullable<typeof rasterTilePlanRequestRef.current>,
  ) {
    return (
      latest.session === completed.session
      && latest.navDataEpoch === completed.navDataEpoch
      && latest.selectedMapId === completed.selectedMapId
      && latest.width === completed.width
      && latest.height === completed.height
      && latest.devicePixelRatio === completed.devicePixelRatio
    );
  }

  function landRasterTilePlan(
    request: NonNullable<typeof rasterTilePlanRequestRef.current>,
    nextTiles: RasterRenderTile[],
    nextChartReferenceAction: RasterTilePlan["chart_reference_action"],
    superseded: boolean,
    startedAt: number,
  ) {
    const landStartedAt = performance.now();
    landedRasterTilePlanRequestIdRef.current = request.id;
    const nextTileKeys = nextTiles.map(rasterTileKey);
    const previousTileKeys = tiles.map(rasterTileKey);
    const sameTileKeys = nextTileKeys.length === previousTileKeys.length
      && nextTileKeys.every((key, index) => key === previousTileKeys[index]);
    const sameViewport = rasterTileViewport !== null
      && rasterTileViewport.zoom === request.viewport.zoom
      && rasterTileViewport.centerWorldX === request.viewport.centerWorldX
      && rasterTileViewport.centerWorldY === request.viewport.centerWorldY;
    const keyingEndedAt = performance.now();
    loadedRasterTileKeysRef.current = new Set(
      nextTileKeys.filter((key) => knownLoadedRasterTileKeysRef.current.has(key)),
    );
    const loadedKeyResetEndedAt = performance.now();
    rasterTileImageLoadStartedAtRef.current = performance.now();
    const imageTimingResetEndedAt = performance.now();
    setFailedRasterTileKeys(new Set());
    rasterTileLoadAttemptsRef.current = new Map();
    setRasterTileLoadAttempts(new Map());
    e2eRasterTileFaultKeysRef.current = new Set();
    setE2eRasterTileFaultKeys(new Set());
    const failedStateQueuedAt = performance.now();
    setTiles(nextTiles);
    setChartReferenceAction(nextChartReferenceAction ?? null);
    const tilesStateQueuedAt = performance.now();
    setRasterTileViewport(request.viewport);
    setRasterTileFrame({ viewport: request.viewport, width: request.width, height: request.height });
    const landEndedAt = performance.now();
    rasterTilePlanLandingTimingRef.current = {
      id: request.id,
      key: request.key,
      requestedAt: request.requestedAt,
      queryStartedAt: startedAt,
      landStartedAt,
      landEndedAt,
      tiles: nextTiles.length,
      previousTiles: tiles.length,
      sameTileKeys,
      sameViewport,
      committed: false,
    };
    perfDebugLog("map.raster.plan.landed", () => ({
      id: request.id,
      key: request.key,
      superseded,
      tiles: nextTiles.length,
      previous_tiles: tiles.length,
      same_tile_keys: sameTileKeys,
      same_viewport: sameViewport,
      land_sync_ms: Math.round(landEndedAt - landStartedAt),
      elapsed_ms: Math.round(landEndedAt - startedAt),
    }));
    perfDebugLog("map.raster.plan.land_steps", () => ({
      id: request.id,
      key: request.key,
      superseded,
      tiles: nextTiles.length,
      previous_tiles: tiles.length,
      same_tile_keys: sameTileKeys,
      same_viewport: sameViewport,
      keying_ms: Math.round(keyingEndedAt - landStartedAt),
      loaded_key_reset_ms: Math.round(loadedKeyResetEndedAt - keyingEndedAt),
      image_timing_reset_ms: Math.round(imageTimingResetEndedAt - loadedKeyResetEndedAt),
      failed_state_queue_ms: Math.round(failedStateQueuedAt - imageTimingResetEndedAt),
      tiles_state_queue_ms: Math.round(tilesStateQueuedAt - failedStateQueuedAt),
      viewport_state_queue_ms: Math.round(landEndedAt - tilesStateQueuedAt),
      land_sync_ms: Math.round(landEndedAt - landStartedAt),
      elapsed_ms: Math.round(landEndedAt - startedAt),
    }));
    logAfterNextPaint("map.raster.plan.after_paint", startedAt, {
      id: request.id,
      key: request.key,
      superseded,
      tiles: nextTiles.length,
      previous_tiles: tiles.length,
      same_tile_keys: sameTileKeys,
      same_viewport: sameViewport,
    });
    reportRasterTilesReadyIfComplete(nextTiles);
  }

  useLayoutEffect(() => {
    const timing = rasterTilePlanLandingTimingRef.current;
    if (!timing || timing.committed) {
      return;
    }
    timing.committed = true;
    const committedAt = performance.now();
    perfDebugLog("map.raster.plan.commit", () => ({
      id: timing.id,
      key: timing.key,
      tiles: timing.tiles,
      previous_tiles: timing.previousTiles,
      same_tile_keys: timing.sameTileKeys,
      same_viewport: timing.sameViewport,
      land_sync_ms: Math.round(timing.landEndedAt - timing.landStartedAt),
      land_to_commit_ms: Math.round(committedAt - timing.landStartedAt),
      request_to_commit_ms: Math.round(committedAt - timing.queryStartedAt),
      queue_to_commit_ms: Math.round(committedAt - timing.requestedAt),
    }));
  }, [rasterTileViewport, tiles]);

  useEffect(() => {
    if (page !== "map" || tiles.length === 0) {
      return;
    }
    const requestId = landedRasterTilePlanRequestIdRef.current;
    const timeout = window.setTimeout(() => {
      if (requestId !== landedRasterTilePlanRequestIdRef.current) {
        return;
      }
      recoverRasterTileLoads(tiles, "watchdog");
    }, RASTER_TILE_LOAD_RECOVERY_DELAY_MS);
    return () => window.clearTimeout(timeout);
  }, [page, rasterTileLoadAttempts, recoverRasterTileLoads, tiles]);

  function pumpRasterTilePlanQueue() {
    if (rasterTilePlanPumpActiveRef.current) {
      return;
    }
    rasterTilePlanPumpActiveRef.current = true;
    void (async () => {
      try {
        while (rasterTilePlanPendingRef.current) {
          rasterTilePlanPendingRef.current = false;
          const request = rasterTilePlanRequestRef.current;
          if (!request) {
            continue;
          }
          const startedAt = performance.now();
          try {
            perfDebugLog("map.raster.plan.start", () => ({
              id: request.id,
              key: request.key,
              selected_map_id: request.selectedMapId,
              zoom: request.viewport.zoom,
              center_world_x: request.viewport.centerWorldX,
              center_world_y: request.viewport.centerWorldY,
              width: request.width,
              height: request.height,
              device_pixel_ratio: request.devicePixelRatio,
              queue_wait_ms: Math.round(startedAt - request.requestedAt),
            }));
            const plan = await request.session.queryRasterTilePlan(
              request.viewport,
              request.width,
              request.height,
              request.devicePixelRatio,
            );
            const latestPlanRequest = rasterTilePlanRequestRef.current;
            const planSuperseded = latestPlanRequest?.id !== request.id;
            if (planSuperseded && !shouldLandCompletedCoalescedWork(
              request,
              latestPlanRequest,
              landedRasterTilePlanRequestIdRef.current,
              rasterPlanRequestsAreCompatible,
            )) {
              perfDebugLog("map.raster.plan.stale_result", () => ({
                id: request.id,
                elapsed_ms: Math.round(performance.now() - startedAt),
                tiles: plan.tiles.length,
              }));
              continue;
            }
            if (planSuperseded) {
              perfDebugLog("map.raster.plan.superseded_result", () => ({
                id: request.id,
                latest_id: rasterTilePlanRequestRef.current?.id ?? null,
                elapsed_ms: Math.round(performance.now() - startedAt),
                tiles: plan.tiles.length,
              }));
            }
            const pageTilePaintTiming = request.pageTilePaintTiming;
            if (!planSuperseded && pageTilePaintTiming) {
              perfDebugLog("web.page-to-map.plan", () => ({
                id: pageTilePaintTiming.id,
                from_page: pageTilePaintTiming.fromPage,
                elapsed_ms: Math.round(performance.now() - pageTilePaintTiming.startedAt),
                tiles: plan.tiles.length,
                device_pixel_ratio: request.devicePixelRatio,
              }));
            }
            perfDebugLog("map.raster.plan.done", () => ({
              id: request.id,
              key: request.key,
              superseded: planSuperseded,
              elapsed_ms: Math.round(performance.now() - startedAt),
              tiles: plan.tiles.length,
            }));
            const resolveStartedAt = performance.now();
            const nextTiles = plan.tiles.map((tile) =>
              renderTileFromCore(tile, 1 / request.devicePixelRatio),
            );
            const latestUrlRequest = rasterTilePlanRequestRef.current;
            const urlsSuperseded = latestUrlRequest?.id !== request.id;
            if (urlsSuperseded && !shouldLandCompletedCoalescedWork(
              request,
              latestUrlRequest,
              landedRasterTilePlanRequestIdRef.current,
              rasterPlanRequestsAreCompatible,
            )) {
              perfDebugLog("map.raster.tiles.stale_result", () => ({
                id: request.id,
                elapsed_ms: Math.round(performance.now() - resolveStartedAt),
                tiles: nextTiles.length,
              }));
              continue;
            }
            if (urlsSuperseded && !planSuperseded) {
              perfDebugLog("map.raster.tiles.superseded_result", () => ({
                id: request.id,
                latest_id: rasterTilePlanRequestRef.current?.id ?? null,
                elapsed_ms: Math.round(performance.now() - resolveStartedAt),
                tiles: nextTiles.length,
              }));
            }
            perfDebugLog("map.raster.tiles.done", () => ({
              id: request.id,
              key: request.key,
              superseded: urlsSuperseded,
              elapsed_ms: Math.round(performance.now() - resolveStartedAt),
              tiles: nextTiles.length,
            }));
            landRasterTilePlan(
              request,
              nextTiles,
              plan.chart_reference_action,
              urlsSuperseded,
              startedAt,
            );
          } catch (error) {
            if (rasterTilePlanRequestRef.current?.id !== request.id) {
              perfDebugLog("map.raster.plan.stale_error", () => ({
                id: request.id,
                elapsed_ms: Math.round(performance.now() - startedAt),
                error: errorMessage(error),
              }));
              continue;
            }
            debugLog("map.raster.plan.error", {
              id: request.id,
              elapsed_ms: Math.round(performance.now() - startedAt),
              error: errorMessage(error),
            });
            console.error("failed to query raster tile plan", error);
            setTiles([]);
            setChartReferenceAction(null);
            setRasterTileViewport(null);
            setRasterTileFrame(null);
          }
        }
      } finally {
        rasterTilePlanPumpActiveRef.current = false;
        if (rasterTilePlanPendingRef.current) {
          pumpRasterTilePlanQueue();
        }
      }
    })();
  }

  useEffect(() => {
    if (!uiSession || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      rasterTilePlanRequestRef.current = null;
      rasterTilePlanPendingRef.current = false;
      setTiles([]);
      setRasterTileViewport(null);
      setRasterTileFrame(null);
      return;
    }
    const devicePixelRatio = window.devicePixelRatio || 1;
    const key = rasterTilePlanKey(
      rasterPlanningViewport,
      surfaceSize.width,
      surfaceSize.height,
      devicePixelRatio,
      selectedMap.selected_map_id,
      navDataEpoch,
    );
    if (rasterTilePlanRequestKeyRef.current === key) {
      return;
    }
    rasterTilePlanRequestKeyRef.current = key;
    rasterTilePlanRequestRef.current = {
      id: ++rasterTilePlanRequestIdRef.current,
      key,
      requestedAt: performance.now(),
      session: uiSession,
      viewport: rasterPlanningViewport,
      width: surfaceSize.width,
      height: surfaceSize.height,
      devicePixelRatio,
      selectedMapId: selectedMap.selected_map_id,
      navDataEpoch,
      pageTilePaintTiming,
    };
    rasterTilePlanPendingRef.current = true;
    pumpRasterTilePlanQueue();
  }, [navDataEpoch, rasterPlanningViewport, rasterTilePlanKey, selectedMap.selected_map_id, surfaceSize.height, surfaceSize.width, uiSession]);
  useEffect(() => {
    if (page !== "map" || !pageTilePaintTiming || tiles.length === 0) {
      return;
    }
    const loadedKeys = loadedRasterTileKeysRef.current;
    const tileKeys = tiles.map(rasterTileKey);
    const allAlreadyLoaded = tileKeys.length > 0 && tileKeys.every((key) => loadedKeys.has(key));
    if (allAlreadyLoaded) {
      completePageTilePaintTiming(pageTilePaintTiming, "frame");
    }
  }, [completePageTilePaintTiming, page, pageTilePaintTiming, rasterTileKey, tiles]);
  const mapIsVisible = page === "map";
  const [situationRingCandidates, setSituationRingCandidates] = useState<SituationRingCandidate[]>([]);
  useEffect(() => {
    let cancelled = false;
    setSituationRingCandidates(appCoreAdapter.situationRingCandidates());
    void appCoreAdapter.loadSituationRingCandidates().then((candidates) => {
      if (!cancelled) {
        setSituationRingCandidates(candidates);
      }
    }).catch((error) => {
      debugLog("situation_ring_candidates.load.failed", {
        message: error instanceof Error ? error.message : String(error),
      });
    });
    return () => {
      cancelled = true;
    };
  }, [appCoreAdapter]);
  const situationOverlay = useMemo(
    () => resolveSituationOverlay(ownship, viewport, surfaceSize.width, surfaceSize.height, situationRingCandidates),
    [ownship, viewport, surfaceSize.height, surfaceSize.width, situationRingCandidates],
  );
  const mapOverlayLayersVisible =
    mapLayerState.vectors.visible
    || mapLayerState.metars.visible
    || mapLayerState.traffic.visible
    || mapLayerState.offline_regions.visible;
  const setMapLayerVisible = useCallback(async (layerId: MapLayerId, visible: boolean) => {
    if (!uiSession || layerToggleBusyRef.current) {
      return;
    }
    layerToggleBusyRef.current = true;
    try {
      const nextSnapshot = await uiSession.setMapLayerVisibility(layerId, visible);
      onPlaybackSnapshotChange(nextSnapshot);
    } finally {
      layerToggleBusyRef.current = false;
    }
  }, [onPlaybackSnapshotChange, uiSession]);
  const routeScreenSegments = useMemo(() => {
    if (surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return [];
    }
    return flightPlanRoute.map((segment) => ({
      ...segment,
      path: (segment.path.length > 0 ? segment.path : [segment.from, segment.to])
        .map((point) => worldToScreen(viewport, latLonToWorld(point.lat, point.lon), surfaceSize.width, surfaceSize.height)),
      finishLinePaths: (segment.finish_lines ?? []).map((line) =>
        [line.start, line.end].map((point) =>
          worldToScreen(viewport, latLonToWorld(point.lat, point.lon), surfaceSize.width, surfaceSize.height),
        ),
      ),
    }));
  }, [flightPlanRoute, surfaceSize.height, surfaceSize.width, viewport]);
  const routeDistancePillLayouts = useMemo(() => layoutFlightPlanRouteDistancePills(
    flightPlanRouteDistanceAnnotations,
    routeScreenSegments,
    new Set((mapOverlay.flight_plan_features ?? []).map((feature) => feature.id)),
    measureFlightPlanRouteDistancePillWidth,
    plannedMapUpDeg,
  ), [flightPlanRouteDistanceAnnotations, mapOverlay.flight_plan_features, plannedMapUpDeg, routeScreenSegments]);

  useEffect(() => {
    terrainRendererRef.current = new TerrainOverlayRenderer();
    const cache = terrainTileCacheRef.current;
    return () => {
      terrainRendererRef.current?.destroy();
      terrainRendererRef.current = null;
      cache.clear();
      terrainPendingFrameRef.current = null;
      terrainFrameStartRef.current.clear();
      terrainTileInFlightRef.current.clear();
      terrainScheduleRequestRef.current = null;
      terrainSchedulePendingRef.current = false;
    };
  }, [navDataEpoch]);

  const terrainAltitudeBucket = ownship.terrain_altitude_bucket_ft;

  useEffect(() => {
    if (!mapIsVisible || !uiSession || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      terrainPendingFrameRef.current = null;
      terrainScheduleRequestRef.current = null;
      terrainSchedulePendingRef.current = false;
      setTerrainOverlay({ query: null, images: [] });
      return;
    }
    if (!mapLayerState.terrain_warning.visible) {
      terrainPendingFrameRef.current = null;
      terrainScheduleRequestRef.current = null;
      terrainSchedulePendingRef.current = false;
      setTerrainOverlay({ query: null, images: [] });
      return;
    }
    terrainScheduleRequestRef.current = {
      id: ++terrainScheduleRequestIdRef.current,
      session: uiSession,
      viewport,
      width: planningSurfaceSize.width,
      height: planningSurfaceSize.height,
      navDataEpoch,
      altitudeBucket: terrainAltitudeBucket,
    };
    terrainSchedulePendingRef.current = true;
    pumpTerrainRenderQueue();
  }, [mapIsVisible, mapLayerState.terrain_warning.visible, navDataEpoch, planningSurfaceSize.height, planningSurfaceSize.width, surfaceSize.height, surfaceSize.width, terrainAltitudeBucket, uiInvalidationRevisions.terrain_overlay, uiSession, viewport]);

  useEffect(() => () => clearNexradViewportRefreshTimer(), []);

  useEffect(() => () => nexradFrameCacheRef.current?.clear(), []);

  useEffect(() => {
    if (!mapIsVisible || !uiSession || surfaceSize.width <= 0 || surfaceSize.height <= 0 || !mapLayerState.nexrad.visible) {
      clearNexradViewportRefreshTimer();
      return;
    }
    const lastRequest = nexradQueryRequestRef.current;
    if (!nexradOverlayFrame && !lastRequest) {
      return;
    }
    if (
      lastRequest?.viewport === viewport &&
      lastRequest.width === planningSurfaceSize.width &&
      lastRequest.height === planningSurfaceSize.height
    ) {
      return;
    }
    requestThrottledNexradViewportRefresh();
  }, [mapIsVisible, mapLayerState.nexrad.visible, nexradOverlayFrame, planningSurfaceSize.height, planningSurfaceSize.width, surfaceSize.height, surfaceSize.width, uiSession, viewport]);

  useEffect(() => {
    if (!mapIsVisible || !uiSession || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      nexradQueryRequestRef.current = null;
      nexradQueryPendingRef.current = false;
      nexradFrameCacheRef.current?.cancelPendingLoads();
      nexradLastPaintTimingRef.current = null;
      clearNexradViewportRefreshTimer();
      setNexradOverlay({
        status: { state: "hidden" },
        tiles: [],
        stats: emptyNexradOverlayStats(),
        animation: emptyNexradOverlayAnimation(),
      });
      setNexradOverlayFrame(null);
      return;
    }
    nexradFrameCacheRef.current?.cancelPendingLoads();
    nexradQueryRequestRef.current = {
      id: ++nexradQueryRequestIdRef.current,
      session: uiSession,
      viewport,
      width: planningSurfaceSize.width,
      height: planningSurfaceSize.height,
      debugTileLabels: debugState.nexrad_tile_labels,
    };
    nexradQueryPendingRef.current = true;
    pumpNexradQueryQueue();
  }, [debugState.nexrad_tile_labels, mapIsVisible, mapLayerState.nexrad.visible, nexradAnimationTick, nexradViewportRefreshTick, planningSurfaceSize.height, planningSurfaceSize.width, surfaceSize.height, surfaceSize.width, uiInvalidationRevisions.nexrad_overlay, uiSession]);

  useEffect(() => {
    if (!mapIsVisible || !mapLayerState.nexrad.visible || !uiSession) {
      return;
    }
    const targetEpochMs = finiteOrNull(nexradOverlay.animation.next_update_epoch_ms);
    if (targetEpochMs == null) {
      return;
    }
    const scheduledAt = performance.now();
    const scheduledEpochMs = Date.now();
    const timerId = ++nexradTimerSeqRef.current;
    const delayToTargetMs = targetEpochMs - scheduledEpochMs;
    const clampedDelayMs = Math.max(0, delayToTargetMs);
    const targetPerformanceMs = scheduledAt + delayToTargetMs;
    temporaryNexradTimingLog("timer_scheduled", {
      timer_id: timerId,
      requested_delay_ms: nexradOverlay.animation.next_update_delay_ms,
      target_epoch_ms: targetEpochMs,
      scheduled_epoch_ms: scheduledEpochMs,
      delay_to_target_ms: Math.round(delayToTargetMs),
      clamped_delay_ms: clampedDelayMs,
      target_performance_ms: Math.round(targetPerformanceMs),
      phase: nexradOverlay.animation.phase,
      selected_frame_index: nexradOverlay.animation.selected_frame_index,
      frame_count: nexradOverlay.animation.frame_count,
    });
    const timer = window.setTimeout(() => {
      const firedAt = performance.now();
      const firedEpochMs = Date.now();
      temporaryNexradTimingLog("timer_fired", {
        timer_id: timerId,
        requested_delay_ms: nexradOverlay.animation.next_update_delay_ms,
        target_epoch_ms: targetEpochMs,
        scheduled_epoch_ms: scheduledEpochMs,
        fired_epoch_ms: firedEpochMs,
        delay_to_target_ms: Math.round(delayToTargetMs),
        clamped_delay_ms: clampedDelayMs,
        actual_delay_ms: Math.round(firedAt - scheduledAt),
        timer_late_ms: Math.round(firedAt - scheduledAt - clampedDelayMs),
        target_late_ms: Math.round(firedAt - targetPerformanceMs),
        epoch_target_late_ms: Math.round(firedEpochMs - targetEpochMs),
        phase: nexradOverlay.animation.phase,
        selected_frame_index: nexradOverlay.animation.selected_frame_index,
        frame_count: nexradOverlay.animation.frame_count,
      });
      setNexradAnimationTick((tick) => tick + 1);
    }, clampedDelayMs);
    return () => {
      window.clearTimeout(timer);
    };
  }, [mapIsVisible, mapLayerState.nexrad.visible, nexradOverlay, uiSession]);

  useEffect(() => {
    perfDebugLog("map.nav_element.render", () => ({
      plan_id: planUiState?.plan_id ?? null,
      plan_guidance: planUiState?.guidance?.nav_element ?? null,
      ownship_mode: ownship.mode,
      ownship_draw_cdi: ownship.draw_cdi,
      ownship_position: ownship.position,
    }));
  }, [ownship.draw_cdi, ownship.mode, ownship.position, planUiState]);

  useEffect(() => {
    if (!uiSession) {
      setFlightPlanRouteProjection({
        flight_plan_route_revision: -1,
        segments: [],
        distance_annotations: [],
      });
      return;
    }
    const session = uiSession;
    let cancelled = false;

    async function resolveFlightPlanRoute() {
      const startedAt = performance.now();
      const projection = await session.projectFlightPlanRoute();
      const segments = projection.segments;
      const elapsedMs = Math.round(performance.now() - startedAt);
      perfDebugLog("map.route.segments", () => ({
        count: segments.length,
        elapsed_ms: elapsedMs,
        segments: segments.map((segment) => ({
          id: segment.id,
          from: segment.from,
          to: segment.to,
          status: segment.status,
        })),
      }));
      if (elapsedMs > 250) {
        onHighLatencyWarning("map.route.resolve.slow", {
          count: segments.length,
          elapsed_ms: elapsedMs,
        });
      }
      if (!cancelled) {
        setFlightPlanRouteProjection(projection);
      }
    }

    resolveFlightPlanRoute().catch((error: unknown) => {
      console.error("failed to resolve flight plan route", error);
      if (!cancelled) {
        setFlightPlanRouteProjection({
          flight_plan_route_revision: flightPlanRouteRevision,
          segments: [],
          distance_annotations: [],
        });
      }
    });

    return () => {
      cancelled = true;
    };
  }, [
    onHighLatencyWarning,
    flightPlanRouteRevision,
    uiSession,
  ]);

  useEffect(() => {
    const deadline = mapOverlay.traffic_next_refresh_epoch_ms;
    if (!mapLayerState.traffic.visible || deadline == null) {
      return;
    }
    const timer = window.setTimeout(
      () => setTrafficRefreshTick((tick) => tick + 1),
      Math.max(0, deadline - Date.now()),
    );
    return () => window.clearTimeout(timer);
  }, [mapLayerState.traffic.visible, mapOverlay.traffic_next_refresh_epoch_ms]);

  useEffect(() => {
    if (!mapIsVisible || !mapOverlayLayersVisible) {
      mapOverlayQueryRequestRef.current = null;
      mapOverlayQueryPendingRef.current = false;
      setMapOverlay({
        visible_features: [],
        visible_metars: [],
        visible_pireps: [],
        visible_traffic: [],
        traffic_next_refresh_epoch_ms: null,
        airspace_paths: [],
        tfr_paths: [],
        airspace_labels: [],
        offline_regions: [],
      });
      setMapOverlayFrame(null);
      return;
    }
    if (!uiSession || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      mapOverlayQueryRequestRef.current = null;
      mapOverlayQueryPendingRef.current = false;
      setMapOverlay({
        visible_features: [],
        visible_metars: [],
        visible_pireps: [],
        visible_traffic: [],
        traffic_next_refresh_epoch_ms: null,
        airspace_paths: [],
        tfr_paths: [],
        airspace_labels: [],
        offline_regions: [],
      });
      setMapOverlayFrame(null);
      return;
    }
    const center = viewportCenterLatLon(viewport);
    mapOverlayQueryRequestRef.current = {
      id: ++mapOverlayQueryRequestIdRef.current,
      requestedAt: performance.now(),
      session: uiSession,
      viewport,
      center,
      width: planningSurfaceSize.width,
      height: planningSurfaceSize.height,
      layerKey: [
        mapOverlayLayersVisible,
        mapLayerState.metars.visible,
        mapLayerState.offline_regions.visible,
        mapLayerState.traffic.visible,
        mapLayerState.vectors.visible,
      ].join("|"),
      navDataEpoch,
    };
    mapOverlayQueryPendingRef.current = true;
    pumpMapOverlayQueryQueue();
  }, [
    mapLayerState.metars.visible,
    mapLayerState.offline_regions.visible,
    mapLayerState.traffic.visible,
    mapLayerState.vectors.visible,
    mapOverlayLayersVisible,
    mapIsVisible,
    onDebugWarning,
    planningSurfaceSize.height,
    planningSurfaceSize.width,
    surfaceSize.height,
    surfaceSize.width,
    uiInvalidationRevisions.map_overlay,
    navDataEpoch,
    uiSession,
    trafficRefreshTick,
    viewport,
  ]);

  const overlayTransform = useMemo(() => {
    if (!mapOverlayFrame || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return undefined;
    }
    return displayFrameCssTransform(mapOverlayFrame, {
      viewport,
      width: surfaceSize.width,
      height: surfaceSize.height,
    });
  }, [mapOverlayFrame, surfaceSize.height, surfaceSize.width, viewport]);
  const selectedMapHighlight = useMemo(() => {
    const highlight = mapSelection?.selectedItem?.highlight;
    if (!highlight || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return null;
    }
    if (highlight.kind === "spot") {
      const world = latLonToWorld(highlight.lat, highlight.lon);
      return {
        kind: "spot" as const,
        point: worldToScreen(viewport, world, surfaceSize.width, surfaceSize.height),
      };
    }
    if (highlight.kind === "metar") {
      const metarFeature = mapOverlay.visible_metars.find((feature) => feature.station_id === highlight.station_id);
      if (metarFeature) {
        return { kind: "metar" as const, feature: metarFeature };
      }
      const selectedMetarFeature = mapSelection?.selectedItem?.metar_feature;
      if (selectedMetarFeature?.station_id === highlight.station_id) {
        return { kind: "metar" as const, feature: selectedMetarFeature };
      }
      return null;
    }
    if (highlight.kind === "pirep") {
      const pirepFeature = mapOverlay.visible_pireps.find((feature) => feature.id === highlight.id);
      if (pirepFeature) {
        return { kind: "pirep" as const, feature: pirepFeature };
      }
      const selectedPirepFeature = mapSelection?.selectedItem?.pirep_feature;
      if (selectedPirepFeature?.id === highlight.id) {
        return { kind: "pirep" as const, feature: selectedPirepFeature };
      }
      return null;
    }
    if (highlight.kind === "adsb_traffic") {
      const traffic = mapOverlay.visible_traffic.find((feature) => feature.id === highlight.id);
      return traffic ? { kind: "adsb_traffic" as const, feature: traffic } : null;
    }
    if (highlight.kind === "offline_region") {
      const region = mapOverlay.offline_regions.find((feature) => feature.id === highlight.id);
      if (region) {
        return { kind: "offline_region" as const, feature: region };
      }
      return null;
    }
    const pointFeature = mapOverlay.visible_features.find((feature) => feature.id === highlight.id);
    if (pointFeature) {
      return { kind: "point" as const, feature: pointFeature };
    }
    const airspacePath = mapOverlay.airspace_paths.find((feature) => feature.id === highlight.id);
    if (airspacePath) {
      return { kind: "path" as const, feature: airspacePath };
    }
    const tfrPath = mapOverlay.tfr_paths.find((feature) => feature.id === highlight.id);
    if (tfrPath) {
      return { kind: "path" as const, feature: tfrPath };
    }
    return null;
  }, [mapOverlay.airspace_paths, mapOverlay.offline_regions, mapOverlay.tfr_paths, mapOverlay.visible_features, mapOverlay.visible_metars, mapOverlay.visible_pireps, mapOverlay.visible_traffic, mapSelection?.selectedItem, surfaceSize.height, surfaceSize.width, viewport]);
  useEffect(() => {
    if (!hoverWeather) {
      return;
    }
    if (mapSelection || trayGroup.scrimOpen || !mapIsVisible || !mapLayerState.metars.visible) {
      hoverWeatherRequestSerialRef.current += 1;
      setHoverWeather(null);
    }
  }, [hoverWeather, mapIsVisible, mapLayerState.metars.visible, mapSelection, trayGroup.scrimOpen]);
  useEffect(() => {
    setHoverWeather((current) => {
      if (
        !current ||
        mapOverlay.visible_metars.some((feature) => normalizedStationId(feature.station_id) === current.stationId)
      ) {
        return current;
      }
      hoverWeatherRequestSerialRef.current += 1;
      return null;
    });
  }, [mapOverlay.visible_metars]);
  const rasterTileTransform = useMemo(() => {
    if (!rasterTileFrame || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return undefined;
    }
    return displayFrameCssTransform(rasterTileFrame, {
      viewport,
      width: surfaceSize.width,
      height: surfaceSize.height,
    });
  }, [rasterTileFrame, surfaceSize.height, surfaceSize.width, viewport]);
  const nexradOverlayTransform = useMemo(() => {
    if (!nexradOverlayFrame || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return undefined;
    }
    return displayFrameCssTransform(nexradOverlayFrame, {
      viewport,
      width: surfaceSize.width,
      height: surfaceSize.height,
    });
  }, [nexradOverlayFrame, surfaceSize.height, surfaceSize.width, viewport]);

  function transientViewportTransform(renderedViewport: MapViewportState, nextViewport: MapViewportState) {
    if (sameMapViewport(renderedViewport, nextViewport)) {
      return "";
    }
    const currentScale = scaleForZoom(nextViewport.zoom);
    const renderedScale = scaleForZoom(renderedViewport.zoom);
    const scaleRatio = currentScale / renderedScale;
    const dx = (renderedViewport.centerWorldX - nextViewport.centerWorldX) * currentScale;
    const dy = (renderedViewport.centerWorldY - nextViewport.centerWorldY) * currentScale;
    return `translate(${dx}px, ${dy}px) scale(${scaleRatio})`;
  }

  function applyImperativeMapContentTransform() {
    const element = mapContentTransformRef.current;
    if (!element) {
      return;
    }
    const transform = transientViewportTransform(committedViewportRef.current, viewportRef.current);
    element.style.transform = transform;
    element.style.transformOrigin = "center center";
  }

  function clearPendingReactViewportCommit() {
    if (pendingReactViewportTimerRef.current !== null) {
      window.clearTimeout(pendingReactViewportTimerRef.current);
      pendingReactViewportTimerRef.current = null;
    }
  }

  function flushPendingReactViewportCommit() {
    clearPendingReactViewportCommit();
    const next = pendingReactViewportRef.current;
    pendingReactViewportRef.current = null;
    if (next && !sameMapViewport(next, committedViewportRef.current)) {
      perfDebugLog("map.viewport.react_commit.flush", () => ({
        zoom: next.zoom,
        center_world_x: next.centerWorldX,
        center_world_y: next.centerWorldY,
      }));
      onViewportChange(next);
    }
  }

  function scheduleReactViewportCommit(next: MapViewportState) {
    pendingReactViewportRef.current = next;
    if (pendingReactViewportTimerRef.current !== null) {
      return;
    }
    pendingReactViewportTimerRef.current = window.setTimeout(() => {
      pendingReactViewportTimerRef.current = null;
      const pending = pendingReactViewportRef.current;
      pendingReactViewportRef.current = null;
      if (pending && !sameMapViewport(pending, committedViewportRef.current)) {
        perfDebugLog("map.viewport.react_commit.throttled", () => ({
          zoom: pending.zoom,
          center_world_x: pending.centerWorldX,
          center_world_y: pending.centerWorldY,
        }));
        onViewportChange(pending);
      }
    }, dragViewportReactCommitThrottleMs);
  }

  function updateViewport(next: MapViewportState, options: { deferReactCommit?: boolean } = {}) {
    mapSelectionRequestGenerationRef.current += 1;
    viewportRef.current = next;
    applyImperativeMapContentTransform();
    if (options.deferReactCommit) {
      scheduleReactViewportCommit(next);
      return;
    }
    pendingReactViewportRef.current = null;
    clearPendingReactViewportCommit();
    onViewportChange(next);
  }

  function noteViewportGesture(durationMs = 300) {
    viewportGestureUntilRef.current = Math.max(viewportGestureUntilRef.current, Date.now() + durationMs);
    if (!gestureActiveRef.current) {
      onViewportGestureActivity();
    }
  }

  function setViewportGestureActive(active: boolean) {
    if (!active) {
      flushPendingReactViewportCommit();
    }
    if (gestureActiveRef.current === active) {
      return;
    }
    gestureActiveRef.current = active;
    onViewportGestureActiveChange(active);
  }

  function syncFollowStateForViewport(nextViewport: MapViewportState) {
    if (!uiSession || !mapFollowUiState.following || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return;
    }
    if (gestureActiveRef.current) {
      deferredFollowSyncViewportRef.current = nextViewport;
      perfDebugLog("map.follow.sync.deferred_for_gesture", () => ({
        zoom: nextViewport.zoom,
        center_world_x: nextViewport.centerWorldX,
        center_world_y: nextViewport.centerWorldY,
      }));
      return;
    }
    deferredFollowSyncViewportRef.current = null;
    const serial = followSyncSerialRef.current + 1;
    followSyncSerialRef.current = serial;
    followTargetGateRef.current.beginSync(nextViewport);
    setFollowSyncPendingSerial(serial);
    perfDebugLog("map.follow.sync.request", () => ({
      serial,
      zoom: nextViewport.zoom,
      center_world_x: nextViewport.centerWorldX,
      center_world_y: nextViewport.centerWorldY,
      gesture_active: gestureActiveRef.current,
    }));
    void uiSession
      .syncMapFollow(nextViewport, surfaceSize.width, surfaceSize.height)
      .then((nextSnapshot) => {
        if (followSyncSerialRef.current !== serial) {
          perfDebugLog("map.follow.sync.stale_response", () => ({ serial, latest_serial: followSyncSerialRef.current }));
          return;
        }
        const syncedTarget = nextSnapshot.map_follow_target_viewport
          ? mapViewportFromCore(nextSnapshot.map_follow_target_viewport)
          : null;
        followTargetGateRef.current.acknowledgeSyncSnapshot({
          following: nextSnapshot.map_follow_ui_state.following,
          targetViewport: syncedTarget,
        });
        props.onPlaybackSnapshotChange(nextSnapshot);
      })
      .catch(() => {})
      .finally(() => {
        if (followSyncSerialRef.current === serial) {
          setFollowSyncPendingSerial(0);
        }
      });
  }

  function flushDeferredFollowSync() {
    const nextViewport = deferredFollowSyncViewportRef.current;
    if (!nextViewport) {
      return;
    }
    deferredFollowSyncViewportRef.current = null;
    syncFollowStateForViewport(nextViewport);
  }

  const runLiveDragPerf = useCallback(async () => {
    if (!uiSession || liveDragPerfRunning || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return;
    }
    const runId = createLiveDragPerfRunId();
    const globalWithRunId = globalThis as typeof globalThis & { __aerobagPerfRunId?: unknown };
    const priorRunId = globalWithRunId.__aerobagPerfRunId;
    const hadPriorRunId = Object.prototype.hasOwnProperty.call(globalWithRunId, "__aerobagPerfRunId");
    const dragCount = 20;
    const stepsPerDrag = 8;
    const stepDelayMs = 12;
    const dragIntervalMs = 500;
    const settleMs = 4000;
    globalWithRunId.__aerobagPerfRunId = runId;
    activePointersRef.current.clear();
    dragRef.current = null;
    pinchRef.current = null;
    clickCandidateRef.current = null;
    setMapSelection(null);
    setLiveDragPerfRunning(true);
    let followDisabledForRun = false;
    if (mapFollowUiState.following) {
      try {
        const nextSnapshot = await uiSession.disengageMapFollow(viewportRef.current);
        onPlaybackSnapshotChange(nextSnapshot);
        followDisabledForRun = true;
      } catch (error) {
        debugLog("automated-test-disable-follow.failed", {
          run_id: runId,
          error: errorMessage(error),
        });
      }
    }
    debugLog("automated-test-begin", {
      run_id: runId,
      mode: "live_browser",
      drag_count: dragCount,
      steps_per_drag: stepsPerDrag,
      step_delay_ms: stepDelayMs,
      interval_ms: dragIntervalMs,
      following: mapFollowUiState.following && !followDisabledForRun,
      following_before_disable: mapFollowUiState.following,
      follow_disabled_for_run: followDisabledForRun,
      start_zoom: viewportRef.current.zoom,
      width: surfaceSize.width,
      height: surfaceSize.height,
    });
    try {
      for (let dragIndex = 0; dragIndex < dragCount; dragIndex += 1) {
        const dx = surfaceSize.width * 0.66 / stepsPerDrag;
        const dy = surfaceSize.height * 0.66 / stepsPerDrag;
        setViewportGestureActive(true);
        for (let stepIndex = 0; stepIndex < stepsPerDrag; stepIndex += 1) {
          const nextViewport = dragViewport(viewportRef.current, dx, dy, mapUpDegRef.current);
          noteViewportGesture();
          perfDebugLog("map.drag.viewport", () => ({
            automated: true,
            run_id: runId,
            drag_index: dragIndex,
            step_index: stepIndex,
            dx,
            dy,
            zoom: nextViewport.zoom,
            center_world_x: nextViewport.centerWorldX,
            center_world_y: nextViewport.centerWorldY,
            following: mapFollowUiState.following,
          }));
          updateViewport(nextViewport, { deferReactCommit: true });
          if (!followDisabledForRun) {
            syncFollowStateForViewport(nextViewport);
          }
          await sleepMs(stepDelayMs);
        }
        setViewportGestureActive(false);
        if (!followDisabledForRun) {
          flushDeferredFollowSync();
        }
        pumpRasterTilePlanQueue();
        pumpMapOverlayQueryQueue();
        debugLog("automated-test-drag", {
          run_id: runId,
          index: dragIndex,
          zoom: viewportRef.current.zoom,
          center_world_x: viewportRef.current.centerWorldX,
          center_world_y: viewportRef.current.centerWorldY,
        });
        await sleepMs(dragIntervalMs);
      }
      await sleepMs(settleMs);
      debugLog("automated-test-end", {
        run_id: runId,
        settle_ms: settleMs,
      });
    } finally {
      setViewportGestureActive(false);
      if (!followDisabledForRun) {
        flushDeferredFollowSync();
      }
      pumpRasterTilePlanQueue();
      pumpMapOverlayQueryQueue();
      setLiveDragPerfRunning(false);
      if (hadPriorRunId) {
        globalWithRunId.__aerobagPerfRunId = priorRunId;
      } else {
        delete globalWithRunId.__aerobagPerfRunId;
      }
    }
  }, [liveDragPerfRunning, mapFollowUiState.following, onPlaybackSnapshotChange, surfaceSize.height, surfaceSize.width, uiSession]);

  useEffect(() => {
    const globalWithAutomation = globalThis as typeof globalThis & { __aerobagRunLiveDragPerf?: () => Promise<void> };
    globalWithAutomation.__aerobagRunLiveDragPerf = runLiveDragPerf;
    return () => {
      if (globalWithAutomation.__aerobagRunLiveDragPerf === runLiveDragPerf) {
        delete globalWithAutomation.__aerobagRunLiveDragPerf;
      }
    };
  }, [runLiveDragPerf]);

  useEffect(() => {
    if (!uiSession || !mapFollowUiState.following || mapFollowTargetViewport) {
      return;
    }
    let cancelled = false;
    void uiSession.engageMapFollow(viewport).then((nextSnapshot) => {
      if (!cancelled) {
        props.onPlaybackSnapshotChange(nextSnapshot);
      }
    }).catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [mapFollowTargetViewport, mapFollowUiState.following, props.onPlaybackSnapshotChange, uiSession, viewport]);

  useEffect(() => {
    if (!mapFollowUiState.following || !mapFollowTargetViewport) {
      followTargetGateRef.current.clear();
      return;
    }
    const remainingGestureMs = viewportGestureUntilRef.current - Date.now();
    if (gestureActiveRef.current || followSyncPendingSerial !== 0 || remainingGestureMs > 0) {
      perfDebugLog("map.follow.target.skip_during_gesture", () => ({
        pending_sync_serial: followSyncPendingSerial,
        remaining_gesture_ms: Math.max(0, remainingGestureMs),
        zoom: mapFollowTargetViewport.zoom,
        center_lat: mapFollowTargetViewport.center.lat,
        center_lon: mapFollowTargetViewport.center.lon,
      }));
      if (!gestureActiveRef.current && followSyncPendingSerial === 0 && remainingGestureMs > 0) {
        const timeout = window.setTimeout(() => {
          setFollowTargetRetryToken((token) => token + 1);
        }, remainingGestureMs + 16);
        return () => window.clearTimeout(timeout);
      }
      return;
    }
    const nextViewport = mapViewportFromCore(mapFollowTargetViewport);
    const awaitedViewport = followTargetGateRef.current.awaitedViewport();
    if (!followTargetGateRef.current.shouldApplyTarget(nextViewport)) {
      perfDebugLog("map.follow.target.skip_stale_sync_target", () => ({
        target_zoom: nextViewport.zoom,
        target_center_world_x: nextViewport.centerWorldX,
        target_center_world_y: nextViewport.centerWorldY,
        awaited_zoom: awaitedViewport?.zoom,
        awaited_center_world_x: awaitedViewport?.centerWorldX,
        awaited_center_world_y: awaitedViewport?.centerWorldY,
      }));
      return;
    }
    if (!sameMapViewport(nextViewport, viewport)) {
      perfDebugLog("map.follow.target.apply", () => ({
        zoom: nextViewport.zoom,
        center_world_x: nextViewport.centerWorldX,
        center_world_y: nextViewport.centerWorldY,
      }));
      updateViewport(nextViewport, { deferReactCommit: true });
    }
  }, [followSyncPendingSerial, followTargetRetryToken, mapFollowTargetViewport, mapFollowUiState.following, viewport]);

  function handlePointerDown(event: React.PointerEvent<HTMLDivElement>) {
    if (trayGroup.scrimOpen || mapSelection) {
      return;
    }
    if (event.pointerType === "mouse") {
      activePointersRef.current.clear();
      dragRef.current = null;
      pinchRef.current = null;
    }
    const rect = event.currentTarget.getBoundingClientRect();
    const point = { x: event.clientX - rect.left, y: event.clientY - rect.top };
    activePointersRef.current.set(event.pointerId, point);
    setViewportGestureActive(activePointersRef.current.size > 0);
    event.currentTarget.setPointerCapture(event.pointerId);
    if (activePointersRef.current.size === 1) {
      dragRef.current = { id: event.pointerId, last: point };
      clickCandidateRef.current = { pointerId: event.pointerId, start: point, latest: point };
      pinchRef.current = null;
    } else if (activePointersRef.current.size >= 2 && surfaceSize.width > 0 && surfaceSize.height > 0) {
      clickCandidateRef.current = null;
      const [first, second] = Array.from(activePointersRef.current.values());
      pinchRef.current = createPinchSnapshot(
        viewportRef.current,
        first,
        second,
        surfaceSize.width,
        surfaceSize.height,
        mapUpDegRef.current,
      );
      dragRef.current = null;
    }
  }

  function handlePointerMove(event: React.PointerEvent<HTMLDivElement>) {
    if (trayGroup.scrimOpen || mapSelection || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return;
    }
    const rect = event.currentTarget.getBoundingClientRect();
    const point = { x: event.clientX - rect.left, y: event.clientY - rect.top };
    if (!activePointersRef.current.has(event.pointerId)) {
      return;
    }
    activePointersRef.current.set(event.pointerId, point);
    if (clickCandidateRef.current?.pointerId === event.pointerId) {
      clickCandidateRef.current = { ...clickCandidateRef.current, latest: point };
    }
    const pointers = Array.from(activePointersRef.current.entries());
    if (pointers.length === 1 && dragRef.current?.id === event.pointerId) {
      const dx = point.x - dragRef.current.last.x;
      const dy = point.y - dragRef.current.last.y;
      const nextViewport = dragViewport(viewportRef.current, dx, dy, mapUpDegRef.current);
      noteViewportGesture();
      perfDebugLog("map.drag.viewport", () => ({
        dx,
        dy,
        zoom: nextViewport.zoom,
        center_world_x: nextViewport.centerWorldX,
        center_world_y: nextViewport.centerWorldY,
        following: mapFollowUiState.following,
      }));
      updateViewport(nextViewport, { deferReactCommit: true });
      syncFollowStateForViewport(nextViewport);
      dragRef.current = { id: event.pointerId, last: point };
      if (clickCandidateRef.current && distanceBetween(clickCandidateRef.current.start, point) > 8) {
        clickCandidateRef.current = null;
      }
      return;
    }
    if (pointers.length >= 2) {
      const [first, second] = pointers;
      if (!pinchRef.current) {
        pinchRef.current = createPinchSnapshot(
          viewportRef.current,
          first[1],
          second[1],
          surfaceSize.width,
          surfaceSize.height,
          mapUpDegRef.current,
        );
      }
      const nextViewport = applyPinchGesture(
        pinchRef.current,
        first[1],
        second[1],
        selectedMap,
        surfaceSize.width,
        surfaceSize.height,
      );
      noteViewportGesture();
      perfDebugLog("map.pinch.viewport", () => ({
        zoom: nextViewport.zoom,
        center_world_x: nextViewport.centerWorldX,
        center_world_y: nextViewport.centerWorldY,
        following: mapFollowUiState.following,
      }));
      updateViewport(nextViewport);
      syncFollowStateForViewport(nextViewport);
    }
  }

  function handlePointerRelease(event: React.PointerEvent<HTMLDivElement>) {
    if (trayGroup.scrimOpen || mapSelection) {
      activePointersRef.current.delete(event.pointerId);
      clickCandidateRef.current = null;
      pinchRef.current = null;
      dragRef.current = null;
      setViewportGestureActive(false);
      return;
    }
    const clickCandidate = clickCandidateRef.current;
    activePointersRef.current.delete(event.pointerId);
    setViewportGestureActive(activePointersRef.current.size > 0);
    if (activePointersRef.current.size === 0) {
      flushDeferredFollowSync();
      pumpRasterTilePlanQueue();
      pumpMapOverlayQueryQueue();
    }
    pinchRef.current = null;
    const remaining = Array.from(activePointersRef.current.entries());
    if (remaining.length === 1) {
      dragRef.current = { id: remaining[0][0], last: remaining[0][1] };
    } else {
      dragRef.current = null;
    }
    if (
      clickCandidate &&
      clickCandidate.pointerId === event.pointerId &&
      activePointersRef.current.size === 0 &&
      surfaceSize.width > 0 &&
      surfaceSize.height > 0 &&
      uiSession
    ) {
      clickCandidateRef.current = null;
      const world = screenToWorld(
        viewportRef.current,
        clickCandidate.latest,
        surfaceSize.width,
        surfaceSize.height,
        mapUpDegRef.current,
      );
      const click = worldToLatLon(world.x, world.y);
      const selectionGeneration = ++mapSelectionRequestGenerationRef.current;
      void uiSession
        .queryMapSelection(viewportRef.current, surfaceSize.width, surfaceSize.height, click)
        .then((result) => {
          if (selectionGeneration !== mapSelectionRequestGenerationRef.current) {
            return;
          }
          setMapSelection({
            point: clickCandidate.latest,
            result,
            selectedItem: mapSelectionItemById(result, result.initial_selected_item_id ?? null),
            detailModal: null,
          });
        })
        .catch((error) => {
          debugLog("map.selection.failed", { error: errorMessage(error) });
        });
    } else if (activePointersRef.current.size === 0) {
      clickCandidateRef.current = null;
    }
  }

  function handleLostPointerCapture(event: React.PointerEvent<HTMLDivElement>) {
    activePointersRef.current.delete(event.pointerId);
    if (clickCandidateRef.current?.pointerId === event.pointerId) {
      clickCandidateRef.current = null;
    }
    setViewportGestureActive(activePointersRef.current.size > 0);
    if (activePointersRef.current.size === 0) {
      flushDeferredFollowSync();
      pumpRasterTilePlanQueue();
      pumpMapOverlayQueryQueue();
    }
    pinchRef.current = null;
    const remaining = Array.from(activePointersRef.current.entries());
    dragRef.current = remaining.length === 1 ? { id: remaining[0][0], last: remaining[0][1] } : null;
  }

  function handleWheel(event: React.WheelEvent<HTMLDivElement>) {
    if (trayGroup.scrimOpen || mapSelection || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      event.preventDefault();
      return;
    }
    event.preventDefault();
    const nextViewport = zoomAroundPoint(
      viewportRef.current,
      selectedMap,
      { x: event.nativeEvent.offsetX, y: event.nativeEvent.offsetY },
      surfaceSize.width,
      surfaceSize.height,
      viewportRef.current.zoom - event.deltaY / 360,
      mapUpDegRef.current,
    );
    noteViewportGesture();
    updateViewport(nextViewport);
    syncFollowStateForViewport(nextViewport);
  }

  function handleDoubleClick(event: React.MouseEvent<HTMLDivElement>) {
    if (trayGroup.scrimOpen || mapSelection || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return;
    }
    const nextViewport = zoomAroundPoint(
      viewportRef.current,
      selectedMap,
      { x: event.nativeEvent.offsetX, y: event.nativeEvent.offsetY },
      surfaceSize.width,
      surfaceSize.height,
      viewportRef.current.zoom + 0.75,
      mapUpDegRef.current,
    );
    noteViewportGesture();
    updateViewport(nextViewport);
    syncFollowStateForViewport(nextViewport);
  }

  function setViewportZoom(nextZoom: number) {
    if (surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return;
    }
    const nextViewport = zoomAroundPoint(
      viewportRef.current,
      selectedMap,
      { x: surfaceSize.width / 2, y: surfaceSize.height / 2 },
      surfaceSize.width,
      surfaceSize.height,
      nextZoom,
      mapUpDegRef.current,
    );
    noteViewportGesture();
    updateViewport(nextViewport);
    syncFollowStateForViewport(nextViewport);
  }

  async function recenterOnNavRef(navRef: NavRef) {
    const position = await props.appCoreAdapter.resolveNavRefPosition(navRef);
    const centerWorld = latLonToWorld(position.lat, position.lon);
    const nextViewport = {
      ...viewportRef.current,
      centerWorldX: centerWorld.x,
      centerWorldY: centerWorld.y,
    };
    updateViewport(nextViewport);
    syncFollowStateForViewport(nextViewport);
    return { position, viewport: nextViewport };
  }

  async function inspectNavRef(navRef: NavRef) {
    if (!uiSession || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      await recenterOnNavRef(navRef);
      return;
    }
    const inspection = await uiSession.queryMapSelectionForNavRef(
      viewportRef.current,
      surfaceSize.width,
      surfaceSize.height,
      navRef,
    );
    const position = inspection.position;
    const centerWorld = latLonToWorld(position.lat, position.lon);
    const nextViewport = {
      ...viewportRef.current,
      centerWorldX: centerWorld.x,
      centerWorldY: centerWorld.y,
      zoom: inspection.target_zoom,
    };
    debugLog("chart.search.inspect_nav_ref", {
      nav_ref: navRef,
      position,
      target_zoom: inspection.target_zoom,
      selected_item_id: inspection.selected_item_id ?? null,
    });
    updateViewport(nextViewport);
    syncFollowStateForViewport(nextViewport);
    const point = worldToScreen(
      nextViewport,
      latLonToWorld(position.lat, position.lon),
      surfaceSize.width,
      surfaceSize.height,
      mapUpDegRef.current,
    );
    const selectedItem = mapSelectionItemById(inspection.selection, inspection.selected_item_id ?? null);
    setMapSelection({
      point,
      result: inspection.selection,
      selectedItem,
      detailModal: null,
    });
  }

  function mapSelectionItemById(result: MapSelectionQueryResult, itemId: string | null) {
    if (!itemId) {
      return null;
    }
    for (const category of result.categories) {
      const item = category.items.find((candidate) => candidate.id === itemId);
      if (item) {
        return item;
      }
    }
    return null;
  }

  async function toggleOpenMapSelectionTimeDisplay(actionId: string) {
    if (!uiSession || !mapSelection) return;
    const previous = mapSelection;
    const selectedItemId = previous.selectedItem?.id ?? null;
    const sourceActionId = previous.detailModal?.kind === "text"
      ? previous.detailModal.sourceActionId
      : null;
    props.onSessionSnapshot(
      await uiSession.performTimeDisplayAction(actionId),
      "time_display_mode",
    );
    const result = await uiSession.queryMapSelection(
      viewportRef.current,
      surfaceSize.width,
      surfaceSize.height,
      { lat: previous.result.click_lat, lon: previous.result.click_lon },
    );
    const selectedItem = mapSelectionItemById(result, selectedItemId);
    const detailAction = selectedItem?.actions.find((action) => action.id === sourceActionId);
    const detailDecision = detailAction?.action_uid
      ? await uiSession.mapSelectionActionDecision(detailAction.action_uid)
      : null;
    const detailEffect = detailDecision?.effect?.kind === "show_detail"
      ? detailDecision.effect
      : null;
    setMapSelection((current) => {
      if (!current || current.result.click_lat !== previous.result.click_lat || current.result.click_lon !== previous.result.click_lon) {
        return current;
      }
      return {
        ...current,
        result,
        selectedItem,
        detailModal: detailEffect ? {
          kind: "text",
          title: detailEffect.title,
          sourceActionId,
          text: detailEffect.text,
          status: detailEffect.status,
        } : current.detailModal,
      };
    });
  }

  const handleMetarHoverEnter = useCallback((event: React.PointerEvent<SVGGElement>, feature: VisibleMetarFeature) => {
    if (
      event.pointerType !== "mouse" ||
      !uiSession ||
      mapSelection ||
      trayGroup.scrimOpen ||
      surfaceSize.width <= 0 ||
      surfaceSize.height <= 0 ||
      activePointersRef.current.size > 0
    ) {
      return;
    }
    const container = containerRef.current;
    if (!container) {
      return;
    }
    const stationId = normalizedStationId(feature.station_id);
    const containerRect = container.getBoundingClientRect();
    const localPoint = {
      x: event.clientX - containerRect.left,
      y: event.clientY - containerRect.top,
    };
    const panelPoint = { x: event.clientX, y: event.clientY };
    const world = screenToWorld(
      viewportRef.current,
      localPoint,
      surfaceSize.width,
      surfaceSize.height,
      mapUpDegRef.current,
    );
    const click = worldToLatLon(world.x, world.y);
    const requestSerial = hoverWeatherRequestSerialRef.current + 1;
    hoverWeatherRequestSerialRef.current = requestSerial;
    setHoverWeather((current) => current?.stationId === stationId ? current : null);
    void uiSession
      .queryMapSelection(viewportRef.current, surfaceSize.width, surfaceSize.height, click)
      .then((result) => {
        if (hoverWeatherRequestSerialRef.current !== requestSerial) {
          return;
        }
        const detail = weatherDetailForMetarSelection(result, stationId);
        if (!detail) {
          setHoverWeather((current) => current?.stationId === stationId ? null : current);
          return;
        }
        setHoverWeather({ stationId, point: panelPoint, detail });
      })
      .catch((error) => {
        if (hoverWeatherRequestSerialRef.current === requestSerial) {
          debugLog("map.metar_hover_weather.failed", {
            station_id: feature.station_id,
            error: errorMessage(error),
          });
        }
      });
  }, [mapSelection, surfaceSize.height, surfaceSize.width, trayGroup.scrimOpen, uiSession]);

  const handleMetarHoverLeave = useCallback((feature: VisibleMetarFeature) => {
    const stationId = normalizedStationId(feature.station_id);
    hoverWeatherRequestSerialRef.current += 1;
    setHoverWeather((current) => current?.stationId === stationId ? null : current);
  }, []);

  function submitChartSearch() {
    const query = chartSearch.query;
    if (!query.trim()) {
      return;
    }
    setChartSearch((current) => ({ ...current, loading: true, error: null }));
    void (async () => {
      const navRef = await props.appCoreAdapter.resolveWaypointIdentifier(query);
      if (!navRef) {
        setChartSearch((current) => ({
          ...current,
          loading: false,
          error: `No waypoint match for ${query}`,
          suggestions: [],
        }));
        return;
      }
      await inspectNavRef(navRef);
      setChartSearch({ query: "", open: false, loading: false, error: null, suggestions: [] });
    })().catch((error) => {
      setChartSearch((current) => ({
        ...current,
        loading: false,
        error: `Search failed: ${errorMessage(error)}`,
      }));
    });
  }

  function reportFirstVisualReady() {
    if (firstVisualReadyRef.current) {
      return;
    }
    firstVisualReadyRef.current = true;
    onFirstVisualReady();
  }

  function reportRasterTileLoaded(tile: RasterRenderTile, loadAttempt: number) {
    reportFirstVisualReady();
    const key = rasterTileKey(tile);
    if ((rasterTileLoadAttemptsRef.current.get(key) ?? 0) !== loadAttempt) {
      return;
    }
    loadedRasterTileKeysRef.current.add(key);
    knownLoadedRasterTileKeysRef.current.add(key);
    if (page !== "map" || tiles.length === 0) {
      return;
    }
    if (!reportRasterTilesReadyIfComplete(tiles)) {
      return;
    }
    const timing = pageTilePaintTiming;
    if (!timing) {
      return;
    }
    perfDebugLog("web.page-to-map.images", () => ({
      id: timing.id,
      from_page: timing.fromPage,
      elapsed_ms: Math.round(performance.now() - timing.startedAt),
      tiles: tiles.length,
    }));
    completePageTilePaintTiming(timing, "images");
  }

  function reportRasterTileError(tile: RasterRenderTile, loadAttempt: number) {
    const key = rasterTileKey(tile);
    if ((rasterTileLoadAttemptsRef.current.get(key) ?? 0) !== loadAttempt) {
      return;
    }
    if (e2eRasterTileFaultKeysRef.current.has(key)) {
      const remainingFaults = new Set(e2eRasterTileFaultKeysRef.current);
      remainingFaults.delete(key);
      e2eRasterTileFaultKeysRef.current = remainingFaults;
      setE2eRasterTileFaultKeys(remainingFaults);
    }
    const decision = recoverRasterTileLoads([tile], "error");
    debugLog("map.raster.tile.error", {
      selected_map_id: selectedMap.selected_map_id,
      selected_family_id: selectedFamily?.id ?? null,
      viewport_zoom: viewportRef.current.zoom,
      zoom: tile.zoom,
      x: tile.x,
      y_tms: tile.yTms,
      family: tile.chartFamily,
      map_view_id: tile.mapViewId,
      package_name: tile.packageName,
      src: tile.src,
      load_attempt: loadAttempt,
      retrying: decision.retry.includes(key),
    });
  }

  const visibleTerrainImages = useMemo(() => {
    if (!terrainOverlay.query) {
      return [];
    }
    const requestedKeys = new Set(terrainOverlay.query.tile_requests.map((request) => request.key));
    return terrainOverlay.images
      .filter((image) => requestedKeys.has(image.key))
      .map((image) => terrainImageForViewport(image, viewport, surfaceSize.width, surfaceSize.height));
  }, [terrainOverlay, viewport, surfaceSize.height, surfaceSize.width]);
  const layerTrayOptions: TrayOption[] = mapLayerState.options.map((option) => {
    const toggleState = mapLayerToggleState(mapLayerState, option.layer_id);
    return {
      id: option.layer_id,
      label: option.label,
      iconSrc: layerIconSrc(option.layer_id),
      toggleState,
      disabled: !toggleState.enabled,
      disabledReason: toggleState.disabled_reason ?? null,
      onSelect: () => void setMapLayerVisible(option.layer_id, !toggleState.visible),
    };
  });
  const ownshipSourceOptions: TrayOption[] = ownshipControls.sources.map((source) => ({
    id: sourceIdString(source.source_id),
    label: source.label,
    active: source.active,
    disabled: !source.enabled || !uiSession,
    disabledReason: !uiSession ? "Ownship controls are not ready yet." : source.disabled_reason ?? null,
    onSelect: () => {
      if (!uiSession) {
        return;
      }
      void uiSession
        .selectOwnshipSource({ kind: "source", source_id: sourceIdString(source.source_id) })
        .then(onPlaybackSnapshotChange)
        .finally(() => {
          if (!source.keep_tray_open_on_select) trayGroup.close("ownship");
        });
    },
  }));
  const centerHereDisabled = !mapFollowUiState.can_center_here && !mapFollowUiState.following;
  const centerHereDisabledReason = disabledReasonText(mapFollowUiState.disabled_reason);

  async function performSelectedMapAction(action: MapSelectionItem["actions"][number]) {
    if (!uiSession || !action.action_uid) {
      return;
    }
    try {
      const decision = await uiSession.mapSelectionActionDecision(action.action_uid);
      if (decision.perform_session_mutation) {
        const nextSnapshot = await uiSession.performMapSelectionUiAction(action.action_uid);
        props.onSessionSnapshot(nextSnapshot, "map_selection_action");
      }
      const effect = decision.effect;
      switch (effect?.kind) {
        case "show_weather":
          setMapSelection((current) => current ? {
            ...current,
            detailModal: { kind: "weather", detail: effect.detail },
          } : current);
          break;
        case "load_airport_info": {
          const { airport_id: airportId, loading_text: loadingText, failure_prefix: failurePrefix } = effect;
          const requestSerial = ++airportInfoRequestSerialRef.current;
          setMapSelection((current) => current ? {
            ...current,
            detailModal: { kind: "text", title: airportId, text: loadingText },
          } : current);
          try {
            const detail = await uiSession.airportInfo(airportId);
            setMapSelection((current) =>
              airportInfoRequestSerialRef.current === requestSerial
                && current?.detailModal?.kind === "text"
                && current.detailModal.title === airportId
                ? { ...current, detailModal: { kind: "airport", detail } }
                : current);
          } catch (error) {
            setMapSelection((current) =>
              airportInfoRequestSerialRef.current === requestSerial
                && current?.detailModal?.kind === "text"
                && current.detailModal.title === airportId
                ? {
                    ...current,
                    detailModal: {
                      kind: "text",
                      title: airportId,
                      text: `${failurePrefix} ${errorMessage(error)}`,
                    },
                  }
                : current);
          }
          break;
        }
        case "show_detail":
          setMapSelection((current) => current ? {
            ...current,
            detailModal: {
              kind: "text",
              title: effect.title,
              sourceActionId: action.id,
              text: effect.text,
              status: effect.status,
            },
          } : current);
          break;
        case "open_plate_target":
          onOpenPlateTarget(effect.airport_id, effect.target);
          break;
        case "open_external_url":
          window.open(effect.url, "_blank", "noopener,noreferrer");
          break;
        case undefined:
          break;
      }
      if (decision.dismiss_selection) {
        setMapSelection(null);
      }
    } catch (error) {
      debugLog("map.selection.action.failed", {
        action_uid: action.action_uid,
        action_id: action.id,
        error: errorMessage(error),
      });
      setMapSelection((current) => current ? {
        ...current,
        detailModal: {
          kind: "text",
          title: action.label,
          text: errorMessage(error),
        },
      } : current);
    }
  }

  useMapCommitProbe({
    high_rate_snapshot: highRateSnapshot,
    viewport,
    ui_invalidations: uiInvalidationRevisions,
    map_overlay: mapOverlay,
    map_overlay_frame: mapOverlayFrame,
    terrain_overlay: terrainOverlay,
    nexrad_overlay: nexradOverlay,
    nexrad_overlay_frame: nexradOverlayFrame,
    raster_tile_frame: rasterTileFrame,
    flight_plan_route: flightPlanRouteProjection,
    follow_sync: followSyncPendingSerial,
    follow_retry: followTargetRetryToken,
    surface_size: surfaceSize,
    map_selection: mapSelection,
  });

  useEffect(() => {
    if (!__AEROBAG_E2E_ENABLED__) return;
    const raster = () => ({
      selected_map_id: selectedMap.selected_map_id,
      request: rasterTilePlanRequestRef.current ? {
        id: rasterTilePlanRequestRef.current.id,
        key: rasterTilePlanRequestRef.current.key,
        requested_at_ms: rasterTilePlanRequestRef.current.requestedAt,
      } : null,
      request_pending: rasterTilePlanPendingRef.current,
      pump_active: rasterTilePlanPumpActiveRef.current,
      landed_request_id: landedRasterTilePlanRequestIdRef.current,
      landing: rasterTilePlanLandingTimingRef.current,
      planned_tiles: tiles.length,
      loaded_tile_keys: loadedRasterTileKeysRef.current.size,
      failed_tile_keys: failedRasterTileKeys.size,
      load_retry_attempts: Object.fromEntries(rasterTileLoadAttempts),
      recovery_count: rasterTileRecoveryCountRef.current,
      images: [...document.querySelectorAll<HTMLImageElement>(".rasterTileLayer .mapTileImage")]
        .map((image) => ({
          src: image.currentSrc || image.src,
          complete: image.complete,
          natural_width: image.naturalWidth,
        })),
    });
    window.__aerobagE2e = { ...(window.__aerobagE2e ?? {}), raster };
    return () => {
      if (window.__aerobagE2e?.raster === raster) delete window.__aerobagE2e.raster;
    };
  }, [failedRasterTileKeys, rasterTileLoadAttempts, selectedMap.selected_map_id, tiles]);

  useEffect(() => {
    if (!__AEROBAG_E2E_ENABLED__) return;
    const rasterFaultOnce = () => {
      const firstTile = tiles[0];
      const faultKeys = new Set(firstTile ? [rasterTileKey(firstTile)] : []);
      for (const key of faultKeys) {
        loadedRasterTileKeysRef.current.delete(key);
        knownLoadedRasterTileKeysRef.current.delete(key);
      }
      e2eRasterTileFaultKeysRef.current = faultKeys;
      setE2eRasterTileFaultKeys(faultKeys);
      return faultKeys.size;
    };
    window.__aerobagE2e = { ...(window.__aerobagE2e ?? {}), rasterFaultOnce };
    return () => {
      if (window.__aerobagE2e?.rasterFaultOnce === rasterFaultOnce) {
        delete window.__aerobagE2e.rasterFaultOnce;
      }
    };
  }, [rasterTileKey, tiles]);

  return (
    <section className="pageSurface" data-testid="parity:page:map">
      <Profiler id="MapSurface" onRender={logReactProfilerRender}>
        <div
          ref={containerRef}
          className="mapSurface chartSurface"
          data-testid="map-surface"
          onPointerDown={handlePointerDown}
          onPointerMove={handlePointerMove}
          onPointerUp={handlePointerRelease}
          onPointerCancel={handlePointerRelease}
          onPointerLeave={handlePointerRelease}
          onLostPointerCapture={handleLostPointerCapture}
          onWheel={handleWheel}
          onDoubleClick={handleDoubleClick}
        >
        <span
          hidden
          data-testid={`parity:viewport:center-x:${viewport.centerWorldX.toFixed(3)}:center-y:${viewport.centerWorldY.toFixed(3)}:zoom:${viewport.zoom.toFixed(3)}:up:${plannedMapUpDeg.toFixed(1)}`}
        />
        <span hidden data-testid={`parity:map-family:${selectedFamily?.id ?? "none"}:map:${selectedMap.selected_map_id}`} />
        {mapLayerState.options.map((option) => {
          const state = mapLayerToggleState(mapLayerState, option.layer_id);
          return (
            <span
              key={`layer-probe:${option.layer_id}`}
              hidden
              data-testid={`parity:map-layer:${option.layer_id}:visible:${state.visible}:enabled:${state.enabled}`}
            />
          );
        })}
        <div className="mapBackdrop" />
        <RenderDependencyBoundary
          dependencies={[
            flightDataBanner,
            flightDataBannerEdge,
            flightDataBannerEdgeColumnCount,
            flightDataBannerEdgeLayout,
            statusControlDockLowered,
            uiSession,
            props.onSessionSnapshot,
          ]}
          render={() => (
            <Profiler id="FlightDataBanner" onRender={logReactProfilerRender}>
              <FlightDataBanner
                banner={flightDataBanner}
                edge={flightDataBannerEdge}
                edgeColumnCount={flightDataBannerEdgeColumnCount}
                edgeLayout={flightDataBannerEdgeLayout}
                lowered={statusControlDockLowered}
                onAction={(actionId) => {
                  if (!uiSession) return;
                  void uiSession.performTimeDisplayAction(actionId).then((snapshot) => {
                    props.onSessionSnapshot(snapshot, "time_display_mode");
                  });
                }}
              />
            </Profiler>
          )}
        />
        {trayGroup.scrimOpen ? <TrayScrim ariaLabel="Close chart tray" onClose={trayGroup.closeAll} /> : null}
        {mapSelection ? (
          <>
            <TrayScrim ariaLabel="Close map selection" onClose={() => setMapSelection(null)} />
            {mapSelection.detailModal?.kind === "weather" ? (
              <WeatherDetailModal detail={mapSelection.detailModal.detail} />
            ) : mapSelection.detailModal?.kind === "airport" ? (
              <AirportInfoModal
                detail={mapSelection.detailModal.detail}
                onTimeDisplayAction={async (actionId) => {
                  if (!uiSession) return;
                  const airportId = mapSelection.detailModal?.kind === "airport"
                    ? mapSelection.detailModal.detail.airport_id
                    : null;
                  if (!airportId) return;
                  props.onSessionSnapshot(
                    await uiSession.performTimeDisplayAction(actionId),
                    "time_display_mode",
                  );
                  const detail = await uiSession.airportInfo(airportId);
                  setMapSelection((current) => current?.detailModal?.kind === "airport"
                    && current.detailModal.detail.airport_id === airportId
                    ? { ...current, detailModal: { kind: "airport", detail } }
                    : current);
                }}
              />
            ) : mapSelection.detailModal ? (
              <MapSelectionDetailModal
                title={mapSelection.detailModal.title}
                text={mapSelection.detailModal.text}
                status={mapSelection.detailModal.status}
                onTimeDisplayAction={toggleOpenMapSelectionTimeDisplay}
              />
            ) : (
              <MapSelectionTray
                point={mapSelection.point}
                result={mapSelection.result}
                selectedItem={mapSelection.selectedItem}
                onSelectItem={(item) => {
                  setMapSelection((current) => current ? {
                    ...current,
                    selectedItem: item,
                    detailModal: null,
                  } : current);
                  const automaticAction = item.actions.find(
                    (action) => action.action_uid === item.automatic_action_uid,
                  );
                  if (automaticAction) {
                    void performSelectedMapAction(automaticAction);
                  }
                }}
                onDisabledAction={showDisabledAction}
                onSelectAction={(action) => void performSelectedMapAction(action)}
              />
            )}
          </>
        ) : null}
        {!mapSelection && hoverWeather ? (
          <WeatherDetailModal
            detail={hoverWeather.detail}
            className="hoverWeatherDetailModal"
            style={hoverWeatherPanelStyle(hoverWeather.point)}
          />
        ) : null}
        {disabledActionToast ? (
          <div className="mapSelectionToast" data-testid="disabled-action-toast" role="status" aria-live="polite">
            {disabledActionToast.message}
          </div>
        ) : null}
        <div
          data-testid={`parity:ownship-state:mode:${ownship.mode}:source:${activeOwnshipSource}:draw:${ownship.draw_aircraft}:position:${ownship.position ? `${ownship.position.lat.toFixed(5)},${ownship.position.lon.toFixed(5)}` : "none"}:track:${ownship.track_deg_true == null ? "none" : ownship.track_deg_true.toFixed(1)}`}
          aria-hidden="true"
          style={{ display: "none" }}
        />
        <div
          data-testid={`parity:live-overlay:metars:${mapOverlay.visible_metars.length}:pireps:${mapOverlay.visible_pireps.length}:obstacles:${mapOverlay.visible_features.filter((feature) => feature.symbol_kind === "obstacle").length}:tfrs:${mapOverlay.tfr_paths.length}`}
          aria-hidden="true"
          style={{ display: "none" }}
        />
        <div
          data-testid={`parity:nexrad-state:tiles:${nexradOverlay.tiles.length}:frame:${nexradOverlay.animation.selected_frame_index ?? "none"}:frames:${nexradOverlay.animation.frame_count}`}
          aria-hidden="true"
          style={{ display: "none" }}
        />
        <div
          ref={mapBearingTransformRef}
          className="mapBearingTransform"
          style={{
            transform: `rotate(${-plannedMapUpDeg}deg)`,
            ["--map-up-deg" as string]: `${plannedMapUpDeg}deg`,
          }}
        >
        <div ref={mapContentTransformRef} className="mapContentTransform">
          <Profiler id="RasterLayer" onRender={logReactProfilerRender}>
            <div
              className="rasterTileLayer"
              data-testid={`parity:raster-state:plan:${rasterTilePlanLandingTimingRef.current?.id ?? 0}:maps:${[...new Set(tiles.map((tile) => tile.mapViewId))].sort().map(encodeURIComponent).join(",") || "none"}:planned:${tiles.length}`}
              aria-hidden="true"
              style={rasterTileTransform ? { transform: rasterTileTransform, transformOrigin: "0 0" } : undefined}
            >
              {tiles.map((tile) => {
                const tileKey = rasterTileKey(tile);
                const loadAttempt = rasterTileLoadAttempts.get(tileKey) ?? 0;
                const tileSource = __AEROBAG_E2E_ENABLED__
                  && loadAttempt === 0
                  && e2eRasterTileFaultKeys.has(tileKey)
                  ? e2eRasterTileStallUrl(tile.src)
                  : rasterTileLoadUrl(tile.src, loadAttempt);
                return (
                  <div
                    key={tileKey}
                    className="mapTile"
                    style={{
                      left: `${tile.left}px`,
                      top: `${tile.top}px`,
                      // Fractional overzoomed tile sizes can expose subpixel seams between rasters.
                      width: `${tile.size + RASTER_TILE_OVERDRAW_PX}px`,
                      height: `${tile.size + RASTER_TILE_OVERDRAW_PX}px`,
                      zIndex: tile.zIndex,
                    }}
                  >
                    {failedRasterTileKeys.has(tileKey) ? null : (
                      <img
                        key={`${tileKey}:load:${loadAttempt}`}
                        className="mapTileImage"
                        src={tileSource}
                        alt=""
                        draggable={false}
                        onLoad={() => reportRasterTileLoaded(tile, loadAttempt)}
                        onError={() => reportRasterTileError(tile, loadAttempt)}
                      />
                    )}
                    {debugState.tile_labels ? (
                      <div className="tileLabel">
                        z{tile.zoom} x{tile.x} y{tile.yTms}
                      </div>
                    ) : null}
                  </div>
                );
              })}
            </div>
          </Profiler>
          <RenderDependencyBoundary
            dependencies={[visibleTerrainImages]}
            render={() => (
              <Profiler id="TerrainLayer" onRender={logReactProfilerRender}>
                {visibleTerrainImages.length > 0 ? (
                  <div className="terrainOverlay" aria-hidden="true">
                    {visibleTerrainImages.map((tile) => (
                      <TerrainOverlayCanvasTile
                        key={tile.key}
                        tile={tile}
                      />
                    ))}
                  </div>
                ) : null}
              </Profiler>
            )}
          />
          {nexradOverlay.tiles.length > 0 ? (
            <svg
              className="nexradOverlay"
              viewBox={`0 0 ${surfaceSize.width} ${surfaceSize.height}`}
              preserveAspectRatio="none"
              aria-hidden="true"
              style={nexradOverlayTransform ? { transform: nexradOverlayTransform, transformOrigin: "0 0" } : undefined}
            >
              {nexradOverlay.tiles.map((tile) => {
                const bounds = nexradTileBounds(tile);
                const label = `res${tile.res} x${tile.x} y${tile.y}`;
                const labelTone = (tile.res + tile.x + tile.y) % 2 === 0 ? " isBlue" : " isOrange";
                return (
                  <g key={tile.key}>
                    <svg
                      x={bounds.left}
                      y={bounds.top}
                      width={bounds.width}
                      height={bounds.height}
                      viewBox={`${tile.source_x} ${tile.source_y} ${tile.source_width} ${tile.source_height}`}
                      preserveAspectRatio="none"
                      overflow="hidden"
                    >
                      <image
                        href={nexradFrameCacheRef.current?.imageUrlFor(resolveLiveFeedResourceUrl(tile.src)) ?? undefined}
                        x={0}
                        y={0}
                        width={tile.image_width}
                        height={tile.image_height}
                        preserveAspectRatio="none"
                      />
                    </svg>
                    {debugState.nexrad_tile_labels ? (
                      <g className={`nexradTileLabel${labelTone}`} transform={`translate(${bounds.left + 4} ${bounds.top + 15})`}>
                        <rect x={-3} y={-12} width={label.length * 7 + 6} height={16} rx={4} />
                        <text x={0} y={0}>{label}</text>
                      </g>
                    ) : null}
                  </g>
                );
              })}
            </svg>
          ) : null}
          <RenderDependencyBoundary
            dependencies={[
              debugState.sequencing_finish_lines,
              handleMetarHoverEnter,
              handleMetarHoverLeave,
              mapIsVisible,
              mapOverlay,
              overlayTransform,
              routeDistancePillLayouts,
              routeScreenSegments,
              selectedMapHighlight,
              surfaceSize.height,
              surfaceSize.width,
            ]}
            render={() => (
          <Profiler id="VectorLayer" onRender={logReactProfilerRender}>
            <>
              <>
                {mapIsVisible && (mapOverlay.airspace_paths.length > 0 || mapOverlay.tfr_paths.length > 0 || mapOverlay.airspace_labels.length > 0) ? (
                  <svg
                    className="airspaceOverlay"
                    viewBox={`0 0 ${surfaceSize.width} ${surfaceSize.height}`}
                    preserveAspectRatio="none"
                    style={overlayTransform ? { transform: overlayTransform, transformOrigin: "0 0" } : undefined}
                  >
                    {mapOverlay.airspace_paths.map((feature) => (
                      <AirspaceDisplayPathGroup key={feature.id} feature={feature} />
                    ))}
                    {mapOverlay.tfr_paths.map((feature) => (
                      <AirspaceDisplayPathGroup key={feature.id} feature={feature} />
                    ))}
                    {mapOverlay.airspace_labels.map((label) => {
                      return (
                        <g
                          key={`${label.feature_id}:${label.glyph.upper}:${label.glyph.lower}:${label.screen_x}:${label.screen_y}`}
                          transform={`translate(${label.screen_x} ${label.screen_y})`}
                        >
                          <g className={`mapUpright airspaceFractionLabel airspaceLabel-${label.glyph.style_key}`}>
                            <AirspaceLimitGlyph glyph={label.glyph} />
                          </g>
                        </g>
                      );
                    })}
                  </svg>
                ) : null}
              </>
              <>
                {mapIsVisible && mapOverlay.offline_regions.length > 0 ? (
                  <svg
                    className="offlineRegionsOverlay"
                    viewBox={`0 0 ${surfaceSize.width} ${surfaceSize.height}`}
                    preserveAspectRatio="none"
                    style={overlayTransform ? { transform: overlayTransform, transformOrigin: "0 0" } : undefined}
                  >
                    {mapOverlay.offline_regions.map((region) => {
                      const color = aviationThemeColor(region.color_key);
                      const summaryLines = offlineRegionSummaryLines(region);
                      return (
                        <g key={region.id}>
                          <polygon
                            points={offlineRegionPoints(region.points)}
                            fill="none"
                            stroke="rgba(255,255,255,0.8)"
                            strokeWidth="5"
                            strokeLinejoin="round"
                            vectorEffect="non-scaling-stroke"
                          />
                          <polygon
                            points={offlineRegionPoints(region.points)}
                            fill="none"
                            stroke={color}
                            strokeWidth="2.5"
                            strokeLinejoin="round"
                            vectorEffect="non-scaling-stroke"
                          />
                          <g transform={`translate(${region.label_x} ${region.label_y})`}>
                            <g className="mapUpright">
                              <text
                                x="0"
                                y="0"
                                textAnchor="middle"
                                dominantBaseline="middle"
                                fill="white"
                                stroke="rgba(0,0,0,0.7)"
                                strokeWidth="4"
                                paintOrder="stroke"
                              >
                                <tspan x="0" dy={summaryLines.length ? "-0.72em" : "0"}>{region.label}</tspan>
                                {summaryLines.map((summary, index) => (
                                  <tspan key={`${region.id}:summary:${index}`} x="0" dy="1.35em">{summary}</tspan>
                                ))}
                              </text>
                              <text
                                x="0"
                                y="0"
                                textAnchor="middle"
                                dominantBaseline="middle"
                                fill={color}
                              >
                                <tspan x="0" dy={summaryLines.length ? "-0.72em" : "0"}>{region.label}</tspan>
                                {summaryLines.map((summary, index) => (
                                  <tspan key={`${region.id}:summary:${index}`} x="0" dy="1.35em">{summary}</tspan>
                                ))}
                              </text>
                            </g>
                          </g>
                        </g>
                      );
                    })}
                  </svg>
                ) : null}
              </>
              <>
                {mapIsVisible && routeScreenSegments.length > 0 ? (
                  <svg
                    className="flightPlanOverlay"
                    data-testid={`parity:flight-plan-route-overlay:segments:${routeScreenSegments.length}`}
                    viewBox={`0 0 ${surfaceSize.width} ${surfaceSize.height}`}
                    preserveAspectRatio="none"
                  >
                    {routeScreenSegments.map((segment, segmentIndex) => (
                      <FlightPlanRoutePath
                        key={`contrast:${flightPlanRouteSegmentRenderKey(segment, segmentIndex)}`}
                        segment={segment}
                        layer="contrast"
                      />
                    ))}
                    {routeDistancePillLayouts.map((layout, index) => (
                      <rect
                        key={`contrast:${index}:${layout.annotation.id}`}
                        transform={`translate(${layout.center.x} ${layout.center.y}) rotate(${layout.rotationDegrees})`}
                        x={-layout.width / 2}
                        y={-FLIGHT_PLAN_ROUTE_DISTANCE_PILL_HEIGHT_PX / 2}
                        width={layout.width}
                        height={FLIGHT_PLAN_ROUTE_DISTANCE_PILL_HEIGHT_PX}
                        rx={FLIGHT_PLAN_ROUTE_DISTANCE_PILL_HEIGHT_PX / 2}
                        fill="none"
                        stroke={loadedUiTheme.flight_plan_route.contrast}
                        strokeWidth="6"
                      />
                    ))}
                    {routeScreenSegments.map((segment, segmentIndex) => (
                      <FlightPlanRoutePath
                        key={`route:${flightPlanRouteSegmentRenderKey(segment, segmentIndex)}`}
                        segment={segment}
                        layer="color"
                      />
                    ))}
                    {routeScreenSegments.flatMap((segment, segmentIndex) =>
                      debugState.sequencing_finish_lines && segment.status === "active"
                        ? segment.finishLinePaths.map((finishLinePath, index) => (
                            <line
                              key={`finish:${segmentIndex}:${index}`}
                              x1={finishLinePath[0].x}
                              y1={finishLinePath[0].y}
                              x2={finishLinePath[1].x}
                              y2={finishLinePath[1].y}
                              stroke="#b100ff"
                              strokeWidth="1.5"
                              strokeLinecap="round"
                              opacity="0.9"
                            />
                          ))
                        : [],
                    )}
                    {routeDistancePillLayouts.map((layout, index) => (
                      <rect
                        key={`fill:${index}:${layout.annotation.id}`}
                        transform={`translate(${layout.center.x} ${layout.center.y}) rotate(${layout.rotationDegrees})`}
                        x={-layout.width / 2}
                        y={-FLIGHT_PLAN_ROUTE_DISTANCE_PILL_HEIGHT_PX / 2}
                        width={layout.width}
                        height={FLIGHT_PLAN_ROUTE_DISTANCE_PILL_HEIGHT_PX}
                        rx={FLIGHT_PLAN_ROUTE_DISTANCE_PILL_HEIGHT_PX / 2}
                        fill={loadedUiTheme.flight_plan_route.distance_pill_bg}
                      />
                    ))}
                    {routeDistancePillLayouts.map((layout, index) => (
                      <g
                        key={`stroke:${index}:${layout.annotation.id}`}
                        transform={`translate(${layout.center.x} ${layout.center.y}) rotate(${layout.rotationDegrees})`}
                        data-testid={`flight-plan-distance:${layout.annotation.id}`}
                      >
                        <rect
                          x={-layout.width / 2}
                          y={-FLIGHT_PLAN_ROUTE_DISTANCE_PILL_HEIGHT_PX / 2}
                          width={layout.width}
                          height={FLIGHT_PLAN_ROUTE_DISTANCE_PILL_HEIGHT_PX}
                          rx={FLIGHT_PLAN_ROUTE_DISTANCE_PILL_HEIGHT_PX / 2}
                          fill="none"
                          stroke={routeSegmentColor(layout.annotation.status)}
                          strokeWidth="2"
                        />
                        <text
                          x="0"
                          y="0"
                          textAnchor="middle"
                          dominantBaseline="central"
                          fill={loadedUiTheme.flight_plan_route.distance_pill_fg}
                          fontSize={FLIGHT_PLAN_ROUTE_DISTANCE_PILL_FONT_PX}
                          fontWeight="800"
                        >
                          {layout.annotation.text}
                        </text>
                      </g>
                    ))}
                  </svg>
                ) : null}
              </>
              <>
                {mapIsVisible && mapOverlay.visible_features.length > 0 ? (
                  <svg
                    className="vectorOverlay"
                    data-testid={`parity:vector-state:features:${mapOverlay.visible_features.length}`}
                    viewBox={`0 0 ${surfaceSize.width} ${surfaceSize.height}`}
                    preserveAspectRatio="none"
                    style={overlayTransform ? { transform: overlayTransform, transformOrigin: "0 0" } : undefined}
                  >
                    {mapOverlay.visible_features.map((feature) => {
                      return (
                        <g
                          key={feature.id}
                          transform={`translate(${feature.screen_x} ${feature.screen_y})`}
                          data-testid={feature.label ? `parity:map-feature:${feature.kind}:${feature.label}:${feature.id}` : undefined}
                        >
                          <VectorPointSymbol feature={feature} />
                        </g>
                      );
                    })}
                  </svg>
                ) : null}
              </>
              <>
                {mapIsVisible && mapOverlay.visible_metars.length > 0 ? (
                  <svg
                    className="metarOverlay"
                    viewBox={`0 0 ${surfaceSize.width} ${surfaceSize.height}`}
                    preserveAspectRatio="none"
                    style={overlayTransform ? { transform: overlayTransform, transformOrigin: "0 0" } : undefined}
                  >
                    {mapOverlay.visible_metars.map((feature) => (
                      <g
                        key={wrappedFeatureRenderKey(feature.station_id, feature.screen_x, feature.screen_y)}
                        className="metarHoverTarget"
                        data-testid={`parity:metar-hover-target:${normalizedStationId(feature.station_id)}`}
                        transform={`translate(${feature.screen_x} ${feature.screen_y})`}
                        onPointerEnter={(event) => handleMetarHoverEnter(event, feature)}
                        onPointerLeave={() => handleMetarHoverLeave(feature)}
                      >
                        <circle className="metarHoverHitTarget" r="19" />
                        <MetarSymbol feature={feature} />
                      </g>
                    ))}
                  </svg>
                ) : null}
              </>
              <>
                {mapIsVisible && mapOverlay.visible_pireps.length > 0 ? (
                  <svg
                    className="metarOverlay"
                    viewBox={`0 0 ${surfaceSize.width} ${surfaceSize.height}`}
                    preserveAspectRatio="none"
                    style={overlayTransform ? { transform: overlayTransform, transformOrigin: "0 0" } : undefined}
                  >
                    {mapOverlay.visible_pireps.map((feature) => (
                      <g
                        key={wrappedFeatureRenderKey(feature.id, feature.screen_x, feature.screen_y)}
                        transform={`translate(${feature.screen_x} ${feature.screen_y})`}
                      >
                        <PirepSymbol feature={feature} scale={0.32} />
                      </g>
                    ))}
                  </svg>
                ) : null}
              </>
              <>
                {mapIsVisible && (mapOverlay.flight_plan_features ?? []).length > 0 ? (
                  <svg
                    className="vectorOverlay flightPlanVectorOverlay"
                    viewBox={`0 0 ${surfaceSize.width} ${surfaceSize.height}`}
                    preserveAspectRatio="none"
                    style={overlayTransform ? { transform: overlayTransform, transformOrigin: "0 0" } : undefined}
                  >
                    {(mapOverlay.flight_plan_features ?? []).map((feature) => {
                      return (
                        <g
                          key={feature.id}
                          transform={`translate(${feature.screen_x} ${feature.screen_y})`}
                          data-testid={feature.label ? `parity:map-fp-feature:${feature.kind}:${feature.label}:${feature.id}` : undefined}
                        >
                          <VectorPointSymbol feature={feature} />
                        </g>
                      );
                    })}
                  </svg>
                ) : null}
              </>
              <>
                {mapIsVisible && mapOverlay.visible_traffic.length > 0 ? (
                  <svg
                    className="trafficOverlay"
                    viewBox={`0 0 ${surfaceSize.width} ${surfaceSize.height}`}
                    preserveAspectRatio="none"
                    style={overlayTransform ? { transform: overlayTransform, transformOrigin: "0 0" } : undefined}
                  >
                    {mapOverlay.visible_traffic.map((feature) => (
                      <g
                        key={feature.id}
                        transform={`translate(${feature.screen_x} ${feature.screen_y})`}
                      >
                        <AdsbTrafficSymbol feature={feature} />
                      </g>
                    ))}
                  </svg>
                ) : null}
              </>
              <>
                {mapIsVisible && selectedMapHighlight ? (
                  <svg
                    className="mapSelectionHighlightOverlay"
                    viewBox={`0 0 ${surfaceSize.width} ${surfaceSize.height}`}
                    preserveAspectRatio="none"
                    style={selectedMapHighlight.kind === "spot" ? undefined : overlayTransform ? { transform: overlayTransform, transformOrigin: "0 0" } : undefined}
                  >
                    {selectedMapHighlight.kind === "point" ? (
                      <g transform={`translate(${selectedMapHighlight.feature.screen_x} ${selectedMapHighlight.feature.screen_y})`}>
                        <g className="mapSelectionFeatureContrast">
                          <VectorPointSymbol feature={selectedMapHighlight.feature} />
                        </g>
                        <VectorPointSymbol feature={selectedMapHighlight.feature} />
                      </g>
                    ) : selectedMapHighlight.kind === "metar" ? (
                      <g transform={`translate(${selectedMapHighlight.feature.screen_x} ${selectedMapHighlight.feature.screen_y})`}>
                        <g className="mapSelectionFeatureContrast">
                          <MetarSymbol feature={selectedMapHighlight.feature} />
                        </g>
                        <MetarSymbol feature={selectedMapHighlight.feature} />
                      </g>
                    ) : selectedMapHighlight.kind === "pirep" ? (
                      <g transform={`translate(${selectedMapHighlight.feature.screen_x} ${selectedMapHighlight.feature.screen_y})`}>
                        <g className="mapSelectionFeatureContrast">
                          <PirepSymbol feature={selectedMapHighlight.feature} scale={0.32} />
                        </g>
                        <PirepSymbol feature={selectedMapHighlight.feature} scale={0.32} />
                      </g>
                    ) : selectedMapHighlight.kind === "adsb_traffic" ? (
                      <g transform={`translate(${selectedMapHighlight.feature.screen_x} ${selectedMapHighlight.feature.screen_y})`}>
                        <g className="mapSelectionFeatureContrast">
                          <AdsbTrafficSymbol feature={selectedMapHighlight.feature} />
                        </g>
                        <AdsbTrafficSymbol feature={selectedMapHighlight.feature} />
                      </g>
                    ) : selectedMapHighlight.kind === "path" ? (
                      <g>
                        {selectedMapHighlight.feature.paths.map((path, index) => (
                          <path
                            key={`${selectedMapHighlight.feature.id}:highlight:${index}`}
                            d={airspaceSvgPathD(path)}
                            fill="none"
                            stroke="white"
                            strokeWidth="9"
                            strokeLinecap="round"
                            strokeLinejoin="round"
                            vectorEffect="non-scaling-stroke"
                          />
                        ))}
                        <AirspaceDisplayPathGroup feature={selectedMapHighlight.feature} />
                      </g>
                    ) : selectedMapHighlight.kind === "offline_region" ? (
                      <polygon
                        points={offlineRegionPoints(selectedMapHighlight.feature.points)}
                        fill="rgba(255,255,255,0.08)"
                        stroke="white"
                        strokeWidth="6"
                        strokeLinejoin="round"
                        vectorEffect="non-scaling-stroke"
                      />
                    ) : (
                      <g transform={`translate(${selectedMapHighlight.point.x} ${selectedMapHighlight.point.y})`}>
                        <MapSelectionSpotSymbol />
                      </g>
                    )}
                  </svg>
                ) : null}
              </>
            </>
          </Profiler>
            )}
          />
        <RenderDependencyBoundary
          dependencies={[mapIsVisible, situationOverlay, surfaceSize.height, surfaceSize.width]}
          render={() => (
            <Profiler id="SituationLayer" onRender={logReactProfilerRender}>
              {mapIsVisible && situationOverlay ? (
                <svg className="situationOverlay" viewBox={`0 0 ${surfaceSize.width} ${surfaceSize.height}`} preserveAspectRatio="none">
              {situationOverlay.ring ? (
                <>
                  <circle
                    cx={situationOverlay.point.x}
                    cy={situationOverlay.point.y}
                    r={situationOverlay.ring.radiusPx}
                    fill="none"
                    stroke="rgba(0, 0, 0, 0.4)"
                    strokeWidth="8"
                  />
                  <circle
                    cx={situationOverlay.point.x}
                    cy={situationOverlay.point.y}
                    r={situationOverlay.ring.radiusPx}
                    fill="none"
                    stroke="#ffffff"
                    strokeWidth="3"
                  />
                  {situationOverlay.ring.tickMarks.map((tick, index) => (
                    <Fragment key={index}>
                      <line
                        x1={tick.inner.x}
                        y1={tick.inner.y}
                        x2={tick.outer.x}
                        y2={tick.outer.y}
                        stroke="rgba(0, 0, 0, 0.4)"
                        strokeWidth="8"
                        strokeLinecap="round"
                      />
                      <line
                        x1={tick.inner.x}
                        y1={tick.inner.y}
                        x2={tick.outer.x}
                        y2={tick.outer.y}
                        stroke="#ffffff"
                        strokeWidth="3"
                        strokeLinecap="round"
                      />
                    </Fragment>
                  ))}
                  {situationOverlay.ring.cardinalLabels.map((label) => (
                    <Fragment key={label.text}>
                      <text
                        x={label.point.x}
                        y={label.point.y}
                        fill="none"
                        stroke="rgba(0, 0, 0, 0.4)"
                        strokeWidth="5"
                        strokeLinejoin="round"
                        fontSize="16"
                        fontWeight="700"
                        textAnchor="middle"
                        dominantBaseline="middle"
                        transform={`rotate(${label.rotationDeg} ${label.point.x} ${label.point.y})`}
                      >
                        {label.text}
                      </text>
                      <text
                        x={label.point.x}
                        y={label.point.y}
                        fill="#ffffff"
                        fontSize="16"
                        fontWeight="700"
                        textAnchor="middle"
                        dominantBaseline="middle"
                        transform={`rotate(${label.rotationDeg} ${label.point.x} ${label.point.y})`}
                      >
                        {label.text}
                      </text>
                    </Fragment>
                  ))}
                  <circle
                    cx={situationOverlay.point.x}
                    cy={situationOverlay.point.y}
                    r={situationOverlay.ring.radiusPx}
                    fill="none"
                    stroke="#ffffff"
                    strokeWidth="3"
                  />
                  <text
                    x={situationOverlay.ring.label.point.x}
                    y={situationOverlay.ring.label.point.y}
                    fill="none"
                    stroke="rgba(0, 0, 0, 0.4)"
                    strokeWidth="5"
                    strokeLinejoin="round"
                    fontSize="16"
                    fontWeight="700"
                    textAnchor="middle"
                    dominantBaseline="middle"
                    transform={`rotate(${situationOverlay.ring.label.rotationDeg} ${situationOverlay.ring.label.point.x} ${situationOverlay.ring.label.point.y})`}
                  >
                    {situationOverlay.ring.label.text}
                  </text>
                  <text
                    x={situationOverlay.ring.label.point.x}
                    y={situationOverlay.ring.label.point.y}
                    fill="#ffffff"
                    fontSize="16"
                    fontWeight="700"
                    textAnchor="middle"
                    dominantBaseline="middle"
                    transform={`rotate(${situationOverlay.ring.label.rotationDeg} ${situationOverlay.ring.label.point.x} ${situationOverlay.ring.label.point.y})`}
                  >
                    {situationOverlay.ring.label.text}
                  </text>
                </>
              ) : null}
              {situationOverlay.predictor ? (
                <g>
                  {(() => {
                    const shaftEnd = arrowShaftEndPoint(situationOverlay.point, situationOverlay.predictor);
                    return (
                      <>
                  <line
                    x1={situationOverlay.point.x}
                    y1={situationOverlay.point.y}
                    x2={shaftEnd.x}
                    y2={shaftEnd.y}
                    stroke="rgba(0, 0, 0, 0.4)"
                    strokeWidth="8"
                    strokeLinecap="round"
                  />
                  <line
                    x1={situationOverlay.point.x}
                    y1={situationOverlay.point.y}
                    x2={shaftEnd.x}
                    y2={shaftEnd.y}
                    stroke="#ffffff"
                    strokeWidth="6"
                    strokeLinecap="round"
                  />
                  <polygon
                    points={arrowHeadPoints(situationOverlay.point, situationOverlay.predictor)}
                    fill="#ffffff"
                    stroke="rgba(0, 0, 0, 0.4)"
                    strokeWidth="1.5"
                    strokeLinejoin="round"
                  />
                      </>
                    );
                  })()}
                </g>
              ) : null}
              <SituationAircraftSvg
                pathData={aircraftPlanViewPath}
                point={situationOverlay.point}
                headingDeg={situationOverlay.headingDeg}
                sizePx={thumbPixels(1.44)}
              />
                </svg>
              ) : null}
            </Profiler>
          )}
        />
        </div>
        </div>
        <Profiler id="MapControls" onRender={logReactProfilerRender}>
          <>
        <Profiler id="StatusControls" onRender={logReactProfilerRender}>
          <StatusControlDock
          controls={ownshipControls}
          dataStatuses={statusControls.controls.map((control) => {
            const trayId = statusControlTrayId(control.id);
            return {
              id: control.id,
              state: control.state,
              open: trayGroup.isOpen(trayId),
              onToggle: () => trayGroup.toggle(trayId),
              onAction: onStatusAction,
              testIdPrefix: statusControlTestIdPrefix(control.id),
            };
          })}
          lowered={statusControlDockLowered}
          ownshipOpen={trayGroup.isOpen("ownship")}
          onOwnshipToggle={() => trayGroup.toggle("ownship")}
          options={ownshipSourceOptions}
          onDisabledAction={showDisabledAction}
          transportControls={
            <SituationControlFooter
              controls={ownshipControls}
              onInput={onSituationControlInput}
              onTextAction={(actionId, value) => {
                if (!uiSession) return;
                void uiSession.performOwnshipTextAction(actionId, value, Date.now())
                  .then(onPlaybackSnapshotChange)
                  .catch((error: unknown) => showDisabledAction(errorMessage(error)));
              }}
              onDisabledAction={showDisabledAction}
            />
          }
          />
        </Profiler>

        <Profiler id="MapChartControls" onRender={logReactProfilerRender}>
          <div className="chartDock mapChartDock">
          <TrayDock
            launcherLabel={selectedFamily?.launcher_label ?? "---"}
            launcherImageSrc={chartFamilyIconSrc(selectedFamily?.id)}
            open={trayGroup.isOpen("family")}
            onToggle={() => trayGroup.toggle("family")}
            ariaLabel="Chart family"
            testId="chart-family-button"
            onDisabledAction={showDisabledAction}
            options={familyOptions.map((family) => ({
              id: family.id,
              label: family.label,
              iconSrc: chartFamilyIconSrc(family.id),
              active: family.active,
              disabled: !family.enabled,
              disabledReason: family.disabled_reason ?? null,
              dismissTrayOnSelect: true,
              accessory: chartReferenceAction?.family_id === family.id
                ? {
                  iconSrc: CHART_REFERENCE_ICON_SRC,
                  ariaLabel: `Open ${family.label} legends and insets`,
                  onSelect: () => onOpenChartReference(chartReferenceAction),
                }
                : undefined,
              onSelect: () => {
                onSelectMapFamily(family.id);
              },
            }))}
          />
          <TrayDock
            launcherLabel="LAYERS"
            launcherImageSrc={layerIconSrc("vectors")}
            open={trayGroup.isOpen("layers")}
            onToggle={() => trayGroup.toggle("layers")}
            ariaLabel="Layers"
            testId="layers-button"
            onDisabledAction={showDisabledAction}
            options={layerTrayOptions}
          />
          <ChartSearchBox
            state={chartSearch}
            onQueryChange={(query) => setChartSearch((current) => ({ ...current, query, open: true }))}
            onFocus={() => setChartSearch((current) => ({ ...current, open: true }))}
            onClose={() => setChartSearch((current) => ({ ...current, open: false }))}
            onSubmit={submitChartSearch}
            onSelect={(suggestion) => {
              setChartSearch((current) => ({ ...current, loading: true, error: null }));
              void inspectNavRef(navRefFromWaypointSuggestion(suggestion.nav_ref))
                .then(() => setChartSearch({ query: "", open: false, loading: false, error: null, suggestions: [] }))
                .catch((error) => {
                  setChartSearch((current) => ({
                    ...current,
                    loading: false,
                    error: `Search failed: ${errorMessage(error)}`,
                  }));
                });
            }}
          />
          <button
            type="button"
            className={`centerHereButton${mapFollowUiState.following ? " isActive" : ""}${centerHereDisabled ? " isDisabled" : ""}`}
            data-testid="center-here-button"
            disabled={centerHereDisabled && !centerHereDisabledReason}
            aria-disabled={centerHereDisabled ? "true" : undefined}
            aria-pressed={mapFollowUiState.following}
            aria-label={mapFollowUiState.following ? "Stop centering on ownship" : "Center on ownship"}
            title={centerHereDisabledReason ?? undefined}
            onPointerDown={stopPointer}
            onPointerUp={stopPointer}
            onDoubleClick={stopDoubleClick}
            onClick={() => {
              if (centerHereDisabled) {
                if (centerHereDisabledReason) {
                  showDisabledAction(centerHereDisabledReason);
                }
                return;
              }
              if (!uiSession) {
                return;
              }
              followTargetGateRef.current.clear();
              const nextSnapshot = mapFollowUiState.following
                ? uiSession.disengageMapFollow(viewportRef.current)
                : uiSession.engageMapFollow(viewportRef.current);
              void nextSnapshot.then(props.onPlaybackSnapshotChange).catch(() => {});
            }}
          >
            <svg className="centerHereIcon" viewBox="-20 -20 40 40" aria-hidden="true">
              <RenderNavSymbolLayers
                layers={mapFollowUiState.following ? mapFollowActiveSymbol : mapFollowInactiveSymbol}
              />
            </svg>
            <span className="chartButtonLabel">CTR</span>
          </button>
          <MapOrientationButton
            mode={mapOrientationMode}
            mapUpDeg={plannedMapUpDeg}
            magneticVariationDeg={ownship.magnetic_variation_deg}
            onToggle={() => {
              onMapOrientationModeChange(mapOrientationMode === "north" ? "track" : "north");
            }}
          />
          </div>
        </Profiler>

        <RenderDependencyBoundary
          dependencies={[page, planUiState?.guidance?.nav_element, onSelectPage, onOpenPlan]}
          render={() => (
            <Profiler id="PrimaryNavigation" onRender={logReactProfilerRender}>
              <PrimaryNavigationDock
                page={page}
                navElement={planUiState?.guidance?.nav_element}
                onSelectPage={onSelectPage}
                onOpenPlan={onOpenPlan}
              />
            </Profiler>
          )}
        />

        {playbackPanelState.visible ? (
          <PlaybackWidget
            uiSession={uiSession}
            playbackUiState={playbackUiState}
            sourcePath={props.playbackSourcePath}
            onSourcePathChange={props.onPlaybackSourcePathChange}
            onSnapshotChange={props.onPlaybackSnapshotChange}
            surfaceWidth={surfaceSize.width}
            dock="left"
            onDisabledAction={showDisabledAction}
          />
        ) : null}

        <Profiler id="ZoomControl" onRender={logReactProfilerRender}>
          <ZoomControl
            zoom={viewport.zoom}
            minZoom={selectedMap.min_zoom}
            maxZoom={selectedMap.max_zoom}
            onZoomChange={setViewportZoom}
            raisedForPrimaryNavigation={bottomCornerControlsRaised}
          />
        </Profiler>
          </>
        </Profiler>

        </div>
      </Profiler>
    </section>
  );
}

function MapOrientationButton(props: {
  mode: MapOrientationMode;
  mapUpDeg: number;
  magneticVariationDeg: number | null;
  onToggle: () => void;
}) {
  const trackUp = props.mode === "track";
  const label = trackUp ? "TRK" : "N";
  const needleRotationDeg = compassNeedleRotationDegrees(
    props.mapUpDeg,
    props.magneticVariationDeg,
  );
  return (
    <button
      type="button"
      className={`chartButton mapOrientationButton${trackUp ? " isTrackUp" : ""}`}
      data-testid="map-orientation-button"
      aria-label={`Map orientation: ${trackUp ? "track up" : "north up"}`}
      aria-pressed={trackUp}
      title={trackUp ? "Switch to north-up" : "Switch to track-up"}
      onPointerDown={stopPointer}
      onPointerUp={stopPointer}
      onDoubleClick={stopDoubleClick}
      onClick={props.onToggle}
    >
      <svg className="mapOrientationCompass" viewBox="-20 -20 40 40" aria-hidden="true">
        <g transform={`rotate(${needleRotationDeg})`}>
          <RenderNavSymbolLayers layers={compassSymbol} />
        </g>
      </svg>
      <span className="chartButtonLabel">{label}</span>
    </button>
  );
}

function NavElementButton(props: {
  navElement: NavElementUiView | null | undefined;
  className?: string;
  onClick?: () => void;
  onPointerDown?: (event: PointerEvent<HTMLElement>) => void;
  onPointerUp?: (event: PointerEvent<HTMLElement>) => void;
  onDoubleClick?: (event: MouseEvent<HTMLElement>) => void;
}) {
  const { navElement, className = "navElement", onClick, onPointerDown, onPointerUp, onDoubleClick } = props;
  const displayedNavElement = navElement ?? {
    active_leg_summary: "NO ACTIVE LEG",
    cdi_indicator_dots: null,
    cdi_offscale_readout: null,
  };
  return (
    <button
      type="button"
      className={`${className}${navElement ? "" : " isUnavailable"}`}
      data-testid="nav-cdi"
      onPointerDown={onPointerDown}
      onPointerUp={onPointerUp}
      onDoubleClick={onDoubleClick}
      onClick={onClick}
    >
      <NavElementView navElement={displayedNavElement} />
    </button>
  );
}

function FlightDataBanner(props: {
  banner: FlightDataBannerModel;
  edge: FlightDataBannerEdge;
  edgeColumnCount?: number;
  edgeLayout?: boolean;
  lowered?: boolean;
  onAction: (actionId: string) => void;
}) {
  const cells = props.banner.cells;
  if (cells.length === 0) {
    return null;
  }
  const edgeClass = props.edge === "left" ? " isLeftEdge" : " isRightEdge";
  const edgeColumnClass =
    props.edgeLayout && props.edgeColumnCount && props.edgeColumnCount > 1
      ? ` isEdgeColumns${props.edgeColumnCount}`
      : "";
  return (
    <div
      className={`flightDataBanner${props.edgeLayout ? ` isEdgeLayout${edgeClass}` : ""}${edgeColumnClass}${props.lowered ? " isLowered" : ""}`}
      aria-label="Flight data"
    >
      {cells.map((cell) => (
        <div
          key={cell.id}
          className={`flightDataCell${cell.action_id ? " isActionable" : ""}`}
          role={cell.action_id ? "button" : undefined}
          tabIndex={cell.action_id ? 0 : undefined}
          onPointerDown={cell.action_id ? stopPointer : undefined}
          onPointerUp={cell.action_id ? stopPointer : undefined}
          onClick={cell.action_id ? (event) => {
            event.stopPropagation();
            props.onAction(cell.action_id!);
          } : undefined}
          onKeyDown={cell.action_id ? (event) => {
            if (event.key === "Enter" || event.key === " ") {
              event.preventDefault();
              props.onAction(cell.action_id!);
            }
          } : undefined}
        >
          <FlightDataCellContents cell={cell} />
        </div>
      ))}
    </div>
  );
}

function FlightDataCellContents(props: { cell: FlightDataBannerModel["cells"][number] }) {
  return (
    <>
      <span className="flightDataLabel">{props.cell.label}</span>
      <span className={`flightDataValue${props.cell.value ? "" : " isMissing"}`}>
        {props.cell.value ?? "\u2014"}
      </span>
    </>
  );
}

function NavElementView(props: { navElement: NavElementUiView }) {
  const { navElement } = props;
  const width = 180;
  const height = 18;
  const unit = width / 4.5;
  const fullScaleDots = 2;
  const offscaleDots = 2.1;
  const dotXs = [0.25, 1.25, 3.25, 4.25].map((value) => value * unit);
  const centerX = 2.25 * unit;
  const baselineY = height * 0.5;
  const dotRadius = unit * 0.04375;
  const centerTriangleHalfWidth = unit * 0.25;
  const centerTriangleTopY = 0;
  const centerTriangleBottomY = height + 1;
  const pointerPosition = navElement.cdi_indicator_dots;
  const clampedPointerPosition =
    pointerPosition === null
      ? null
      : Math.max(-fullScaleDots, Math.min(fullScaleDots, pointerPosition));
  const pointerX =
    clampedPointerPosition === null
      ? null
      : (clampedPointerPosition + 2.25) * unit;
  const offscaleDirection =
    pointerPosition === null || Math.abs(pointerPosition) <= offscaleDots
      ? null
      : pointerPosition > 0
        ? "R"
        : "L";
  const offscaleBaseX = offscaleDirection === "R"
    ? (2.25 + offscaleDots) * unit
    : offscaleDirection === "L"
      ? (2.25 - offscaleDots) * unit
      : null;
  const offscaleTipX = offscaleDirection === "R" ? width : offscaleDirection === "L" ? 0 : null;
  const offscaleTrianglePoints = offscaleDirection && offscaleBaseX !== null && offscaleTipX !== null
    ? `${offscaleBaseX},${height * 0.18} ${offscaleBaseX},${height * 0.82} ${offscaleTipX},${baselineY}`
    : null;
  const offscaleReadout = navElement.cdi_offscale_readout;
  const offscaleReadoutDotIndex = offscaleReadout && offscaleDirection === "R"
    ? 2
    : offscaleReadout && offscaleDirection === "L"
      ? 1
      : null;
  const offscaleReadoutX = offscaleReadoutDotIndex === null ? null : dotXs[offscaleReadoutDotIndex];
  const cdiTitle =
    pointerPosition === null
      ? "No CDI deviation"
      : `${Math.abs(pointerPosition).toFixed(1)} dots ${pointerPosition > 0 ? "right" : "left"} of center`;
  return (
    <>
      <span className="navElementTop">{navElement.active_leg_summary}</span>
      <svg className="navElementBottom" viewBox={`0 0 ${width} ${height}`} aria-hidden="true">
        <title>{cdiTitle}</title>
        <path
          className="navElementCdiCenter"
          d={`M ${centerX - centerTriangleHalfWidth} ${centerTriangleBottomY} L ${centerX + centerTriangleHalfWidth} ${centerTriangleBottomY} L ${centerX} ${centerTriangleTopY} Z`}
        />
        {dotXs.map((x, index) => (
          index === offscaleReadoutDotIndex ? null : (
            <circle key={index} className="navElementCdiDot" cx={x} cy={baselineY} r={dotRadius} />
          )
        ))}
        {offscaleTrianglePoints ? (
          <>
            {offscaleReadout ? (
              <text className="navElementCdiOffscaleReadout" x={offscaleReadoutX ?? centerX} y={baselineY} textAnchor="middle" dominantBaseline="central">
                {offscaleReadout}
              </text>
            ) : null}
            <polygon className="navElementCdiOffscalePointer" points={offscaleTrianglePoints} />
          </>
        ) : pointerX !== null ? (
          <line className="navElementCdiPointer" x1={pointerX} y1={0} x2={pointerX} y2={height} />
        ) : null}
      </svg>
    </>
  );
}

function playbackWidgetMaxWidthPx(surfaceWidth: number) {
  if (surfaceWidth <= 0) {
    return 0;
  }
  const thumb = thumbPixels(1);
  const gap = thumbPixels(0.1);
  const navWidth = thumb * 5 + gap * 2;
  const navRightEdge = surfaceWidth / 2 + navWidth / 2;
  return Math.max(thumb * 2.8, surfaceWidth - navRightEdge - gap * 2);
}

function profilePathData(
  samples: Array<number | null>,
  width: number,
  height: number,
  leftInset: number,
  rightInset: number,
) {
  const usable = samples
    .map((value, index) => ({ value, index }))
    .filter((entry): entry is { value: number; index: number } => typeof entry.value === "number");
  if (usable.length === 0) {
    return "";
  }
  const lastIndex = Math.max(samples.length - 1, 1);
  const usableWidth = Math.max(width - leftInset - rightInset, 0);
  return usable
    .map(({ value, index }, pointIndex) => {
      const x = leftInset + (index / lastIndex) * usableWidth;
      const y = height - value * height;
      return `${pointIndex === 0 ? "M" : "L"} ${x.toFixed(2)} ${y.toFixed(2)}`;
    })
    .join(" ");
}

function PlaybackWidget(props: {
  uiSession: UiSession | null;
  playbackUiState: PlaybackUiState;
  sourcePath: string;
  onSourcePathChange: Dispatch<SetStateAction<string>>;
  onSnapshotChange: Dispatch<SetStateAction<UiSessionSnapshot>>;
  surfaceWidth: number;
  dock?: "left" | "right";
  onDisabledAction?: (message: string) => void;
}) {
  const {
    uiSession,
    playbackUiState,
    sourcePath,
    onSourcePathChange,
    onSnapshotChange,
    surfaceWidth,
    dock = "right",
    onDisabledAction,
  } = props;
  const [isBusy, setIsBusy] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [scrubCursorSeconds, setScrubCursorSeconds] = useState<number | null>(null);
  const seekRequestIdRef = useRef(0);
  const scrubRef = useRef<HTMLDivElement | null>(null);
  const gapPatternId = useId();
  const maxWidthPx = playbackWidgetMaxWidthPx(surfaceWidth);
  const durationSeconds = Math.max(playbackUiState.duration_seconds, 0);
  const committedCursorSeconds = Math.min(Math.max(playbackUiState.cursor_seconds, 0), durationSeconds || 0);
  const cursorSeconds =
    scrubCursorSeconds === null
      ? committedCursorSeconds
      : Math.min(Math.max(scrubCursorSeconds, 0), durationSeconds || 0);
  const canControl = uiSession !== null;
  const loadDisabledReason = !canControl
    ? "Playback controls are not ready yet."
    : isBusy
      ? "Trace load is already running."
      : !sourcePath.trim()
        ? "Enter a trace URL before loading."
        : null;
  const playDisabledReason = !canControl
    ? "Playback controls are not ready yet."
    : playbackUiState.status === "empty"
      ? "Load a trace before replaying."
      : null;
  const summary = playbackUiState.title_label;
  const overviewWidth = 320;
  const overviewHeight = 34;
  const knobRadius = 7;
  const cursorRatio = durationSeconds > 0 ? cursorSeconds / durationSeconds : 0;
  const cursorX = knobRadius + cursorRatio * Math.max(overviewWidth - knobRadius * 2, 0);
  const speedPath = profilePathData(playbackUiState.speed_profile_norm, overviewWidth, overviewHeight, knobRadius, knobRadius);
  const altitudePath = profilePathData(playbackUiState.altitude_profile_norm, overviewWidth, overviewHeight, knobRadius, knobRadius);
  const gapRects = playbackUiState.gap_spans
    .map((gap, index) => {
      if (durationSeconds <= 0 || gap.end_seconds <= gap.start_seconds) {
        return null;
      }
      const usableWidth = Math.max(overviewWidth - knobRadius * 2, 0);
      const startRatio = Math.min(Math.max(gap.start_seconds / durationSeconds, 0), 1);
      const endRatio = Math.min(Math.max(gap.end_seconds / durationSeconds, 0), 1);
      const x = knobRadius + startRatio * usableWidth;
      const width = Math.max((endRatio - startRatio) * usableWidth, 1);
      return { key: `${gap.start_seconds}:${gap.end_seconds}:${index}`, x, width };
    })
    .filter((rect): rect is { key: string; x: number; width: number } => rect !== null);

  useEffect(() => {
    if (scrubCursorSeconds === null) {
      return;
    }
    if (Math.abs(scrubCursorSeconds - committedCursorSeconds) < 1e-6) {
      perfDebugLog("playback.seek.scrub_cleared", () => ({
        scrub_cursor_seconds: scrubCursorSeconds,
        committed_cursor_seconds: committedCursorSeconds,
      }));
      setScrubCursorSeconds(null);
    }
  }, [committedCursorSeconds, scrubCursorSeconds]);

  useEffect(() => {
    perfDebugLog("playback.seek.state", () => ({
      committed_cursor_seconds: committedCursorSeconds,
      scrub_cursor_seconds: scrubCursorSeconds,
      displayed_cursor_seconds: cursorSeconds,
      duration_seconds: durationSeconds,
      status: playbackUiState.status,
    }));
  }, [committedCursorSeconds, cursorSeconds, durationSeconds, playbackUiState.status, scrubCursorSeconds]);

  async function loadTrace() {
    if (!uiSession || !sourcePath.trim()) {
      return;
    }
    setIsBusy(true);
    setLoadError(null);
    try {
      const fetched = await fetchTextResource(sourcePath);
      if (fetched.attempts > 1) {
        debugLog("playback.load.transport_recovered", {
          source_path: sourcePath,
          attempts: fetched.attempts,
        });
      }
      const nextSnapshot = await uiSession.loadPlaybackTrace(sourcePath, fetched.text);
      debugLog("playback.load.result", {
        source_path: sourcePath,
        playback_panel_visible: nextSnapshot.playback_panel_state.visible,
        playback_status: nextSnapshot.playback_ui_state.status,
        playback_title: nextSnapshot.playback_ui_state.title_label,
        ownship_mode: nextSnapshot.app_ui_state.ownship.controls.mode,
        ownship_launcher_label: nextSnapshot.app_ui_state.ownship.controls.launcher_label,
        ownship_selection: nextSnapshot.app_ui_state.ownship.controls.selection,
      });
      onSnapshotChange(nextSnapshot);
    } catch (error) {
      console.error(error);
      setLoadError(errorMessage(error));
      debugLog("playback.load.error", {
        source_path: sourcePath,
        message: errorMessage(error),
      });
    } finally {
      setIsBusy(false);
    }
  }

  async function playPause() {
    if (!uiSession) {
      return;
    }
    try {
      const nextSnapshot =
        playbackUiState.status === "playing"
          ? await uiSession.pausePlayback(Date.now())
          : await uiSession.playPlayback(Date.now());
      onSnapshotChange(nextSnapshot);
    } catch (error) {
      console.error(error);
    }
  }

  async function commitSeek(nextCursorSeconds: number, options?: { clearScrub?: boolean }) {
    if (!uiSession) {
      return;
    }
    const clearScrub = options?.clearScrub ?? true;
    const requestId = seekRequestIdRef.current + 1;
    seekRequestIdRef.current = requestId;
    debugLog("playback.seek.commit_start", {
      request_id: requestId,
      requested_cursor_seconds: nextCursorSeconds,
      committed_cursor_seconds: committedCursorSeconds,
      scrub_cursor_seconds: scrubCursorSeconds,
    });
    try {
      const nextSnapshot = await uiSession.seekPlayback(nextCursorSeconds, Date.now());
      debugLog("playback.seek.commit_done", {
        request_id: requestId,
        requested_cursor_seconds: nextCursorSeconds,
        returned_cursor_seconds: nextSnapshot.playback_ui_state.cursor_seconds,
        current_request_id: seekRequestIdRef.current,
      });
      if (seekRequestIdRef.current === requestId) {
        onSnapshotChange(nextSnapshot);
        if (clearScrub) {
          setScrubCursorSeconds(null);
        }
      }
    } catch (error) {
      debugLog("playback.seek.commit_error", {
        request_id: requestId,
        requested_cursor_seconds: nextCursorSeconds,
        error: error instanceof Error ? error.message : String(error),
      });
      console.error(error);
    }
  }

  function cursorSecondsForPointer(clientX: number) {
    const rect = scrubRef.current?.getBoundingClientRect();
    if (!rect || rect.width <= 0 || durationSeconds <= 0) {
      return 0;
    }
    const usableWidth = Math.max(rect.width - knobRadius * 2, 1);
    const rawX = clientX - rect.left;
    const clampedX = Math.min(Math.max(rawX - knobRadius, 0), usableWidth);
    return (clampedX / usableWidth) * durationSeconds;
  }

  function beginScrub(clientX: number) {
    const nextCursorSeconds = cursorSecondsForPointer(clientX);
    perfDebugLog("playback.seek.pointer_move", () => ({
      pointer_x: clientX,
      next_cursor_seconds: nextCursorSeconds,
      committed_cursor_seconds: committedCursorSeconds,
      scrub_cursor_seconds: scrubCursorSeconds,
    }));
    setScrubCursorSeconds(nextCursorSeconds);
    void commitSeek(nextCursorSeconds, { clearScrub: false });
  }

  return (
    <section
      className={`playbackWidget${dock === "left" ? " isLeftDocked" : ""}`}
      data-testid={`parity:playback-widget:status:${playbackUiState.status}:cursor:${committedCursorSeconds.toFixed(3)}:duration:${durationSeconds.toFixed(3)}:rate:${playbackUiState.rate.toFixed(2)}:gaps:${playbackUiState.gap_spans.length}`}
      onPointerDown={stopPointer}
      onPointerUp={stopPointer}
      onDoubleClick={stopDoubleClick}
      onClick={stopClick}
      style={maxWidthPx > 0 ? ({ width: `${maxWidthPx}px` } as CSSProperties) : undefined}
    >
      <div className="playbackWidgetTop">
        <span className="playbackWidgetTitle">{summary}</span>
        <span className="playbackWidgetMeta">{playbackUiState.rate.toFixed(1)}x</span>
      </div>
      <div className="playbackWidgetRow">
        <input
          className="playbackWidgetInput"
          data-testid="playback-source-input"
          value={sourcePath}
          onChange={(event) => {
            setLoadError(null);
            onSourcePathChange(event.target.value);
          }}
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
        />
        <button
          type="button"
          className={`playbackWidgetButton${loadDisabledReason ? " isDisabled" : ""}`}
          data-testid="playback-load-button"
          disabled={Boolean(loadDisabledReason && !onDisabledAction)}
          aria-disabled={loadDisabledReason ? "true" : undefined}
          title={loadDisabledReason ?? undefined}
          onClick={() => {
            if (loadDisabledReason) {
              onDisabledAction?.(loadDisabledReason);
              return;
            }
            void loadTrace();
          }}
        >
          LOAD
        </button>
      </div>
      {loadError ? (
        <p className="playbackWidgetError" role="alert" data-testid="playback-load-error">
          Trace load failed: {loadError}
        </p>
      ) : null}
      <div className="playbackWidgetRow">
        <button
          type="button"
          className={`playbackWidgetButton playbackWidgetMediaButton${playDisabledReason ? " isDisabled" : ""}`}
          data-testid="playback-play-toggle"
          disabled={Boolean(playDisabledReason && !onDisabledAction)}
          aria-disabled={playDisabledReason ? "true" : undefined}
          title={playDisabledReason ?? undefined}
          onClick={() => {
            if (playDisabledReason) {
              onDisabledAction?.(playDisabledReason);
              return;
            }
            void playPause();
          }}
          aria-label={playbackUiState.status === "playing" ? "Pause playback" : "Play playback"}
        >
          {playbackUiState.status === "playing" ? (
            <svg className="playbackWidgetMediaIcon" viewBox="0 0 24 24" aria-hidden="true">
              <rect x="7" y="6" width="3.5" height="12" rx="0.8" />
              <rect x="13.5" y="6" width="3.5" height="12" rx="0.8" />
            </svg>
          ) : (
            <svg className="playbackWidgetMediaIcon" viewBox="0 0 24 24" aria-hidden="true">
              <path d="M8 6.5v11l9-5.5z" />
            </svg>
          )}
        </button>
        <label className="playbackWidgetRateLabel">
          SPD
          <input
            className="playbackWidgetRate"
            data-testid="playback-rate-input"
            type="range"
            min={0.25}
            max={11}
            step={0.25}
            value={playbackUiState.rate}
            disabled={!canControl || playbackUiState.status === "empty"}
            title={playDisabledReason ?? undefined}
            onChange={(event) => {
              if (!uiSession) {
                return;
              }
              void uiSession.setPlaybackRate(Number(event.target.value), Date.now()).then(onSnapshotChange).catch((error) => {
                console.error(error);
              });
            }}
          />
        </label>
      </div>
      <div
        ref={scrubRef}
        className="playbackWidgetOverview"
        data-testid="playback-overview"
        onPointerDown={(event) => {
          stopPointer(event);
          event.currentTarget.setPointerCapture(event.pointerId);
          beginScrub(event.clientX);
        }}
        onPointerMove={(event) => {
          if ((event.buttons & 1) !== 1) {
            return;
          }
          beginScrub(event.clientX);
        }}
        onPointerUp={(event) => {
          stopPointer(event);
          const nextCursorSeconds = cursorSecondsForPointer(event.clientX);
          debugLog("playback.seek.pointer_up_custom", {
            pointer_x: event.clientX,
            next_cursor_seconds: nextCursorSeconds,
            committed_cursor_seconds: committedCursorSeconds,
            scrub_cursor_seconds: scrubCursorSeconds,
          });
          setScrubCursorSeconds(nextCursorSeconds);
          void commitSeek(nextCursorSeconds, { clearScrub: true });
        }}
      >
        <svg className="playbackWidgetOverviewSvg" viewBox={`0 0 ${overviewWidth} ${overviewHeight}`} preserveAspectRatio="none" aria-hidden="true">
          <defs>
            <pattern id={gapPatternId} patternUnits="userSpaceOnUse" width="8" height="8" patternTransform="rotate(45)">
              <rect width="8" height="8" className="playbackWidgetGapPatternBase" />
              <line x1="0" y1="0" x2="0" y2="8" className="playbackWidgetGapPatternLine" />
            </pattern>
          </defs>
          {altitudePath ? <path className="playbackWidgetAltitudeProfile" d={altitudePath} /> : null}
          {speedPath ? <path className="playbackWidgetSpeedProfile" d={speedPath} /> : null}
          {gapRects.map((gap) => (
            <rect
              key={gap.key}
              className="playbackWidgetGapSpan"
              fill={`url(#${gapPatternId})`}
              x={gap.x}
              y={0}
              width={gap.width}
              height={overviewHeight}
            />
          ))}
          <line className="playbackWidgetCursorLine" x1={cursorX} y1={0} x2={cursorX} y2={overviewHeight} />
          <circle className="playbackWidgetCursorKnob" cx={cursorX} cy={overviewHeight - 1} r={knobRadius} />
        </svg>
      </div>
      <div className="playbackWidgetSeekRow">
        <span className="playbackWidgetClock">{playbackUiState.cursor_label}</span>
        <span className="playbackWidgetClock">{playbackUiState.duration_label}</span>
      </div>
    </section>
  );
}

function AltitudePlannerPage(props: {
  page: AppPage;
  planUiState: FlightPlanUiState;
  mostRecentChartOrPlatePage: AppPage;
  onOpenRecentChartOrPlate: () => void;
  onSelectPage: (page: AppPage) => void;
  onQueryAltitudeComparisons: () => Promise<AltitudeComparisonPanelUiView>;
  onPerformAltitudePlannerAction: (actionUid: string) => void | Promise<void>;
  onSetDepartureInput: (field: "time" | "when", input: string) => void | Promise<void>;
  onToggleDepartureTimeBasis: () => void | Promise<void>;
}) {
  const planner = props.planUiState.altitude_planner;
  const [panel, setPanel] = useState<AltitudeComparisonPanelUiView | null>(null);
  const [loading, setLoading] = useState(false);
  const [showUserActionSpinner, setShowUserActionSpinner] = useState(false);
  const [comparisonRefreshRevision, setComparisonRefreshRevision] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [departureTimeInput, setDepartureTimeInput] = useState(planner.departure.time_value);
  const [departureWhenInput, setDepartureWhenInput] = useState(planner.departure.when_value);
  const [openControlId, setOpenControlId] = useState<string | null>(null);
  const departureTimeFocused = useRef(false);
  const departureWhenFocused = useRef(false);
  const suppressDepartureBlurSubmit = useRef(false);
  const userActionPendingRefresh = useRef(false);
  const comparisonRequestGeneration = useRef(0);
  const previousPage = useRef<AppPage | null>(null);
  const { toast: disabledActionToast, show: showDisabledAction } = useDisabledActionToast();
  const comparisonControlKey = planner.controls.map((control) => {
    const selectedOption = control.options?.find((option) => option.selected)?.action_uid ?? "";
    return `${control.id}:${control.label}:${control.action_uid ?? ""}:${selectedOption}`;
  }).join("|");

  const reload = () => {
    const generation = ++comparisonRequestGeneration.current;
    setLoading(true);
    setError(null);
    void props.onQueryAltitudeComparisons()
      .then((nextPanel) => {
        if (comparisonRequestGeneration.current === generation) setPanel(nextPanel);
      })
      .catch((reason: unknown) => {
        if (comparisonRequestGeneration.current === generation) {
          setPanel(null);
          setError(errorMessage(reason));
        }
      })
      .finally(() => {
        if (comparisonRequestGeneration.current === generation) {
          setLoading(false);
          setShowUserActionSpinner(false);
        }
      });
  };

  useEffect(() => {
    const enteredAltitudePlanner = previousPage.current !== "altitude" && props.page === "altitude";
    previousPage.current = props.page;
    if (props.page === "altitude") {
      if (userActionPendingRefresh.current) return;
      if (enteredAltitudePlanner) setShowUserActionSpinner(true);
      reload();
    }
  }, [
    props.page,
    props.planUiState.plan_version,
    planner.estimate_summary.label,
    planner.departure.time_value,
    comparisonControlKey,
    comparisonRefreshRevision,
  ]);

  useEffect(() => {
    if (!departureTimeFocused.current) setDepartureTimeInput(planner.departure.time_value);
    if (!departureWhenFocused.current) setDepartureWhenInput(planner.departure.when_value);
  }, [planner.departure.time_value, planner.departure.when_value]);

  const performAction = (actionUid: string) => {
    setOpenControlId(null);
    setError(null);
    comparisonRequestGeneration.current += 1;
    setLoading(true);
    setShowUserActionSpinner(true);
    userActionPendingRefresh.current = true;
    void Promise.resolve(props.onPerformAltitudePlannerAction(actionUid))
      .then(() => {
        userActionPendingRefresh.current = false;
        setComparisonRefreshRevision((revision) => revision + 1);
      })
      .catch((reason: unknown) => {
        userActionPendingRefresh.current = false;
        setLoading(false);
        setShowUserActionSpinner(false);
        setError(errorMessage(reason));
      });
  };

  const submitDepartureInput = (field: "time" | "when", input: string) => {
    setError(null);
    comparisonRequestGeneration.current += 1;
    setLoading(true);
    setShowUserActionSpinner(true);
    userActionPendingRefresh.current = true;
    void Promise.resolve(props.onSetDepartureInput(field, input))
      .then(() => {
        userActionPendingRefresh.current = false;
        setComparisonRefreshRevision((revision) => revision + 1);
      })
      .catch((reason: unknown) => {
        userActionPendingRefresh.current = false;
        setLoading(false);
        setShowUserActionSpinner(false);
        setDepartureTimeInput(planner.departure.time_value);
        setDepartureWhenInput(planner.departure.when_value);
        setError(errorMessage(reason));
      });
  };

  const toggleDepartureTimeBasis = () => {
    setError(null);
    comparisonRequestGeneration.current += 1;
    setLoading(true);
    setShowUserActionSpinner(true);
    userActionPendingRefresh.current = true;
    void (async () => {
      try {
        if (departureTimeInput !== planner.departure.time_value) {
          await props.onSetDepartureInput("time", departureTimeInput);
        }
        if (departureWhenInput !== planner.departure.when_value) {
          await props.onSetDepartureInput("when", departureWhenInput);
        }
        await props.onToggleDepartureTimeBasis();
        userActionPendingRefresh.current = false;
        setComparisonRefreshRevision((revision) => revision + 1);
      } catch (reason: unknown) {
        userActionPendingRefresh.current = false;
        setLoading(false);
        setShowUserActionSpinner(false);
        setDepartureTimeInput(planner.departure.time_value);
        setDepartureWhenInput(planner.departure.when_value);
        setError(errorMessage(reason));
      } finally {
        suppressDepartureBlurSubmit.current = false;
      }
    })();
  };

  const interactionEnabled = !loading;

  return (
    <section className="appPage altitudePlannerPage" data-testid="parity:page:altitude_planner">
      {openControlId ? (
        <TrayScrim ariaLabel="Close altitude planner menu" onClose={() => setOpenControlId(null)} />
      ) : null}
      <header className="altitudePlannerPageHeader">
        <h1>{planner.title}</h1>
        <div className="altitudePlannerControls" data-testid="altitude-planner-control-tray">
          {planner.controls.map((control) => {
            const disabledReason = disabledReasonText(control.disabled_reason);
            const enabled = control.enabled && interactionEnabled;
            const hasOptions = (control.options?.length ?? 0) > 0;
            const open = openControlId === control.id;
            if (hasOptions) {
              return (
                <TrayDock
                  key={control.id}
                  launcherLabel={control.label}
                  open={open}
                  onToggle={() => setOpenControlId(open ? null : control.id)}
                  ariaLabel={`${control.label.replace(/\n/g, " ")} options`}
                  disabled={!enabled}
                  disabledReason={interactionEnabled ? disabledReason : "Calculation in progress."}
                  onDisabledAction={showDisabledAction}
                  style="wide"
                  launcherClassName="altitudePlannerButton"
                  testId={`altitude-planner-control-${control.id}`}
                  options={(control.options ?? []).map((option) => ({
                    id: option.action_uid,
                    label: option.label,
                    active: option.selected,
                    aircraftSymbol: option.trailing_symbol,
                    onSelect: () => performAction(option.action_uid),
                  }))}
                />
              );
            }
            return (
              <button
                key={control.id}
                type="button"
                className={`trayButton altitudePlannerButton${enabled ? "" : " isDisabled"}${open ? " isChecked" : ""}`}
                data-testid={`altitude-planner-control-${control.id}`}
                aria-disabled={enabled ? undefined : "true"}
                title={interactionEnabled ? disabledReason ?? undefined : "Calculation in progress."}
                onClick={() => {
                  if (enabled && control.action_uid) {
                    performAction(control.action_uid);
                  } else {
                    showDisabledAction(interactionEnabled ? disabledReason ?? "Action unavailable." : "Calculation in progress.");
                  }
                }}
              >
                {control.label}
              </button>
            );
          })}
          <section
            className={`altitudePlannerDeparture${planner.departure.enabled ? "" : " isDisabled"}`}
            aria-label={planner.departure.title}
            title={planner.departure.disabled_reason ?? undefined}
          >
            <h2>{planner.departure.title}</h2>
            <label>
              {planner.departure.time_label ? <span>{planner.departure.time_label}</span> : null}
              <input
                data-testid="altitude-planner-departure-time"
                type="text"
                inputMode="text"
                value={departureTimeInput}
                disabled={!planner.departure.enabled || !interactionEnabled}
                aria-label="Departure time"
                onChange={(event) => setDepartureTimeInput(event.currentTarget.value)}
                onFocus={() => { departureTimeFocused.current = true; }}
                onBlur={() => {
                  departureTimeFocused.current = false;
                  if (suppressDepartureBlurSubmit.current) return;
                  if (departureTimeInput !== planner.departure.time_value) {
                    submitDepartureInput("time", departureTimeInput);
                  }
                }}
                onKeyDown={(event) => {
                  if (event.key === "Enter") event.currentTarget.blur();
                }}
              />
              <button
                type="button"
                className="trayButton altitudePlannerDepartureBasis"
                data-testid="altitude-planner-departure-basis"
                disabled={!planner.departure.enabled || !interactionEnabled}
                onMouseDown={() => { suppressDepartureBlurSubmit.current = true; }}
                onClick={toggleDepartureTimeBasis}
              >
                {planner.departure.basis_label}
              </button>
            </label>
            <label>
              <span>{planner.departure.when_label}</span>
              <input
                data-testid="altitude-planner-departure-when"
                className={`altitudePlannerDepartureWhen${planner.departure.when_is_past ? " isWarning" : ""}`}
                type="text"
                inputMode="text"
                value={departureWhenInput}
                disabled={!planner.departure.enabled || !interactionEnabled}
                aria-label="Departure offset"
                onChange={(event) => setDepartureWhenInput(event.currentTarget.value)}
                onFocus={() => { departureWhenFocused.current = true; }}
                onBlur={() => {
                  departureWhenFocused.current = false;
                  if (suppressDepartureBlurSubmit.current) return;
                  if (departureWhenInput !== planner.departure.when_value) {
                    submitDepartureInput("when", departureWhenInput);
                  }
                }}
                onKeyDown={(event) => {
                  if (event.key === "Enter") event.currentTarget.blur();
                }}
              />
              <span>{planner.departure.when_suffix}</span>
            </label>
          </section>
        </div>
      </header>

      <div className="altitudePlannerPageBody">
        {planner.forecast ? (
          <div className="altitudePlannerProvenance" data-testid="altitude-planner-forecast">
            {planner.forecast.rows.map((row) => {
              const action = row.action;
              return (
                <div
                  className={`altitudePlannerWindModelRow${row.selected ? " isSelected" : ""}`}
                  data-testid={`altitude-planner-wind-row-${row.id}`}
                  key={row.id}
                >
                  <strong>{row.label}</strong>
                  <span>{row.description}</span>
                  <div className="altitudePlannerWindModelActionSlot">
                    {action ? (
                      <button
                        type="button"
                        className={`trayButton altitudePlannerForecastAction${row.selected ? " isActive selectedControlHighlight" : ""}${interactionEnabled && (action.enabled || row.selected) ? "" : " isDisabled"}`}
                        data-testid={`altitude-planner-wind-action-${row.id}`}
                        aria-pressed={row.selected}
                        aria-disabled={interactionEnabled && (action.enabled || row.selected) ? undefined : "true"}
                        title={interactionEnabled ? action.disabled_reason ?? undefined : "Calculation in progress."}
                        onClick={() => {
                          if (interactionEnabled && action.enabled && action.action_uid) {
                            performAction(action.action_uid);
                          } else if (!interactionEnabled) {
                            showDisabledAction("Calculation in progress.");
                          } else if (!row.selected && action.disabled_reason) {
                            showDisabledAction(action.disabled_reason);
                          }
                        }}
                      >
                        {action.label}
                      </button>
                    ) : null}
                  </div>
                </div>
              );
            })}
          </div>
        ) : null}
        {(planner.unavailable_reasons ?? []).length > 0 ? (
          <div className="altitudePlannerReasons" data-testid="altitude-planner-status">
            {(planner.unavailable_reasons ?? []).map((reason) => (
              <p key={reason.code}>{reason.message}</p>
            ))}
          </div>
        ) : null}
        {error ? <p className="altitudePlannerError">{error}</p> : null}
        <div className="altitudeComparisonRegion" aria-busy={loading}>
          {(panel?.advisories ?? []).length > 0 ? (
            <div className="altitudePlannerAdvisories">
              {(panel?.advisories ?? []).map((message) => <p key={message}>{message}</p>)}
            </div>
          ) : null}
          {panel ? (
            <div className="altitudeComparisonTable" data-testid="altitude-comparison-panel">
              <div className="altitudeComparisonHeader">
                {panel.columns.map((column) => <span key={column.id}>{column.label}</span>)}
              </div>
              {panel.rows.map((row, index) => (
                <button
                  key={row.action_uid ?? `disabled-${index}`}
                  type="button"
                  className={`altitudeComparisonRow${row.selected ? " isSelected" : ""}${row.enabled ? "" : " isDisabled"}`}
                  data-testid={`altitude-comparison-row-${index}`}
                  aria-selected={row.selected ? "true" : "false"}
                  aria-disabled={interactionEnabled && row.enabled ? undefined : "true"}
                  title={interactionEnabled ? row.disabled_reason ?? undefined : "Calculation in progress."}
                  onClick={() => {
                    if (interactionEnabled && row.enabled && row.action_uid) {
                      performAction(row.action_uid);
                    } else if (!interactionEnabled) {
                      showDisabledAction("Calculation in progress.");
                    } else if (row.disabled_reason) {
                      showDisabledAction(row.disabled_reason);
                    }
                  }}
                >
                  {row.cells.map((cell) => <span key={cell.id}>{cell.value ?? "—"}</span>)}
                </button>
              ))}
            </div>
          ) : null}
          {loading && showUserActionSpinner ? (
            <div
              className="altitudeComparisonLoading"
              data-testid="altitude-comparison-loading"
              role="status"
              aria-live="polite"
            >
              <span className="altitudeComparisonSpinner" aria-hidden="true" />
              <span>Calculating…</span>
            </div>
          ) : null}
        </div>
      </div>

      <PrimaryNavigationDock
        page={props.page}
        navElement={props.planUiState.guidance?.nav_element}
        chartPlateTargetPage={props.mostRecentChartOrPlatePage}
        onSelectPage={props.onSelectPage}
        onOpenPlan={() => props.onSelectPage("plan")}
        onOpenChartOrPlate={props.onOpenRecentChartOrPlate}
      />
      {disabledActionToast ? (
        <div className="mapSelectionToast" data-testid="disabled-action-toast" role="status" aria-live="polite">
          {disabledActionToast.message}
        </div>
      ) : null}
    </section>
  );
}

function FlightPlanPage(props: {
  appCoreAdapter: AppCoreAdapter | null;
  uiSession: UiSession | null;
  page: AppPage;
  pageHistory: AppViewSnapshot[];
  planUiState: FlightPlanUiState | null;
  mostRecentChartOrPlatePage: AppPage;
  onOpenRecentChartOrPlate: () => void;
  onSelectPage: (page: AppPage) => void;
  onOpenWeatherDetail: (detail: WeatherDetailUiView) => void;
  onOpenCharts: (airportId: string | null, chartId?: string | null) => void;
  onInsertAirportWaypointAtRow: (rowUid: string, before: boolean, airportId: string) => void | Promise<void>;
  onPreviewFlightPlanEntry: (input: string) => Promise<FlightPlanEntryPreview>;
  onAppendFlightPlanEntry: (input: string) => void | Promise<void>;
  onPerformFlightPlanControl: (controlId: FlightPlanControlId) => void | Promise<void>;
  onPerformFlightPlanRowAction: (rowUid: string, actionUid: string) => void | Promise<void>;
  onInsertAirwayAtRow: (
    rowUid: string,
    entryPointUid: string,
    exitPointUid: string,
    presentation: AirwayPresentationPlan,
  ) => void | Promise<void>;
  onSelectProcedureAtRow: (rowUid: string, airportId: string, procedureId: string, kind: ProcedureKind, runwayTransition: string | null, enrouteTransition: string | null) => void | Promise<void>;
  onFlightPlanColumnAction: (actionId: string) => Promise<void>;
  onTimeDisplayAction: (actionId: string) => Promise<void>;
}) {
  const [selectedWaypointUid, setSelectedWaypointUid] = useState<string | null>(null);
  const [selectedWaypointAnchor, setSelectedWaypointAnchor] = useState<{ top: number; height: number } | null>(null);
  const [flightPlanAirportInfoModal, setFlightPlanAirportInfoModal] = useState<{
    airportId: string;
    detail: AirportInfoUiView | null;
    error: string | null;
  } | null>(null);
  const [airwayPicker, setAirwayPicker] = useState<{
    loading: boolean;
    error: string | null;
    mode: "insert";
    rowUid: string | null;
    header: string;
    originAnchor: NavRef;
    destinationAnchor: NavRef | null;
    suggestions: AirwaySuggestion[];
    selectedAirwayName: string | null;
    presentation: AirwayPresentationPlan | null;
    selectedEntryUid: string | null;
  } | null>(null);
  const [procedurePicker, setProcedurePicker] = useState<{
    loading: boolean;
    error: string | null;
    rowUid: string;
    airportId: string;
    kind: ProcedureKind;
    title: string;
    emptyMessage: string;
    procedures: ProcedureSummary[];
    selectedProcedureId: string | null;
    options: ProcedureOptions | null;
  } | null>(null);
  const [airportInsert, setAirportInsert] = useState<{
    rowUid: string;
    before: boolean;
    airportId: string;
    error: string | null;
    loading: boolean;
    suggestions: WaypointIdentifierSuggestion[];
  } | null>(null);
  const [routeEntryText, setRouteEntryText] = useState("");
  const [routeEntryPreview, setRouteEntryPreview] = useState<FlightPlanEntryPreview>({
    can_commit: false,
    tokens: [],
    issues: [],
  });
  const [routeEntryLoading, setRouteEntryLoading] = useState(false);
  const [routeEntryError, setRouteEntryError] = useState<string | null>(null);
  const [routeEntrySubmitting, setRouteEntrySubmitting] = useState(false);
  const { toast: disabledActionToast, show: showDisabledAction } = useDisabledActionToast();
  const previewFlightPlanEntryRef = useRef(props.onPreviewFlightPlanEntry);
  useEffect(() => {
    previewFlightPlanEntryRef.current = props.onPreviewFlightPlanEntry;
  }, [props.onPreviewFlightPlanEntry]);
  const pageRef = useRef<HTMLElement | null>(null);
  const planScrollSurfaceRef = useRef<HTMLDivElement | null>(null);
  const waypointModalRef = useRef<HTMLElement | null>(null);
  const planControlsRef = useRef<HTMLDivElement | null>(null);
  const planUiState = props.planUiState;
  if (!planUiState) {
    throw new Error("FlightPlanPage requires core-projected FlightPlanUiState");
  }
  const guidance = planUiState.guidance ?? null;
  const planControls = planUiState.controls;
  useEffect(() => {
    if (props.page !== "plan") {
      return;
    }
    const handleHistoryKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented || isEditableTarget(event.target)) {
        return;
      }
      const controlId = flightPlanHistoryControlForKey(event);
      if (!controlId) {
        return;
      }
      event.preventDefault();
      if (!planControls.some((control) => control.id === controlId && control.enabled)) {
        return;
      }
      void props.onPerformFlightPlanControl(controlId);
    };
    window.addEventListener("keydown", handleHistoryKeyDown);
    return () => window.removeEventListener("keydown", handleHistoryKeyDown);
  }, [planControls, props.onPerformFlightPlanControl, props.page]);
  const activeFromRowUid = guidance?.active_from_row_uid ?? null;
  const activeToRowUid = guidance?.active_to_row_uid ?? null;
  const activeGuidanceRowsKey = guidance?.active_leg
    ? `${activeFromRowUid ?? ""}->${activeToRowUid ?? ""}`
    : null;
  const structuredSurfaceRef = useRef<HTMLDivElement | null>(null);
  const structuredTableRef = useRef<HTMLDivElement | null>(null);
  const planScrollViewportRef = useRef<HTMLDivElement | null>(null);
  const planScrollContentRef = useRef<HTMLDivElement | null>(null);
  const structuredRowRefs = useRef(new Map<string, HTMLElement>());
  const [structuredArrow, setStructuredArrow] = useState<{ path: string; head: string } | null>(null);
  const [structuredGroupBoxes, setStructuredGroupBoxes] = useState<Array<{ key: string; top: number; left: number; width: number; height: number }>>([]);
  const [waypointModalTop, setWaypointModalTop] = useState<number | null>(null);
  const [waypointModalMaxHeight, setWaypointModalMaxHeight] = useState<number | null>(null);
  const waypointSuggestionPlanKey = `${planUiState.plan_id}:${planUiState.plan_version}`;
  useEffect(() => {
    const editor = airportInsert;
    if (!editor) {
      return;
    }
    if (!props.uiSession) {
      setAirportInsert((current) => current ? {
        ...current,
        loading: false,
        suggestions: [],
        error: "core session unavailable",
      } : current);
      return;
    }
    const query = editor.airportId.trim().toUpperCase();
    if (!query) {
      setAirportInsert((current) => current ? { ...current, loading: false, suggestions: [] } : current);
      return;
    }
    let cancelled = false;
    setAirportInsert((current) => current ? { ...current, loading: true } : current);
    props.uiSession
      .suggestWaypointIdentifiersAtFlightPlanRow(editor.rowUid, editor.before, query, 8)
      .then((suggestions) => {
        if (!cancelled) {
          setAirportInsert((current) => current ? { ...current, loading: false, suggestions } : current);
        }
      })
      .catch((error) => {
        if (!cancelled) {
          setAirportInsert((current) => current ? {
            ...current,
            loading: false,
            suggestions: [],
            error: error instanceof Error ? error.message : String(error),
          } : current);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [airportInsert?.airportId, airportInsert?.before, airportInsert?.rowUid, props.uiSession, waypointSuggestionPlanKey]);
  useEffect(() => {
    if (!routeEntryText.trim()) {
      setRouteEntryPreview({
        can_commit: false,
        tokens: [],
        issues: [],
      });
      setRouteEntryLoading(false);
      setRouteEntryError(null);
      return;
    }
    let cancelled = false;
    setRouteEntryLoading(true);
    previewFlightPlanEntryRef.current(routeEntryText)
      .then((preview) => {
        if (!cancelled) {
          setRouteEntryPreview(preview);
          setRouteEntryLoading(false);
          setRouteEntryError(null);
        }
      })
      .catch((error) => {
        if (!cancelled) {
          setRouteEntryLoading(false);
          setRouteEntryError(errorMessage(error));
        }
      });
    return () => {
      cancelled = true;
    };
  }, [routeEntryText, waypointSuggestionPlanKey]);
  const displayRows = useMemo(() => {
    return planUiState.display_rows.map((row) => ({
        showPlateTargetId:
          typeof (row as { show_plate_target_id?: unknown }).show_plate_target_id === "string"
            ? (row as { show_plate_target_id?: string | null }).show_plate_target_id ?? null
            : null,
        id:
          row.row_kind === "group"
            ? row.uid
            : row.row_kind === "discontinuity"
              ? row.uid
              : row.depth === 0
                ? row.uid
                : row.uid,
        rowUid: row.uid,
        label: row.label,
        dataCells: row.data_cells,
        active: row.active,
        enabled: row.enabled ?? true,
        disabledReason: row.disabled_reason ?? null,
        syntheticDirectTo: row.synthetic_direct_to ?? false,
        depth: row.depth,
        rowKind: row.row_kind,
        refKey:
          row.row_kind === "group"
            ? row.uid
            : row.row_kind === "discontinuity"
              ? row.uid
              : row.depth === 0
                ? row.uid
                : row.uid,
        chartAirportId: row.chart_airport_id,
        removeLegIndex: null as number | null,
        originAnchor: row.origin_anchor,
        destinationAnchor: row.destination_anchor,
        navRef: row.nav_ref,
        symbolFeature: row.symbol_feature,
        weatherBadge: row.weather_badge ?? null,
        groupKey: row.row_kind === "group" || row.depth > 0 ? `group:${row.component_uid!}` : null,
        componentKind: row.component_kind,
        procedureId: row.procedure_id,
        procedureKind: row.procedure_kind,
        precedingWaypoint: row.preceding_waypoint,
        followingWaypoint: row.following_waypoint,
        actionMatrix: row.action_matrix ?? [],
      }));
  }, [planUiState.display_rows]);
  const planDataColumns = planUiState.data_columns;
  const selectedRow = selectedWaypointUid === null
    ? null
    : displayRows.find((row) => row.rowUid === selectedWaypointUid) ?? null;

  const rowActionRows = useMemo(() => {
    if (!selectedRow) {
    return [] as Array<Array<{ id: string; uid: string; menuColumn: number; label: string; enabled: boolean; disabledReason?: string | null; onSelect: () => void }>>;
    }

    const closeTray = () => {
      setSelectedWaypointUid(null);
      setAirwayPicker(null);
      setProcedurePicker(null);
      setAirportInsert(null);
    };

    const performSelectedRowAction = async (actionUid: string) => {
      const uiSession = props.uiSession;
      if (!uiSession) {
        return;
      }
      const decision = await uiSession.flightPlanRowActionDecision(
        selectedRow.rowUid,
        actionUid,
      );
      if (decision.perform_session_mutation) {
        await props.onPerformFlightPlanRowAction(selectedRow.rowUid, actionUid);
      }
      const effect = decision.effect;
      if (effect?.kind === "show_weather") {
        props.onOpenWeatherDetail(effect.detail);
      } else if (effect?.kind === "load_airport_info") {
        const airportId = effect.airport_id;
        setFlightPlanAirportInfoModal({ airportId, detail: null, error: null });
        void uiSession.airportInfo(airportId).then((detail) => {
          setFlightPlanAirportInfoModal((current) =>
            current?.airportId === airportId ? { ...current, detail } : current);
        }).catch((error) => {
          setFlightPlanAirportInfoModal((current) =>
            current?.airportId === airportId
              ? { ...current, error: errorMessage(error) }
              : current);
        });
      } else if (effect?.kind === "open_airport_charts") {
        props.onOpenCharts(effect.airport_id);
      } else if (effect?.kind === "open_plate_target") {
        props.onOpenCharts(effect.airport_id, effect.target);
      } else if (effect?.kind === "open_waypoint_insert") {
        setAirportInsert({
          rowUid: effect.row_uid,
          before: effect.before,
          airportId: "",
          error: null,
          loading: false,
          suggestions: [],
        });
      } else if (effect?.kind === "open_airway_picker") {
        const adapter = props.appCoreAdapter;
        if (!adapter) {
          return;
        }
        setAirwayPicker({
          loading: true,
          error: null,
          mode: "insert",
          rowUid: effect.row_uid,
          header: effect.header,
          originAnchor: effect.origin_anchor,
          destinationAnchor: effect.destination_anchor ?? null,
          suggestions: [],
          selectedAirwayName: null,
          presentation: null,
          selectedEntryUid: null,
        });
        window.requestAnimationFrame(() => {
          void adapter.suggestAirwaysNearAnchor(effect.origin_anchor).then((suggestions) => {
            setAirwayPicker((current) => current ? {
              ...current,
              loading: false,
              suggestions,
            } : current);
          }).catch((error) => {
            setAirwayPicker((current) => current ? {
              ...current,
              loading: false,
              error: error instanceof Error ? error.message : String(error),
            } : current);
          });
        });
      } else if (effect?.kind === "open_procedure_picker") {
        const adapter = props.appCoreAdapter;
        if (!adapter) {
          return;
        }
        const trace = {
          row_uid: effect.row_uid,
          airport_id: effect.airport_id,
          procedure_kind: effect.procedure_kind,
        };
        debugLog("plan.procedure_picker.open.start", trace);
        setProcedurePicker({
          loading: true,
          error: null,
          rowUid: effect.row_uid,
          airportId: effect.airport_id,
          kind: effect.procedure_kind,
          title: effect.title,
          emptyMessage: effect.empty_message,
          procedures: [],
          selectedProcedureId: null,
          options: null,
        });
        window.requestAnimationFrame(() => {
          void debugTiming(
            "plan.procedure_picker.list_procedures",
            () => adapter.listProcedures(effect.airport_id, effect.procedure_kind),
            trace,
          ).then((procedures) => {
            debugLog("plan.procedure_picker.open.done", { ...trace, procedure_count: procedures.length });
            setProcedurePicker((current) => current ? {
              ...current,
              loading: false,
              procedures,
            } : current);
          }).catch((error) => {
            debugLog("plan.procedure_picker.open.error", { ...trace, message: errorMessage(error) });
            setProcedurePicker((current) => current ? {
              ...current,
              loading: false,
              error: error instanceof Error ? error.message : String(error),
            } : current);
          });
        });
      }
      if (decision.dismiss_tray) {
        closeTray();
      }
    };

    const actionForUi = (action: { id: string; uid: string; menu_column?: number; label: string; enabled: boolean; disabled_reason?: string | null }) => {
      return {
        id: action.id,
        uid: action.uid,
        menuColumn: action.menu_column ?? 0,
        label: action.label,
        enabled: action.enabled,
        disabledReason: action.disabled_reason ?? null,
        onSelect: () => {
          if (!action.enabled) {
            return;
          }
          void performSelectedRowAction(action.uid).catch((error) => {
            debugLog("plan.row_action.failed", {
              row_uid: selectedRow.rowUid,
              action_uid: action.uid,
              message: errorMessage(error),
            });
          });
        },
      };
    };
    return selectedRow.actionMatrix.map((row) => row.map(actionForUi));
  }, [props, selectedRow]);
  const rowActions = useMemo(() => rowActionRows.flat(), [rowActionRows]);

  useEffect(() => {
    const surface = structuredSurfaceRef.current;
    const table = structuredTableRef.current;
    if (!surface || !table) {
      setStructuredGroupBoxes([]);
      return;
    }

    let animationFrame = 0;
    let settleTimer = 0;

    const measureGroupBoxes = () => {
      const surfaceRect = surface.getBoundingClientRect();
      const tableRect = table.getBoundingClientRect();
      const computedStyle = window.getComputedStyle(table);
      const rowGap = Number.parseFloat(computedStyle.rowGap || computedStyle.gap || "0") || 0;
      const columnGap = Number.parseFloat(computedStyle.columnGap || computedStyle.gap || "0") || 0;
      const verticalInset = rowGap * 0.6;
      const horizontalInset = columnGap * 0.6;
      const orderedGroupKeys = displayRows
        .filter((row) => row.rowKind === "group" && row.groupKey)
        .map((row) => row.groupKey as string);

      const nextBoxes = orderedGroupKeys.flatMap((groupKey) => {
        const groupRows = displayRows.filter((row) => row.groupKey === groupKey);
        const firstRow = groupRows[0];
        const lastRow = groupRows[groupRows.length - 1];
        if (!firstRow || !lastRow) {
          return [];
        }
        const firstElement = structuredRowRefs.current.get(firstRow.refKey);
        const lastElement = structuredRowRefs.current.get(lastRow.refKey);
        if (!firstElement || !lastElement) {
          return [];
        }
        const firstRect = firstElement.getBoundingClientRect();
        const lastRect = lastElement.getBoundingClientRect();
        return [{
          key: groupKey,
          left: tableRect.left - surfaceRect.left - horizontalInset,
          width: tableRect.width + horizontalInset * 2,
          top: firstRect.top - surfaceRect.top - verticalInset,
          height: lastRect.bottom - firstRect.top + verticalInset * 2,
        }];
      });
      setStructuredGroupBoxes(nextBoxes);
    };

    const scheduleMeasure = () => {
      window.cancelAnimationFrame(animationFrame);
      animationFrame = window.requestAnimationFrame(measureGroupBoxes);
    };

    const handleTransitionEnd = () => {
      scheduleMeasure();
    };

    scheduleMeasure();
    settleTimer = window.setTimeout(measureGroupBoxes, 220);
    table.addEventListener("transitionend", handleTransitionEnd);

    return () => {
      window.cancelAnimationFrame(animationFrame);
      window.clearTimeout(settleTimer);
      table.removeEventListener("transitionend", handleTransitionEnd);
    };
  }, [displayRows]);

  useEffect(() => {
    if (!activeGuidanceRowsKey) {
      setStructuredArrow(null);
      return;
    }
    const scrollPane = planScrollViewportRef.current;
    const content = planScrollContentRef.current;
    if (!scrollPane || !content) {
      setStructuredArrow(null);
      return;
    }

    const fromIndex = activeFromRowUid
      ? displayRows.findIndex((row) => row.rowUid === activeFromRowUid)
      : -1;
    const toIndex = activeToRowUid
      ? displayRows.findIndex((row) => row.rowUid === activeToRowUid)
      : -1;
    if (toIndex < 0) {
      setStructuredArrow(null);
      return;
    }

    const toElement = structuredRowRefs.current.get(displayRows[toIndex]?.refKey ?? "");
    const fromElement = fromIndex >= 0 ? structuredRowRefs.current.get(displayRows[fromIndex]?.refKey ?? "") : null;
    if (!toElement || (fromIndex >= 0 && !fromElement)) {
      setStructuredArrow(null);
      return;
    }

    let animationFrame = 0;
    const measureArrow = () => {
      const surfaceRect = content.getBoundingClientRect();
      const toRect = toElement.getBoundingClientRect();
      const leftGutterX = thumbPixels(0.12);
      const waypointColumnLeftX = thumbPixels(0.5);
      const waypointColumnHeadInsetX = thumbPixels(0.08);
      const toPoint = {
        x: waypointColumnLeftX + waypointColumnHeadInsetX,
        y: toRect.top - surfaceRect.top + toRect.height / 2,
      };
      const elbowX = leftGutterX;
      const headLength = 20;
      const shaftEnd = { x: Math.max(elbowX, toPoint.x - headLength + 5), y: toPoint.y };
      if (!fromElement) {
        const stubStart = { x: elbowX, y: toPoint.y };
        setStructuredArrow({
          path: `M ${stubStart.x} ${stubStart.y} H ${shaftEnd.x}`,
          head: arrowHeadPoints(shaftEnd, toPoint),
        });
        return;
      }

      const fromRect = fromElement.getBoundingClientRect();
      const fromPoint = {
        x: waypointColumnLeftX,
        y: fromRect.top - surfaceRect.top + fromRect.height / 2,
      };

      setStructuredArrow({
        path: `M ${fromPoint.x} ${fromPoint.y} H ${elbowX} V ${toPoint.y} H ${shaftEnd.x}`,
        head: arrowHeadPoints(shaftEnd, toPoint),
      });
    };
    const scheduleMeasure = () => {
      window.cancelAnimationFrame(animationFrame);
      animationFrame = window.requestAnimationFrame(measureArrow);
    };

    measureArrow();
    scrollPane?.addEventListener("scroll", scheduleMeasure, { passive: true });
    window.addEventListener("resize", scheduleMeasure);
    return () => {
      window.cancelAnimationFrame(animationFrame);
      scrollPane?.removeEventListener("scroll", scheduleMeasure);
      window.removeEventListener("resize", scheduleMeasure);
    };
  }, [activeFromRowUid, activeGuidanceRowsKey, activeToRowUid, displayRows]);

  useEffect(() => {
    if (!activeGuidanceRowsKey || !activeToRowUid) {
      return;
    }
    const fromElement = activeFromRowUid
      ? structuredRowRefs.current.get(
          displayRows.find((row) => row.rowUid === activeFromRowUid)?.refKey ?? "",
        )
      : null;
    const toElement = structuredRowRefs.current.get(
      displayRows.find((row) => row.rowUid === activeToRowUid)?.refKey ?? "",
    );
    if (!toElement) {
      return;
    }

    const handle = window.requestAnimationFrame(() => {
      fromElement?.scrollIntoView({ block: "nearest", inline: "nearest" });
      toElement.scrollIntoView({ block: "nearest", inline: "nearest" });
    });
    return () => {
      window.cancelAnimationFrame(handle);
    };
  }, [activeGuidanceRowsKey]);

  useEffect(() => {
    if (selectedRow === null) {
      setWaypointModalTop(null);
      setWaypointModalMaxHeight(null);
      return;
    }
    const page = pageRef.current;
    const modal = waypointModalRef.current;
    if (!page || !modal) {
      return;
    }
    const pageRect = page.getBoundingClientRect();
    const topPadding = thumbPixels(0.5);
    const bottomPadding = thumbPixels(0.1);
    const blockers = [planControlsRef.current]
      .flatMap((element) => (element ? [element.getBoundingClientRect().top - pageRect.top] : []));
    const bottomLimit = blockers.length > 0 ? Math.min(...blockers) : page.clientHeight;
    const maxHeight = Math.max(thumbPixels(1), bottomLimit - topPadding - bottomPadding);
    const modalHeight = Math.min(modal.scrollHeight || modal.getBoundingClientRect().height, maxHeight);
    const anchorCenter = selectedWaypointAnchor
      ? selectedWaypointAnchor.top + selectedWaypointAnchor.height / 2
      : topPadding + modalHeight / 2;
    const centeredTop = anchorCenter - modalHeight / 2;
    const maxTop = Math.max(topPadding, bottomLimit - bottomPadding - modalHeight);
    const top = Math.min(Math.max(centeredTop, topPadding), maxTop);

    setWaypointModalTop(top);
    setWaypointModalMaxHeight(maxHeight);
  }, [airwayPicker, selectedRow, selectedWaypointAnchor, rowActions.length]);

  useEffect(() => {
    if (!airwayPicker || airwayPicker.loading) {
      return;
    }
    if (!airwayPicker.presentation || airwayPicker.selectedAirwayName === null) {
      return;
    }
    const modal = waypointModalRef.current;
    if (!modal) {
      return;
    }
    const handle = window.requestAnimationFrame(() => {
      const suggested = modal.querySelector<HTMLButtonElement>(".airwayChoiceButton.isSuggested");
      suggested?.scrollIntoView({ block: "center", inline: "nearest" });
    });
    return () => window.cancelAnimationFrame(handle);
  }, [
    airwayPicker?.loading,
    airwayPicker?.presentation,
    airwayPicker?.selectedAirwayName,
    airwayPicker?.selectedEntryUid,
  ]);

  return (
    <section className="appPage planPage" ref={pageRef} data-testid="parity:page:flight_plan">
      <span
        data-testid={`parity:plan-state:rows:${planUiState.display_rows.length}:active:${planUiState.display_rows.filter((row) => row.active).map((row) => row.uid).join(",") || "none"}:from:${guidance?.active_from_row_uid ?? "none"}:to:${guidance?.active_to_row_uid ?? "none"}`}
        aria-hidden="true"
      />
      <div className="planScrollViewport" ref={planScrollViewportRef}>
        <div className="planScrollContent" ref={planScrollContentRef}>
          {structuredArrow ? (
            <svg className="planStructuredArrowLayer" aria-hidden="true">
              <path className="planStructuredArrowPath" d={structuredArrow.path} />
              <polygon className="planStructuredArrowHead" points={structuredArrow.head} />
            </svg>
          ) : null}
          <div className="planScrollSurface" ref={planScrollSurfaceRef}>
            <div className="planTableWrap isStructured" ref={structuredSurfaceRef}>
              <div className="planStructuredGroupBoxLayer" aria-hidden="true">
                {structuredGroupBoxes.map((box) => (
                  <div
                    key={box.key}
                    className="planStructuredGroupBoxOverlay"
                    style={{ top: `${box.top}px`, left: `${box.left}px`, width: `${box.width}px`, height: `${box.height}px` }}
                  />
                ))}
              </div>
              <div className="planTable" ref={structuredTableRef}>
                <div className="planHeader planWaypointCell">Waypoint</div>
                {planDataColumns.map((column) => (
                  <div
                    key={column.id}
                    data-testid={`parity:plan-column:${column.id}`}
                    data-e2e-state={column.label}
                    className={`planHeader${column.action_id ? " isActionable" : ""}`}
                    role={column.action_id ? "button" : undefined}
                    tabIndex={column.action_id ? 0 : undefined}
                    onClick={column.action_id ? () => void props.onFlightPlanColumnAction(column.action_id!) : undefined}
                    onKeyDown={column.action_id ? (event) => {
                      if (event.key === "Enter" || event.key === " ") {
                        event.preventDefault();
                        void props.onFlightPlanColumnAction(column.action_id!);
                      }
                    } : undefined}
                  >
                    {column.label}
                  </div>
                ))}
                {displayRows.map((row) => {
                  const procedureGroupCell = row.rowKind === "group" && row.componentKind === "procedure";
                  return (
                    <Fragment key={row.id}>
                      {row.rowKind === "summary" ? (
                        <div
                          key={`${row.id}:waypoint`}
                          data-testid={`plan-row-${row.rowUid}`}
                          ref={(node) => {
                            if (row.refKey === null) {
                              return;
                            }
                            if (node) {
                              structuredRowRefs.current.set(row.refKey, node);
                            } else {
                              structuredRowRefs.current.delete(row.refKey);
                            }
                          }}
                          className="planWaypointCell planStructuredWaypointCell planSummaryCell"
                        >
                          {row.label}
                        </div>
                      ) : (
                        <button
                          key={`${row.id}:waypoint`}
                          type="button"
                          data-testid={`plan-row-${row.rowUid}`}
                          ref={(node) => {
                            if (row.refKey === null) {
                              return;
                            }
                            if (node) {
                              structuredRowRefs.current.set(row.refKey, node);
                            } else {
                              structuredRowRefs.current.delete(row.refKey);
                            }
                          }}
                          className={[
                            "planWaypointCell",
                            "planWaypointButton",
                            selectedWaypointUid === row.rowUid ? "isSelected" : "",
                            row.active ? "isActiveLeg" : "",
                            !row.enabled ? "isDisabled" : "",
                            row.syntheticDirectTo ? "isSyntheticDirectTo" : "",
                            "planStructuredWaypointCell",
                            row.rowKind === "group" ? "isGroupHeader" : "",
                            procedureGroupCell ? "isProcedureCell" : "",
                            row.depth > 0 ? "isChildRow" : "",
                            row.rowKind === "discontinuity" ? "isDiscontinuityItem" : "",
                          ].filter(Boolean).join(" ")}
                          title={disabledReasonText(row.disabledReason) ?? undefined}
                          aria-disabled={!row.enabled && !row.syntheticDirectTo ? "true" : undefined}
                          onClick={(event) => {
                            if (!row.enabled && !row.syntheticDirectTo) {
                              const reason = disabledReasonText(row.disabledReason);
                              if (reason) {
                                showDisabledAction(reason);
                              }
                              return;
                            }
                            const page = pageRef.current;
                            if (page) {
                              const pageRect = page.getBoundingClientRect();
                              const rowRect = event.currentTarget.getBoundingClientRect();
                              setSelectedWaypointAnchor({
                                top: rowRect.top - pageRect.top,
                                height: rowRect.height,
                              });
                            }
                            setSelectedWaypointUid(row.rowUid);
                            setAirwayPicker(null);
                            setProcedurePicker(null);
                          }}
                        >
                          {row.rowKind === "group" && row.procedureId ? (
                            <span
                              data-testid={`parity:plan-procedure-row:${row.procedureId}:uid:${row.rowUid}`}
                              aria-hidden="true"
                            />
                          ) : null}
                          <WaypointButtonContent
                            label={row.label}
                            symbolFeature={row.symbolFeature}
                            weatherBadge={row.weatherBadge}
                            indented={row.depth > 0}
                            fullWidthLabel={procedureGroupCell}
                          />
                        </button>
                      )}
                      {row.dataCells.map((cell, cellIndex) => (
                        <div
                          key={`${row.id}:data:${planDataColumns[cellIndex]?.id ?? cellIndex}`}
                          data-testid={`parity:plan-data:${row.rowUid}:${planDataColumns[cellIndex]?.id ?? cellIndex}:${cell.value ?? "none"}`}
                          className={[
                            "planCell",
                            cell.action_id ? "isActionable" : "",
                            row.depth > 0 ? "planStructuredDataCell isChildRow" : "",
                            row.rowKind === "summary" ? "planSummaryCell" : "",
                            cell.estimate_kind === "modeled" ? "isModeled" : "",
                            cell.tone === "passed" ? "isPassed" : "",
                            cell.tone === "active" ? "isActive" : "",
                          ].filter(Boolean).join(" ")}
                          onClick={cell.action_id ? () => void props.onTimeDisplayAction(cell.action_id!) : undefined}
                          role={cell.action_id ? "button" : undefined}
                          tabIndex={cell.action_id ? 0 : undefined}
                          onKeyDown={cell.action_id ? (event) => {
                            if (event.key === "Enter" || event.key === " ") {
                              event.preventDefault();
                              void props.onTimeDisplayAction(cell.action_id!);
                            }
                          } : undefined}
                        >
                          {cell.value ?? "\u2014"}
                        </div>
                      ))}
                    </Fragment>
                  );
                })}
              </div>
            </div>
          </div>
          <div className="planEntryDock">
            <div className="planEntryCell">
              <form
                className="planEntryForm"
                data-testid={`parity:plan-append-route-state:can_commit:${routeEntryPreview.can_commit}:loading:${routeEntryLoading}`}
                onSubmit={async (event) => {
                  event.preventDefault();
                  if (!routeEntryPreview.can_commit || routeEntrySubmitting) {
                    return;
                  }
                  setRouteEntrySubmitting(true);
                  setRouteEntryError(null);
                  try {
                    await props.onAppendFlightPlanEntry(routeEntryText);
                    setRouteEntryText("");
                    setRouteEntryPreview({
                      can_commit: false,
                      tokens: [],
                      issues: [],
                    });
                  } catch (error) {
                    setRouteEntryError(errorMessage(error));
                  } finally {
                    setRouteEntrySubmitting(false);
                  }
                }}
              >
                <div className={`planEntryInputShell${routeEntryPreview.can_commit ? " isReady" : ""}`}>
                  {routeEntryText ? (
                    <div className="planEntryOverlay" aria-hidden="true">
                      {flightPlanEntryPreviewSegments(routeEntryText, routeEntryPreview).map((segment, index) => (
                        <span
                          key={`${index}:${segment.text}`}
                          className={[
                            "planEntrySegment",
                            `is${segment.tokenState[0].toUpperCase()}${segment.tokenState.slice(1)}`,
                            segment.issue ? "hasIssue" : "",
                          ].filter(Boolean).join(" ")}
                        >
                          {segment.text}
                        </span>
                      ))}
                    </div>
                  ) : (
                    <div className="planEntryPlaceholder" aria-hidden="true">Append route...</div>
                  )}
                  <textarea
                    className="planEntryInput"
                    data-testid="plan-append-route-input"
                    value={routeEntryText}
                    spellCheck={false}
                    autoCapitalize="characters"
                    autoCorrect="off"
                    onChange={(event) => {
                      setRouteEntryText(event.target.value.toUpperCase());
                      setRouteEntryError(null);
                    }}
                    onKeyDown={(event) => {
                      if (event.key !== "Enter" || event.shiftKey) {
                        return;
                      }
                      event.preventDefault();
                      event.currentTarget.form?.requestSubmit();
                    }}
                  />
                </div>
              </form>
              {routeEntryError ? (
                <div className="planEntryFeedback" data-testid="plan-append-route-feedback">{routeEntryError}</div>
              ) : routeEntryPreview.issues[0] ? (
                <div className="planEntryFeedback" data-testid="plan-append-route-feedback">{routeEntryPreview.issues[0].message}</div>
              ) : routeEntryLoading ? (
                <div className="planEntryFeedback" data-testid="plan-append-route-feedback">Checking...</div>
              ) : null}
            </div>
          </div>
        </div>
      </div>

      <div className="planControls" ref={planControlsRef}>
        {planControls.map((control) => {
          const disabledReason = disabledReasonText(control.disabled_reason);
          const disabled = !control.enabled;
          const symbol = actionSymbol(control.symbol_id);
          return (
            <button
              key={control.id}
              type="button"
              className={`trayButton trayButtonSquare planControlButton${control.selected ? " selectedControlHighlight" : ""}${disabled ? " isDisabled" : ""}`}
              data-testid={`plan-control-${control.id}`}
              disabled={disabled && !disabledReason}
              aria-disabled={disabled ? "true" : undefined}
              aria-pressed={control.selected}
              aria-keyshortcuts={flightPlanHistoryAriaKeyShortcuts(control.id)}
              title={disabledReason ?? undefined}
              onClick={() => {
                if (disabled) {
                  if (disabledReason) {
                    showDisabledAction(disabledReason);
                  }
                  return;
                }
                void props.onPerformFlightPlanControl(control.id);
              }}
            >
              {symbol ? <ActionIcon layers={symbol} /> : null}
              <span className="planControlButtonLabel">{control.label}</span>
            </button>
          );
        })}
        <button
          type="button"
          className={`planEstimateMode${planUiState.altitude_planner.estimate_summary.estimate_kind === "modeled" ? " isModeled" : ""}`}
          data-testid="plan-estimate-mode"
          onClick={() => props.onSelectPage("altitude")}
        >
          {planUiState.altitude_planner.estimate_summary.label}
        </button>
      </div>

      <PrimaryNavigationDock
        page={props.page}
        navElement={planUiState.guidance?.nav_element}
        chartPlateTargetPage={props.mostRecentChartOrPlatePage}
        onSelectPage={props.onSelectPage}
        onOpenChartOrPlate={props.onOpenRecentChartOrPlate}
      />

      {selectedRow !== null ? (
        <>
          <button
            type="button"
            className="trayScrim"
            data-testid="plan-row-tray-scrim"
            aria-label="Close waypoint actions"
            onClick={() => {
              setSelectedWaypointUid(null);
              setSelectedWaypointAnchor(null);
              setAirwayPicker(null);
              setProcedurePicker(null);
              setAirportInsert(null);
            }}
          />
          <section
            ref={waypointModalRef}
            className={`waypointModal${airportInsert ? " isAirportInsert" : ""}${procedurePicker && procedurePicker.selectedProcedureId === null ? " isProcedureChoice" : ""}`}
            aria-label="Waypoint actions"
            style={waypointModalTop === null ? undefined : {
              top: `${waypointModalTop}px`,
              maxHeight: waypointModalMaxHeight === null ? undefined : `${waypointModalMaxHeight}px`,
            }}
          >
            {airportInsert ? (
              <form
                className="waypointActionTray airportInsertTray"
                onSubmit={async (event) => {
                  event.preventDefault();
                  const airportId = airportInsert.airportId.trim().toUpperCase();
                  if (!airportId) {
                    setAirportInsert((current) => current ? { ...current, error: "Enter airport id" } : current);
                    return;
                  }
                  try {
                    await props.onInsertAirportWaypointAtRow(airportInsert.rowUid, airportInsert.before, airportId);
                    setAirportInsert(null);
                    setSelectedWaypointUid(null);
                  } catch (error) {
                    setAirportInsert((current) => current ? {
                      ...current,
                      error: error instanceof Error ? error.message : String(error),
                    } : current);
                  }
                }}
              >
                <div className="airportInsertInputRow">
                  <input
                    className="chartSearchInput airportInsertInput"
                    data-testid="plan-insert-airport-input"
                    autoFocus
                    value={airportInsert.airportId}
                    placeholder={airportInsert.before ? "INSERT BEFORE" : "INSERT AFTER"}
                    spellCheck={false}
                    autoCapitalize="characters"
                    autoCorrect="off"
                    onChange={(event) => {
                      setAirportInsert((current) => current ? {
                        ...current,
                        airportId: event.target.value.toUpperCase().replace(/[^A-Z0-9]/g, "").slice(0, 8),
                        error: null,
                      } : current);
                    }}
                  />
                </div>
                {airportInsert.error ? <div className="planGuidanceSummary">{airportInsert.error}</div> : null}
                {airportInsert.loading ? <div className="planGuidanceSummary">Searching...</div> : null}
                {airportInsert.suggestions.length > 0 ? (
                  <div className="airportInsertSuggestions">
                    {airportInsert.suggestions.map((suggestion) => (
                      <button
                        key={`${suggestion.kind}:${suggestion.identifier}`}
                        type="button"
                        className="trayButton airwayChoiceButton planWaypointButton airportInsertSuggestion"
                        data-testid={`plan-insert-suggestion-${suggestion.identifier}`}
                        onPointerDown={stopPointer}
                        onPointerUp={stopPointer}
                        onClick={async () => {
                          try {
                            await props.onInsertAirportWaypointAtRow(
                              airportInsert.rowUid,
                              airportInsert.before,
                              suggestion.identifier,
                            );
                            setAirportInsert(null);
                            setSelectedWaypointUid(null);
                          } catch (error) {
                            setAirportInsert((current) => current ? {
                              ...current,
                              error: error instanceof Error ? error.message : String(error),
                            } : current);
                          }
                        }}
                      >
                        <WaypointButtonContent
                          label={suggestion.identifier}
                          symbolFeature={suggestion.symbol_feature}
                          details={[
                            waypointSuggestionName(suggestion),
                            waypointSuggestionDistance(suggestion),
                          ]}
                        />
                      </button>
                    ))}
                  </div>
                ) : null}
              </form>
            ) : procedurePicker ? (
              <div className="waypointActionTray procedureChoiceTray" data-testid="plan-procedure-picker">
                <div className="trayHeader">
                  {procedurePicker.title}
                </div>
                {procedurePicker.error ? <div className="trayHeader isDestructive">{procedurePicker.error}</div> : null}
                {procedurePicker.loading ? (
                  <div className="airwayLoadingPanel" aria-live="polite">
                    <div className="spinner" aria-hidden="true" />
                    <div className="planGuidanceSummary">Loading…</div>
                  </div>
                ) : procedurePicker.selectedProcedureId === null ? (
                  procedurePicker.procedures.length > 0 ? (
                    <div className="procedureChoiceGrid">
                      {procedurePicker.procedures.map((procedure) => (
                      <button
                        key={procedure.procedure_id}
                        type="button"
                        className={`trayButton airwayChoiceButton procedureChoiceButton${procedure.enabled ? "" : " isDisabled"}`}
                        data-testid={`plan-procedure-${procedure.procedure_id}`}
                        aria-disabled={procedure.enabled ? undefined : "true"}
                        title={procedure.disabled_reason ?? undefined}
                        style={{ ["--tray-accent" as string]: plateFolderColor(procedure.accent_category) } as CSSProperties}
                        onPointerDown={stopPointer}
                        onPointerUp={stopPointer}
                        onClick={async () => {
                          if (!procedure.enabled) {
                            if (procedure.disabled_reason) {
                              showDisabledAction(procedure.disabled_reason);
                            }
                            return;
                          }
                          const trace = {
                            row_uid: procedurePicker.rowUid,
                            airport_id: procedurePicker.airportId,
                            procedure_id: procedure.procedure_id,
                          };
                          setProcedurePicker((current) => current ? {
                            ...current,
                            loading: true,
                            error: null,
                          } : current);
                          try {
                            const options = await debugTiming(
                              "plan.procedure_picker.describe_options",
                              () => props.appCoreAdapter!.describeProcedureOptions(
                                procedurePicker.airportId,
                                procedure.procedure_id,
                                procedurePicker.kind,
                              ),
                              trace,
                            );
                            setProcedurePicker((current) => current ? {
                              ...current,
                              loading: false,
                              selectedProcedureId: procedure.procedure_id,
                              options,
                            } : current);
                          } catch (error) {
                            debugLog("plan.procedure_picker.describe_options.error", { ...trace, message: errorMessage(error) });
                            setProcedurePicker((current) => current ? {
                              ...current,
                              loading: false,
                              error: error instanceof Error ? error.message : String(error),
                            } : current);
                          }
                        }}
                      >
                        {procedure.display_label}
                      </button>
                      ))}
                    </div>
                  ) : (
                    <div className="trayHeader">{procedurePicker.emptyMessage}</div>
                  )
                ) : procedurePicker.options ? (
                  <>
                    {procedurePicker.options.valid_choices.length > 0 ? procedurePicker.options.valid_choices.map((choice, index) => (
                      <button
                        key={`${procedurePicker.selectedProcedureId}:${choice.runway_transition ?? "none"}:${choice.enroute_transition ?? "none"}:${index}`}
                        type="button"
                        className="trayButton airwayChoiceButton"
                        data-testid={`plan-procedure-transition-${choice.enroute_transition ?? "none"}`}
                        onPointerDown={stopPointer}
                        onPointerUp={stopPointer}
                        onClick={async () => {
                          const trace = {
                            row_uid: procedurePicker.rowUid,
                            airport_id: procedurePicker.airportId,
                            procedure_id: procedurePicker.selectedProcedureId,
                            runway_transition: choice.runway_transition,
                            enroute_transition: choice.enroute_transition,
                          };
                          setProcedurePicker((current) => current ? {
                            ...current,
                            loading: true,
                            error: null,
                          } : current);
                          try {
                            await debugTiming(
                              "plan.procedure_picker.select_choice",
                              () => props.onSelectProcedureAtRow(
                                procedurePicker.rowUid,
                                procedurePicker.airportId,
                                procedurePicker.selectedProcedureId!,
                                procedurePicker.kind,
                                choice.runway_transition,
                                choice.enroute_transition,
                              ),
                              trace,
                            );
                            setProcedurePicker(null);
                            setSelectedWaypointUid(null);
                          } catch (error) {
                            debugLog("plan.procedure_picker.select_choice.error", { ...trace, message: errorMessage(error) });
                            setProcedurePicker((current) => current ? {
                              ...current,
                              loading: false,
                              error: error instanceof Error ? error.message : String(error),
                            } : current);
                          }
                        }}
                      >
                        {choice.label}
                      </button>
                    )) : <div className="trayHeader">{procedurePicker.options.empty_message}</div>}
                    <button
                      type="button"
                      className="trayButton airwayChoiceButton"
                      onPointerDown={stopPointer}
                      onPointerUp={stopPointer}
                      onClick={() => setProcedurePicker((current) => current ? {
                        ...current,
                        selectedProcedureId: null,
                        options: null,
                      } : current)}
                    >
                      Back
                    </button>
                  </>
                ) : null}
              </div>
            ) : airwayPicker ? (
              <div className="waypointActionTray" data-testid="plan-airway-picker">
                <div className="planGuidanceSummary">
                  {airwayPicker.header}
                </div>
                {airwayPicker.error ? <div className="planGuidanceSummary">{airwayPicker.error}</div> : null}
                {airwayPicker.loading ? (
                  <div className="airwayLoadingPanel" aria-live="polite">
                    <div className="spinner" aria-hidden="true" />
                    <div className="planGuidanceSummary">Loading…</div>
                  </div>
                ) : airwayPicker.selectedAirwayName === null ? (
                  <div className="airwaySuggestionGrid">
                    {airwayPicker.suggestions.map((suggestion) => (
                        <button
                          key={`${suggestion.airway_name}:${suggestion.nearest_branch_key ?? ""}`}
                          type="button"
                          className="trayButton trayButtonSquare airwaySuggestionButton"
                          data-testid={`parity:plan-airway-suggestion:${suggestion.airway_name}`}
                          onPointerDown={stopPointer}
                          onPointerUp={stopPointer}
                          onClick={async () => {
                            const uiSession = props.uiSession;
                            if (!uiSession || airwayPicker.rowUid === null) {
                              return;
                            }
                            setAirwayPicker((current) => current ? { ...current, loading: true, error: null } : current);
                            try {
                              const presentation = await uiSession.prepareAirwayPresentationAtFlightPlanRow(
                                airwayPicker.rowUid,
                                suggestion.airway_name,
                              );
                              setAirwayPicker((current) => current ? {
                                ...current,
                                loading: false,
                                selectedAirwayName: suggestion.airway_name,
                                presentation,
                              } : current);
                            } catch (error) {
                              setAirwayPicker((current) => current ? {
                                ...current,
                                loading: false,
                                error: error instanceof Error ? error.message : String(error),
                              } : current);
                            }
                          }}
                        >
                          {suggestion.airway_name}
                        </button>
                      ))}
                  </div>
                ) : airwayPicker.selectedEntryUid === null && airwayPicker.presentation ? (
                  <>
                    {airwayPicker.presentation.points.map((point) => (
                      <button
                        key={point.uid}
                        type="button"
                        className={`trayButton airwayChoiceButton${point.uid === airwayPicker.presentation?.suggested_entry_uid ? " isSuggested" : ""}`}
                        data-testid={`parity:plan-airway-entry:${point.label}`}
                        onPointerDown={stopPointer}
                        onPointerUp={stopPointer}
                        onClick={() => {
                          setAirwayPicker((current) => current ? {
                            ...current,
                            selectedEntryUid: point.uid,
                          } : current);
                        }}
                      >
                        {point.uid === airwayPicker.presentation?.suggested_entry_uid ? "▸ " : ""}
                        {point.label}
                      </button>
                    ))}
                    <button
                      type="button"
                      className="trayButton airwayChoiceButton"
                      onPointerDown={stopPointer}
                      onPointerUp={stopPointer}
                      onClick={() => setAirwayPicker((current) => current ? {
                        ...current,
                        selectedAirwayName: null,
                        presentation: null,
                      } : current)}
                    >
                      Back
                    </button>
                  </>
                ) : (
                  <>
                    {airwayPicker.presentation?.points.map((exit) => {
                      const isEntry = exit.uid === airwayPicker.selectedEntryUid;
                      return (
                      <button
                        key={exit.uid}
                        type="button"
                        className={`trayButton airwayChoiceButton${exit.uid === airwayPicker.presentation?.suggested_exit_uid && !isEntry ? " isSuggested" : ""}${isEntry ? " isDisabled" : ""}`}
                        data-testid={`parity:plan-airway-exit:${exit.label}`}
                        aria-disabled={isEntry ? "true" : undefined}
                        title={isEntry ? exit.same_point_exit_disabled_reason : undefined}
                        onPointerDown={stopPointer}
                        onPointerUp={stopPointer}
                        onClick={async () => {
                          if (isEntry) {
                            showDisabledAction(exit.same_point_exit_disabled_reason);
                            return;
                          }
                          const presentation = airwayPicker.presentation;
                          const selectedEntryUid = airwayPicker.selectedEntryUid;
                          if (!presentation || selectedEntryUid === null) {
                            return;
                          }
                          setAirwayPicker((current) => current ? { ...current, loading: true, error: null } : current);
                          try {
                            if (airwayPicker.rowUid !== null) {
                              await props.onInsertAirwayAtRow(
                                airwayPicker.rowUid,
                                selectedEntryUid,
                                exit.uid,
                                presentation,
                              );
                            } else {
                              throw new Error("airway picker missing insertion row");
                            }
                            setAirwayPicker(null);
                            setSelectedWaypointUid(null);
                          } catch (error) {
                            setAirwayPicker((current) => current ? {
                              ...current,
                              loading: false,
                              error: error instanceof Error ? error.message : String(error),
                            } : current);
                          }
                        }}
                      >
                        {exit.uid === airwayPicker.presentation?.suggested_exit_uid ? "▸ " : ""}
                        {exit.label}
                      </button>
                      );
                    }) ?? null}
                    <button
                      type="button"
                      className="trayButton airwayChoiceButton"
                      onPointerDown={stopPointer}
                      onPointerUp={stopPointer}
                      onClick={() => setAirwayPicker((current) => current ? {
                        ...current,
                        selectedEntryUid: null,
                      } : current)}
                    >
                      Back
                    </button>
                  </>
                )}
              </div>
            ) : (
              <div className="waypointActionGrid">
                {rowActionRows.map((row, rowIndex) => (
                  <div key={`row-${rowIndex}`} className="waypointActionGridRow">
                    {row.map((action) => {
                      const disabledReason = disabledReasonText(action.disabledReason);
                      const disabled = !action.enabled;
                      const symbol = actionSymbol(action.id);
                      return (
                        <button
                          key={action.id}
                          type="button"
                          className={`trayButton airwayChoiceButton${disabled ? " isDisabled" : ""}`}
                          style={{ gridColumnStart: action.menuColumn + 1 }}
                          data-testid={`plan-row-action-${action.id}`}
                          disabled={disabled && !disabledReason}
                          aria-disabled={disabled ? "true" : undefined}
                          title={disabledReason ?? undefined}
                          onPointerDown={stopPointer}
                          onPointerUp={stopPointer}
                          onClick={() => {
                            if (disabled) {
                              if (disabledReason) {
                                showDisabledAction(disabledReason);
                              }
                              return;
                            }
                            action.onSelect();
                          }}
                        >
                          <span className={`flightPlanActionButtonContent${symbol ? " hasIcon" : ""}`}>
                            <span className="flightPlanActionButtonLabel">{action.label}</span>
                            {symbol ? <ActionIcon layers={symbol} /> : null}
                          </span>
                        </button>
                      );
                    })}
                  </div>
                ))}
              </div>
            )}
          </section>
        </>
      ) : null}
      {disabledActionToast ? (
        <div className="mapSelectionToast" data-testid="disabled-action-toast" role="status" aria-live="polite">
          {disabledActionToast.message}
        </div>
      ) : null}
      {flightPlanAirportInfoModal ? (
        <>
          <TrayScrim ariaLabel="Close airport info" onClose={() => setFlightPlanAirportInfoModal(null)} />
          {flightPlanAirportInfoModal.detail ? (
            <AirportInfoModal
              detail={flightPlanAirportInfoModal.detail}
              onTimeDisplayAction={async (actionId) => {
                const airportId = flightPlanAirportInfoModal.airportId;
                await props.onTimeDisplayAction(actionId);
                if (!props.uiSession) return;
                const detail = await props.uiSession.airportInfo(airportId);
                setFlightPlanAirportInfoModal((current) => current?.airportId === airportId
                  ? { airportId, detail, error: null }
                  : current);
              }}
            />
          ) : (
            <MapSelectionDetailModal
              title={flightPlanAirportInfoModal.airportId}
              text={flightPlanAirportInfoModal.error
                ? `Airport info unavailable: ${flightPlanAirportInfoModal.error}`
                : "Loading airport info..."}
            />
          )}
        </>
      ) : null}
    </section>
  );
}

function ChartPlateToggleButton(props: {
  page: AppPage;
  onSelectPage: (page: AppPage) => void;
}) {
  const pageOptions = useNavigationPageOptions();
  const chartSelected = props.page === "map";
  const active = props.page === "map" || props.page === "charts";
  const option = chartSelected
    ? pageOptions.find((entry) => entry.id === "map")
    : pageOptions.find((entry) => entry.id === "charts");
  const targetPage: AppPage = chartSelected ? "charts" : "map";
  return (
    <button
      type="button"
      className={`chartButton pageToggleButton${active ? " isOpen" : ""}`}
      data-testid={chartSelected ? "page-button-plate" : "page-button-chart"}
      onPointerDown={stopPointer}
      onPointerUp={stopPointer}
      onDoubleClick={stopDoubleClick}
      onClick={() => props.onSelectPage(targetPage)}
      aria-label={chartSelected ? "Open plate page" : "Open chart page"}
    >
      {option?.iconSrc ? <img className="chartButtonIcon" src={option.iconSrc} alt="" aria-hidden="true" /> : null}
      <span className={`pageToggleTrack${chartSelected ? " isChart" : " isPlate"}`} aria-hidden="true">
        <span className="pageToggleKnob" />
      </span>
      <span className="chartButtonLabel">{option?.launcherLabel ?? (chartSelected ? "CHART" : "PLATE")}</span>
    </button>
  );
}

function mostRecentChartOrPlatePageFromHistory(
  pageHistory: AppViewSnapshot[],
  returnTargetPages: ReadonlySet<AppPage>,
  defaultReturnPage: AppPage,
): AppPage {
  return pageHistory
    .slice()
    .reverse()
    .find((snapshot) => returnTargetPages.has(snapshot.page))
    ?.page ?? defaultReturnPage;
}

function ChartPlateReturnButton(props: {
  targetPage: AppPage;
  onClick: () => void;
}) {
  const pageOptions = useNavigationPageOptions();
  const chartSelected = props.targetPage !== "charts";
  const option = chartSelected
    ? pageOptions.find((entry) => entry.id === "map")
    : pageOptions.find((entry) => entry.id === "charts");
  return (
    <button
      type="button"
      className="chartButton"
      data-testid={chartSelected ? "page-button-return-chart" : "page-button-return-plate"}
      onPointerDown={stopPointer}
      onPointerUp={stopPointer}
      onDoubleClick={stopDoubleClick}
      onClick={props.onClick}
      aria-label={chartSelected ? "Return to chart page" : "Return to plate page"}
    >
      {option?.iconSrc ? <img className="chartButtonIcon" src={option.iconSrc} alt="" aria-hidden="true" /> : null}
      <span className="chartButtonLabel">{option?.launcherLabel ?? (chartSelected ? "CHART" : "PLATE")}</span>
    </button>
  );
}

function HomeNavButton(props: {
  active: boolean;
  onClick: () => void;
}) {
  const pageOptions = useNavigationPageOptions();
  const option = pageOptions.find((entry) => entry.id === "home");
  return (
    <button
      type="button"
      className={`chartButton${props.active ? " isOpen" : ""}`}
      data-testid="page-button-home"
      onPointerDown={stopPointer}
      onPointerUp={stopPointer}
      onDoubleClick={stopDoubleClick}
      onClick={props.onClick}
      aria-label="Open home page"
    >
      {option?.iconSrc ? <img className="chartButtonIcon" src={option.iconSrc} alt="" aria-hidden="true" /> : null}
      <span className="chartButtonLabel">{option?.launcherLabel ?? "HOME"}</span>
    </button>
  );
}

function PrimaryNavigationDock(props: {
  page: AppPage;
  navElement: NavElementUiView | null | undefined;
  chartPlateTargetPage?: AppPage;
  className?: string;
  onSelectPage: (page: AppPage) => void;
  onOpenPlan?: () => void;
  onOpenChartOrPlate?: () => void;
}) {
  const navigationPolicy = useNavigationPagePolicy();
  const chartOrPlatePage = navigationPolicy.chartOrPlateReturnPages.has(props.page);
  return (
    <nav
      className={`primaryNavigationDock${props.className ? ` ${props.className}` : ""}`}
      aria-label="Primary navigation"
    >
      <HomeNavButton
        active={props.page === "home"}
        onClick={() => props.onSelectPage("home")}
      />
      <NavElementButton
        navElement={props.navElement}
        className="navElement primaryNavigationCdi"
        onPointerDown={stopPointer}
        onPointerUp={stopPointer}
        onDoubleClick={stopDoubleClick}
        onClick={props.page === "plan" ? undefined : props.onOpenPlan}
      />
      {chartOrPlatePage ? (
        <ChartPlateToggleButton page={props.page} onSelectPage={props.onSelectPage} />
      ) : (
        <ChartPlateReturnButton
          targetPage={props.chartPlateTargetPage ?? navigationPolicy.defaultChartOrPlateReturnPage}
          onClick={props.onOpenChartOrPlate ?? (() => props.onSelectPage(navigationPolicy.defaultChartOrPlateReturnPage))}
        />
      )}
    </nav>
  );
}

function TrayDock(props: {
  launcherLabel: string;
  launcherImageSrc?: string;
  launcherStyle?: CSSProperties;
  open: boolean;
  onToggle: () => void;
  ariaLabel: string;
  disabled?: boolean;
  disabledReason?: string | null;
  onDisabledAction?: (message: string) => void;
  style?: TrayDockStyle;
  launcherClassName?: string;
  launcherAccentColor?: string;
  options: TrayOption[];
  header?: string;
  headerTone?: "normal" | "destructive";
  footer?: ReactNode;
  testId?: string;
}) {
  const { launcherLabel, launcherImageSrc, launcherStyle, open, onToggle, ariaLabel, disabled = false, disabledReason, onDisabledAction, style = "compact", launcherClassName, launcherAccentColor, options, header, headerTone = "normal", footer, testId } = props;
  const launcherRef = useRef<HTMLButtonElement | null>(null);
  const trayRef = useRef<HTMLElement | null>(null);
  const [trayPosition, setTrayPosition] = useState<{ left: number; top: number } | null>(null);
  const [trayThemeStyle, setTrayThemeStyle] = useState<CSSProperties | null>(null);
  const launcherWide = style === "plate_wide" || style === "wide" || style === "situation";
  const trayWide = style === "plate_narrow" || style === "plate_wide" || style === "wide";
  const launcherDisabled = disabled && !open;
  const launcherDisabledReason = disabledReasonText(disabledReason);

  useEffect(() => {
    if (!open) {
      setTrayPosition(null);
      return;
    }

    function updatePosition() {
      const launcher = launcherRef.current;
      const tray = trayRef.current;
      if (!launcher || !tray) {
        return;
      }
      const launcherStyle = getComputedStyle(launcher);
      const launcherRect = launcher.getBoundingClientRect();
      const gap = thumbPixels(0.1);
      const minInset = gap;
      const maxLeft = Math.max(minInset, window.innerWidth - tray.offsetWidth - gap);
      const maxTop = Math.max(minInset, window.innerHeight - tray.offsetHeight - gap);
      setTrayPosition({
        left: Math.min(Math.max(minInset, launcherRect.left), maxLeft),
        top: Math.min(Math.max(minInset, launcherRect.bottom + gap), maxTop),
      });
      setTrayThemeStyle({
        ["--theme-button-unchecked" as string]: launcherStyle.getPropertyValue("--theme-button-unchecked"),
        ["--theme-button-checked" as string]: launcherStyle.getPropertyValue("--theme-button-checked"),
        ["--theme-button-disabled" as string]: launcherStyle.getPropertyValue("--theme-button-disabled"),
        ["--theme-button-disabled-icon-saturation" as string]: launcherStyle.getPropertyValue("--theme-button-disabled-icon-saturation"),
        ["--theme-button-disabled-icon-opacity" as string]: launcherStyle.getPropertyValue("--theme-button-disabled-icon-opacity"),
        ["--theme-disabled-accent-percent" as string]: launcherStyle.getPropertyValue("--theme-disabled-accent-percent"),
        ["--theme-button-fg" as string]: launcherStyle.getPropertyValue("--theme-button-fg"),
        ["--theme-situation-status-unavailable-fg" as string]: launcherStyle.getPropertyValue("--theme-situation-status-unavailable-fg"),
      });
    }

    updatePosition();
    window.addEventListener("resize", updatePosition);
    window.addEventListener("scroll", updatePosition, true);
    return () => {
      window.removeEventListener("resize", updatePosition);
      window.removeEventListener("scroll", updatePosition, true);
    };
  }, [header, open, options.length, style]);

  return (
    <div className="chartDockColumn">
      <button
        ref={launcherRef}
        type="button"
        className={`chartButton${launcherWide ? " chartButtonWide" : ""}${launcherClassName ? ` ${launcherClassName}` : ""}${open ? " isOpen" : ""}${launcherDisabled ? " isDisabled" : ""}`}
        data-testid={testId}
        aria-disabled={launcherDisabled}
        style={{
          ...launcherStyle,
          ...(launcherAccentColor ? ({ ["--tray-accent" as string]: launcherAccentColor } as CSSProperties) : undefined),
        }}
        onPointerDown={stopPointer}
        onPointerUp={stopPointer}
        onDoubleClick={stopDoubleClick}
        title={launcherDisabledReason ?? undefined}
        onClick={launcherDisabled
          ? launcherDisabledReason && onDisabledAction
            ? () => onDisabledAction(launcherDisabledReason)
            : undefined
          : onToggle}
      >
        {launcherImageSrc ? <img className="chartButtonIcon" src={launcherImageSrc} alt="" aria-hidden="true" /> : null}
        <span className={`chartButtonLabel${launcherWide ? " chartButtonLabelWide" : ""}`}>{launcherLabel}</span>
      </button>
      {open && typeof document !== "undefined"
        ? createPortal(
            <section
              ref={trayRef}
              className={`chartTray chartTrayPortal${trayWide ? " chartTrayWide" : ""}${style === "situation" ? " chartTraySituation" : ""} isOpen`}
              aria-label={ariaLabel}
              style={
                trayPosition
                  ? {
                    ...trayThemeStyle,
                    left: `${trayPosition.left}px`,
                    top: `${trayPosition.top}px`,
                    ...(style === "situation" ? ({ ["--situation-source-count" as string]: String(Math.max(1, options.length)) } as CSSProperties) : null),
                  }
                  : {
                    ...trayThemeStyle,
                    visibility: "hidden",
                    ...(style === "situation" ? ({ ["--situation-source-count" as string]: String(Math.max(1, options.length)) } as CSSProperties) : null),
                  }
              }
              onPointerDown={stopPointer}
              onPointerUp={stopPointer}
            >
              {header ? (
                <div className={`trayHeader${headerTone === "destructive" ? " isDestructive" : ""}`}>
                  {header}
                </div>
              ) : null}
              <div className={style === "situation" ? "situationSourceRow" : "trayOptions"}>
                {options.map((option) => {
                  if (option.kind === "separator") {
                    return (
                      <div key={option.id} className="traySeparator" role="separator">
                        {option.label}
                      </div>
                    );
                  }
                  const optionDisabledReason = disabledReasonText(option.disabledReason);
                  const optionDisabled = option.disabled ?? false;
                  const optionButton = (
                    <button
                      key={option.id}
                      type="button"
                      className={`trayButton${option.active ? " isActive" : ""}${optionDisabled ? " isDisabled" : ""}${option.iconSrc || option.aircraftSymbol ? " trayButtonWithIcon" : ""}${option.toggleState ? " trayButtonHasToggle" : ""}${option.toggleState?.visible && option.toggleState.enabled ? " isOn" : ""}${option.toggleState && option.toggleState.enabled && !option.toggleState.visible ? " isOff" : ""}`}
                      data-testid={`tray-option-${option.id}`}
                      disabled={optionDisabled && !optionDisabledReason}
                      aria-disabled={optionDisabled ? "true" : undefined}
                      aria-pressed={(option.toggleState?.visible ?? option.active) ? "true" : "false"}
                      title={optionDisabledReason ?? undefined}
                      style={option.accentColor ? ({ ["--tray-accent" as string]: option.accentColor } as CSSProperties) : undefined}
                      onPointerDown={stopPointer}
                      onPointerUp={stopPointer}
                      onDoubleClick={stopDoubleClick}
                      onClick={() => {
                        if (optionDisabled) {
                          if (optionDisabledReason && onDisabledAction) {
                            onDisabledAction(optionDisabledReason);
                          }
                          return;
                        }
                        option.onSelect();
                        if (option.dismissTrayOnSelect) {
                          onToggle();
                        }
                      }}
                    >
                      {option.iconSrc || option.aircraftSymbol || option.toggleState ? (
                        <span className="trayButtonContent">
                          {option.iconSrc ? (
                            <span className="trayButtonIconFrame" aria-hidden="true">
                              <img className="trayButtonIcon" src={option.iconSrc} alt="" />
                            </span>
                          ) : null}
                          <span className="trayButtonText">{option.label}</span>
                          {option.aircraftSymbol ? (
                            <AircraftSymbolIcon symbol={option.aircraftSymbol} />
                          ) : null}
                          {option.toggleState ? (
                            <span
                              className={`trayButtonToggle${option.toggleState.visible ? " isOn" : ""}${option.toggleState.enabled ? "" : " isDisabled"}`}
                              aria-hidden="true"
                            >
                              <span className="trayButtonToggleKnob" />
                            </span>
                          ) : null}
                        </span>
                      ) : option.label}
                    </button>
                  );
                  if (!option.accessory) {
                    return optionButton;
                  }
                  return (
                    <div key={option.id} className="trayButtonSplit">
                      {optionButton}
                      <button
                        type="button"
                        className="trayButtonAccessory"
                        data-testid={`tray-option-accessory-${option.id}`}
                        aria-label={option.accessory.ariaLabel}
                        title={option.accessory.ariaLabel}
                        onPointerDown={stopPointer}
                        onPointerUp={stopPointer}
                        onDoubleClick={stopDoubleClick}
                        onClick={() => {
                          option.accessory?.onSelect();
                          if (option.dismissTrayOnSelect) {
                            onToggle();
                          }
                        }}
                      >
                        <img
                          className="trayButtonAccessoryIcon"
                          src={option.accessory.iconSrc}
                          alt=""
                          aria-hidden="true"
                        />
                      </button>
                    </div>
                  );
                })}
              </div>
              {footer}
            </section>,
            document.body,
          )
        : null}
    </div>
  );
}

function ChartSearchBox(props: {
  state: {
    query: string;
    open: boolean;
    loading: boolean;
    error: string | null;
    suggestions: WaypointIdentifierSuggestion[];
  };
  onQueryChange: (query: string) => void;
  onFocus: () => void;
  onClose: () => void;
  onSubmit: () => void;
  onSelect: (suggestion: WaypointIdentifierSuggestion) => void;
}) {
  const { state, onQueryChange, onFocus, onClose, onSubmit, onSelect } = props;
  const showTray = state.open && (state.query.trim() || state.loading || state.error || state.suggestions.length > 0);

  return (
    <div className="chartSearch">
      <input
        className="chartSearchInput"
        data-testid="chart-search-input"
        type="text"
        value={state.query}
        placeholder="SEARCH"
        autoCapitalize="characters"
        autoCorrect="off"
        spellCheck={false}
        onPointerDown={stopPointer}
        onPointerUp={stopPointer}
        onDoubleClick={stopDoubleClick}
        onFocus={onFocus}
        onChange={(event) => onQueryChange(event.currentTarget.value)}
        onKeyDown={(event) => {
          event.stopPropagation();
          if (event.key === "Enter") {
            event.preventDefault();
            onSubmit();
          } else if (event.key === "Escape") {
            event.preventDefault();
            onClose();
          }
        }}
      />
      {showTray ? (
        <section
          className="chartSearchTray"
          aria-label="Waypoint search results"
          onPointerDown={stopPointer}
          onPointerUp={stopPointer}
        >
          {state.loading ? <div className="chartSearchStatus">Searching...</div> : null}
          {state.error ? <div className="chartSearchStatus isError">{state.error}</div> : null}
          {!state.loading && !state.error && state.suggestions.length === 0 ? (
            <div className="chartSearchStatus">No matches</div>
          ) : null}
          {state.suggestions.map((suggestion) => (
            <button
              key={`${suggestion.kind}:${suggestion.identifier}`}
              type="button"
              className="trayButton airwayChoiceButton planWaypointButton airportInsertSuggestion chartSearchSuggestion"
              data-testid={`chart-search-suggestion-${suggestion.identifier}`}
              onPointerDown={stopPointer}
              onPointerUp={stopPointer}
              onDoubleClick={stopDoubleClick}
              onClick={() => onSelect(suggestion)}
            >
              <WaypointButtonContent
                label={suggestion.identifier}
                symbolFeature={suggestion.symbol_feature}
                details={[
                  waypointSuggestionName(suggestion),
                  waypointSuggestionDistance(suggestion),
                ]}
              />
            </button>
          ))}
        </section>
      ) : null}
    </div>
  );
}

function MapSelectionTray(props: {
  point: ScreenPoint;
  result: MapSelectionQueryResult;
  selectedItem: MapSelectionItem | null;
  onSelectItem: (item: MapSelectionItem) => void;
  onDisabledAction: (message: string) => void;
  onSelectAction: (action: MapSelectionItem["actions"][number]) => void | Promise<void>;
}) {
  const { point, result, selectedItem, onSelectItem, onDisabledAction, onSelectAction } = props;
  const edgePad = thumbPixels(0.1);
  const actionSlots: MapSelectionItem["actions"] = selectedItem
    ? selectedItem.actions
    : Array.from({ length: 6 }, (_, index) => ({
      id: `placeholder-${index}`,
      label: "",
      enabled: false,
      display_only: true,
      action_uid: null,
      placeholder: true,
    }));
  const horizontalStyle = point.x < window.innerWidth / 2
    ? { right: `${edgePad}px` }
    : { left: `${edgePad}px` };
  const verticalStyle = point.y < window.innerHeight / 2
    ? { bottom: `${edgePad}px` }
    : { top: `${edgePad}px` };

  return (
    <section
      className="mapSelectionTray"
      data-testid="map-selection-tray"
      style={{ ...horizontalStyle, ...verticalStyle }}
      aria-label="Map selection"
      onPointerDown={stopPointer}
      onPointerMove={stopPointer}
      onPointerUp={stopPointer}
      onPointerCancel={stopPointer}
      onWheel={stopWheel}
      onClick={stopClick}
      onDoubleClick={stopDoubleClick}
    >
      {result.categories.map((category) => (
        <div key={category.id} className="mapSelectionCategory">
          <div className="mapSelectionRow">
            {category.items.length === 0 ? (
              <div className="mapSelectionEmpty" aria-hidden="true">
                no {category.label.toLowerCase()}s
              </div>
            ) : category.items.map((item) => (
              <button
                key={item.id}
                type="button"
                className={`mapSelectionItem${selectedItem?.id === item.id ? " isSelected selectedControlHighlight" : ""}`}
                data-testid={`map-selection-item-${category.id}-${item.label}`}
                onPointerDown={stopPointer}
                onPointerUp={stopPointer}
                onDoubleClick={stopDoubleClick}
                onClick={() => onSelectItem(item)}
                title={item.sublabel}
              >
                <MapSelectionItemIcon item={item} />
                <span className="mapSelectionItemLabel">{item.label}</span>
              </button>
            ))}
          </div>
        </div>
      ))}
      <div className="mapSelectionActions">
        <div
          className="mapSelectionActionTitle"
          data-testid={`parity:map-selection-selected:${selectedItem?.label ?? "none"}`}
        >
          <div className="mapSelectionActionTitlePrimary">
            {selectedItem ? (
              <>
                <strong>{selectedItem.label}</strong>
                {selectedItem.description || selectedItem.distance ? (
                  <span className="mapSelectionActionDescription">
                    {" · "}{[selectedItem.description, selectedItem.distance].filter(Boolean).join(" · ")}
                  </span>
                ) : null}
              </>
            ) : "\u00a0"}
          </div>
          <div className="mapSelectionActionTitleSecondary">
            {selectedItem?.secondary_description?.trim() || "\u00a0"}
          </div>
        </div>
        <div className="mapSelectionActionGrid">
          {actionSlots.map((action) => {
            const disabledReason = action.disabled_reason?.trim() || null;
            const symbol = actionSymbol(action.id);
            const inert = Boolean(
              action.placeholder || action.display_only || (!action.enabled && !disabledReason),
            );
            const styledDisabled = !action.enabled && !action.display_only && !action.placeholder;
            return (
              <button
                key={action.id}
                type="button"
                className={`mapSelectionAction${action.display_only ? " isDisplayOnly" : ""}${action.placeholder ? " isPlaceholder" : ""}${styledDisabled ? " isDisabled" : ""}`}
                data-testid={action.placeholder ? undefined : `map-selection-action-${action.id}`}
                disabled={inert}
                title={disabledReason ?? undefined}
                onPointerDown={stopPointer}
                onPointerUp={stopPointer}
                onDoubleClick={stopDoubleClick}
                onClick={() => {
                  if (!selectedItem || action.placeholder || action.display_only || !action.action_uid) {
                    return;
                  }
                  if (!action.enabled) {
                    if (disabledReason) {
                      onDisabledAction(disabledReason);
                    }
                    return;
                  }
                  void onSelectAction(action);
                }}
                aria-disabled={styledDisabled ? "true" : undefined}
                aria-hidden={action.placeholder ? "true" : undefined}
                tabIndex={action.placeholder ? -1 : undefined}
              >
                {action.airspace_limit ? (
                  <svg className="mapSelectionAirspaceLimitGlyph" viewBox="-32 -32 64 64" aria-hidden="true">
                    <AirspaceLimitGlyph glyph={action.airspace_limit} scale={1.45} />
                  </svg>
                ) : (
                  <>
                    {symbol ? <ActionIcon layers={symbol} /> : null}
                    <span className="mapSelectionActionLabel">{action.label}</span>
                  </>
                )}
              </button>
            );
          })}
          {selectedItem?.detail_text ? (
            <div className="mapSelectionDetailText mapSelectionInlineDetailText">{selectedItem.detail_text}</div>
          ) : null}
        </div>
      </div>
    </section>
  );
}

function MapSelectionDetailModal(props: {
  title: string;
  text: string;
  status?: { text: string; color_key: string; action_id?: string | null } | null;
  onTimeDisplayAction?: (actionId: string) => void | Promise<void>;
}) {
  return (
    <section
      className="mapSelectionDetailModal weatherDetailModal"
      data-testid={`map-selection-detail-modal:${props.title}`}
      aria-label={props.title}
      onPointerDown={stopPointer}
      onPointerMove={stopPointer}
      onPointerUp={stopPointer}
      onPointerCancel={stopPointer}
      onWheel={stopWheel}
      onClick={stopClick}
      onDoubleClick={stopDoubleClick}
    >
      <div className="mapSelectionDetailTitle">{props.title}</div>
      {props.status ? (
        <div
          className={`mapSelectionDetailStatus${props.status.action_id ? " isActionable" : ""}`}
          style={{ color: aviationThemeColor(props.status.color_key) }}
          role={props.status.action_id ? "button" : undefined}
          tabIndex={props.status.action_id ? 0 : undefined}
          onClick={props.status.action_id ? () => void props.onTimeDisplayAction?.(props.status!.action_id!) : undefined}
          onKeyDown={props.status.action_id ? (event) => {
            if (event.key === "Enter" || event.key === " ") {
              event.preventDefault();
              void props.onTimeDisplayAction?.(props.status!.action_id!);
            }
          } : undefined}
        >
          {props.status.text}
        </div>
      ) : null}
      <div className="weatherDetailSections mapSelectionTextDetailSections">
        <WeatherDetailSection
          label={null}
          ageLabel={null}
          ageWarning={false}
          text={props.text}
        />
      </div>
    </section>
  );
}

function normalizedStationId(stationId: string): string {
  return stationId.trim().toUpperCase();
}

function weatherDetailForMetarSelection(result: MapSelectionQueryResult, stationId: string): WeatherDetailUiView | null {
  const targetStationId = normalizedStationId(stationId);
  for (const category of result.categories) {
    for (const item of category.items) {
      const itemStationId = item.metar_feature?.station_id
        ?? (item.highlight.kind === "metar" ? item.highlight.station_id : null);
      if (itemStationId && normalizedStationId(itemStationId) === targetStationId) {
        return immediateWeatherDetailForMapSelectionItem(item);
      }
    }
  }
  return null;
}

function immediateWeatherDetailForMapSelectionItem(item: MapSelectionItem): WeatherDetailUiView | null {
  return item.metar_feature ? item.weather_detail ?? null : null;
}

function hoverWeatherPanelStyle(point: ScreenPoint): CSSProperties {
  const edgePad = thumbPixels(0.14);
  return {
    ...(point.x < window.innerWidth / 2
      ? { right: `${edgePad}px` }
      : { left: `${edgePad}px` }),
    ...(point.y < window.innerHeight / 2
      ? { bottom: `${edgePad}px` }
      : { top: `${edgePad}px` }),
  };
}

function WeatherDetailModal(props: { detail: WeatherDetailUiView; className?: string; style?: CSSProperties }) {
  const { detail } = props;
  return (
    <section
      className={`mapSelectionDetailModal weatherDetailModal${props.className ? ` ${props.className}` : ""}`}
      data-testid="weather-detail-modal"
      style={props.style}
      aria-label={`Weather ${detail.station_id}`}
      onPointerDown={stopPointer}
      onPointerMove={stopPointer}
      onPointerUp={stopPointer}
      onPointerCancel={stopPointer}
      onWheel={stopWheel}
      onClick={stopClick}
      onDoubleClick={stopDoubleClick}
    >
      <div className="mapSelectionDetailTitle">{detail.title}</div>
      <div className="weatherDetailAdvisory">{detail.advisory_text}</div>
      <div className="weatherDetailSections">
        {detail.sections.map((section, index) => section.kind === "notams" ? (
          <NotamSection
            key={`${section.kind}:${section.label}:${index}`}
            label={section.label}
            trailingLabel={section.trailing_label ?? ""}
            notams={section.notams ?? []}
            emptyText={section.empty_text}
          />
        ) : (
          <WeatherDetailSection
            key={`${section.kind}:${section.label}:${index}`}
            label={section.label}
            ageLabel={section.trailing_label ?? null}
            ageWarning={section.trailing_warning ?? false}
            text={section.text ?? null}
            emptyText={section.empty_text}
          />
        ))}
      </div>
    </section>
  );
}

function AirportInfoModal(props: {
  detail: AirportInfoUiView;
  onTimeDisplayAction: (actionId: string) => void | Promise<void>;
}) {
  const { detail } = props;
  const [scrollTop, setScrollTop] = useState(0);
  return (
    <section
      className="mapSelectionDetailModal airportInfoModal"
      data-testid={`airport-info-modal:${detail.airport_id}`}
      onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}
      aria-label={`Airport info ${detail.airport_id}`}
      onPointerDown={stopPointer}
      onPointerMove={stopPointer}
      onPointerUp={stopPointer}
      onPointerCancel={stopPointer}
      onWheel={stopWheel}
      onClick={stopClick}
      onDoubleClick={stopDoubleClick}
    >
      <span hidden data-testid={`parity:airport-info-scroll:${Math.round(scrollTop)}`} />
      <header className="airportInfoHeader">
        <div className="mapSelectionDetailTitle">{detail.airport_id}</div>
        <div className="airportInfoName">{detail.name}</div>
        {detail.location_label ? (
          <div className="airportInfoLocation">{detail.location_label}</div>
        ) : null}
      </header>
      {detail.fact_sections.map((section, sectionIndex) => (
        <section className="airportInfoSection" key={`${section.title ?? "facts"}:${sectionIndex}`}>
          {section.title ? <h2>{section.title}</h2> : null}
          <div className="airportInfoFacts">
            {section.facts.map((fact, index) => (
              <div
                key={`${fact.label}:${fact.value}:${index}`}
                data-testid={`airport-info-fact:${fact.label}:${fact.value}`}
              >
                <AirportInfoFact
                  label={fact.label}
                  value={fact.link_url ? <a href={fact.link_url}>{fact.value}</a> : fact.value}
                  nextIn={fact.next_in_label}
                  onClick={fact.action_id
                    ? () => props.onTimeDisplayAction(fact.action_id!)
                    : undefined}
                />
              </div>
            ))}
          </div>
        </section>
      ))}
      {detail.runways.length > 0 ? (
        <section className="airportInfoSection">
          <h2>{detail.runways_section_title}</h2>
          <div
            className="airportRunwayList"
            data-testid={`airport-info-runways:complex:${detail.runway_diagram_complex}:count:${detail.runways.length}`}
          >
            {detail.runways.map((runway, index) => (
              <article
                className="airportRunwayRow"
                data-testid={`airport-info-runway:${runway.end_a_label}:${runway.end_b_label}`}
                key={`${runway.end_a_label}:${runway.end_b_label}:${index}`}
              >
                <RunwayDiagram
                  runways={detail.runways}
                  activeRunwayIndex={index}
                  complex={detail.runway_diagram_complex}
                />
                <div className="airportRunwayText">
                  <div>{runway.end_a_label} /</div>
                  <div>{runway.end_b_label}</div>
                  <div>{runway.dimensions_label}</div>
                  <div>{runway.surface_label}</div>
                </div>
              </article>
            ))}
          </div>
        </section>
      ) : null}
    </section>
  );
}

function AirportInfoFact(props: {
  label: string;
  value: ReactNode;
  nextIn?: string | null;
  onClick?: () => void;
}) {
  return (
    <div className="airportInfoFact">
      <div className="airportInfoFactLabel">{props.label}</div>
      <div
        className={`airportInfoFactValue${props.onClick ? " isActionable" : ""}`}
        data-testid={props.onClick ? "airport-info-time-toggle" : undefined}
        role={props.onClick ? "button" : undefined}
        tabIndex={props.onClick ? 0 : undefined}
        onClick={props.onClick}
        onKeyDown={props.onClick ? (event) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            props.onClick?.();
          }
        } : undefined}
      >
        {props.value}
        {props.nextIn ? <span className="airportInfoNextEvent">◷ {props.nextIn}</span> : null}
      </div>
    </div>
  );
}

function runwayDiagramPolygon(runway: AirportInfoUiView["runways"][number]): string {
  const dx = runway.diagram_end_b_x - runway.diagram_end_a_x;
  const dy = runway.diagram_end_b_y - runway.diagram_end_a_y;
  const length = Math.hypot(dx, dy);
  const minimumExtent = 0.024;
  const displayLength = Math.max(length, minimumExtent);
  const directionX = length > 0 ? dx / length : 0;
  const directionY = length > 0 ? dy / length : -1;
  const centerX = (runway.diagram_end_a_x + runway.diagram_end_b_x) / 2;
  const centerY = (runway.diagram_end_a_y + runway.diagram_end_b_y) / 2;
  const endAX = centerX - directionX * displayLength / 2;
  const endAY = centerY - directionY * displayLength / 2;
  const endBX = centerX + directionX * displayLength / 2;
  const endBY = centerY + directionY * displayLength / 2;
  const halfWidth = Math.max(runway.diagram_width_ratio, minimumExtent) / 2;
  const px = -directionY * halfWidth;
  const py = directionX * halfWidth;
  return [
    [endAX + px, endAY + py],
    [endBX + px, endBY + py],
    [endBX - px, endBY - py],
    [endAX - px, endAY - py],
  ].map(([x, y]) => `${x},${y}`).join(" ");
}

function runwayPatternPoints(
  pattern: NonNullable<AirportInfoUiView["runways"][number]["diagram_end_a_pattern"]>,
): string {
  return [
    [pattern.base_x, pattern.base_y],
    [pattern.corner_x, pattern.corner_y],
    [pattern.final_x, pattern.final_y],
  ].map(([x, y]) => `${x},${y}`).join(" ");
}

function RunwayDiagram(props: {
  runways: AirportInfoUiView["runways"];
  activeRunwayIndex: number;
  complex: boolean;
}) {
  const { runways, activeRunwayIndex, complex } = props;
  const activeRunway = runways[activeRunwayIndex];
  const displayedRunways = complex
    ? runways
      .map((runway, index) => ({ runway, index }))
      .sort((left, right) => Number(left.index === activeRunwayIndex) - Number(right.index === activeRunwayIndex))
    : [{ runway: activeRunway, index: activeRunwayIndex }];
  return (
    <svg className="airportRunwayDiagram" viewBox="-0.58 -0.58 1.16 1.16" role="img" aria-label="North-up runway diagram">
      {displayedRunways.map(({ runway, index }) => {
        const isActive = !complex || index === activeRunwayIndex;
        return (
          <polygon
            key={`${runway.end_a_label}:${runway.end_b_label}:${index}`}
            points={runwayDiagramPolygon(runway)}
            fill={isActive ? aviationThemeColor(activeRunway.surface_color_key) : "none"}
            stroke={isActive ? "none" : aviationThemeColor("airport_runway_inactive")}
            strokeWidth={isActive ? undefined : 0.012}
          />
        );
      })}
      {[activeRunway.diagram_end_a_pattern, activeRunway.diagram_end_b_pattern]
        .map((pattern, index) => pattern ? (
          <polyline
            key={`pattern:${index}`}
            points={runwayPatternPoints(pattern)}
            fill="none"
            stroke={aviationThemeColor("airport_runway_pattern")}
            strokeWidth={0.018}
            strokeLinecap="square"
            strokeLinejoin="miter"
          />
        ) : null)}
    </svg>
  );
}

function NotamSection(props: {
  notams: NonNullable<WeatherDetailUiView["notams"]>;
  label: string;
  trailingLabel: string;
  emptyText: string;
}) {
  return (
    <section className="weatherDetailSection airportNotamSection">
      <div className="weatherDetailSectionTitle">
        <span>{props.label}</span>
        <span>{props.trailingLabel}</span>
      </div>
      <div className="airportNotamList">
        {props.notams.length > 0 ? props.notams.map((notam) => (
          <article className="airportNotamCell" key={notam.id}>
            <div className="airportNotamLabel">{notam.label}</div>
            <div className="airportNotamText">{notam.text}</div>
          </article>
        )) : (
          <div className="airportNotamEmpty">{props.emptyText}</div>
        )}
      </div>
    </section>
  );
}

function ProcedureNotamModal(props: {
  detail: NonNullable<ChartAsset["procedure_notam_badge"]>["detail"];
}) {
  return (
    <section
      className="mapSelectionDetailModal weatherDetailModal procedureNotamDetailModal"
      data-testid="procedure-notam-modal"
      aria-label={props.detail.title}
      onPointerDown={stopPointer}
      onPointerMove={stopPointer}
      onPointerUp={stopPointer}
      onPointerCancel={stopPointer}
      onWheel={stopWheel}
      onClick={stopClick}
      onDoubleClick={stopDoubleClick}
    >
      <div className="mapSelectionDetailTitle">{props.detail.title}</div>
      <div className="weatherDetailAdvisory">{props.detail.advisory_text}</div>
      <div className="weatherDetailSections">
        <NotamSection
          notams={props.detail.notams}
          label="NOTAM"
          trailingLabel={String(props.detail.notams.length)}
          emptyText={props.detail.empty_text}
        />
      </div>
    </section>
  );
}

function PlateProcedureNotamBadgeButton(props: {
  badge: NonNullable<ChartAsset["procedure_notam_badge"]>;
  placement: "folder" | "dock";
  onOpen: () => void;
}) {
  return (
    <button
      type="button"
      className={`plateProcedureNotamBadge plateProcedureNotamBadge-${props.placement}`}
      data-testid={`plate-notam:${props.badge.action_id}`}
      aria-label={props.badge.accessibility_label}
      title={props.badge.accessibility_label}
      data-action-id={props.badge.action_id}
      onPointerDown={stopPointer}
      onPointerUp={stopPointer}
      onDoubleClick={stopDoubleClick}
      onClick={(event) => {
        event.stopPropagation();
        props.onOpen();
      }}
    >
      <span>{props.badge.label}</span>
      <span>{props.badge.count}</span>
    </button>
  );
}

function WeatherDetailSection(props: { label: string | null; ageLabel: string | null; ageWarning: boolean; text: string | null; emptyText?: string }) {
  return (
    <section className="weatherDetailSection">
      {props.label || props.ageLabel ? (
        <div className="weatherDetailSectionTitle">
          {props.label ? <span>{props.label}</span> : <span />}
          {props.ageLabel ? (
            <span className={`weatherDetailAge${props.ageWarning ? " isWarning" : ""}`}>
              {props.ageLabel}
            </span>
          ) : null}
        </div>
      ) : null}
      <pre className={`weatherDetailText${props.text ? "" : " isMissing"}`}>
        {props.text ?? props.emptyText ?? `No ${props.label ?? "text"} available.`}
      </pre>
    </section>
  );
}

function AirspaceLimitGlyph(props: { glyph: { upper: string; lower: string; style_key: string; color_key: string }; scale?: number }) {
  const { glyph, scale = 1 } = props;
  const color = aviationThemeColor(glyph.color_key);
  const labelStyle = { fill: color, fontSize: `${14 * scale}px` };
  const parts = { upper: glyph.upper, lower: glyph.lower };
  const dividerWidth = airspaceLabelDividerWidth(parts) * scale;
  return (
    <g className={`airspaceLabel-${glyph.style_key}`} style={{ "--airspace-label-color": color } as React.CSSProperties}>
      <text className="airspaceLabel" style={labelStyle} x="0" y={-7 * scale}>
        {parts.upper}
      </text>
      <line
        className="airspaceLabelDividerContrast"
        x1={-dividerWidth / 2}
        y1="0"
        x2={dividerWidth / 2}
        y2="0"
      />
      <line
        className="airspaceLabelDivider"
        x1={-dividerWidth / 2}
        y1="0"
        x2={dividerWidth / 2}
        y2="0"
      />
      <text className="airspaceLabel" style={labelStyle} x="0" y={9 * scale}>
        {parts.lower}
      </text>
    </g>
  );
}

function MapSelectionItemIcon(props: { item: MapSelectionItem }) {
  const { item } = props;
  if (item.symbol_feature) {
    return (
      <span className="mapSelectionItemIcon" aria-hidden="true">
        <PlanWaypointSymbol feature={item.symbol_feature} />
      </span>
    );
  }
  if (item.metar_feature) {
    return (
      <svg className="mapSelectionItemIcon mapSelectionMetarIcon" viewBox="-20 -20 40 40" aria-hidden="true">
        <MetarSymbol feature={item.metar_feature} />
      </svg>
    );
  }
  if (item.pirep_feature) {
    return (
      <svg className="mapSelectionItemIcon mapSelectionMetarIcon" viewBox="-34 -34 74 78" aria-hidden="true">
        <PirepSymbol feature={item.pirep_feature} />
      </svg>
    );
  }
  if (item.highlight.kind === "spot") {
    return (
      <svg className="mapSelectionItemIcon mapSelectionSpotIcon" viewBox="-20 -40 40 46" aria-hidden="true">
        <MapSelectionSpotSymbol />
      </svg>
    );
  }
  if (item.airspace_icon) {
    return (
      <svg className="mapSelectionItemIcon mapSelectionAirspaceIcon" viewBox="0 0 64 64" aria-hidden="true">
        <AirspaceDisplayPathGroup feature={item.airspace_icon} />
      </svg>
    );
  }
  return <span className="mapSelectionItemTextIcon">{item.sublabel || item.label}</span>;
}

function DataStatusWarningFace(props: { count?: string | null }) {
  return (
    <>
      <svg className="dataStatusLauncherSymbol" viewBox="-50 -50 100 100" aria-hidden="true" focusable="false">
        <RenderNavSymbolLayers layers={dataStatusWarningSymbol} />
      </svg>
      {props.count ? <span className="dataStatusLauncherCount">{props.count}</span> : null}
    </>
  );
}

function statusControlTrayId(id: UiSurfaceStatusControlId): "procedureWarning" | "status" {
  switch (id) {
    case "global":
      return "status";
    case "procedure_geometry":
      return "procedureWarning";
  }
}

function statusControlTestIdPrefix(id: UiSurfaceStatusControlId): string | undefined {
  return id === "procedure_geometry" ? "procedure-status" : undefined;
}

function ChartsPage(props: {
  sessionRenderStore: SessionRenderStore;
  appCoreAdapter: AppCoreAdapter | null;
  page: AppPage;
  planUiState: FlightPlanUiState | null;
  flightPlanRouteRevision: number;
  navDataEpoch: number;
  airportMenuEntries: DerivedChartPageState["airport_menu_entries"];
  selectedCollection: ChartPageData["airports"][number] | null;
  selectedChart: ChartAsset | null;
  suggestedChartIds: string[];
  collectionControl: DerivedChartPageState["collection_control"];
  chartControl: DerivedChartPageState["chart_control"];
  projectedProcedureLoadMenu: ProcedureLoadMenu;
  statusControls: UiSurfaceStatusState;
  folderOpen: boolean;
  viewport: ImageViewportState | null;
  onViewportChange: (next: ImageViewportState | null) => void;
  onFolderOpenChange: (next: boolean) => void;
  onSelectPage: (page: AppPage) => void;
  onOpenPlan: () => void;
  onSelectAirport: (airportId: string) => void;
  onSelectReference: (familyId: ChartFamilyId) => void;
  onSelectChart: (chartId: string) => void;
  playbackSourcePath: string;
  onPlaybackSourcePathChange: Dispatch<SetStateAction<string>>;
  onPlaybackSnapshotChange: Dispatch<SetStateAction<UiSessionSnapshot>>;
  onSituationControlInput: (input: SituationControlInput) => void;
  onStatusAction: (actionId: string) => void | Promise<void>;
  debugState: UiDebugState;
  uiSession: UiSession | null;
  onFirstVisualReady: () => void;
}) {
  recordSessionRender("charts");
  const highRateSnapshot = useSessionSnapshotGroups(
    props.sessionRenderStore,
    props.page === "charts" ? HIGH_RATE_SESSION_UPDATE_GROUPS : NO_SESSION_UPDATE_GROUPS,
  );
  const flightPlanSnapshot = useSessionSnapshotGroups(
    props.sessionRenderStore,
    props.page === "charts" ? FLIGHT_PLAN_SESSION_UPDATE_GROUPS : NO_SESSION_UPDATE_GROUPS,
  );
  const ownship = highRateSnapshot.app_ui_state.ownship.render;
  const aircraftPlanViewPath = flightPlanSnapshot.app_ui_state.aircraft_plan_view_path;
  const ownshipControls = highRateSnapshot.app_ui_state.ownship.controls;
  const playbackUiState = highRateSnapshot.playback_ui_state;
  const playbackPanelState = highRateSnapshot.playback_panel_state;
  const { appCoreAdapter, page, planUiState, airportMenuEntries, selectedCollection, selectedChart, suggestedChartIds, collectionControl, chartControl, projectedProcedureLoadMenu, statusControls, folderOpen, viewport, onViewportChange, onFolderOpenChange, onSelectPage, onOpenPlan, onSelectAirport, onSelectReference, onSelectChart, onStatusAction, uiSession, onFirstVisualReady, navDataEpoch } = props;
  const containerRef = useRef<HTMLDivElement | null>(null);
  const imageRef = useRef<HTMLImageElement | null>(null);
  const [surfaceSize, setSurfaceSize] = useState<SurfaceSize>({ width: 0, height: 0 });
  const [imageSize, setImageSize] = useState<{ chartId: string; width: number; height: number } | null>(null);
  const viewportRef = useRef<ImageViewportState | null>(null);
  const lastLocalViewportRef = useRef<ImageViewportState | null>(null);
  const wheelGestureUntilRef = useRef(0);
  const activePointersRef = useRef<Map<number, ScreenPoint>>(new Map());
  const dragRef = useRef<{ id: number; last: ScreenPoint } | null>(null);
  const pinchRef = useRef<{ viewport: ImageViewportState; distance: number; midpoint: ScreenPoint } | null>(null);
  const statusControlDockLowered = shouldLowerStatusControlDock(
    surfaceSize.width,
    statusControls.controls.filter((control) => control.state.boxes.length > 0).length,
  );
  const lastChartLayoutKeyRef = useRef("");
  const firstVisualReadyRef = useRef(false);
  const trayGroup = useModalTrayGroup(["airport", "chart", "load", "procedureWarning", "status", "ownship"] as const);
  const [procedureNotamDetail, setProcedureNotamDetail] = useState<
    NonNullable<ChartAsset["procedure_notam_badge"]>["detail"] | null
  >(null);
  const [plateProcedureLoadMenu, setPlateProcedureLoadMenu] = useState<ProcedureLoadMenu>(
    projectedProcedureLoadMenu,
  );
  const [plateFlightPlanRouteProjection, setPlateFlightPlanRouteProjection] = useState<FlightPlanRouteProjection>({
    flight_plan_route_revision: -1,
    segments: [],
    distance_annotations: [],
  });
  const [resolvedChartUrls, setResolvedChartUrls] = useState<Record<string, ResolvedChartUrls>>({});
  const { toast: disabledActionToast, show: showDisabledAction } = useDisabledActionToast();
  const trayOpen = trayGroup.scrimOpen;
  const sortedCharts = selectedCollection?.charts ?? [];
  const planProcedureLoadKey = props.flightPlanRouteRevision;
  const selectedImageSize = imageSize && imageSize.chartId === (selectedChart?.id ?? "") ? imageSize : null;
  const selectedChartAssetUrl = selectedChart ? resolvedChartUrls[selectedChart.id]?.assetUrl ?? null : null;
  const fallbackViewport = useMemo(() => {
    if (!selectedImageSize || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return null;
    }
    return createInitialImageViewport(selectedImageSize.width, selectedImageSize.height, surfaceSize.width, surfaceSize.height);
  }, [selectedImageSize, surfaceSize.height, surfaceSize.width]);
  const effectiveViewport = viewport ?? fallbackViewport;
  const displaySize = useMemo(() => {
    if (!selectedImageSize || !effectiveViewport || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return null;
    }
    return imageDisplaySize(
      selectedImageSize.width,
      selectedImageSize.height,
      surfaceSize.width,
      surfaceSize.height,
      effectiveViewport.zoom,
    );
  }, [selectedImageSize, surfaceSize.height, surfaceSize.width, effectiveViewport]);
  const plateOwnshipOverlay = useMemo(
    () => resolvePlateOwnshipOverlay(ownship, selectedChart?.georef ?? null, selectedImageSize, effectiveViewport, displaySize),
    [displaySize, effectiveViewport, ownship, selectedChart?.georef, selectedImageSize],
  );
  const plateFlightPlanScreenSegments = useMemo(() => {
    if (
      !props.debugState.plate_flight_plan ||
      plateFlightPlanRouteProjection.flight_plan_route_revision !== props.flightPlanRouteRevision ||
      !selectedChart?.georef || !selectedImageSize || !effectiveViewport || !displaySize
    ) {
      return [];
    }
    return projectPlateFlightPlanSegments({
      segments: plateFlightPlanRouteProjection.segments,
      georef: selectedChart.georef,
      imageSize: selectedImageSize,
      viewport: effectiveViewport,
      displaySize,
      surfaceSize,
    });
  }, [
    displaySize,
    effectiveViewport,
    plateFlightPlanRouteProjection,
    props.debugState.plate_flight_plan,
    props.flightPlanRouteRevision,
    selectedChart?.georef,
    selectedImageSize,
    surfaceSize,
  ]);

  useEffect(() => {
    if (
      page !== "charts" ||
      !props.debugState.plate_flight_plan ||
      !selectedChart?.georef ||
      !uiSession
    ) {
      setPlateFlightPlanRouteProjection({
        flight_plan_route_revision: props.flightPlanRouteRevision,
        segments: [],
        distance_annotations: [],
      });
      return;
    }
    let cancelled = false;
    void uiSession.projectFlightPlanRoute().then((projection) => {
      if (!cancelled) {
        setPlateFlightPlanRouteProjection(projection);
      }
    }).catch((error: unknown) => {
      debugLog("charts.plate_flight_plan.unavailable", {
        chart_id: selectedChart.id,
        error: errorMessage(error),
      });
      if (!cancelled) {
        setPlateFlightPlanRouteProjection({
          flight_plan_route_revision: props.flightPlanRouteRevision,
          segments: [],
          distance_annotations: [],
        });
      }
    });
    return () => {
      cancelled = true;
    };
  }, [
    page,
    props.debugState.plate_flight_plan,
    props.flightPlanRouteRevision,
    selectedChart?.georef,
    selectedChart?.id,
    uiSession,
  ]);

  useEffect(() => {
    if (!containerRef.current) {
      return;
    }
    const observer = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (!entry) {
        return;
      }
      setSurfaceSize({
        width: entry.contentRect.width,
        height: entry.contentRect.height,
      });
    });
    observer.observe(containerRef.current);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    setResolvedChartUrls({});
    setImageSize(null);
    viewportRef.current = null;
    lastLocalViewportRef.current = null;
    lastChartLayoutKeyRef.current = "";
  }, [navDataEpoch, selectedChart?.id]);

  useEffect(() => {
    if (!selectedChart || !uiSession) {
      return;
    }
    let cancelled = false;
    void uiSession.resolveChartAssetUrl(selectedChart.id, "asset")
      .then((assetUrl) => {
        if (cancelled) {
          return;
        }
        setResolvedChartUrls((current) => ({
          ...current,
          [selectedChart.id]: {
            ...current[selectedChart.id],
            assetUrl,
          },
        }));
      })
      .catch((error) => {
        debugLog("charts.asset.resolve_failed", {
          chart_id: selectedChart.id,
          error: errorMessage(error),
        });
        if (!cancelled) {
          setResolvedChartUrls((current) => ({
            ...current,
            [selectedChart.id]: {
              ...current[selectedChart.id],
              assetUrl: null,
            },
          }));
        }
      });
    return () => {
      cancelled = true;
    };
  }, [selectedChart?.id, uiSession, navDataEpoch]);

  useEffect(() => {
    if (!folderOpen || !uiSession) {
      return;
    }
    let cancelled = false;
    const chartsToResolve = sortedCharts.filter((chart) =>
      chart.has_thumbnail && resolvedChartUrls[chart.id]?.thumbnailUrl === undefined,
    );
    if (chartsToResolve.length === 0) {
      return;
    }
    void Promise.all(chartsToResolve.map(async (chart) => {
      try {
        return {
          chart,
          thumbnailUrl: chart.has_thumbnail
            ? await uiSession.resolveChartAssetUrl(chart.id, "thumbnail")
            : null,
        };
      } catch (error) {
        debugLog("charts.thumbnail.resolve_failed", {
          chart_id: chart.id,
          error: errorMessage(error),
        });
        return { chart, thumbnailUrl: null };
      }
    })).then((resolved) => {
      if (cancelled) {
        return;
      }
      setResolvedChartUrls((current) => {
        const next = { ...current };
        for (const { chart, thumbnailUrl } of resolved) {
          next[chart.id] = {
            ...next[chart.id],
            thumbnailUrl,
          };
        }
        return next;
      });
    });
    return () => {
      cancelled = true;
    };
  }, [folderOpen, navDataEpoch, resolvedChartUrls, sortedCharts, uiSession]);

  useEffect(() => {
    const img = imageRef.current;
    if (!selectedChart || !img) {
      return;
    }
    if (!img.complete || img.naturalWidth <= 0 || img.naturalHeight <= 0) {
      return;
    }
    setImageSize((current) => {
      if (
        current &&
        current.chartId === selectedChart.id &&
        current.width === img.naturalWidth &&
        current.height === img.naturalHeight
      ) {
        return current;
      }
      return {
        chartId: selectedChart.id,
        width: img.naturalWidth,
        height: img.naturalHeight,
      };
    });
  }, [selectedChart?.id, selectedChartAssetUrl]);

  useEffect(() => {
    if (viewport === null) {
      viewportRef.current = null;
      lastLocalViewportRef.current = null;
      return;
    }
    if (activePointersRef.current.size > 0) {
      return;
    }
    if (Date.now() < wheelGestureUntilRef.current) {
      return;
    }
    if (lastLocalViewportRef.current === viewport) {
      return;
    }
    viewportRef.current = viewport;
  }, [viewport]);

  useEffect(() => {
    if (!selectedImageSize || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
      return;
    }
    const layoutKey = `${selectedChart?.id ?? ""}:${selectedImageSize.width}:${selectedImageSize.height}:${surfaceSize.width}:${surfaceSize.height}`;
    if (viewport === null) {
      const next = createInitialImageViewport(selectedImageSize.width, selectedImageSize.height, surfaceSize.width, surfaceSize.height);
      viewportRef.current = next;
      lastLocalViewportRef.current = next;
      lastChartLayoutKeyRef.current = layoutKey;
      onViewportChange(next);
      return;
    }
    if (lastChartLayoutKeyRef.current === layoutKey) {
      return;
    }
    const normalized = clampImageViewport(
      viewport,
      selectedImageSize.width,
      selectedImageSize.height,
      surfaceSize.width,
      surfaceSize.height,
      overscrollPx,
    );
    viewportRef.current = normalized;
    lastLocalViewportRef.current = normalized;
    lastChartLayoutKeyRef.current = layoutKey;
    if (normalized.left !== viewport.left || normalized.top !== viewport.top || normalized.zoom !== viewport.zoom) {
      onViewportChange(normalized);
    }
  }, [selectedImageSize, selectedChart?.id, surfaceSize.width, surfaceSize.height, viewport, onViewportChange]);

  const overscrollPx = 64;

  useEffect(() => {
    if (page !== "charts") {
      return;
    }
    if (!props.uiSession || !selectedChart || selectedChart.kind !== "plate") {
      setPlateProcedureLoadMenu(projectedProcedureLoadMenu);
      return;
    }
    let cancelled = false;
    debugLog("charts.load_procedure.query", { plate_id: selectedChart.id });
    void props.uiSession.describePlateProcedureLoads(selectedChart.id).then((menu) => {
      debugLog("charts.load_procedure.result", {
        plate_id: selectedChart.id,
        load_count: menu.options.length,
        menu,
      });
      if (!cancelled) {
        setPlateProcedureLoadMenu(menu);
      }
    }).catch((error: unknown) => {
      debugLog("charts.load_procedure.unavailable", {
        plate_id: selectedChart.id,
        error: errorMessage(error),
      });
      if (!cancelled) {
        setPlateProcedureLoadMenu(projectedProcedureLoadMenu);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [appCoreAdapter, page, planProcedureLoadKey, projectedProcedureLoadMenu, selectedChart?.id]);

  const loadProcedureOptions = useMemo(() => {
    return plateProcedureLoadMenu.options.map((load, index) => ({
        id: `${load.load_id}:${index}`,
        label: load.label,
        active: false,
        onSelect: () => {
          if (!props.uiSession) {
            return;
          }
          void props.uiSession.loadPlateProcedure(load.load_id).then((nextSnapshot) => {
            props.onPlaybackSnapshotChange(nextSnapshot);
            trayGroup.close("load");
          }).catch(() => {});
        },
      }));
  }, [plateProcedureLoadMenu.options, props, trayGroup]);
  const ownshipSourceOptions: TrayOption[] = ownshipControls.sources.map((source) => ({
    id: sourceIdString(source.source_id),
    label: source.label,
    active: source.active,
    disabled: !source.enabled || !props.uiSession,
    disabledReason: !props.uiSession ? "Ownship controls are not ready yet." : source.disabled_reason ?? null,
    onSelect: () => {
      if (!props.uiSession) {
        return;
      }
      void props.uiSession
        .selectOwnshipSource({ kind: "source", source_id: sourceIdString(source.source_id) })
        .then(props.onPlaybackSnapshotChange)
        .finally(() => {
          if (!source.keep_tray_open_on_select) trayGroup.close("ownship");
        });
    },
  }));
  const folderDisabledReason = trayOpen ? "Close the open tray first." : null;

  function localPointFromPointerEvent(
    event:
      | React.PointerEvent<HTMLDivElement>
      | React.MouseEvent<HTMLDivElement>
      | React.WheelEvent<HTMLDivElement>,
  ) {
    const rect = event.currentTarget.getBoundingClientRect();
    return {
      x: event.clientX - rect.left,
      y: event.clientY - rect.top,
    };
  }

  function updateViewport(next: ImageViewportState) {
    viewportRef.current = next;
    lastLocalViewportRef.current = next;
    onViewportChange(next);
  }

  function handlePointerDown(event: React.PointerEvent<HTMLDivElement>) {
    if (!viewportRef.current || !selectedImageSize || trayOpen || folderOpen) {
      return;
    }
    const point = localPointFromPointerEvent(event);
    activePointersRef.current.set(event.pointerId, point);
    event.currentTarget.setPointerCapture(event.pointerId);
    if (activePointersRef.current.size === 1) {
      dragRef.current = { id: event.pointerId, last: point };
      pinchRef.current = null;
    } else if (activePointersRef.current.size >= 2) {
      const [first, second] = Array.from(activePointersRef.current.values());
      pinchRef.current = {
        viewport: viewportRef.current,
        distance: distanceBetween(first, second),
        midpoint: midpoint(first, second),
      };
      dragRef.current = null;
    }
  }

  function handlePointerMove(event: React.PointerEvent<HTMLDivElement>) {
    if (!viewportRef.current || !selectedImageSize || trayOpen || folderOpen) {
      return;
    }
    const point = localPointFromPointerEvent(event);
    if (!activePointersRef.current.has(event.pointerId)) {
      return;
    }
    activePointersRef.current.set(event.pointerId, point);
    const pointers = Array.from(activePointersRef.current.values());
    if (pointers.length === 1 && dragRef.current?.id === event.pointerId) {
      const dx = point.x - dragRef.current.last.x;
      const dy = point.y - dragRef.current.last.y;
      const next = dragImageViewport(
        viewportRef.current,
        dx,
        dy,
        selectedImageSize.width,
        selectedImageSize.height,
        surfaceSize.width,
        surfaceSize.height,
        overscrollPx,
      );
      updateViewport(next);
      dragRef.current = { id: event.pointerId, last: point };
      return;
    }
    if (pointers.length >= 2 && pinchRef.current) {
      const [first, second] = pointers;
      const nextDistance = distanceBetween(first, second);
      const nextMidpoint = midpoint(first, second);
      const zoomDelta = pinchRef.current.distance > 0 ? Math.log2(nextDistance / pinchRef.current.distance) : 0;
      let next = zoomImageAroundPoint(
        pinchRef.current.viewport,
        pinchRef.current.midpoint.x,
        pinchRef.current.midpoint.y,
        clampImageZoom(pinchRef.current.viewport.zoom + zoomDelta),
        selectedImageSize.width,
        selectedImageSize.height,
        surfaceSize.width,
        surfaceSize.height,
        overscrollPx,
      );
      next = dragImageViewport(
        next,
        nextMidpoint.x - pinchRef.current.midpoint.x,
        nextMidpoint.y - pinchRef.current.midpoint.y,
        selectedImageSize.width,
        selectedImageSize.height,
        surfaceSize.width,
        surfaceSize.height,
        overscrollPx,
      );
      updateViewport(next);
    }
  }

  function handlePointerRelease(event: React.PointerEvent<HTMLDivElement>) {
    activePointersRef.current.delete(event.pointerId);
    pinchRef.current = null;
    const remaining = Array.from(activePointersRef.current.entries());
    if (remaining.length === 1) {
      dragRef.current = { id: remaining[0][0], last: remaining[0][1] };
    } else {
      dragRef.current = null;
    }
  }

  function handleWheel(event: React.WheelEvent<HTMLDivElement>) {
    if (folderOpen) {
      return;
    }
    if (!viewportRef.current || !selectedImageSize || trayOpen) {
      event.preventDefault();
      return;
    }
    event.preventDefault();
    wheelGestureUntilRef.current = Date.now() + 160;
    const point = localPointFromPointerEvent(event);
    const zoomTarget = viewportRef.current.zoom - event.deltaY / 360;
    const next = zoomImageAroundPoint(
      viewportRef.current,
      point.x,
      point.y,
      zoomTarget,
      selectedImageSize.width,
      selectedImageSize.height,
      surfaceSize.width,
      surfaceSize.height,
      overscrollPx,
    );
    updateViewport(next);
  }

  function handleDoubleClick(event: React.MouseEvent<HTMLDivElement>) {
    if (!viewportRef.current || !selectedImageSize || trayOpen || folderOpen) {
      return;
    }
    const point = localPointFromPointerEvent(event);
    updateViewport(
      zoomImageAroundPoint(
        viewportRef.current,
        point.x,
        point.y,
        viewportRef.current.zoom + 0.75,
        selectedImageSize.width,
        selectedImageSize.height,
        surfaceSize.width,
        surfaceSize.height,
        overscrollPx,
      ),
    );
  }

  function reportFirstVisualReady() {
    if (firstVisualReadyRef.current) {
      return;
    }
    firstVisualReadyRef.current = true;
    onFirstVisualReady();
  }

  return (
    <section className="pageSurface" data-testid="parity:page:plate">
      <div
        ref={containerRef}
        className="mapSurface chartSurface"
        data-testid="plate-surface"
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerRelease}
        onPointerCancel={handlePointerRelease}
        onPointerLeave={handlePointerRelease}
        onWheel={handleWheel}
        onDoubleClick={handleDoubleClick}
      >
        {effectiveViewport && selectedChart ? (
          <span
            data-testid={`parity:plate-viewport:chart:${selectedChart.id}:zoom:${effectiveViewport.zoom.toFixed(3)}:left:${effectiveViewport.left.toFixed(1)}:top:${effectiveViewport.top.toFixed(1)}`}
            aria-hidden="true"
          />
        ) : null}
        <span
          data-testid={`parity:plate-ownship-input:draw:${ownship.draw_aircraft}:position:${ownship.position ? `${ownship.position.lat.toFixed(5)},${ownship.position.lon.toFixed(5)}` : "none"}:georef:${selectedChart?.georef ? "yes" : "no"}:image-point:${ownship.position && selectedChart?.georef ? (() => { const point = plateImagePoint(ownship.position!, selectedChart.georef!); return point ? `${point.x.toFixed(1)},${point.y.toFixed(1)}` : "none"; })() : "none"}:image-size:${selectedImageSize ? `${selectedImageSize.width},${selectedImageSize.height}` : "none"}`}
          aria-hidden="true"
        />
        <div className="mapBackdrop" />
        <StatusControlDock
          controls={ownshipControls}
          dataStatuses={statusControls.controls.map((control) => {
            const trayId = statusControlTrayId(control.id);
            return {
              id: control.id,
              state: control.state,
              open: trayGroup.isOpen(trayId),
              onToggle: () => trayGroup.toggle(trayId),
              onAction: onStatusAction,
              testIdPrefix: statusControlTestIdPrefix(control.id),
            };
          })}
          lowered={statusControlDockLowered}
          leadingControl={!folderOpen && selectedChart?.procedure_notam_badge ? (
            <PlateProcedureNotamBadgeButton
              badge={selectedChart.procedure_notam_badge}
              placement="dock"
              onOpen={() => {
                trayGroup.closeAll();
                setProcedureNotamDetail(selectedChart.procedure_notam_badge!.detail);
              }}
            />
          ) : null}
          ownshipOpen={trayGroup.isOpen("ownship")}
          onOwnshipToggle={() => trayGroup.toggle("ownship")}
          options={ownshipSourceOptions}
          onDisabledAction={showDisabledAction}
          transportControls={
            <SituationControlFooter
              controls={ownshipControls}
              onInput={props.onSituationControlInput}
              onTextAction={(actionId, value) => {
                if (!props.uiSession) return;
                void props.uiSession.performOwnshipTextAction(actionId, value, Date.now())
                  .then(props.onPlaybackSnapshotChange)
                  .catch((error: unknown) => showDisabledAction(errorMessage(error)));
              }}
              onDisabledAction={showDisabledAction}
            />
          }
        />
        {trayOpen ? <TrayScrim ariaLabel="Close chart tray" onClose={trayGroup.closeAll} /> : null}

        {folderOpen ? (
          <>
            <div className="plateFolderGrid" onPointerDown={stopPointer} onPointerUp={stopPointer} onDoubleClick={stopDoubleClick}>
              {sortedCharts.map((chart) => (
                <div className="plateThumbShell" key={chart.id}>
                  <button
                    type="button"
                    className={`plateThumb${chart.id === selectedChart?.id ? " isActive" : ""}${suggestedChartIds.includes(chart.id) ? " isSuggested" : ""}`}
                    data-testid={`plate-folder-tile:${chart.id}`}
                    onPointerDown={stopPointer}
                    onPointerUp={stopPointer}
                    onDoubleClick={stopDoubleClick}
                    onClick={() => {
                      onSelectChart(chart.id);
                    }}
                  >
                    <div className="plateThumbMedia" style={{ backgroundColor: plateFolderTheme.thumbnail_bg }}>
                      {resolvedChartUrls[chart.id]?.thumbnailUrl ? (
                        <img
                          className="plateThumbImage"
                          src={resolvedChartUrls[chart.id]?.thumbnailUrl ?? undefined}
                          alt=""
                          draggable={false}
                        />
                      ) : null}
                      <div className="plateThumbLabel" style={{ backgroundColor: plateFolderColor(chart.folder_category) }}>
                        {chart.label}
                      </div>
                    </div>
                  </button>
                  {chart.procedure_notam_badge || chart.procedure_geometry_warning_count > 0 ? (
                    <div className="plateThumbStickerRow">
                      {chart.procedure_notam_badge ? (
                        <PlateProcedureNotamBadgeButton
                          badge={chart.procedure_notam_badge}
                          placement="folder"
                          onOpen={() => setProcedureNotamDetail(chart.procedure_notam_badge!.detail)}
                        />
                      ) : null}
                      {chart.procedure_geometry_warning_count > 0 ? (
                        <span
                          className="plateProcedureWarningMini"
                          aria-label={`${chart.procedure_geometry_warning_count} procedure geometry warning${chart.procedure_geometry_warning_count === 1 ? "" : "s"}; verify against the published plate`}
                          title="Computed procedure geometry requires verification against the published plate"
                        >
                          <DataStatusWarningFace count={chart.procedure_geometry_warning_count.toString()} />
                        </span>
                      ) : null}
                    </div>
                  ) : null}
                </div>
              ))}
            </div>
            {selectedCollection?.unmatched_procedure_notam_badge ? (
              <div className="plateFolderUnmatchedNotamBadge">
                <PlateProcedureNotamBadgeButton
                  badge={selectedCollection.unmatched_procedure_notam_badge}
                  placement="dock"
                  onOpen={() => setProcedureNotamDetail(selectedCollection.unmatched_procedure_notam_badge!.detail)}
                />
              </div>
            ) : null}
          </>
        ) : selectedChart && selectedChartAssetUrl ? (
          <>
            <img
              key={selectedChart.id}
              ref={imageRef}
              className="chartImage"
              src={selectedChartAssetUrl}
              alt={selectedChart.label}
              draggable={false}
              onLoad={(event) =>
                {
                  setImageSize({
                    chartId: selectedChart.id,
                    width: event.currentTarget.naturalWidth,
                    height: event.currentTarget.naturalHeight,
                  });
                  reportFirstVisualReady();
                }
              }
              style={{
                left: `${selectedImageSize && effectiveViewport ? effectiveViewport.left : 0}px`,
                top: `${selectedImageSize && effectiveViewport ? effectiveViewport.top : 0}px`,
                width: displaySize ? `${displaySize.width}px` : undefined,
                height: displaySize ? `${displaySize.height}px` : undefined,
                visibility: selectedImageSize && effectiveViewport ? "visible" : "hidden",
              }}
            />
            {plateFlightPlanScreenSegments.length > 0 ? (
              <svg
                className="plateFlightPlanOverlay"
                data-testid={`plate-flight-plan-overlay:segments:${plateFlightPlanScreenSegments.length}`}
                viewBox={`0 0 ${surfaceSize.width} ${surfaceSize.height}`}
                preserveAspectRatio="none"
              >
                {plateFlightPlanScreenSegments.map((segment, segmentIndex) => (
                  <Fragment key={`${segment.id}:${segmentIndex}`}>
                    <FlightPlanRoutePath segment={segment} />
                  </Fragment>
                ))}
              </svg>
            ) : null}
            {plateOwnshipOverlay ? (
              <div data-testid="plate-ownship-overlay">
                <SituationAircraft
                  pathData={aircraftPlanViewPath}
                  point={plateOwnshipOverlay.point}
                  headingDeg={plateOwnshipOverlay.headingDeg}
                />
              </div>
            ) : null}
          </>
        ) : null}

        {procedureNotamDetail ? (
          <>
            <TrayScrim ariaLabel="Close procedure NOTAMs" onClose={() => setProcedureNotamDetail(null)} />
            <ProcedureNotamModal detail={procedureNotamDetail} />
          </>
        ) : null}

        <div className="chartDock chartDockDouble plateDock">
          <TrayDock
            launcherLabel={collectionControl.launcher_label}
            open={trayGroup.isOpen("airport")}
            disabled={!collectionControl.enabled}
            disabledReason={collectionControl.disabled_reason}
            onDisabledAction={showDisabledAction}
            onToggle={() => trayGroup.toggle("airport")}
            ariaLabel="Airport"
            style="plate_narrow"
            testId="plate-airport-button"
            options={airportMenuEntries.map((entry, index) => {
              if (entry.kind === "separator") {
                return {
                  kind: "separator",
                  id: `separator:${index}:${entry.label}`,
                  label: entry.label,
                };
              }
              if (entry.kind === "airport") {
                const { airport } = entry;
                return {
                  id: `airport:${airport.id}`,
                  label: airport.id,
                  active: airport.id === selectedCollection?.id,
                  onSelect: () => {
                    onSelectAirport(airport.id);
                    trayGroup.close("airport");
                  },
                };
              }
              if (entry.kind === "external_link") {
                return {
                  id: `external-link:${entry.url}`,
                  label: entry.label,
                  onSelect: () => {
                    if (typeof window !== "undefined") {
                      window.open(entry.url, "_blank", "noopener,noreferrer");
                    }
                    trayGroup.close("airport");
                  },
                };
              }
              const { reference } = entry;
              return {
                id: `reference:${reference.id}`,
                label: reference.label,
                active: reference.id === selectedCollection?.id,
                onSelect: () => {
                  onSelectReference(reference.id as ChartFamilyId);
                  trayGroup.close("airport");
                },
              };
            })}
          />
          <TrayDock
            launcherLabel={chartControl.launcher_label}
            open={trayGroup.isOpen("chart")}
            disabled={!chartControl.enabled}
            disabledReason={chartControl.disabled_reason}
            onDisabledAction={showDisabledAction}
            launcherClassName="plateChartSelector"
            launcherAccentColor={selectedChart ? plateFolderColor(selectedChart.folder_category) : undefined}
            onToggle={() => trayGroup.toggle("chart")}
            ariaLabel="Chart"
            style="plate_wide"
            testId="plate-chart-button"
            options={sortedCharts.map((chart) => ({
              id: chart.id,
              label: chart.label,
              active: chart.id === selectedChart?.id,
              accentColor: plateFolderColor(chart.folder_category),
              onSelect: () => {
                onSelectChart(chart.id);
                trayGroup.close("chart");
              },
            }))}
          />
          <TrayDock
            launcherLabel={plateProcedureLoadMenu.launcher_label}
            open={trayGroup.isOpen("load")}
            disabled={!plateProcedureLoadMenu.enabled}
            disabledReason={plateProcedureLoadMenu.disabled_reason}
            onToggle={() => trayGroup.toggle("load")}
            ariaLabel="Load procedure"
            testId="plate-load-button"
            onDisabledAction={showDisabledAction}
            header={plateProcedureLoadMenu.header}
            headerTone={plateProcedureLoadMenu.header_tone}
            options={loadProcedureOptions}
          />
          <button
            type="button"
            className={`chartButton${folderOpen ? " isOpen" : ""}`}
            aria-disabled={trayOpen || folderOpen}
            title={folderDisabledReason ?? undefined}
            tabIndex={trayOpen ? -1 : undefined}
            onPointerDown={stopPointer}
            onPointerUp={stopPointer}
            onDoubleClick={stopDoubleClick}
            onClick={() => {
              if (folderDisabledReason) {
                showDisabledAction(folderDisabledReason);
                return;
              }
              if (folderOpen) {
                return;
              }
              onFolderOpenChange(true);
            }}
            aria-pressed={folderOpen}
            aria-label="Open plate folder view"
            data-testid="plate-folder-button"
          >
            <span className="chartButtonLabel">FLDR</span>
          </button>
        </div>

        <PrimaryNavigationDock
          page={page}
          navElement={planUiState?.guidance?.nav_element}
          onSelectPage={onSelectPage}
          onOpenPlan={onOpenPlan}
        />

        {playbackPanelState.visible ? (
          <PlaybackWidget
            uiSession={props.uiSession}
            playbackUiState={playbackUiState}
            sourcePath={props.playbackSourcePath}
            onSourcePathChange={props.onPlaybackSourcePathChange}
            onSnapshotChange={props.onPlaybackSnapshotChange}
            surfaceWidth={surfaceSize.width}
            dock="left"
            onDisabledAction={showDisabledAction}
          />
        ) : null}
        {disabledActionToast ? (
          <div className="mapSelectionToast" data-testid="disabled-action-toast" role="status" aria-live="polite">
            {disabledActionToast.message}
          </div>
        ) : null}
      </div>
    </section>
  );
}

function HomePage(props: {
  page: AppPage;
  state: UiHomePageState;
  planUiState: FlightPlanUiState | null;
  mostRecentChartOrPlatePage: AppPage;
  onOpenRecentChartOrPlate: () => void;
  onSelectPage: (page: AppPage) => void;
  onOpenPlan: () => void;
}) {
  const { page, planUiState, onSelectPage, onOpenPlan } = props;
  const { toast: disabledActionToast, show: showDisabledAction } = useDisabledActionToast();

  return (
    <section
      className="appPage homePage"
      data-testid="parity:page:home"
      style={{ "--home-page-backdrop": `url(${HOME_PAGE_BACKDROP_SRC})` } as CSSProperties}
    >
      <div
        className="homeGrid"
        aria-label="Home navigation"
        style={{ "--home-grid-column-count": HOME_GRID_COLUMN_COUNT } as CSSProperties}
      >
        {props.state.buttons.map((button) => {
          const presentation = webHomeButtonPresentation(button.destination);
          const disabledReason = disabledReasonText(button.disabled_reason);
          const disabled = !button.enabled;
          return (
            <button
              key={button.destination}
              type="button"
              className={`chartButton chartButtonDouble homeButton${presentation.page === page ? " isOpen" : ""}${disabled ? " isDisabled" : ""}`}
              data-testid={`home-button-${button.destination}`}
              disabled={disabled && !disabledReason}
              aria-disabled={disabled ? "true" : undefined}
              title={disabledReason ?? undefined}
              onPointerDown={stopPointer}
              onPointerUp={stopPointer}
              onDoubleClick={stopDoubleClick}
              onClick={() => {
                if (disabled) {
                  if (disabledReason) {
                    showDisabledAction(disabledReason);
                  }
                  return;
                }
                if (!presentation.page) {
                  throw new Error(`Enabled core Home button has no web navigation target: ${button.destination}`);
                }
                onSelectPage(presentation.page);
              }}
            >
              {presentation.iconSrc ? <img className="chartButtonIcon" src={presentation.iconSrc} alt="" aria-hidden="true" /> : null}
              <span className="chartButtonLabel chartButtonLabelDouble">{button.label}</span>
            </button>
          );
        })}
      </div>

      <PrimaryNavigationDock
        page={page}
        navElement={planUiState?.guidance?.nav_element}
        chartPlateTargetPage={props.mostRecentChartOrPlatePage}
        onSelectPage={onSelectPage}
        onOpenPlan={onOpenPlan}
        onOpenChartOrPlate={props.onOpenRecentChartOrPlate}
      />
      {disabledActionToast ? (
        <div className="mapSelectionToast" data-testid="disabled-action-toast" role="status" aria-live="polite">
          {disabledActionToast.message}
        </div>
      ) : null}
    </section>
  );
}

function AboutPage() {
  const [metadata, setMetadata] = useState<AndroidApkDownloadMetadata | null>(null);
  const [metadataError, setMetadataError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setMetadataError(null);
    fetch(androidApkMetadataPath, { cache: "no-cache" })
      .then(async (response) => {
        if (!response.ok) {
          throw new Error(`HTTP ${response.status}`);
        }
        const parsed = await response.json() as Partial<AndroidApkDownloadMetadata>;
        if (
          typeof parsed.apk_url !== "string" ||
          typeof parsed.filename !== "string" ||
          typeof parsed.apk_size_bytes !== "number" ||
          !Number.isFinite(parsed.apk_size_bytes) ||
          parsed.apk_size_bytes < 0 ||
          typeof parsed.git_commit !== "string" ||
          typeof parsed.version_name !== "string" ||
          typeof parsed.built_at_utc !== "string" ||
          typeof parsed.version_code !== "number"
        ) {
          throw new Error("invalid android-apk metadata");
        }
        if (!cancelled) {
          setMetadata(parsed as AndroidApkDownloadMetadata);
        }
      })
      .catch((error) => {
        if (!cancelled) {
          setMetadata(null);
          setMetadataError(errorMessage(error));
        }
      })
      .finally(() => {
        if (!cancelled) {
          setLoading(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const shortCommit = metadata?.git_commit.slice(0, 8) ?? "";
  const apkSize = metadata ? formatApkSize(metadata.apk_size_bytes) : "";
  const apkUnavailableTitle = loading
    ? "Checking Android download metadata..."
    : `Android APK is not published in this static tree${metadataError ? `: ${metadataError}` : "."}`;
  return (
    <section className="appPage aboutPage" data-testid="parity:page:about">
      <div className="aboutPagePanel">
        <div className="aboutActionRow">
          <div className="aboutActionColumn">
            {metadata ? (
              <a
                className="aboutActionButton"
                href={metadata.apk_url}
                title={`Download ${metadata.filename} (${metadata.version_name}, ${shortCommit})`}
              >
                Android APK
              </a>
            ) : (
              <button type="button" className="aboutActionButton" disabled title={apkUnavailableTitle}>
                Android APK
              </button>
            )}
            {metadata ? (
              <dl className="aboutMetadata">
                <div>
                  <dt>APK size</dt>
                  <dd>{apkSize}</dd>
                </div>
                <div>
                  <dt>Android Version</dt>
                  <dd>{metadata.version_name}</dd>
                </div>
                <div>
                  <dt>Build</dt>
                  <dd>{shortCommit}</dd>
                </div>
                <div>
                  <dt>Published</dt>
                  <dd>{metadata.built_at_utc}</dd>
                </div>
              </dl>
            ) : (
              <p className="aboutDownloadUnavailable">{apkUnavailableTitle}</p>
            )}
          </div>
          <div className="aboutActionColumn">
            <a href={urlForAppPage("home")} className="aboutActionButton aboutWebActionButton">
              Open Web App
            </a>
          </div>
        </div>

        <div className="aboutReadmeRegion">
          <article
            className="aboutReadmeContent"
            dangerouslySetInnerHTML={{ __html: `${noWarrantyHtml}\n${aboutReadmeHtml}` }}
          />
        </div>
      </div>
    </section>
  );
}

function formatApkSize(bytes: number): string {
  const megabytes = bytes / 1_000_000;
  return `${megabytes >= 10 ? megabytes.toFixed(0) : megabytes.toFixed(1)} MB`;
}

function CloudQrCode({ code }: { code: UiQrCode }) {
  const quietZone = code.quiet_zone_modules;
  const moduleCount = code.rows.length + (quietZone * 2);
  const darkModules: string[] = [];
  code.rows.forEach((row, rowIndex) => {
    for (let columnIndex = 0; columnIndex < row.length; columnIndex += 1) {
      if (row[columnIndex] === "1") {
        darkModules.push(`M${columnIndex + quietZone} ${rowIndex + quietZone}h1v1h-1z`);
      }
    }
  });
  return (
    <svg
      className="cloudSetupQrCode"
      viewBox={`0 0 ${moduleCount} ${moduleCount}`}
      role="img"
      aria-label={code.accessibility_label}
      shapeRendering="crispEdges"
    >
      <rect width={moduleCount} height={moduleCount} fill="#fff" />
      <path d={darkModules.join("")} fill="#000" />
    </svg>
  );
}

function CloudPage(props: {
  page: AppPage;
  state: UiSessionSnapshot["cloud_page_state"];
  navElement: NavElementUiView | null | undefined;
  mostRecentChartOrPlatePage: AppPage;
  onOpenPlan: () => void;
  onOpenRecentChartOrPlate: () => void;
  onSelectPage: (page: AppPage) => void;
  onAction: (
    actionId: CloudUiActionId,
    fields: CloudUiFieldValue[],
    platformEffect: CloudPlatformEffect | null,
  ) => Promise<string | null>;
}) {
  const [fieldValues, setFieldValues] = useState<Partial<Record<CloudUiFieldId, string>>>({});
  const [copyStatus, setCopyStatus] = useState("");
  const [actionError, setActionError] = useState("");

  const invoke = async (action: UiSessionSnapshot["cloud_page_state"]["overall_status"]["actions"][number]) => {
    setActionError("");
    setCopyStatus("");
    try {
      const fields = Object.entries(fieldValues).map(([id, value]) => ({
        id: id as CloudUiFieldId,
        value: value ?? "",
      }));
      const completionLabel = await props.onAction(
        action.id,
        fields,
        action.platform_effect ?? null,
      );
      setCopyStatus(completionLabel ?? "");
    } catch (error) {
      setActionError(errorMessage(error));
    }
  };

  const renderPanel = (
    panel: UiSessionSnapshot["cloud_page_state"]["sync_account_panels"][number],
    region: "account" | "provider" | "status",
  ) => {
    const control = panel.control;
    return <section
      key={`${region}:${panel.id}`}
      className={`cloudFlowPanel is-${panel.state}`}
      data-e2e-state={panel.state}
      data-e2e-action-revision={region === "status" ? props.state.action_revision : undefined}
      data-testid={region === "provider"
        ? "cloud-provider-card"
        : region === "status"
          ? "cloud-overall-status"
          : `cloud-panel-${panel.id}`}
    >
      <header>
        <h2>{panel.title}</h2>
        {panel.state_label ? <span>{panel.state_label}</span> : null}
      </header>
      {panel.summary ? <p>{panel.summary}</p> : null}
      {panel.time_facts.length > 0 ? (
        <dl className="cloudTimeFacts">
          {panel.time_facts.map((fact) => (
            <div key={fact.label}>
              <dt>{fact.label}</dt>
              <dd>{fact.value}</dd>
            </div>
          ))}
        </dl>
      ) : null}
      {control?.kind === "device_setup_code_input" ? (
        <label className="cloudSetupCodeField">
          <span>{control.label}</span>
          <textarea
            data-testid="cloud-setup-code-input"
            value={fieldValues[control.field_id] ?? ""}
            onChange={(event) => setFieldValues((current) => ({
              ...current,
              [control.field_id]: event.target.value,
            }))}
            placeholder={control.placeholder}
            spellCheck={false}
          />
        </label>
      ) : null}
      {control?.kind === "device_setup_code_output" ? (
        <div className="cloudSetupCodeOutput">
          <CloudQrCode code={control.qr_code} />
          <textarea
            data-testid="cloud-setup-code-output"
            readOnly
            value={control.setup_code}
            spellCheck={false}
          />
          <button
            type="button"
            data-testid={`cloud-action-${control.copy_action.id}`}
            onClick={() => void invoke(control.copy_action)}
          >
            {control.copy_action.label}
          </button>
          {copyStatus ? (
            <span className="cloudCopyStatus" data-testid="cloud-copy-status">{copyStatus}</span>
          ) : null}
        </div>
      ) : null}
      {panel.actions.length > 0 ? (
        <div className="cloudFlowActions">
          {panel.actions.map((action) => {
            const enabled = action.enabled
              && action.required_fields.every((fieldId) =>
                (fieldValues[fieldId] ?? "").trim().length > 0);
            return (
              <div className="cloudFlowAction" key={action.id}>
                <button
                  type="button"
                  data-testid={`cloud-action-${action.id}`}
                  disabled={!enabled}
                  onClick={() => void invoke(action)}
                >
                  {action.label}
                </button>
                {!enabled && action.disabled_reason ? <small>{action.disabled_reason}</small> : null}
              </div>
            );
          })}
        </div>
      ) : null}
    </section>;
  };

  return (
    <section className="appPage cloudPage" data-testid="parity:page:cloud">
      <PrimaryNavigationDock
        page={props.page}
        navElement={props.navElement}
        chartPlateTargetPage={props.mostRecentChartOrPlatePage}
        onSelectPage={props.onSelectPage}
        onOpenPlan={props.onOpenPlan}
        onOpenChartOrPlate={props.onOpenRecentChartOrPlate}
      />
      <div className="cloudFlow" aria-label={props.state.title}>
        <header className="cloudPageHeader">
          <h1>{props.state.title}</h1>
          <p>{props.state.summary}</p>
        </header>
        {actionError ? <p className="cloudActionError" role="alert">{actionError}</p> : null}
        <div className={`cloudFlowLayout${props.state.provider_card ? " has-provider" : ""}`}>
          <section className="cloudFlowColumn cloudAccountColumn" aria-label={props.state.sync_account_heading}>
            <h2 className="cloudFlowColumnTitle">{props.state.sync_account_heading}</h2>
            <div className="cloudFlowPanels">
              {props.state.sync_account_panels.map((panel) => renderPanel(panel, "account"))}
            </div>
          </section>
          {props.state.provider_card ? (
            <aside className="cloudFlowColumn cloudProviderColumn" aria-label={props.state.provider_heading}>
              <h2 className="cloudFlowColumnTitle">{props.state.provider_heading}</h2>
              {renderPanel(props.state.provider_card, "provider")}
            </aside>
          ) : null}
          <div className="cloudOverallStatus" aria-label={props.state.overall_status_label}>
            {renderPanel(props.state.overall_status, "status")}
          </div>
        </div>
      </div>
    </section>
  );
}

function SettingsPage(props: {
  page: AppPage;
  state: UiSessionSnapshot["settings_page_state"];
  navElement: NavElementUiView | null | undefined;
  mostRecentChartOrPlatePage: AppPage;
  onOpenPlan: () => void;
  onOpenRecentChartOrPlate: () => void;
  onSelectPage: (page: AppPage) => void;
  onSettingsAction: (actionId: string, valueId: string) => void;
  onAircraftLibraryAction: (actionId: string, sourceJson?: string) => void;
}) {
  const { toast: settingsHelpToast, show: showSettingsHelp } = useDisabledActionToast();
  return (
    <section className="appPage settingsPage" data-testid="parity:page:settings">
      <PrimaryNavigationDock
        page={props.page}
        navElement={props.navElement}
        chartPlateTargetPage={props.mostRecentChartOrPlatePage}
        onSelectPage={props.onSelectPage}
        onOpenPlan={props.onOpenPlan}
        onOpenChartOrPlate={props.onOpenRecentChartOrPlate}
      />

      <div className="settingsPagePanel" aria-label={props.state.title}>
        <header className="settingsPageHeader">
          <h1>{props.state.title}</h1>
          {props.state.summary ? <p>{props.state.summary}</p> : null}
        </header>
        <div className="settingsPageRows">
          {props.state.rows.map((row) => (
            <SettingsPageRowView
              key={row.id}
              row={row}
              onSettingsAction={props.onSettingsAction}
              onHelp={showSettingsHelp}
            />
          ))}
          {props.state.aircraft_library ? (
            <SettingsAircraftLibrary
              state={props.state.aircraft_library}
              onAction={props.onAircraftLibraryAction}
              onDisabledAction={showSettingsHelp}
            />
          ) : null}
          {props.state.sections.map((section) => (
            <SettingsPageSectionView
              key={section.id}
              section={section}
              onSettingsAction={props.onSettingsAction}
              onHelp={showSettingsHelp}
            />
          ))}
        </div>
      </div>
      {settingsHelpToast ? (
        <div className="mapSelectionToast" role="status" aria-live="polite">
          {settingsHelpToast.message}
        </div>
      ) : null}
    </section>
  );
}

type AircraftLibraryState = NonNullable<UiSessionSnapshot["settings_page_state"]["aircraft_library"]>;

function SettingsAircraftLibrary(props: {
  state: AircraftLibraryState;
  onAction: (actionId: string, sourceJson?: string) => void;
  onDisabledAction: (message: string) => void;
}) {
  const [sourceJson, setSourceJson] = useState(props.state.editor?.source_json ?? "");
  useEffect(() => {
    setSourceJson(props.state.editor?.source_json ?? "");
  }, [props.state.editor?.source_json]);

  const invoke = (action: AircraftLibraryState["add_action"], source?: string) => {
    const reason = disabledReasonText(action.disabled_reason);
    if (!action.enabled) {
      if (reason) props.onDisabledAction(reason);
      return;
    }
    props.onAction(action.action_id, source);
  };

  return (
    <section
      className="settingsAircraftLibrary"
      aria-label={props.state.title}
      data-testid="settings-aircraft-library"
    >
      <div className="settingsAircraftLibraryHeader">
        <div>
          <h2>{props.state.title}</h2>
          <p>{props.state.summary}</p>
        </div>
        <SettingsSyncIndicatorView
          indicator={props.state.sync_indicator}
          testId="aircraft-library"
          onHelp={props.onDisabledAction}
        />
        {!props.state.editor ? (
          <button
            type="button"
            data-testid="settings-aircraft-add"
            onClick={() => invoke(props.state.add_action)}
          >
            {props.state.add_action.label}
          </button>
        ) : null}
      </div>
      <div className="settingsAircraftGrid">
        {props.state.entries.map((entry) => (
          <article
            className={`settingsAircraftEntry${entry.included ? "" : " isHidden"}`}
            key={entry.definition_hash}
            data-testid={`settings-aircraft-entry-${entry.source_label.toLowerCase()}`}
          >
            <div className="settingsAircraftIdentity">
              <span>
                <strong>{entry.label}</strong>
                <small>{entry.source_label}</small>
              </span>
              <AircraftSymbolIcon symbol={entry.symbol} />
            </div>
            <div className="settingsAircraftActions">
              <button
                type="button"
                data-testid={`settings-aircraft-toggle-${entry.source_label.toLowerCase()}`}
                onClick={() => invoke(entry.toggle_action)}
              >
                {entry.toggle_action.label}
              </button>
              {entry.edit_action ? (
                <button type="button" onClick={() => invoke(entry.edit_action!)}>
                  {entry.edit_action.label}
                </button>
              ) : null}
            </div>
          </article>
        ))}
      </div>
      {props.state.editor ? (
        <section className="settingsAircraftEditor">
          <h3>{props.state.editor.title}</h3>
          <label>
            <span>{props.state.editor.field_label}</span>
            <textarea
              data-testid="settings-aircraft-source"
              spellCheck={false}
              value={sourceJson}
              onChange={(event) => setSourceJson(event.currentTarget.value)}
            />
          </label>
          {props.state.editor.validation_error ? (
            <p className="settingsAircraftError" role="alert">
              {props.state.editor.validation_error}
            </p>
          ) : null}
          <div className="settingsAircraftEditorActions">
            <button
              type="button"
              data-testid="settings-aircraft-save"
              onClick={() => invoke(props.state.editor!.save_action, sourceJson)}
            >
              {props.state.editor.save_action.label}
            </button>
            <button
              type="button"
              data-testid="settings-aircraft-cancel"
              onClick={() => invoke(props.state.editor!.cancel_action)}
            >
              {props.state.editor.cancel_action.label}
            </button>
          </div>
        </section>
      ) : null}
    </section>
  );
}

type SettingsPageRowState = UiSessionSnapshot["settings_page_state"]["rows"][number];
type SettingsPageSectionState = UiSessionSnapshot["settings_page_state"]["sections"][number];

function SettingsPageSectionView(props: {
  section: SettingsPageSectionState;
  onSettingsAction: (actionId: string, valueId: string) => void;
  onHelp: (message: string) => void;
}) {
  const [expanded, setExpanded] = useState(!props.section.collapsed_by_default);
  const contentId = useId();
  return (
    <section className={`settingsPageSection${expanded ? " isExpanded" : ""}`}>
      <button
        type="button"
        className="settingsPageSectionHeader"
        data-testid={`settings-section-${props.section.id}`}
        aria-expanded={expanded}
        aria-controls={contentId}
        onClick={() => setExpanded((value) => !value)}
      >
        <span className="settingsPageSectionChevron" aria-hidden="true">{expanded ? "\u25BE" : "\u25B8"}</span>
        <span>{props.section.title}</span>
      </button>
      <div id={contentId} className="settingsPageSectionRows" hidden={!expanded}>
        {props.section.rows.map((row) => (
          <SettingsPageRowView
            key={row.id}
            row={row}
            onSettingsAction={props.onSettingsAction}
            onHelp={props.onHelp}
          />
        ))}
      </div>
    </section>
  );
}

function SettingsPageRowView(props: {
  row: SettingsPageRowState;
  onSettingsAction: (actionId: string, valueId: string) => void;
  onHelp: (message: string) => void;
}) {
  const { row } = props;
  const rowStyle = row.indent_level > 0
    ? ({ marginInlineStart: `calc(var(--thumb) * ${0.35 * row.indent_level})` } as CSSProperties)
    : undefined;
  if (row.kind === "toggle") {
    const enabled = row.value_id === "on";
    const inputId = `settings-toggle-input-${row.id}`;
    return (
      <section className="settingsPageRow settingsToggleRow" style={rowStyle}>
        <span className="settingsToggleRowTitle">
          <label htmlFor={inputId}>{row.title}</label>
          <SettingsSyncIndicatorView
            indicator={row.sync_indicator}
            testId={row.id}
            onHelp={props.onHelp}
          />
        </span>
        <input
          id={inputId}
          type="checkbox"
          data-testid={`settings-toggle-${row.id}`}
          checked={enabled}
          onChange={() => props.onSettingsAction(row.action_id, enabled ? "off" : "on")}
        />
      </section>
    );
  }
  return (
    <section className="settingsPageRow" style={rowStyle}>
      <SettingsPageRowTitle row={row} onHelp={props.onHelp} />
      {row.kind === "grid_choices" ? (
        <div className="settingsFlightDataGrid">
          {row.items.map((item) => (
            <button
              key={item.cell.id}
              type="button"
              data-testid={`settings-choice-${row.id}-${item.cell.id}`}
              className={`flightDataCell settingsFlightDataCell${item.enabled ? "" : " isDisabled"}`}
              aria-pressed={item.enabled}
              onClick={() => props.onSettingsAction(row.action_id, item.cell.id)}
            >
              <FlightDataCellContents cell={item.cell} />
            </button>
          ))}
        </div>
      ) : null}
      {row.kind === "slider" && row.stops.length > 0 ? (
        <div className={`settingsSliderStops${row.stops.length === 2 ? " isBinary" : ""}`}>
          {row.stops.map((stop) => (
            <button
              key={stop.id}
              type="button"
              data-testid={`settings-choice-${row.id}-${stop.id}`}
              className={stop.id === row.value_id ? "isActive" : ""}
              aria-pressed={stop.id === row.value_id}
              onClick={() => props.onSettingsAction(row.action_id, stop.id)}
            >
              {stop.label}
            </button>
          ))}
        </div>
      ) : null}
    </section>
  );
}

function SettingsPageRowTitle(props: {
  row: SettingsPageRowState;
  onHelp: (message: string) => void;
}) {
  const helpText = props.row.help_text?.trim();
  return (
    <div className="settingsPageRowTitle">
      <h2>{props.row.title}</h2>
      <SettingsSyncIndicatorView
        indicator={props.row.sync_indicator}
        testId={props.row.id}
        onHelp={props.onHelp}
      />
      {helpText ? (
        <span className="settingsPageRowHelp">
          <button
            type="button"
            aria-label={`Help for ${props.row.title}`}
            title={helpText}
            data-testid={`settings-help-${props.row.id}`}
            onClick={() => props.onHelp(helpText)}
          >
            ?
          </button>
        </span>
      ) : null}
    </div>
  );
}

function SettingsSyncIndicatorView(props: {
  indicator: SettingsPageRowState["sync_indicator"];
  testId: string;
  onHelp: (message: string) => void;
}) {
  if (!props.indicator) return null;
  return (
    <button
      type="button"
      className="settingsSyncIndicator"
      aria-label={props.indicator.help_text}
      title={props.indicator.help_text}
      data-testid={`settings-sync-${props.testId}`}
      onClick={() => props.onHelp(props.indicator!.help_text)}
    >
      {props.indicator.symbol}
    </button>
  );
}

function DataStatusPage(props: {
  page: AppPage;
  state: UiDataStatusPageState;
  navElement: NavElementUiView | null | undefined;
  mostRecentChartOrPlatePage: AppPage;
  onOpenPlan: () => void;
  onOpenRecentChartOrPlate: () => void;
  onSelectPage: (page: AppPage) => void;
  onTimeDisplayAction: (actionId: string) => void | Promise<void>;
}) {
  return (
    <section className="appPage dataStatusPage" data-testid="parity:page:data_status">
      <PrimaryNavigationDock
        page={props.page}
        navElement={props.navElement}
        chartPlateTargetPage={props.mostRecentChartOrPlatePage}
        onSelectPage={props.onSelectPage}
        onOpenPlan={props.onOpenPlan}
        onOpenChartOrPlate={props.onOpenRecentChartOrPlate}
      />

      <div className="dataStatusPagePanel" aria-label={props.state.title}>
        <header className="dataStatusPageHeader">
          <h1>{props.state.title}</h1>
          <p>{props.state.summary}</p>
        </header>
        <div className="dataStatusPageRows">
          {props.state.rows.map((row) => (
            <DataStatusPageRowArticle
              key={row.id}
              row={row}
              onTimeDisplayAction={props.onTimeDisplayAction}
            />
          ))}
        </div>
      </div>
    </section>
  );
}

function DataStatusPageRowArticle(props: {
  row: UiDataStatusPageState["rows"][number];
  onTimeDisplayAction: (actionId: string) => void | Promise<void>;
}) {
  const { row } = props;
  return (
    <article
      className={`dataStatusPageRow statusSeverity-${row.severity}`}
      data-testid={`parity:data-status-row:${row.id}:severity:${row.severity}`}
    >
      <div className="dataStatusPageRowHeader">
        <span className="dataStatusPageRowLabel">{row.label}</span>
        <span className="dataStatusPageRowValue">{row.value}</span>
      </div>
      {row.detail ? <div className="dataStatusPageRowDetail">{row.detail}</div> : null}
      {row.facts.length > 0 ? (
        <dl className="dataStatusPageFacts">
          {row.facts.map((fact) => (
            <div
              key={`${row.id}:${fact.label}`}
              className={`dataStatusPageFact${fact.full_width ? " isFullWidth" : ""}`}
            >
              <dt>{fact.label}</dt>
              <dd
                className={fact.action_id ? "isActionable" : undefined}
                role={fact.action_id ? "button" : undefined}
                tabIndex={fact.action_id ? 0 : undefined}
                onClick={fact.action_id ? () => void props.onTimeDisplayAction(fact.action_id!) : undefined}
                onKeyDown={fact.action_id ? (event) => {
                  if (event.key === "Enter" || event.key === " ") {
                    event.preventDefault();
                    void props.onTimeDisplayAction(fact.action_id!);
                  }
                } : undefined}
              >
                {renderDataStatusFactValue(fact)}
              </dd>
            </div>
          ))}
        </dl>
      ) : null}
    </article>
  );
}

function renderDataStatusFactValue(
  fact: UiDataStatusPageState["rows"][number]["facts"][number],
): ReactNode {
  const value = renderDataStatusFactContent(fact);
  if (fact.link_url) {
    return (
      <a href={fact.link_url} target="_blank" rel="noreferrer">
        {value}
      </a>
    );
  }
  return value;
}

function renderDataStatusFactContent(
  fact: UiDataStatusPageState["rows"][number]["facts"][number],
): ReactNode {
  if (!fact.relative_value) {
    return fact.value;
  }
  return (
    <>
      <span className="dataStatusPageFactValuePrimary">{fact.value}</span>
      <span className="dataStatusPageFactValueRelative">({fact.relative_value})</span>
    </>
  );
}

function ZoomControl(props: {
  zoom: number;
  minZoom: number;
  maxZoom: number;
  onZoomChange: (zoom: number) => void;
  raisedForPrimaryNavigation: boolean;
}) {
  const step = 0.05;
  const buttonStep = 0.5;
  const zoom = Math.min(props.maxZoom, Math.max(props.minZoom, props.zoom));

  return (
    <div
      className={`zoomControl${props.raisedForPrimaryNavigation ? " isRaisedForPrimaryNavigation" : ""}`}
      onPointerDown={stopPointer}
      onPointerUp={stopPointer}
      onDoubleClick={stopDoubleClick}
    >
      <button
        type="button"
        className="zoomControlButton"
        aria-label="Zoom out"
        onClick={() => props.onZoomChange(zoom - buttonStep)}
      >
        −
      </button>
      <input
        className="zoomControlSlider"
        type="range"
        aria-label="Map zoom"
        min={props.minZoom}
        max={props.maxZoom}
        step={step}
        value={zoom}
        onChange={(event) => props.onZoomChange(Number(event.currentTarget.value))}
      />
      <button
        type="button"
        className="zoomControlButton"
        aria-label="Zoom in"
        onClick={() => props.onZoomChange(zoom + buttonStep)}
      >
        ＋
      </button>
    </div>
  );
}

function readPersistedWebUiState(): PersistedWebUiState {
  if (typeof window === "undefined") {
    return {};
  }
  try {
    const raw = window.localStorage.getItem(webUiStateStorageKey);
    if (!raw) {
      return {};
    }
    const parsed = JSON.parse(raw) as PersistedWebUiState;
    const rawPage = (parsed as { page?: string }).page;
    const page = rawPage;
    const mapOrientationMode = parsed.mapOrientationMode === "track" ? "track" : "north";
    return {
      page: page === "map" || page === "plan" || page === "altitude" || page === "charts" || page === "home" || page === "data" || page === "settings" || page === "cloud" ? page : undefined,
      mapOrientationMode,
      selectedAirportId: parsed.selectedAirportId,
      selectedChartId: parsed.selectedChartId,
      recentAirportIds: Array.isArray(parsed.recentAirportIds) ? parsed.recentAirportIds.filter((value): value is string => typeof value === "string") : [],
    };
  } catch {
    return {};
  }
}

function writePersistedWebUiState(state: PersistedWebUiState) {
  if (typeof window === "undefined") {
    return;
  }
  window.localStorage.setItem(webUiStateStorageKey, JSON.stringify(state));
}

function plateFolderColor(category: PlateFolderCategory) {
  return plateFolderTheme.label_colors[category as keyof typeof plateFolderTheme.label_colors] ?? plateFolderTheme.label_colors.other ?? "#52656d";
}

function SituationStatusBadge(props: {
  controls: OwnshipControlModel;
  open: boolean;
  onToggle: () => void;
  options: TrayOption[];
  transportControls?: ReactNode;
  onDisabledAction?: (message: string) => void;
}) {
  return (
    <TrayDock
      launcherLabel={props.controls.launcher_label}
      launcherClassName={`situationStatusLauncher situationStatus-${props.controls.launcher_tone} situationStatusText-${props.controls.launcher_text_tone}`}
      open={props.open}
      onToggle={props.onToggle}
      ariaLabel="Ownship source"
      style="situation"
      options={props.options}
      footer={props.transportControls}
      testId="ownship-source-button"
      onDisabledAction={props.onDisabledAction}
    />
  );
}

function StatusControlDock(props: {
  controls: OwnshipControlModel;
  dataStatuses?: readonly {
    id: string;
    state: UiDataStatusState;
    open: boolean;
    onToggle: () => void;
    onAction: (actionId: string) => void | Promise<void>;
    testIdPrefix?: string;
  }[];
  lowered?: boolean;
  leadingControl?: ReactNode;
  ownshipOpen: boolean;
  onOwnshipToggle: () => void;
  options: TrayOption[];
  transportControls?: ReactNode;
  onDisabledAction?: (message: string) => void;
}) {
  return (
    <div className={`statusControlDock${props.lowered ? " isLowered" : ""}`}>
      {props.leadingControl}
      {props.dataStatuses?.map((status) => (
        <DataStatusDock
          key={status.id}
          dataStatusState={status.state}
          open={status.open}
          onToggle={status.onToggle}
          onAction={status.onAction}
          testIdPrefix={status.testIdPrefix}
        />
      ))}
      <SituationStatusBadge
        controls={props.controls}
        open={props.ownshipOpen}
        onToggle={props.onOwnshipToggle}
        options={props.options}
        transportControls={props.transportControls}
        onDisabledAction={props.onDisabledAction}
      />
    </div>
  );
}

function DataStatusDock(props: {
  dataStatusState: UiDataStatusState;
  lowered?: boolean;
  open: boolean;
  onToggle: () => void;
  onAction: (actionId: string) => void | Promise<void>;
  testIdPrefix?: string;
}) {
  const launcherRef = useRef<HTMLButtonElement | null>(null);
  const panelRef = useRef<HTMLElement | null>(null);
  const [panelPosition, setPanelPosition] = useState<{ left: number; top: number } | null>(null);
  const launcherCount = props.dataStatusState.launcher_count;
  const hasLauncherCount = launcherCount != null;
  const hasStatus = props.dataStatusState.boxes.length > 0;
  const testIdPrefix = props.testIdPrefix ?? "data-status";
  useEffect(() => {
    if (!props.open) {
      setPanelPosition(null);
      return;
    }

    function updatePosition() {
      const launcher = launcherRef.current;
      const panel = panelRef.current;
      if (!launcher || !panel) {
        return;
      }
      const launcherRect = launcher.getBoundingClientRect();
      const gap = thumbPixels(0.1);
      const minInset = thumbPixels(0.1);
      const maxLeft = Math.max(minInset, window.innerWidth - panel.offsetWidth - minInset);
      const maxTop = Math.max(minInset, window.innerHeight - panel.offsetHeight - minInset);
      setPanelPosition({
        left: Math.min(Math.max(minInset, launcherRect.right - panel.offsetWidth), maxLeft),
        top: Math.min(Math.max(minInset, launcherRect.bottom + gap), maxTop),
      });
    }

    updatePosition();
    window.addEventListener("resize", updatePosition);
    window.addEventListener("scroll", updatePosition, true);
    return () => {
      window.removeEventListener("resize", updatePosition);
      window.removeEventListener("scroll", updatePosition, true);
    };
  }, [props.open, props.dataStatusState.boxes.length]);

  if (!hasStatus) {
    return null;
  }
  const severity = props.dataStatusState.launcher_severity;

  return (
    <div className="dataStatusDock">
      <button
        ref={launcherRef}
        type="button"
        className={`dataStatusLauncher statusSeverity-${severity}${props.open ? " isOpen" : ""}${hasLauncherCount ? "" : " isQuiet"}`}
        data-testid={`${testIdPrefix}-launcher`}
        aria-expanded={props.open}
        aria-label="Status"
        onPointerDown={stopPointer}
        onPointerUp={stopPointer}
        onDoubleClick={stopDoubleClick}
        onClick={props.onToggle}
      >
        <DataStatusWarningFace count={launcherCount} />
      </button>
      {props.open && typeof document !== "undefined" ? createPortal(
        <section
          ref={panelRef}
          className="dataStatusPanel"
          data-testid={`${testIdPrefix}-panel`}
          aria-label="Active data status"
          style={panelPosition ? {
            left: `${panelPosition.left}px`,
            top: `${panelPosition.top}px`,
          } : {
            visibility: "hidden",
          }}
          onPointerDown={stopPointer}
          onPointerUp={stopPointer}
        >
          {props.dataStatusState.boxes.map((box) => (
            <div
              key={box.id}
              className={`dataStatusBox statusSeverity-${box.severity}${box.hushed ? " isHushed" : ""}`}
              data-testid={`data-status-box-${box.id}`}
            >
              <div className="dataStatusBoxHeader">
                <span className="dataStatusBoxLabel">{box.label}</span>
                <span className="dataStatusBoxValue">{box.value ?? "—"}</span>
              </div>
              <div className="dataStatusBoxDetail">{box.detail}</div>
              {box.actions.length > 0 ? (
                <div className="dataStatusActions">
                  {box.actions.map((action) => (
                    <button
                      key={action.id}
                      type="button"
                      className={`dataStatusAction dataStatusAction-${action.style}`}
                      data-testid={`data-status-action-${box.id}-${action.id}`}
                      disabled={!action.enabled}
                      onPointerDown={stopPointer}
                      onPointerUp={stopPointer}
                      onDoubleClick={stopDoubleClick}
                      onClick={action.enabled ? () => props.onAction(action.id) : undefined}
                    >
                      {action.label}
                    </button>
                  ))}
                </div>
              ) : null}
            </div>
          ))}
        </section>,
        document.body,
      ) : null}
    </div>
  );
}

function SituationTransportRow(props: {
  controls: OwnshipControlModel["situation_controls"];
  onInput: (input: SituationControlInput) => void;
  onDisabledAction?: (message: string) => void;
}) {
  return (
    <div className="situationTransportRow" role="group" aria-label="Plan preview and replay controls">
      {props.controls.map((button) => {
        const disabledReason = disabledReasonText(button.disabled_reason);
        const disabled = !button.enabled;
        return (
          <button
            key={button.input}
            type="button"
            className={`trayButton trayButtonSquare situationTransportButton${disabled ? " isDisabled" : ""}`}
            aria-label={button.label}
            title={disabledReason ?? button.label}
            disabled={disabled && !disabledReason}
            aria-disabled={disabled ? "true" : undefined}
            onPointerDown={stopPointer}
            onPointerUp={stopPointer}
            onDoubleClick={stopDoubleClick}
            onClick={() => {
              if (disabled) {
                if (disabledReason && props.onDisabledAction) {
                  props.onDisabledAction(disabledReason);
                }
                return;
              }
              props.onInput(button.input);
            }}
          >
            {button.label}
          </button>
        );
      })}
    </div>
  );
}

function SituationControlFooter(props: {
  controls: OwnshipControlModel;
  onInput: (input: SituationControlInput) => void;
  onTextAction: (actionId: string, value: string) => void;
  onDisabledAction?: (message: string) => void;
}) {
  return (
    <div className="situationControlFooter">
      {props.controls.text_action ? (
        <OwnshipTextActionControl
          control={props.controls.text_action}
          onSubmit={props.onTextAction}
          onDisabledAction={props.onDisabledAction}
        />
      ) : null}
      <SituationTransportRow
        controls={props.controls.situation_controls}
        onInput={props.onInput}
        onDisabledAction={props.onDisabledAction}
      />
    </div>
  );
}

function OwnshipTextActionControl(props: {
  control: NonNullable<OwnshipControlModel["text_action"]>;
  onSubmit: (actionId: string, value: string) => void;
  onDisabledAction?: (message: string) => void;
}) {
  const [value, setValue] = useState(props.control.value);
  useEffect(() => setValue(props.control.value), [props.control.action_id, props.control.value]);
  const disabledReason = disabledReasonText(props.control.disabled_reason);
  return (
    <form
      className="situationTextAction"
      onPointerDown={stopPointer}
      onPointerUp={stopPointer}
      onSubmit={(event) => {
        event.preventDefault();
        if (!props.control.enabled) {
          if (disabledReason && props.onDisabledAction) props.onDisabledAction(disabledReason);
          return;
        }
        props.onSubmit(props.control.action_id, value);
      }}
    >
      <label>
        <span>{props.control.label}</span>
        <input
          value={value}
          placeholder={props.control.placeholder}
          aria-label={props.control.label}
          onChange={(event) => setValue(event.target.value.toUpperCase())}
        />
      </label>
      <button
        type="submit"
        className="trayButton situationTextActionSubmit"
        aria-disabled={!props.control.enabled || undefined}
      >
        {props.control.submit_label}
      </button>
    </form>
  );
}

function situationInputForKey(key: string): SituationControlInput | null {
  switch (key) {
    case "<":
      return "skip_backward";
    case "(":
      return "fast_rewind";
    case ")":
      return "fast_forward";
    case ">":
      return "skip_forward";
    default:
      return null;
  }
}

function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) {
    return false;
  }
  const tagName = target.tagName.toLowerCase();
  return tagName === "input" || tagName === "textarea" || tagName === "select" || target.isContentEditable;
}

function sourceIdString(sourceId: { 0: string } | string): string {
  return typeof sourceId === "string" ? sourceId : sourceId[0];
}

function SituationAircraftSvg(props: {
  pathData: string;
  point: { x: number; y: number };
  headingDeg: number;
  sizePx: number;
}) {
  if (!props.pathData) return null;
  const scale = props.sizePx / 100;
  return (
    <g transform={`translate(${props.point.x} ${props.point.y}) rotate(${props.headingDeg}) scale(${scale})`}>
      <g
        style={{
          pointerEvents: "none",
          userSelect: "none",
          filter: "drop-shadow(0 1px 1px rgba(18, 26, 33, 0.45))",
        }}
      >
        <AircraftPlanViewPath pathData={props.pathData} />
      </g>
    </g>
  );
}

function SituationAircraft(props: {
  pathData: string;
  point: { x: number; y: number };
  headingDeg: number;
}) {
  if (!props.pathData) return null;
  return (
    <svg
      viewBox="-50 -50 100 100"
      aria-hidden="true"
      style={{
        position: "absolute",
        zIndex: 2,
        width: `calc(var(--thumb) * 1.44)`,
        height: `calc(var(--thumb) * 1.44)`,
        overflow: "visible",
        pointerEvents: "none",
        userSelect: "none",
        filter: "drop-shadow(0 1px 1px rgba(18, 26, 33, 0.45))",
        left: `${props.point.x}px`,
        top: `${props.point.y}px`,
        transform: `translate(-50%, -50%) rotate(${props.headingDeg}deg)`,
      }}
    >
      <AircraftPlanViewPath pathData={props.pathData} />
    </svg>
  );
}

function AircraftPlanViewPath(props: { pathData: string }) {
  return (
    <>
      <path
        d={props.pathData}
        fill="none"
        stroke="#000000"
        strokeWidth="3.3"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path d={props.pathData} fill="#e6e6e6" />
    </>
  );
}

function AircraftSymbolIcon(props: { symbol: UiAircraftSymbol }) {
  return (
    <svg className="aircraftSymbolIcon" viewBox="-55 -55 110 110" aria-hidden="true">
      <g transform={`rotate(${props.symbol.rotation_degrees})`}>
        <AircraftPlanViewPath pathData={props.symbol.path_data} />
      </g>
    </svg>
  );
}

function resolvePlateOwnshipOverlay(
  ownship: OwnshipRenderState,
  georef: PlateGeoref | null,
  imageSize: { chartId: string; width: number; height: number } | null,
  viewport: ImageViewportState | null,
  displaySize: { width: number; height: number } | null,
) {
  if (!ownship.draw_aircraft || !ownship.position || !georef || !imageSize || !viewport || !displaySize) {
    return null;
  }
  const imagePoint = plateImagePoint(ownship.position, georef);
  if (!imagePoint) {
    return null;
  }
  if (imagePoint.x < 0 || imagePoint.x > imageSize.width || imagePoint.y < 0 || imagePoint.y > imageSize.height) {
    return null;
  }
  const scaleX = displaySize.width / imageSize.width;
  const scaleY = displaySize.height / imageSize.height;
  return {
    point: {
      x: viewport.left + imagePoint.x * scaleX,
      y: viewport.top + imagePoint.y * scaleY,
    },
    headingDeg: ownship.orientation_deg ?? 0,
  };
}

function mapViewportFromCore(viewport: {
  center: LatLon;
  zoom: number;
}) {
  const centerWorld = latLonToWorld(viewport.center.lat, viewport.center.lon);
  return {
    centerWorldX: centerWorld.x,
    centerWorldY: centerWorld.y,
    zoom: viewport.zoom,
  } satisfies MapViewportState;
}

function arrowHeadPoints(from: { x: number; y: number }, to: { x: number; y: number }) {
  const angle = Math.atan2(to.y - from.y, to.x - from.x);
  const size = 20;
  const left = {
    x: to.x - size * Math.cos(angle - Math.PI / 6),
    y: to.y - size * Math.sin(angle - Math.PI / 6),
  };
  const right = {
    x: to.x - size * Math.cos(angle + Math.PI / 6),
    y: to.y - size * Math.sin(angle + Math.PI / 6),
  };
  return `${to.x},${to.y} ${left.x},${left.y} ${right.x},${right.y}`;
}

function arrowShaftEndPoint(from: { x: number; y: number }, to: { x: number; y: number }) {
  const angle = Math.atan2(to.y - from.y, to.x - from.x);
  const headLength = 14;
  return {
    x: to.x - headLength * Math.cos(angle),
    y: to.y - headLength * Math.sin(angle),
  };
}

function flightPlanEntryPreviewSegments(
  input: string,
  preview: FlightPlanEntryPreview,
): Array<{ text: string; tokenState: "neutral" | "recognized" | "invalid"; issue: boolean }> {
  const boundaries = new Set<number>([0, input.length]);
  for (const token of preview.tokens) {
    boundaries.add(token.start);
    boundaries.add(token.end);
  }
  for (const issue of preview.issues) {
    boundaries.add(issue.start);
    boundaries.add(issue.end);
  }
  const ordered = [...boundaries].sort((left, right) => left - right);
  const segments = [] as Array<{ text: string; tokenState: "neutral" | "recognized" | "invalid"; issue: boolean }>;
  for (let index = 0; index < ordered.length - 1; index += 1) {
    const start = ordered[index];
    const end = ordered[index + 1];
    if (start === end) {
      continue;
    }
    const token = preview.tokens.find((entry) => entry.start <= start && entry.end >= end);
    const issue = preview.issues.some((entry) => entry.start <= start && entry.end >= end);
    segments.push({
      text: input.slice(start, end),
      tokenState: token?.state ?? "neutral",
      issue,
    });
  }
  return segments;
}

function routeSegmentColor(status: FlightPlanRouteSegment["status"]) {
  if (status === "completed") {
    return loadedUiTheme.flight_plan_route.completed;
  }
  if (status === "active") {
    return loadedUiTheme.flight_plan_route.active;
  }
  if (status === "active_leg_remaining") {
    return loadedUiTheme.flight_plan_route.active_leg_remaining;
  }
  return loadedUiTheme.flight_plan_route.remaining;
}

function FlightPlanRoutePath(props: {
  segment: Pick<FlightPlanRouteSegment, "status" | "style"> & { path: readonly ScreenPoint[] };
  layer?: "contrast" | "color" | "both";
}) {
  const { segment, layer = "both" } = props;
  const strokeForLayer = (renderLayer: "contrast" | "color") =>
    renderLayer === "contrast"
      ? {
          color: loadedUiTheme.flight_plan_route.contrast,
          width: 7,
        }
      : {
          color: routeSegmentColor(segment.status),
          width: 3.5,
        };
  const renderLayers: Array<"contrast" | "color"> =
    layer === "both" ? ["contrast", "color"] : [layer];
  if (segment.style === "vectors") {
    return renderLayers.flatMap((renderLayer) => {
      const stroke = strokeForLayer(renderLayer);
      return spacedRouteChevronPlacements(segment.path, manualSequenceChevronSpacing).map(
        (placement, index) => (
            <path
              key={`${renderLayer}:vectors-chevron:${index}`}
              d={manualSequenceChevronPath}
              transform={`translate(${placement.x} ${placement.y}) rotate(${placement.angleDegrees})`}
              fill="none"
              stroke={stroke.color}
              strokeWidth={stroke.width}
              strokeLinecap="round"
              strokeLinejoin="round"
            />
        ),
      );
    });
  }

  const points = segment.path.map((point) => `${point.x},${point.y}`).join(" ");
  const strokeDasharray = segment.style === "dashed" ? "10 8" : undefined;
  return (
    <>
      {renderLayers.map((renderLayer) => {
        const stroke = strokeForLayer(renderLayer);
        return (
          <polyline
            key={renderLayer}
            points={points}
            fill="none"
            stroke={stroke.color}
            strokeWidth={stroke.width}
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeDasharray={strokeDasharray}
          />
        );
      })}
    </>
  );
}

function distanceBetween(first: ScreenPoint, second: ScreenPoint) {
  return Math.hypot(second.x - first.x, second.y - first.y);
}

function stopPointer(event: React.PointerEvent<HTMLElement>) {
  event.stopPropagation();
}

function stopClick(event: MouseEvent<HTMLElement>) {
  event.stopPropagation();
}

function stopWheel(event: React.WheelEvent<HTMLElement>) {
  event.stopPropagation();
}

function stopDoubleClick(event: React.MouseEvent<HTMLElement>) {
  event.preventDefault();
  event.stopPropagation();
}

function TrayScrim(props: { ariaLabel: string; onClose: () => void }) {
  function handlePointerDown(event: React.PointerEvent<HTMLButtonElement>) {
    event.preventDefault();
    event.stopPropagation();
    props.onClose();
  }

  function handleClick(event: React.MouseEvent<HTMLButtonElement>) {
    event.preventDefault();
    event.stopPropagation();
    if (event.detail === 0) {
      props.onClose();
    }
  }

  return (
    <button
      type="button"
      className="trayScrim"
      aria-label={props.ariaLabel}
      onPointerDown={handlePointerDown}
      onPointerUp={stopPointer}
      onDoubleClick={stopDoubleClick}
      onClick={handleClick}
    />
  );
}

function useModalTrayGroup<const T extends string>(ids: readonly T[]) {
  const [openId, setOpenId] = useState<T | null>(null);
  const allowedIds = useMemo(() => new Set<T>(ids), [ids]);

  function isOpen(id: T) {
    return openId === id;
  }

  function toggle(id: T) {
    if (!allowedIds.has(id)) {
      return;
    }
    setOpenId((current) => (current === id ? null : current === null ? id : current));
  }

  function close(id: T) {
    setOpenId((current) => (current === id ? null : current));
  }

  function closeAll() {
    setOpenId(null);
  }

  return {
    close,
    closeAll,
    isOpen,
    openId,
    scrimOpen: openId !== null,
    toggle,
  };
}

function midpoint(first: ScreenPoint, second: ScreenPoint): ScreenPoint {
  return { x: (first.x + second.x) / 2, y: (first.y + second.y) / 2 };
}
