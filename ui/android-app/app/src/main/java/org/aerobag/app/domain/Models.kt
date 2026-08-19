// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.domain

sealed interface NavRef {
    data class Airport(val code: String) : NavRef
    data class Navaid(val code: String) : NavRef
    data class ArincNavaid(
        val identifier: String,
        val icaoCode: String,
        val sectionCode: String,
        val subsectionCode: String,
    ) : NavRef
    data class TerminalNavaid(
        val airportId: String,
        val identifier: String,
        val icaoCode: String,
        val sectionCode: String,
        val subsectionCode: String,
    ) : NavRef
    data class Fix(val code: String) : NavRef
    data class LatLon(val lat: Double, val lon: Double) : NavRef
    data class Spot(val lat: Double, val lon: Double) : NavRef
}

typealias WaypointIdentifierSuggestion =
    org.aerobag.app.generated.WaypointIdentifierSuggestion

internal fun org.aerobag.app.generated.WaypointSuggestionNavRef.toNavRef(): NavRef =
    when (this) {
        is org.aerobag.app.generated.WaypointSuggestionNavRef.Airport -> NavRef.Airport(code)
        is org.aerobag.app.generated.WaypointSuggestionNavRef.Navaid -> NavRef.Navaid(code)
        is org.aerobag.app.generated.WaypointSuggestionNavRef.ArincNavaid -> NavRef.ArincNavaid(
            identifier = identifier,
            icaoCode = icaoCode,
            sectionCode = sectionCode,
            subsectionCode = subsectionCode,
        )
        is org.aerobag.app.generated.WaypointSuggestionNavRef.TerminalNavaid ->
            NavRef.TerminalNavaid(
                airportId = airportId,
                identifier = identifier,
                icaoCode = icaoCode,
                sectionCode = sectionCode,
                subsectionCode = subsectionCode,
            )
        is org.aerobag.app.generated.WaypointSuggestionNavRef.Fix -> NavRef.Fix(code)
        is org.aerobag.app.generated.WaypointSuggestionNavRef.LatLon ->
            NavRef.LatLon(position.lat, position.lon)
        is org.aerobag.app.generated.WaypointSuggestionNavRef.Spot ->
            NavRef.Spot(position.lat, position.lon)
    }

data class LatLonPoint(
    val lat: Double,
    val lon: Double,
)

data class PlanLeg(
    val from: NavRef,
    val to: NavRef,
    val airway: String? = null,
)

data class AirwaySuggestion(
    val airwayName: String,
    val nearestBranchKey: String?,
    val nearestNavRef: NavRef,
    val nearestSequence: Int,
    val distanceFromAnchorNm: Double,
)

data class AirwayPresentationPoint(
    val uid: String,
    val sequence: Int,
    val navRef: NavRef,
    val label: String,
    val samePointExitDisabledReason: String,
)

data class AirwayPresentationPlan(
    val airwayName: String,
    val branchKey: String,
    val points: List<AirwayPresentationPoint>,
    val suggestedEntryUid: String,
    val suggestedExitUid: String?,
)

enum class ProcedureKind {
    Sid,
    Star,
    Approach,
}

data class ProcedureSummary(
    val airportId: String,
    val procedureId: String,
    val displayLabel: String,
    val kind: ProcedureKind,
    val accentCategory: String,
    val enabled: Boolean,
    val disabledReason: String?,
)

data class ProcedureLoadOption(
    val loadId: String,
    val label: String,
)

enum class ProcedureLoadHeaderTone {
    Normal,
    Destructive,
}

data class ProcedureLoadMenu(
    val procedureKind: ProcedureKind?,
    val launcherLabel: String,
    val header: String,
    val headerTone: ProcedureLoadHeaderTone,
    val options: List<ProcedureLoadOption>,
)

data class ProcedureDistinctRow(
    val routeType: String,
    val transitionId: String,
)

data class ProcedureSpecChoice(
    val runwayTransition: String?,
    val enrouteTransition: String?,
    val label: String,
)

data class ProcedureOptions(
    val airportId: String,
    val procedureId: String,
    val kind: ProcedureKind,
    val runwayTransitions: List<String>,
    val enrouteTransitions: List<String>,
    val hasCommonSegment: Boolean,
    val emptyMessage: String,
    val validChoices: List<ProcedureSpecChoice>,
)

data class ProcedureLegMaterializationKey(
    val airportId: String,
    val procedureId: String,
    val routeType: String,
    val transitionId: String,
)

data class ProcedureLegMaterializationRecord(
    val key: ProcedureLegMaterializationKey,
    val sequence: Int,
    val navRef: NavRef?,
    val pathTermination: String,
)

enum class RouteSegmentStatus {
    Completed,
    Active,
    ActiveLegRemaining,
    Remaining,
}

data class FlightPlanRouteSegment(
    val id: String,
    val legId: String,
    val from: LatLonPoint,
    val to: LatLonPoint,
    val path: List<LatLonPoint>,
    val style: String,
    val distanceNm: Double,
    val courseDeg: Double,
    val status: RouteSegmentStatus,
)

data class FlightPlanRouteDistanceAnnotation(
    val id: String,
    val segmentIndexes: List<Int>,
    val text: String,
    val distanceNm: Double,
    val status: RouteSegmentStatus,
    val requiredFeatureIds: List<String>,
    val minimumPathToPillWidthRatio: Double,
)

data class FlightPlanRouteProjection(
    val flightPlanRouteRevision: Long,
    val segments: List<FlightPlanRouteSegment>,
    val distanceAnnotations: List<FlightPlanRouteDistanceAnnotation> = emptyList(),
)

enum class SequencingMode {
    FollowPlan,
    Suspended,
    DirectTo,
}

enum class RouteComponentViewKind {
    Waypoint,
    Airway,
    Procedure,
}

data class DirectToUiView(
    val start: NavRef,
    val target: NavRef,
    val targetRowId: String,
    val onPlanTarget: Boolean,
)

data class GuidanceUiView(
    val sequencingMode: SequencingMode,
    val activeFromRowUid: String? = null,
    val activeToRowUid: String? = null,
    val activeLeg: PlanLeg?,
    val navElement: NavElementUiView,
    val directTo: DirectToUiView?,
    val suspendBoundaryAfterActiveLeg: Boolean,
)

typealias FlightPlanControlId = org.aerobag.app.generated.FlightPlanControlId
typealias FlightPlanControlUiView = org.aerobag.app.generated.FlightPlanControlUiView

data class NavElementUiView(
    val activeLegSummary: String,
    val cdiIndicatorDots: Float?,
    val cdiOffscaleReadout: String?,
)

data class FlightPlanUiState(
    val planId: String,
    val planVersion: Long,
    val displayRows: List<FlightPlanDisplayRowUiView>,
    val dataColumns: List<FlightDataColumn>,
    val controls: List<FlightPlanControlUiView>,
    val altitudePlanner: AltitudePlannerUiView,
    val guidance: GuidanceUiView?,
)

data class AltitudePlannerControlUiView(
    val id: String,
    val label: String,
    val enabled: Boolean,
    val actionUid: String?,
    val disabledReason: String?,
    val options: List<AltitudePlannerControlOptionUiView>,
)

data class AltitudePlannerControlOptionUiView(
    val label: String,
    val actionUid: String,
    val selected: Boolean,
)

data class AltitudePlannerUnavailableReason(
    val code: String,
    val message: String,
)

data class AltitudeComparisonUiView(
    val actionUid: String?,
    val selected: Boolean,
    val enabled: Boolean,
    val disabledReason: String?,
    val advisory: String?,
    val cells: List<FlightDataCell>,
)

data class AltitudeComparisonPanelUiView(
    val columns: List<FlightDataColumn>,
    val rows: List<AltitudeComparisonUiView>,
    val advisories: List<String>,
)

data class AltitudePlannerForecastUiView(
    val summary: String,
)

data class AltitudePlannerDepartureEditorUiView(
    val title: String,
    val timeLabel: String,
    val timeValue: String,
    val basisLabel: String,
    val timeDisplayActionId: String,
    val whenLabel: String,
    val whenValue: String,
    val whenSuffix: String,
    val whenIsPast: Boolean,
    val enabled: Boolean,
    val disabledReason: String?,
)

data class AltitudePlannerUiView(
    val title: String,
    val estimateKind: String,
    val estimateSummary: FlightPlanEstimateModeUiView,
    val controls: List<AltitudePlannerControlUiView>,
    val departure: AltitudePlannerDepartureEditorUiView,
    val forecast: AltitudePlannerForecastUiView?,
    val advisories: List<String>,
    val unavailableReasons: List<AltitudePlannerUnavailableReason>,
)

data class FlightPlanEstimateModeUiView(
    val label: String,
    val estimateKind: String,
)

enum class FlightPlanDisplayRowKind {
    Waypoint,
    Group,
    Discontinuity,
    Summary,
}

data class FlightPlanRowActionUiView(
    val id: String,
    val uid: String = "",
    val menuColumn: Int = 0,
    val label: String,
    val enabled: Boolean,
    val disabledReason: String? = null,
)

data class FlightPlanRowActionDecision(
    val performSessionMutation: Boolean,
    val dismissTray: Boolean,
    val effect: FlightPlanRowActionEffect?,
)

sealed interface FlightPlanRowActionEffect {
    data class ShowWeather(val detail: WeatherDetailUiView) : FlightPlanRowActionEffect
    data class LoadAirportInfo(val airportId: String) : FlightPlanRowActionEffect
    data class OpenAirportCharts(val airportId: String) : FlightPlanRowActionEffect
    data class OpenPlateTarget(
        val airportId: String,
        val target: String,
    ) : FlightPlanRowActionEffect
    data class OpenWaypointInsert(
        val rowUid: String,
        val before: Boolean,
    ) : FlightPlanRowActionEffect
    data class OpenAirwayPicker(
        val rowUid: String,
        val header: String,
        val originAnchor: NavRef,
        val destinationAnchor: NavRef?,
    ) : FlightPlanRowActionEffect
    data class OpenProcedurePicker(
        val rowUid: String,
        val airportId: String,
        val procedureKind: ProcedureKind,
        val title: String,
        val emptyMessage: String,
    ) : FlightPlanRowActionEffect
}

data class AirportInfoUiView(
    val airportId: String,
    val name: String,
    val locationLabel: String? = null,
    val elevationLabel: String,
    val trafficPatternAltitudeLabel: String,
    val trafficPatternAltitudeSource: String,
    val timeLabel: String,
    val timeDisplayActionId: String,
    val timeZoneLabel: String,
    val sunrise: AirportSolarEventUiView?,
    val sunset: AirportSolarEventUiView?,
    val communications: List<AirportCommunicationUiView>,
    val runwayDiagramComplex: Boolean,
    val runways: List<AirportRunwayUiView>,
)

data class AirportSolarEventUiView(
    val timeLabel: String,
    val timeDisplayActionId: String,
    val nextInLabel: String?,
)

data class AirportCommunicationUiView(
    val label: String,
    val value: String,
    val kind: String,
)

data class AirportRunwayUiView(
    val endALabel: String,
    val endBLabel: String,
    val dimensionsLabel: String,
    val surfaceLabel: String,
    val surfaceColorKey: String,
    val diagramEndAX: Double,
    val diagramEndAY: Double,
    val diagramEndBX: Double,
    val diagramEndBY: Double,
    val diagramWidthRatio: Double,
    val diagramEndAPattern: AirportRunwayPatternUiView?,
    val diagramEndBPattern: AirportRunwayPatternUiView?,
)

data class AirportRunwayPatternUiView(
    val baseX: Double,
    val baseY: Double,
    val cornerX: Double,
    val cornerY: Double,
    val finalX: Double,
    val finalY: Double,
)

data class WeatherDetailUiView(
    val stationId: String,
    val advisoryText: String,
    val metarText: String?,
    val metarAgeLabel: String?,
    val metarAgeWarning: Boolean = false,
    val tafText: String?,
    val tafAgeLabel: String?,
    val tafAgeWarning: Boolean = false,
    val notams: List<AirportNotamUiView> = emptyList(),
)

data class AirportNotamUiView(
    val id: String,
    val label: String,
    val text: String,
)

data class FlightDataCell(
    val id: String,
    val label: String,
    val value: String?,
    val actionId: String? = null,
    val tone: String = "planned",
    val estimateKind: String = "basic",
)

data class FlightDataColumn(
    val id: String,
    val label: String,
    val actionId: String? = null,
)

data class FlightDataBannerModel(
    val cells: List<FlightDataCell> = emptyList(),
)

data class FlightPlanWeatherBadgeUiView(
    val flightCategory: String,
    val ceilingAmount: String,
)

data class FlightPlanDisplayRowUiView(
    val uid: String = "",
    val label: String,
    val rowKind: FlightPlanDisplayRowKind,
    val componentKind: RouteComponentViewKind?,
    val componentUid: String? = null,
    val procedureId: String?,
    val procedureKind: ProcedureKind?,
    val dataCells: List<FlightDataCell>,
    val showPlateTargetId: String?,
    val chartAirportId: String?,
    val navRef: NavRef?,
    val symbolFeature: NavSymbolFeature?,
    val weatherBadge: FlightPlanWeatherBadgeUiView? = null,
    val depth: Int,
    val active: Boolean,
    val enabled: Boolean,
    val disabledReason: String? = null,
    val syntheticDirectTo: Boolean,
    val canAddAirwayAfter: Boolean,
    val canAddProcedureBefore: Boolean,
    val canRemoveComponent: Boolean,
    val canReorderComponent: Boolean,
    val canReorderUp: Boolean,
    val canReorderDown: Boolean,
    val originAnchor: NavRef?,
    val destinationAnchor: NavRef?,
    val precedingWaypoint: NavRef?,
    val followingWaypoint: NavRef?,
    val actionMatrix: List<List<FlightPlanRowActionUiView>> = emptyList(),
)

data class FlightPlanEntryToken(
    val start: Int,
    val end: Int,
    val state: String,
)

data class FlightPlanEntryIssue(
    val start: Int,
    val end: Int,
    val message: String,
)

data class FlightPlanEntryPreview(
    val canCommit: Boolean,
    val tokens: List<FlightPlanEntryToken>,
    val issues: List<FlightPlanEntryIssue>,
)

enum class TileStorageKind {
    AssetTree,
    SectionalPackage,
    StaticProduct,
}

data class MapViewportSeed(
    val lat: Double,
    val lon: Double,
    val zoom: Double,
)

sealed interface PlateGeoref {
    data class PlateTransformV1(
        val pixelsPerLongitude: Double,
        val pixelsPerLatitude: Double,
        val topLeftLon: Double,
        val topLeftLat: Double,
    ) : PlateGeoref

    data class AirportDiagramTransformV1(
        val pixelXFromLon: Double,
        val pixelXFromLat: Double,
        val pixelXOffset: Double,
        val pixelYFromLon: Double,
        val pixelYFromLat: Double,
        val pixelYOffset: Double,
    ) : PlateGeoref
}

data class ChartAsset(
    val id: String,
    val airportId: String?,
    val collectionId: String,
    val label: String,
    val kind: String,
    val folderCategory: String,
    val hasThumbnail: Boolean,
    val procedureGeometryWarningCount: Int = 0,
    val procedureNotamBadge: PlateProcedureNotamBadge? = null,
    val georef: PlateGeoref? = null,
)

data class PlateProcedureNotamBadge(
    val label: String,
    val count: Int,
    val actionId: String,
    val accessibilityLabel: String,
    val detail: PlateProcedureNotamDetail,
)

data class PlateProcedureNotamDetail(
    val title: String,
    val advisoryText: String,
    val notams: List<AirportNotamUiView>,
)

data class ChartAirport(
    val id: String,
    val label: String,
    val charts: List<ChartAsset>,
)

sealed interface ChartAirportMenuEntry {
    data class Separator(val label: String) : ChartAirportMenuEntry
    data class Airport(val airport: ChartAirport) : ChartAirportMenuEntry
    data class Reference(val reference: ChartAirport) : ChartAirportMenuEntry
    data class ExternalLink(val label: String, val url: String) : ChartAirportMenuEntry
}

data class ChartPageFixture(
    val airports: List<ChartAirport>,
)

data class AppUiState(
    val activePlan: FlightPlanUiState? = null,
    val aircraftPlanViewPath: String = "",
    val ownship: OwnshipUiState = OwnshipUiState(),
    val flightDataBanner: FlightDataBannerModel = FlightDataBannerModel(),
)

sealed interface OwnshipSelection {
    data object Auto : OwnshipSelection
    data class Source(val sourceId: String) : OwnshipSelection
}

enum class OwnshipMode {
    None,
    Live,
    Replay,
    Simulated,
}

enum class OwnshipBannerSeverity {
    Info,
    Caution,
    Warning,
}

enum class OwnshipSourceKind {
    DeviceGps,
    ExternalGps,
    ExternalAhrs,
    GpxPlayback,
    AdsbTrackPlayback,
    LiveNetworkTrack,
    FlightPlanSimulator,
    BadAutopilot,
}

enum class SourceConnectionState {
    Unavailable,
    Searching,
    Connected,
    Stale,
    Failed,
}

enum class OwnshipControlTone {
    Ready,
    Unavailable,
    Neutral,
}

enum class OwnshipLauncherTextTone {
    Normal,
    Unavailable,
}

enum class OwnshipSourcePowerState {
    Running,
    Paused,
    Sleeping,
}

enum class SituationControlInput {
    SkipBackward,
    FastRewind,
    FastForward,
    SkipForward,
    Pause,
    Resume,
}

data class OwnshipRenderState(
    val mode: OwnshipMode = OwnshipMode.None,
    val bannerText: String = "NO GPS POSITION",
    val bannerSeverity: OwnshipBannerSeverity = OwnshipBannerSeverity.Warning,
    val drawAircraft: Boolean = false,
    val drawPredictor: Boolean = false,
    val drawCdi: Boolean = false,
    val position: LatLonPoint? = null,
    val trackDegTrue: Double? = null,
    val orientationDeg: Double? = null,
    val magneticVariationDeg: Double? = null,
    val speedKt: Double? = null,
    val terrainAltitudeBucketFt: Double? = null,
)

data class OwnshipControlModel(
    val mode: OwnshipMode = OwnshipMode.None,
    val selection: OwnshipSelection = OwnshipSelection.Auto,
    val launcherLabel: String = "No GPS",
    val launcherTone: OwnshipControlTone = OwnshipControlTone.Unavailable,
    val launcherTextTone: OwnshipLauncherTextTone = OwnshipLauncherTextTone.Unavailable,
    val sources: List<OwnshipSourceMenuItem> = emptyList(),
    val situationControls: List<SituationControlMenuItem> = emptyList(),
    val textAction: OwnshipTextAction? = null,
    val nextRefreshEpochMs: Long? = null,
)

data class OwnshipTextAction(
    val actionId: String,
    val label: String,
    val value: String,
    val placeholder: String,
    val submitLabel: String,
    val enabled: Boolean,
    val disabledReason: String? = null,
)

data class OwnshipUiState(
    val render: OwnshipRenderState = OwnshipRenderState(),
    val controls: OwnshipControlModel = OwnshipControlModel(),
)

data class SituationRingCandidate(
    val radiusNm: Double,
    val label: String,
)

data class OwnshipSourceMenuItem(
    val sourceId: String,
    val sourceKind: OwnshipSourceKind,
    val label: String,
    val launcherLabel: String,
    val tone: OwnshipControlTone,
    val enabled: Boolean,
    val disabledReason: String? = null,
    val active: Boolean,
    val statusLabel: String,
    val powerState: OwnshipSourcePowerState? = null,
    val keepTrayOpenOnSelect: Boolean = false,
)

data class SituationControlMenuItem(
    val input: SituationControlInput,
    val label: String,
    val enabled: Boolean,
    val disabledReason: String? = null,
)

data class SituationSample(
    val sourceId: String,
    val sourceKind: OwnshipSourceKind,
    val eventTimeEpochMs: Long,
    val receivedTimeEpochMs: Long,
    val position: LatLonPoint? = null,
    val horizontalAccuracyM: Double? = null,
    val verticalAccuracyM: Double? = null,
    val trackDegTrue: Double? = null,
    val headingDegTrue: Double? = null,
    val groundSpeedKt: Double? = null,
    val altitudeMslFt: Double? = null,
    val pressureAltitudeFt: Double? = null,
    val verticalSpeedFpm: Double? = null,
)

data class OwnshipSourceRegistration(
    val sourceId: String,
    val sourceKind: OwnshipSourceKind,
    val displayName: String,
    val selectable: Boolean,
    val autoEligible: Boolean,
    val powerState: OwnshipSourcePowerState? = null,
)

data class OwnshipSourceStatusUpdate(
    val sourceId: String,
    val connectionState: SourceConnectionState,
    val enabled: Boolean,
    val statusLabel: String,
)

data class MapFollowUiState(
    val canCenterHere: Boolean = false,
    val following: Boolean = false,
    val disabledReason: String? = null,
)

data class CoreMapViewport(
    val center: LatLonPoint,
    val zoom: Double,
    val rotationDeg: Double,
    val pitchDeg: Double,
)

enum class PlaybackStatus {
    Empty,
    Paused,
    Playing,
}

data class PlaybackUiState(
    val status: PlaybackStatus = PlaybackStatus.Empty,
    val sourcePath: String? = null,
    val titleLabel: String = "Playback",
    val registration: String? = null,
    val icao: String? = null,
    val aircraftType: String? = null,
    val pointCount: Int = 0,
    val durationSeconds: Double = 0.0,
    val cursorSeconds: Double = 0.0,
    val cursorLabel: String = "0:00",
    val durationLabel: String = "0:00",
    val rate: Double = 1.0,
    val tickIntervalMs: Int = 100,
    val speedProfileNorm: List<Double?> = emptyList(),
    val altitudeProfileNorm: List<Double?> = emptyList(),
    val gapSpans: List<PlaybackGapSpan> = emptyList(),
)

data class PlaybackGapSpan(
    val startSeconds: Double,
    val endSeconds: Double,
)
