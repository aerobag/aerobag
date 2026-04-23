package net.jonh.aerobag.prototype

import android.content.Context
import android.graphics.BitmapFactory
import android.graphics.Paint
import android.graphics.Typeface
import android.os.Bundle
import android.os.SystemClock
import android.util.Log
import android.view.KeyEvent as AndroidKeyEvent
import android.view.MotionEvent
import androidx.appcompat.content.res.AppCompatResources
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.focusable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.wrapContentSize
import androidx.compose.foundation.layout.wrapContentWidth
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items as lazyColumnItems
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items as lazyGridItems
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Checkbox
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathFillType
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.pointer.PointerId
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.pointerInteropFilter
import androidx.compose.ui.input.pointer.positionChanged
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PlatformImeOptions
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.zIndex
import androidx.compose.ui.window.Popup
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import net.jonh.aerobag.prototype.domain.ChartAirport
import net.jonh.aerobag.prototype.domain.ChartAsset
import net.jonh.aerobag.prototype.domain.ChartPackages
import net.jonh.aerobag.prototype.domain.AppState
import net.jonh.aerobag.prototype.domain.AirwayPresentationPlan
import net.jonh.aerobag.prototype.domain.AirwaySuggestion
import net.jonh.aerobag.prototype.domain.WaypointIdentifierSuggestion
import net.jonh.aerobag.prototype.domain.ConcretizedNavItem
import net.jonh.aerobag.prototype.domain.CoreMapViewport
import net.jonh.aerobag.prototype.domain.FlightPlanUiMutation
import net.jonh.aerobag.prototype.domain.FlightPlanDisplayRowKind
import net.jonh.aerobag.prototype.domain.FlightPlanDisplayRowUiView
import net.jonh.aerobag.prototype.domain.FlightPlanRowActionId
import net.jonh.aerobag.prototype.domain.FlightPlanRowActionUiView
import net.jonh.aerobag.prototype.domain.FlightPlanRouteSegment
import net.jonh.aerobag.prototype.domain.FlightPlanUiState
import net.jonh.aerobag.prototype.domain.GuidanceLegGeometry
import net.jonh.aerobag.prototype.domain.GuidanceState
import net.jonh.aerobag.prototype.domain.LatLonPoint
import net.jonh.aerobag.prototype.domain.MapChartFamily
import net.jonh.aerobag.prototype.domain.MapFollowUiState
import net.jonh.aerobag.prototype.domain.MapOverlayQueryResult
import net.jonh.aerobag.prototype.domain.MapView
import net.jonh.aerobag.prototype.domain.MapViewportState
import net.jonh.aerobag.prototype.domain.NativeAppCoreAdapter
import net.jonh.aerobag.prototype.domain.NativeUiSession
import net.jonh.aerobag.prototype.domain.NavRef
import net.jonh.aerobag.prototype.domain.NavElementUiView
import net.jonh.aerobag.prototype.domain.OwnshipMode
import net.jonh.aerobag.prototype.domain.OwnshipRenderState
import net.jonh.aerobag.prototype.domain.OwnshipSourceKind
import net.jonh.aerobag.prototype.domain.OwnshipSourceRegistration
import net.jonh.aerobag.prototype.domain.OwnshipSourceStatusUpdate
import net.jonh.aerobag.prototype.domain.PlaybackStatus
import net.jonh.aerobag.prototype.domain.PlaybackUiState
import net.jonh.aerobag.prototype.domain.PointTilePayload
import net.jonh.aerobag.prototype.domain.ProcedureKind
import net.jonh.aerobag.prototype.domain.ProcedureOptions
import net.jonh.aerobag.prototype.domain.ProcedureSummary
import net.jonh.aerobag.prototype.domain.ResolvedLeg
import net.jonh.aerobag.prototype.domain.ResolvedLegSource
import net.jonh.aerobag.prototype.domain.RouteSegmentStatus
import net.jonh.aerobag.prototype.domain.RouteComponentUiView
import net.jonh.aerobag.prototype.domain.RouteComponentViewKind
import net.jonh.aerobag.prototype.domain.RouteComponent
import net.jonh.aerobag.prototype.domain.ScreenPoint
import net.jonh.aerobag.prototype.domain.SectionalPackages
import net.jonh.aerobag.prototype.domain.SampleData
import net.jonh.aerobag.prototype.domain.SequencingMode
import net.jonh.aerobag.prototype.domain.SituationRingCandidate
import net.jonh.aerobag.prototype.domain.SituationSample
import net.jonh.aerobag.prototype.domain.SourceConnectionState
import net.jonh.aerobag.prototype.domain.TileStorageKind
import net.jonh.aerobag.prototype.domain.UiTheme
import net.jonh.aerobag.prototype.domain.UiThemeLoader
import net.jonh.aerobag.prototype.domain.UiSessionSnapshot
import net.jonh.aerobag.prototype.domain.applyPinchGesture
import net.jonh.aerobag.prototype.domain.clampZoom
import net.jonh.aerobag.prototype.domain.createInitialImageViewport
import net.jonh.aerobag.prototype.domain.createInitialViewport
import net.jonh.aerobag.prototype.domain.createPinchSnapshot
import net.jonh.aerobag.prototype.domain.dragImageViewport
import net.jonh.aerobag.prototype.domain.dragViewport
import net.jonh.aerobag.prototype.domain.imageDisplaySize
import net.jonh.aerobag.prototype.domain.latLonToWorld
import net.jonh.aerobag.prototype.domain.preserveViewportForMap
import net.jonh.aerobag.prototype.domain.renderTiles
import net.jonh.aerobag.prototype.domain.scaleForZoom
import net.jonh.aerobag.prototype.domain.screenToWorld
import net.jonh.aerobag.prototype.domain.tileRelativePath
import net.jonh.aerobag.prototype.domain.viewportCenterLatLon
import net.jonh.aerobag.prototype.domain.zoomAroundPoint
import net.jonh.aerobag.prototype.domain.zoomImageAroundPoint
import kotlinx.serialization.json.Json
import java.io.BufferedInputStream
import java.net.URL
import java.util.zip.ZipInputStream
import kotlin.math.abs
import kotlin.math.roundToInt
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.hypot
import kotlin.math.sin

private val LocalAerobagUiTheme = staticCompositionLocalOf<UiTheme> {
    error("Aerobag UI theme not provided")
}

private val ThumbSize = 56.dp
private val ThumbGap = 5.6.dp
private val PlanGridGap = 2.dp
private const val DefaultPlaybackTracePath = "/adsb-traces/n550ar/n550ar-2024-09-29.json"
private const val DefaultAndroidDevServerBaseUrl = "http://10.0.2.2:8080"
private val VampsPosition = LatLon(47.3648944444444, -121.980275)

private data class LatLon(val lat: Double, val lon: Double)

private data class SituationOverlay(
    val pointUnits: Offset,
    val headingDeg: Float,
    val predictorUnits: Offset?,
    val ring: SituationRing,
)

private data class SituationRing(
    val radiusUnits: Float,
    val tickMarks: List<SituationTickMark>,
    val labelPointUnits: Offset,
    val labelRotationDeg: Float,
    val labelText: String,
)

private data class SituationTickMark(
    val innerUnits: Offset,
    val outerUnits: Offset,
)

private object VectorTileAssets {
    private const val VECTOR_ZIP_ASSET_PATH = "fixtures/vectors.zip"
    private val json = Json {
        ignoreUnknownKeys = true
    }
    private val cache = mutableMapOf<String, PointTilePayload?>()

    suspend fun loadPointTiles(context: Context, requests: List<net.jonh.aerobag.prototype.domain.VectorTileRequest>): List<PointTilePayload> =
        withContext(Dispatchers.IO) {
            if (requests.isEmpty()) {
                return@withContext emptyList()
            }
            val entryNames = requests.map { request ->
                "points/${request.layer}/${request.z}/${request.x}/${request.y}.json"
            }
            val missing = synchronized(cache) { entryNames.filter { !cache.containsKey(it) }.toSet() }
            if (missing.isNotEmpty()) {
                val unresolved = missing.toMutableSet()
                context.assets.open(VECTOR_ZIP_ASSET_PATH).use { assetStream ->
                    ZipInputStream(BufferedInputStream(assetStream)).use { zipStream ->
                        while (true) {
                            val entry = zipStream.nextEntry ?: break
                            if (entry.isDirectory || entry.name !in unresolved) {
                                continue
                            }
                            val payload = runCatching {
                                json.decodeFromString<PointTilePayload>(zipStream.readBytes().decodeToString())
                            }.getOrNull()
                            synchronized(cache) {
                                cache[entry.name] = payload
                            }
                            unresolved.remove(entry.name)
                        }
                    }
                }
                synchronized(cache) {
                    unresolved.forEach { entryName ->
                        val parts = entryName.removePrefix("points/").removeSuffix(".json").split("/")
                        if (parts.size == 4) {
                            cache[entryName] = PointTilePayload(
                                schemaVersion = 1,
                                layer = parts[0],
                                z = parts[1].toIntOrNull() ?: 0,
                                x = parts[2].toIntOrNull() ?: 0,
                                y = parts[3].toIntOrNull() ?: 0,
                                records = emptyList(),
                            )
                        }
                    }
                }
            }
            synchronized(cache) {
                entryNames.mapNotNull { cache[it] }
            }
        }
}
private val ThumbRadius = 10.dp
private val FolderThumbGutter = ThumbSize * 0.3f
private val PlateFolderTileWidth = ThumbSize * 2f
private val PlateFolderTileHeight = ThumbSize * 3f
private val PlatePageTrayWidth = ThumbSize * 4f
private val PlanArrowLane = ThumbSize * 0.5f
private val PlanArrowButtonInset = 5.dp
private const val UiPrefsName = "aerobag_ui"
private const val UiPrefsPageKey = "page"
private const val UiPrefsSelectedAirportKey = "selected_airport_id"
private const val UiPrefsSelectedChartKey = "selected_chart_id"
private const val UiPrefsRecentAirportsKey = "recent_airport_ids"
private const val MaxViewHistoryDepth = 64

private enum class AppPage {
    Map,
    Plan,
    Charts,
}

private data class AppViewSnapshot(
    val page: AppPage,
    val selectedMapId: String,
    val mapViewport: MapViewportState,
    val selectedAirportId: String,
    val selectedChartId: String,
    val selectedChartLabel: String,
    val recentAirportIds: List<String>,
    val chartViewport: net.jonh.aerobag.prototype.domain.ImageViewportState?,
    val chartFolderOpen: Boolean,
)

private data class HardwareZoomState(
    val selectedMap: MapView,
    val viewport: MapViewportState,
    val widthUnits: Float,
    val heightUnits: Float,
    val pageTrayOpen: Boolean,
    val chartTrayOpen: Boolean,
    val updateViewport: (MapViewportState) -> Unit,
)

private data class FlightPlanDisplayRow(
    val id: String,
    val selectionKey: String,
    val label: String,
    val rowKind: String,
    val componentKind: RouteComponentViewKind? = null,
    val componentIndex: Int? = null,
    val legIndex: Int? = null,
    val distanceNm: Double? = null,
    val courseDeg: Double? = null,
    val chartAirportId: String? = null,
    val navRef: NavRef? = null,
    val symbolFeature: net.jonh.aerobag.prototype.domain.NavSymbolFeature? = null,
    val depth: Int = 0,
    val active: Boolean = false,
    val canAddAirwayAfter: Boolean = false,
    val canAddProcedureBefore: Boolean = false,
    val canChangeAirway: Boolean = false,
    val canRemoveComponent: Boolean = false,
    val canReorderComponent: Boolean = false,
    val canReorderUp: Boolean = false,
    val canReorderDown: Boolean = false,
    val actions: List<FlightPlanRowActionUiView> = emptyList(),
    val startComponentIndex: Int? = null,
    val endComponentIndex: Int? = null,
    val originAnchor: NavRef? = null,
    val destinationAnchor: NavRef? = null,
)

private sealed interface FlightPlanDisplayBlock {
    data class Single(
        val index: Int,
        val row: FlightPlanDisplayRow,
    ) : FlightPlanDisplayBlock

    data class Group(
        val headerIndex: Int,
        val header: FlightPlanDisplayRow,
        val children: List<Pair<Int, FlightPlanDisplayRow>>,
    ) : FlightPlanDisplayBlock
}

private data class StructuredArrowSpec(
    val fromPoint: Offset,
    val toPoint: Offset,
    val toClipped: Boolean,
    val fromClippedAbove: Boolean,
    val elbowX: Float,
    val shaftEndX: Float,
    val headLength: Float,
)

private data class StructuredArrowEndpoint(
    val point: Offset,
    val clipped: Boolean,
    val clippedAbove: Boolean,
    val clippedBelow: Boolean,
)

@Composable
private fun rememberStructuredRowBounds(
    rowId: String,
    structuredRowBounds: MutableMap<String, Rect>,
): Modifier {
    DisposableEffect(rowId, structuredRowBounds) {
        onDispose {
            structuredRowBounds.remove(rowId)
        }
    }
    return Modifier.onGloballyPositioned { coordinates ->
        structuredRowBounds[rowId] = coordinates.boundsInWindow()
    }
}

private data class AndroidAirwayPickerState(
    val loading: Boolean,
    val error: String?,
    val mode: String,
    val componentIndex: Int?,
    val startComponentIndex: Int?,
    val endComponentIndex: Int?,
    val originAnchor: NavRef,
    val destinationAnchor: NavRef?,
    val suggestions: List<AirwaySuggestion>,
    val selectedAirwayName: String?,
    val presentation: AirwayPresentationPlan?,
    val selectedEntryIndex: Int?,
)

private data class AndroidProcedurePickerState(
    val loading: Boolean,
    val error: String?,
    val airportId: String,
    val startComponentIndex: Int,
    val endComponentIndex: Int,
    val procedures: List<ProcedureSummary>,
    val selectedProcedureId: String?,
    val options: ProcedureOptions?,
)

private data class AndroidAirportInsertState(
    val componentIndex: Int,
    val before: Boolean,
    val airportId: String,
    val error: String?,
    val loading: Boolean,
    val suggestions: List<WaypointIdentifierSuggestion>,
)

private data class PageTrayOption(
    val page: AppPage,
    val label: String,
    val launcherLabel: String,
)

private data class MenuDockOption(
    val key: String,
    val label: String,
    val active: Boolean = false,
    val enabled: Boolean = true,
    val accentColor: Color? = null,
    val onSelect: () -> Unit,
)

private enum class MenuDockStyle(
    val buttonWidth: androidx.compose.ui.unit.Dp,
    val trayWidth: androidx.compose.ui.unit.Dp,
    val launcherMaxLines: Int,
) {
    Compact(
        buttonWidth = ThumbSize,
        trayWidth = ThumbSize * 2.4f,
        launcherMaxLines = 1,
    ),
    PlateAirport(
        buttonWidth = ThumbSize,
        trayWidth = PlatePageTrayWidth,
        launcherMaxLines = 1,
    ),
    PlateWide(
        buttonWidth = ThumbSize * 3f,
        trayWidth = PlatePageTrayWidth,
        launcherMaxLines = 2,
    ),
}

private val PageOptions = listOf(
    PageTrayOption(AppPage.Map, "CHART", "CHT"),
    PageTrayOption(AppPage.Charts, "PLATE", "PLT"),
    PageTrayOption(AppPage.Plan, "PLAN", "PLN"),
)

private data class ChartTrayOption(
    val id: String,
    val label: String,
    val launcherLabel: String,
    val available: Boolean,
    val select: (() -> Unit)?,
)

private data class TileRect(
    val leftPx: Int,
    val topPx: Int,
    val widthPx: Int,
    val heightPx: Int,
)

private data class OverlaySurfaceUnits(
    val width: Float,
    val height: Float,
)

private fun initialMapId(fixture: net.jonh.aerobag.prototype.domain.ContentFixture): String {
    return fixture.mapViews.firstOrNull {
        it.mapView.chartFamily == MapChartFamily.Tac
    }?.id ?: fixture.mapViews.first().id
}

private fun mergeRecentAirportIds(
    airports: List<ChartAirport>,
    storedIds: List<String>,
): List<String> {
    val validIds = airports.map { it.id }.toSet()
    val orderedIds = storedIds.filterIndexed { index, id ->
        validIds.contains(id) && storedIds.indexOf(id) == index
    }.toMutableList()
    airports.forEach { airport ->
        if (!orderedIds.contains(airport.id)) {
            orderedIds += airport.id
        }
    }
    return orderedIds
}

private fun moveAirportToFront(
    currentIds: List<String>,
    airportId: String,
    airports: List<ChartAirport>,
): List<String> = mergeRecentAirportIds(airports, listOf(airportId) + currentIds.filterNot { it == airportId })

private fun boundedHistory(history: List<AppViewSnapshot>): List<AppViewSnapshot> =
    if (history.size <= MaxViewHistoryDepth) history else history.takeLast(MaxViewHistoryDepth)

private fun resolveAirportId(
    airports: List<ChartAirport>,
    candidateAirportId: String?,
    recentAirportIds: List<String>,
): String {
    if (candidateAirportId != null && airports.any { it.id == candidateAirportId }) {
        return candidateAirportId
    }
    return recentAirportIds.firstOrNull() ?: airports.firstOrNull()?.id.orEmpty()
}

private fun resolveChartId(
    airports: List<ChartAirport>,
    airportId: String,
    candidateChartId: String?,
): String {
    val airport = airports.firstOrNull { it.id == airportId }
    if (candidateChartId != null && airport?.charts?.any { it.id == candidateChartId } == true) {
        return candidateChartId
    }
    return airport?.charts?.firstOrNull()?.id.orEmpty()
}

private fun routeSegmentColor(status: RouteSegmentStatus): Color =
    when (status) {
        RouteSegmentStatus.Completed -> Color(0xFF8C9DAD)
        RouteSegmentStatus.Active -> Color(0xFFFF4FCF)
        RouteSegmentStatus.Remaining -> Color.White
    }

private fun latLonToScreenPoint(
    viewport: MapViewportState,
    point: LatLonPoint,
    widthPx: Float,
    heightPx: Float,
): Offset {
    val world = latLonToWorld(point.lat, point.lon)
    val scale = scaleForZoom(viewport.zoom)
    return Offset(
        x = ((world.x - viewport.centerWorldX) * scale + widthPx / 2f).toFloat(),
        y = ((world.y - viewport.centerWorldY) * scale + heightPx / 2f).toFloat(),
    )
}

private fun plateFolderColor(uiTheme: UiTheme, category: String): Color =
    uiTheme.plateFolder.labelColors[category] ?: uiTheme.plateFolder.labelColors["other"] ?: Color(0xFF52656D)

private fun demoOwnshipSourceRegistration() =
    OwnshipSourceRegistration(
        sourceId = "demo-gps",
        sourceKind = OwnshipSourceKind.DeviceGps,
        displayName = "Demo GPS",
        selectable = true,
        autoEligible = true,
    )

private fun demoSituationSample() =
    SituationSample(
        sourceId = "demo-gps",
        sourceKind = OwnshipSourceKind.DeviceGps,
        eventTimeEpochMs = System.currentTimeMillis(),
        receivedTimeEpochMs = System.currentTimeMillis(),
        position = LatLonPoint(lat = VampsPosition.lat, lon = VampsPosition.lon),
        trackDegTrue = 135.0,
        headingDegTrue = 135.0,
        groundSpeedKt = 105.0,
        altitudeMslFt = null,
        pressureAltitudeFt = null,
    )

private fun buildSeededDevPlan(
    adapter: NativeAppCoreAdapter,
    plan: net.jonh.aerobag.prototype.domain.FlightPlan,
): FlightPlanUiMutation {
    return runCatching {
        val waypoints = listOf(
            NavRef.Airport("KPAO"),
            NavRef.Fix("VPDUB"),
            NavRef.Airport("KVCB"),
            NavRef.Airport("KWLW"),
        )
        val resolvedLegs =
            waypoints.zipWithNext().mapIndexed { index, (from, to) ->
                ResolvedLeg(
                    id = "component-$index-${index + 1}",
                    from = from,
                    to = to,
                    source = ResolvedLegSource.RouteComponent(componentIndex = index),
                )
            }
        val seededPlan =
            plan.copy(
                id = "dev-kpao-vpdub-kvcb-kwlw",
                name = "KPAO VPDUB KVCB KWLW",
                legs = resolvedLegs.map { leg -> net.jonh.aerobag.prototype.domain.FlightPlanLeg(leg.from, leg.to, null) },
                routeComponents = waypoints.map { waypoint -> RouteComponent.Waypoint(waypoint) },
                resolvedLegs = resolvedLegs,
                guidance = GuidanceState(
                    activeLegIndex = 0,
                    sequencingMode = SequencingMode.FollowPlan,
                    directTo = null,
                ),
                departure = "KPAO",
                destination = "KWLW",
                updatedAtEpochMs = System.currentTimeMillis(),
                version = plan.version + 1,
            )
        adapter.activateLegUi(seededPlan, 0)
    }.getOrElse {
        Log.e("AerobagSeed", "buildSeededDevPlan fell back to sample plan", it)
        FlightPlanUiMutation(
            plan = plan,
            uiState = FlightPlanUiState(
                components = emptyList(),
                resolvedLegs = emptyList(),
                displayRows = emptyList(),
                guidance = null,
            ),
        )
    }
}

private fun createInitialSituationViewport(mapView: MapView): MapViewportState {
    val center = latLonToWorld(VampsPosition.lat, VampsPosition.lon)
    return MapViewportState(
        centerWorldX = center.x,
        centerWorldY = center.y,
        zoom = 10.0,
    )
}

private fun mapViewportFromCore(viewport: CoreMapViewport): MapViewportState {
    val center = latLonToWorld(viewport.center.lat, viewport.center.lon)
    return MapViewportState(
        centerWorldX = center.x,
        centerWorldY = center.y,
        zoom = viewport.zoom,
    )
}

private fun sameMapViewport(left: MapViewportState, right: MapViewportState): Boolean =
    abs(left.centerWorldX - right.centerWorldX) < 1e-9 &&
        abs(left.centerWorldY - right.centerWorldY) < 1e-9 &&
        abs(left.zoom - right.zoom) < 1e-9

@Composable
private fun SituationStatusBadge(
    ownship: OwnshipRenderState,
    modifier: Modifier = Modifier,
) {
    val tone = when (ownship.mode) {
        OwnshipMode.None -> Triple(ownship.bannerText, Color(0xFFB3261E), "unknown")
        OwnshipMode.Simulated -> Triple(ownship.bannerText, Color(0xFFB1591A), "simulated")
        OwnshipMode.Live,
        OwnshipMode.Replay,
        -> Triple(ownship.bannerText, Color(0xFF2A4F66), "live")
    }
    Surface(
        modifier = modifier,
        shape = RoundedCornerShape(ThumbSize * 0.22f),
        color = Color(0xE6FCF8F1),
        shadowElevation = 4.dp,
    ) {
        Text(
            text = tone.first,
            color = tone.second,
            style = MaterialTheme.typography.labelSmall,
            fontWeight = FontWeight.Bold,
            modifier = Modifier.padding(horizontal = ThumbSize * 0.18f, vertical = ThumbSize * 0.12f),
        )
    }
}

private fun resolveSituationOverlay(
    ownship: OwnshipRenderState,
    viewport: MapViewportState,
    widthUnits: Float,
    heightUnits: Float,
    ringCandidates: List<SituationRingCandidate>,
): SituationOverlay? {
    if (widthUnits <= 0f || heightUnits <= 0f) return null
    if (!ownship.drawAircraft) return null
    val current = ownship.position ?: return null
    val position = LatLon(current.lat, current.lon)
    val point = latLonToScreen(position.lat, position.lon, viewport, widthUnits, heightUnits)
    val heading = (ownship.orientationDeg ?: 0.0).toFloat()
    val predictor = ownship.speedKt?.takeIf { ownship.drawPredictor }?.let { speedKt ->
        val ahead = projectAhead(position.lat, position.lon, heading.toDouble(), speedKt / 60.0)
        latLonToScreen(ahead.lat, ahead.lon, viewport, widthUnits, heightUnits)
    }
    return SituationOverlay(
        pointUnits = point,
        headingDeg = heading,
        predictorUnits = predictor,
        ring = selectSituationRing(position, viewport, widthUnits, heightUnits, ringCandidates),
    )
}

private fun latLonToScreen(
    lat: Double,
    lon: Double,
    viewport: MapViewportState,
    widthUnits: Float,
    heightUnits: Float,
): Offset {
    val world = latLonToWorld(lat, lon)
    val scale = scaleForZoom(viewport.zoom)
    return Offset(
        x = (((world.x - viewport.centerWorldX) * scale) + widthUnits / 2f).toFloat(),
        y = (((world.y - viewport.centerWorldY) * scale) + heightUnits / 2f).toFloat(),
    )
}

private fun projectAhead(lat: Double, lon: Double, bearingDeg: Double, distanceNm: Double): LatLon {
    val angularDistance = distanceNm / 3440.065
    val bearing = Math.toRadians(bearingDeg)
    val startLat = Math.toRadians(lat)
    val startLon = Math.toRadians(lon)
    val nextLat = kotlin.math.asin(
        kotlin.math.sin(startLat) * kotlin.math.cos(angularDistance) +
            kotlin.math.cos(startLat) * kotlin.math.sin(angularDistance) * kotlin.math.cos(bearing),
    )
    val nextLon = startLon + atan2(
        sin(bearing) * sin(angularDistance) * kotlin.math.cos(startLat),
        kotlin.math.cos(angularDistance) - kotlin.math.sin(startLat) * kotlin.math.sin(nextLat),
    )
    return LatLon(Math.toDegrees(nextLat), Math.toDegrees(nextLon))
}

private fun selectSituationRing(
    position: LatLon,
    viewport: MapViewportState,
    widthUnits: Float,
    heightUnits: Float,
    ringCandidates: List<SituationRingCandidate>,
): SituationRing {
    val center = latLonToScreen(position.lat, position.lon, viewport, widthUnits, heightUnits)
    val smaller = minOf(widthUnits, heightUnits)
    val minDiameter = smaller * 0.5f
    val maxDiameter = smaller * 0.8f
    val targetDiameter = smaller * 0.65f
    val best = ringCandidates
        .map { candidate ->
            val edge = projectAhead(position.lat, position.lon, 90.0, candidate.radiusNm)
            val edgePoint = latLonToScreen(edge.lat, edge.lon, viewport, widthUnits, heightUnits)
            val radiusUnits = hypot(edgePoint.x - center.x, edgePoint.y - center.y)
            val diameterUnits = radiusUnits * 2f
            val outOfBounds = when {
                diameterUnits < minDiameter -> minDiameter - diameterUnits
                diameterUnits > maxDiameter -> diameterUnits - maxDiameter
                else -> 0f
            }
            val score = if (outOfBounds > 0f) 10000f + outOfBounds else kotlin.math.abs(diameterUnits - targetDiameter)
            Triple(candidate, radiusUnits, score)
        }
        .minBy { it.third }
    val labelPoint = pointOnCircle(center, best.second + 16f, -45f)
    return SituationRing(
        radiusUnits = best.second,
        tickMarks = buildSituationTickMarks(center, best.second),
        labelPointUnits = labelPoint,
        labelRotationDeg = 45f,
        labelText = best.first.label,
    )
}

private fun buildSituationTickMarks(center: Offset, radiusUnits: Float): List<SituationTickMark> =
    List(12) { index ->
        val angle = index * 30f
        SituationTickMark(
            innerUnits = pointOnCircle(center, radiusUnits - 14f, angle),
            outerUnits = pointOnCircle(center, radiusUnits, angle),
        )
    }

private fun pointOnCircle(center: Offset, radiusUnits: Float, angleDeg: Float): Offset {
    val radians = Math.toRadians(angleDeg.toDouble())
    return Offset(
        x = center.x + (radiusUnits * cos(radians)).toFloat(),
        y = center.y + (radiusUnits * sin(radians)).toFloat(),
    )
}

private fun arrowShaftEndPoint(from: Offset, to: Offset): Offset {
    val angle = atan2(to.y - from.y, to.x - from.x)
    val headLength = 14f
    return Offset(
        x = to.x - headLength * cos(angle),
        y = to.y - headLength * sin(angle),
    )
}

private fun arrowHeadPath(from: Offset, to: Offset): Path {
    val angle = atan2(to.y - from.y, to.x - from.x)
    val size = 20f
    val left = Offset(
        x = to.x - size * cos(angle - Math.PI.toFloat() / 6f),
        y = to.y - size * sin(angle - Math.PI.toFloat() / 6f),
    )
    val right = Offset(
        x = to.x - size * cos(angle + Math.PI.toFloat() / 6f),
        y = to.y - size * sin(angle + Math.PI.toFloat() / 6f),
    )
    return Path().apply {
        moveTo(to.x, to.y)
        lineTo(left.x, left.y)
        lineTo(right.x, right.y)
        close()
    }
}

private fun fixTrianglePath(center: Offset, radius: Float): Path =
    Path().apply {
        moveTo(center.x, center.y - radius)
        lineTo(center.x + radius * 0.875f, center.y + radius * 0.75f)
        lineTo(center.x - radius * 0.875f, center.y + radius * 0.75f)
        close()
    }

private fun vorHexPoints(center: Offset, radius: Float): List<Offset> =
    List(6) { index ->
        val angle = Math.toRadians((-90 + index * 60).toDouble())
        Offset(
            x = center.x + (radius * cos(angle)).toFloat(),
            y = center.y + (radius * sin(angle)).toFloat(),
        )
    }

private fun polygonSignedArea(points: List<Offset>): Float {
    var area = 0f
    points.forEachIndexed { index, point ->
        val next = points[(index + 1) % points.size]
        area += point.x * next.y - next.x * point.y
    }
    return area / 2f
}

private fun intersectLines(originA: Offset, directionA: Offset, originB: Offset, directionB: Offset): Offset {
    val cross = directionA.x * directionB.y - directionA.y * directionB.x
    if (kotlin.math.abs(cross) < 1e-6f) {
        return originA
    }
    val delta = originB - originA
    val t = (delta.x * directionB.y - delta.y * directionB.x) / cross
    return originA + directionA * t
}

private fun offsetPolygonByEdgeDistances(points: List<Offset>, edgeDistances: List<Float>): List<Offset> {
    val signedArea = polygonSignedArea(points)
    fun inwardNormal(from: Offset, to: Offset, distance: Float): Offset {
        val dx = to.x - from.x
        val dy = to.y - from.y
        val length = kotlin.math.hypot(dx, dy).takeIf { it > 0f } ?: 1f
        return if (signedArea > 0f) {
            Offset((dy / length) * distance, (-dx / length) * distance)
        } else {
            Offset((-dy / length) * distance, (dx / length) * distance)
        }
    }

    return points.mapIndexed { index, point ->
        val prevIndex = (index + points.size - 1) % points.size
        val nextIndex = (index + 1) % points.size
        val prevPoint = points[prevIndex]
        val nextPoint = points[nextIndex]
        val prevShift = inwardNormal(prevPoint, point, edgeDistances[prevIndex])
        val nextShift = inwardNormal(point, nextPoint, edgeDistances[index])
        val prevOrigin = prevPoint + prevShift
        val nextOrigin = point + nextShift
        intersectLines(
            prevOrigin,
            point - prevPoint,
            nextOrigin,
            nextPoint - point,
        )
    }
}

private fun polygonPath(points: List<Offset>): Path =
    Path().apply {
        if (points.isNotEmpty()) {
            moveTo(points.first().x, points.first().y)
            points.drop(1).forEach { point -> lineTo(point.x, point.y) }
            close()
        }
    }

private fun vorBandPath(center: Offset, radius: Float): Path {
    val outer = vorHexPoints(center, radius)
    val inner = offsetPolygonByEdgeDistances(outer, listOf(
        radius * 0.47f,
        radius * 0.24f,
        radius * 0.47f,
        radius * 0.24f,
        radius * 0.47f,
        radius * 0.24f,
    ))
    return Path().apply {
        fillType = PathFillType.EvenOdd
        addPath(polygonPath(outer))
        addPath(polygonPath(inner))
    }
}

private fun transformVisibleFeature(
    feature: net.jonh.aerobag.prototype.domain.VisibleMapFeature,
    fromViewport: MapViewportState,
    fromSurface: OverlaySurfaceUnits,
    toViewport: MapViewportState,
    toSurface: OverlaySurfaceUnits,
): net.jonh.aerobag.prototype.domain.VisibleMapFeature {
    val world =
        screenToWorld(
            viewport = fromViewport,
            point = ScreenPoint(feature.screenX.toFloat(), feature.screenY.toFloat()),
            widthPx = fromSurface.width,
            heightPx = fromSurface.height,
        )
    val nextScale = scaleForZoom(toViewport.zoom)
    return feature.copy(
        screenX = (world.x - toViewport.centerWorldX) * nextScale + toSurface.width / 2.0,
        screenY = (world.y - toViewport.centerWorldY) * nextScale + toSurface.height / 2.0,
    )
}

private fun formatPlanDistance(distanceNm: Double?): String =
    when {
        distanceNm == null -> "—"
        distanceNm < 10.0 -> "%.1f".format(distanceNm)
        else -> "%.0f".format(distanceNm)
    }

private fun formatPlanCourse(courseDeg: Double?): String {
    if (courseDeg == null) {
        return "—"
    }
    val rounded = ((courseDeg.roundToInt() % 360) + 360) % 360
    return if (rounded == 0) "360" else rounded.toString().padStart(3, '0')
}

private fun readRecentAirportIds(context: Context): List<String> =
    context.getSharedPreferences(UiPrefsName, Context.MODE_PRIVATE)
        .getString(UiPrefsRecentAirportsKey, "")
        .orEmpty()
        .split('\n')
        .map(String::trim)
        .filter(String::isNotEmpty)

private fun writeUiPrefs(
    context: Context,
    page: AppPage,
    selectedAirportId: String,
    selectedChartId: String,
    recentAirportIds: List<String>,
) {
    context.getSharedPreferences(UiPrefsName, Context.MODE_PRIVATE)
        .edit()
        .putString(UiPrefsPageKey, page.name)
        .putString(UiPrefsSelectedAirportKey, selectedAirportId)
        .putString(UiPrefsSelectedChartKey, selectedChartId)
        .putString(UiPrefsRecentAirportsKey, recentAirportIds.joinToString("\n"))
        .apply()
}

class MainActivity : ComponentActivity() {
    var onHardwareZoomDelta: ((Double) -> Boolean)? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            MaterialTheme {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = Color(0xFFF3EFE4),
                ) {
                    AerobagApp()
                }
            }
        }
    }

    override fun dispatchKeyEvent(event: AndroidKeyEvent): Boolean {
        if (event.action == AndroidKeyEvent.ACTION_DOWN) {
            val delta = when (event.keyCode) {
                AndroidKeyEvent.KEYCODE_EQUALS,
                AndroidKeyEvent.KEYCODE_PLUS,
                AndroidKeyEvent.KEYCODE_NUMPAD_ADD -> 0.35
                AndroidKeyEvent.KEYCODE_MINUS,
                AndroidKeyEvent.KEYCODE_NUMPAD_SUBTRACT -> -0.35
                else -> null
            }
            if (delta != null && (onHardwareZoomDelta?.invoke(delta) == true)) {
                return true
            }
        }
        return super.dispatchKeyEvent(event)
    }
}

@Composable
private fun AerobagApp() {
    val context = LocalContext.current
    val fixture = remember(context) { SampleData.load(context.applicationContext) }
    val uiTheme = remember(context) { UiThemeLoader.load(context.applicationContext) }
    val appCore = remember(fixture.catalogJson, fixture.vectorManifestJson, fixture.chartCatalogJson, fixture.navKvStore) {
        NativeAppCoreAdapter(
            fixture.catalogJson,
            fixture.vectorManifestJson,
            fixture.chartCatalogJson,
            navKvStore = fixture.navKvStore,
        )
    }
    val situationRingCandidates = remember(appCore) { appCore.situationRingCandidates() }
    val initialPlanMutation = remember(appCore, fixture.samplePlan) {
        buildSeededDevPlan(appCore, fixture.samplePlan)
    }
    val prefs = remember(context) { context.applicationContext.getSharedPreferences(UiPrefsName, Context.MODE_PRIVATE) }
    val sessionStartElapsedMs = remember { SystemClock.elapsedRealtime() }
    val uptimeLabel = rememberUptimeLabel(sessionStartElapsedMs)
    val storedRecentAirportIds = remember { readRecentAirportIds(context.applicationContext) }
    val storedSelectedAirportId = remember { prefs.getString(UiPrefsSelectedAirportKey, null).orEmpty() }
    val storedSelectedChartId = remember { prefs.getString(UiPrefsSelectedChartKey, null).orEmpty() }
    var page by remember {
        mutableStateOf(
            runCatching { AppPage.valueOf(prefs.getString(UiPrefsPageKey, AppPage.Map.name) ?: AppPage.Map.name) }
                .getOrDefault(AppPage.Map),
        )
    }
    var pageHistory by remember { mutableStateOf<List<AppViewSnapshot>>(emptyList()) }
    var selectedMapId by remember { mutableStateOf(initialMapId(fixture)) }
    val uiSession = remember(appCore, fixture.resourceIndexJson) {
        appCore.createUiSession(
            initialPlanMutation.plan,
            storedRecentAirportIds,
            storedSelectedAirportId.ifBlank { null },
            storedSelectedChartId.ifBlank { null },
        )
    }
    DisposableEffect(uiSession) {
        onDispose { uiSession.destroy() }
    }
    var sessionSnapshot by remember(uiSession) { mutableStateOf(uiSession.snapshot) }
    val appState = sessionSnapshot.appState
    val appUiState = sessionSnapshot.appUiState
    val currentPlan = appState.activePlan ?: initialPlanMutation.plan
    val chartCatalog = uiSession.chartCatalog
    val derivedChartPageState = sessionSnapshot.chartPageState
    val selectedMap = remember(selectedMapId, fixture.mapViews) {
        fixture.mapViews.find { it.id == selectedMapId } ?: fixture.mapViews.first()
    }
    var mapViewport by remember { mutableStateOf(createInitialSituationViewport(selectedMap.mapView)) }
    var chartViewport by remember { mutableStateOf<net.jonh.aerobag.prototype.domain.ImageViewportState?>(null) }
    var chartFolderOpen by remember { mutableStateOf(false) }
    var playbackSourcePath by remember { mutableStateOf(DefaultPlaybackTracePath) }
    val planListState = rememberLazyListState()
    val chartAirportById = remember(chartCatalog.airports) { chartCatalog.airports.associateBy { it.id } }
    val orderedChartAirports = remember(chartCatalog.airports, derivedChartPageState.orderedAirportIds) {
        derivedChartPageState.orderedAirportIds.mapNotNull { chartAirportById[it] }
    }
    val recentAirportIds = derivedChartPageState.recentAirportIds
    val selectedAirportId = derivedChartPageState.selectedAirportId
    val selectedChartId = derivedChartPageState.selectedChartId
    val selectedAirport = remember(selectedAirportId, orderedChartAirports) {
        orderedChartAirports.find { it.id == selectedAirportId } ?: orderedChartAirports.firstOrNull()
    }
    val selectedChart = remember(selectedAirport, selectedChartId) {
        selectedAirport?.charts?.find { it.id == selectedChartId } ?: selectedAirport?.charts?.firstOrNull()
    }

    LaunchedEffect(page, selectedAirportId, selectedChartId, recentAirportIds) {
        writeUiPrefs(context.applicationContext, page, selectedAirportId, selectedChartId, recentAirportIds)
    }
    LaunchedEffect(uiSession) {
        uiSession.registerOwnshipSource(demoOwnshipSourceRegistration())
        uiSession.updateOwnshipSourceStatus(
            OwnshipSourceStatusUpdate(
                sourceId = "demo-gps",
                connectionState = SourceConnectionState.Connected,
                enabled = true,
                statusLabel = "Connected",
            ),
        )
        sessionSnapshot = uiSession.pushSituationSample(demoSituationSample())
    }
    LaunchedEffect(uiSession, sessionSnapshot.playbackUiState.status) {
        while (sessionSnapshot.playbackUiState.status == PlaybackStatus.Playing) {
            delay(250)
            runCatching { uiSession.tickPlayback(System.currentTimeMillis().toDouble()) }
                .onSuccess { sessionSnapshot = it }
                .onFailure { Log.e("AerobagPlayback", "tick failed", it) }
        }
    }
    val sessionPlanUiState = requireNotNull(sessionSnapshot.appUiState.activePlan) {
        "UiSessionSnapshot must provide active flight-plan UI state"
    }
    val navElement = sessionPlanUiState.guidance?.navElement

    LaunchedEffect(selectedMap.id) {
        mapViewport = preserveViewportForMap(mapViewport, selectedMap.mapView)
    }

    fun currentSnapshot(): AppViewSnapshot = AppViewSnapshot(
        page = page,
        selectedMapId = selectedMapId,
        mapViewport = mapViewport,
        selectedAirportId = selectedAirportId,
        selectedChartId = selectedChartId,
        selectedChartLabel = selectedChart?.label.orEmpty(),
        recentAirportIds = recentAirportIds,
        chartViewport = chartViewport,
        chartFolderOpen = chartFolderOpen,
    )

    fun restoreSnapshot(snapshot: AppViewSnapshot, history: List<AppViewSnapshot>) {
        if (snapshot.selectedAirportId.isNotBlank() || snapshot.selectedChartId.isNotBlank() || snapshot.recentAirportIds.isNotEmpty()) {
            sessionSnapshot =
                uiSession.restoreChartPageState(
                    recentAirportIds = snapshot.recentAirportIds,
                    selectedAirportId = snapshot.selectedAirportId.ifBlank { null },
                    selectedChartId = snapshot.selectedChartId.ifBlank { null },
                )
        }
        pageHistory = history
        page = snapshot.page
        selectedMapId = snapshot.selectedMapId
        mapViewport = snapshot.mapViewport
        chartViewport = snapshot.chartViewport
        chartFolderOpen = snapshot.chartFolderOpen
    }

    fun navigateToPage(nextPage: AppPage) {
        if (nextPage == page) {
            return
        }
        pageHistory = boundedHistory(pageHistory + currentSnapshot())
        page = nextPage
    }

    fun openChartsForAirport(airportId: String) {
        sessionSnapshot = uiSession.selectAirport(airportId)
        val airport = chartAirportById[airportId]
        restoreSnapshot(
            currentSnapshot().copy(
                page = AppPage.Charts,
                selectedAirportId = airportId,
                selectedChartId = airport?.charts?.firstOrNull()?.id.orEmpty(),
                selectedChartLabel = airport?.charts?.firstOrNull()?.label.orEmpty(),
                recentAirportIds = sessionSnapshot.chartPageState.recentAirportIds,
                chartViewport = null,
                chartFolderOpen = true,
            ),
            boundedHistory(pageHistory + currentSnapshot()),
        )
    }

    BackHandler(enabled = pageHistory.isNotEmpty()) {
        val previous = pageHistory.lastOrNull() ?: return@BackHandler
        restoreSnapshot(previous, pageHistory.dropLast(1))
    }

    CompositionLocalProvider(LocalAerobagUiTheme provides uiTheme) {
        Box(modifier = Modifier.fillMaxSize()) {
            when (page) {
            AppPage.Map -> {
                MapExplorerPage(
                    appCore = appCore,
                    page = page,
                    pageHistory = pageHistory,
                    uptimeLabel = uptimeLabel,
                    fixture = fixture,
                    uiSession = uiSession,
                    uiTheme = uiTheme,
                    ownship = appUiState.ownship.render,
                    playbackUiState = sessionSnapshot.playbackUiState,
                    playbackSourcePath = playbackSourcePath,
                    mapFollowUiState = sessionSnapshot.mapFollowUiState,
                    mapFollowTargetViewport = sessionSnapshot.mapFollowTargetViewport,
                    situationRingCandidates = situationRingCandidates,
                    selectedMapId = selectedMapId,
                    viewport = mapViewport,
                    onViewportChange = { mapViewport = it },
                    onSessionSnapshotChange = { sessionSnapshot = it },
                    onPlaybackSourcePathChange = { playbackSourcePath = it },
                    onSelectMapId = {
                        restoreSnapshot(
                            currentSnapshot().copy(
                                page = AppPage.Map,
                                selectedMapId = it,
                            ),
                            boundedHistory(pageHistory + currentSnapshot()),
                        )
                    },
                    onSelectPage = ::navigateToPage,
                    onOpenPlan = { navigateToPage(AppPage.Plan) },
                    navElement = navElement,
                    plan = currentPlan,
                    planUiState = sessionPlanUiState,
                )
            }
            AppPage.Plan -> {
                FlightPlanPage(
                    appCore = appCore,
                    page = page,
                    pageHistory = pageHistory,
                    uptimeLabel = uptimeLabel,
                    navElement = navElement,
                    samplePlan = currentPlan,
                    planUiState = sessionPlanUiState,
                    planListState = planListState,
                    uiTheme = uiTheme,
                    onSelectPage = ::navigateToPage,
                    onOpenPlan = { navigateToPage(AppPage.Plan) },
                    onOpenCharts = { airportId -> if (airportId != null) openChartsForAirport(airportId) },
                    onApplyMutation = { mutation ->
                        sessionSnapshot = uiSession.replaceFlightPlan(mutation.plan)
                    },
                )
            }
            AppPage.Charts -> {
                ChartsPage(
                    page = page,
                    pageHistory = pageHistory,
                    uptimeLabel = uptimeLabel,
                    airports = orderedChartAirports,
                    selectedAirport = selectedAirport,
                    selectedChart = selectedChart,
                    uiTheme = uiTheme,
                    ownship = appUiState.ownship.render,
                    navElement = navElement,
                    folderOpen = chartFolderOpen,
                    viewport = chartViewport,
                    onViewportChange = { chartViewport = it },
                    onFolderOpenChange = {
                        restoreSnapshot(
                            currentSnapshot().copy(
                                    page = AppPage.Charts,
                                    chartFolderOpen = it,
                            ),
                            boundedHistory(pageHistory + currentSnapshot()),
                        )
                    },
                    onSelectPage = ::navigateToPage,
                    onOpenPlan = { navigateToPage(AppPage.Plan) },
                    onSelectAirport = { airportId ->
                        sessionSnapshot = uiSession.selectAirport(airportId)
                        val airport = chartAirportById[airportId]
                        restoreSnapshot(
                            currentSnapshot().copy(
                                page = AppPage.Charts,
                                selectedAirportId = airportId,
                                selectedChartId = airport?.charts?.firstOrNull()?.id.orEmpty(),
                                selectedChartLabel = airport?.charts?.firstOrNull()?.label.orEmpty(),
                                recentAirportIds = sessionSnapshot.chartPageState.recentAirportIds,
                                chartViewport = null,
                                chartFolderOpen = false,
                            ),
                            boundedHistory(pageHistory + currentSnapshot()),
                        )
                    },
                    onSelectChart = {
                        sessionSnapshot = uiSession.selectChart(it)
                        restoreSnapshot(
                            currentSnapshot().copy(
                                page = AppPage.Charts,
                                selectedChartId = it,
                                selectedChartLabel = chartAirportById[sessionSnapshot.chartPageState.selectedAirportId]
                                    ?.charts
                                    ?.firstOrNull { chart -> chart.id == it }
                                    ?.label
                                    .orEmpty(),
                                chartViewport = null,
                                chartFolderOpen = false,
                            ),
                            boundedHistory(pageHistory + currentSnapshot()),
                        )
                    },
                )
            }
        }
    }
    }
}

@OptIn(ExperimentalComposeUiApi::class)
@Composable
private fun MapExplorerPage(
    appCore: NativeAppCoreAdapter,
    page: AppPage,
    pageHistory: List<AppViewSnapshot>,
    uptimeLabel: String,
    fixture: net.jonh.aerobag.prototype.domain.ContentFixture,
    uiSession: NativeUiSession,
    uiTheme: UiTheme,
    ownship: OwnshipRenderState,
    playbackUiState: PlaybackUiState,
    playbackSourcePath: String,
    mapFollowUiState: MapFollowUiState,
    mapFollowTargetViewport: CoreMapViewport?,
    situationRingCandidates: List<SituationRingCandidate>,
    selectedMapId: String,
    viewport: MapViewportState,
    onViewportChange: (MapViewportState) -> Unit,
    onSessionSnapshotChange: (UiSessionSnapshot) -> Unit,
    onPlaybackSourcePathChange: (String) -> Unit,
    onSelectMapId: (String) -> Unit,
    onSelectPage: (AppPage) -> Unit,
    onOpenPlan: () -> Unit,
    navElement: NavElementUiView?,
    plan: net.jonh.aerobag.prototype.domain.FlightPlan,
    planUiState: FlightPlanUiState?,
) {
    val context = LocalContext.current
    val activity = context as? MainActivity
    val density = LocalDensity.current
    val focusRequester = remember { FocusRequester() }
    var pageTrayOpen by remember { mutableStateOf(false) }
    var chartTrayOpen by remember { mutableStateOf(false) }
    var debugPanelOpen by remember { mutableStateOf(false) }
    var debugTileLabels by remember { mutableStateOf(false) }
    var surfaceSize by remember { mutableStateOf(IntSize.Zero) }
    var committedMapOverlay by remember(uiSession) {
        mutableStateOf(
            MapOverlayQueryResult(
                neededPointTiles = emptyList(),
                visibleFeatures = emptyList(),
                warnings = emptyList(),
            ),
        )
    }
    var committedOverlayViewport by remember(uiSession) { mutableStateOf<MapViewportState?>(null) }
    var committedOverlaySurfaceUnits by remember(uiSession) { mutableStateOf<OverlaySurfaceUnits?>(null) }
    var mapOverlayError by remember(uiSession) { mutableStateOf<String?>(null) }
    var flightPlanRoute by remember(plan.id, plan.version) { mutableStateOf<List<FlightPlanRouteSegment>>(emptyList()) }
    var guidanceGeometryKey by remember(uiSession) { mutableStateOf<String?>(null) }
    var mapGestureActive by remember { mutableStateOf(false) }
    var installingPackage by remember { mutableStateOf<String?>(null) }
    var installRevision by remember { mutableStateOf(0) }
    val selectedMap = remember(selectedMapId, fixture.mapViews) {
        fixture.mapViews.find { it.id == selectedMapId } ?: fixture.mapViews.first()
    }
    val selectedFamilyMapViews = remember(selectedMap, fixture.mapViews) {
        fixture.mapViews.filter { it.mapView.chartFamily == selectedMap.mapView.chartFamily }
    }
    val viewportState = rememberUpdatedState(viewport)
    val center = remember(viewport) { viewportCenterLatLon(viewport) }
    val surfaceWidthUnits = remember(surfaceSize, density) { with(density) { surfaceSize.width.toDp().value } }
    val surfaceHeightUnits = remember(surfaceSize, density) { with(density) { surfaceSize.height.toDp().value } }
    val tiles = remember(viewport, surfaceSize, selectedFamilyMapViews) {
        if (surfaceSize.width == 0 || surfaceSize.height == 0) {
            emptyList()
        } else {
            renderTiles(
                mapViews = selectedFamilyMapViews.map { it.id to it.mapView },
                viewport = viewport,
                widthPx = surfaceWidthUnits,
                heightPx = surfaceHeightUnits,
            )
        }
    }
    val selectedPackageName = selectedMap.mapView.packageName
    val topLeftTrayOpen = pageTrayOpen || chartTrayOpen
    val selectedPackageInstalled = remember(selectedPackageName, installRevision) {
        selectedPackageName?.let { SectionalPackages.isInstalled(context, it) } ?: true
    }
    val familyPackageNames = remember(selectedFamilyMapViews) {
        selectedFamilyMapViews.mapNotNull { it.mapView.packageName }.distinct()
    }
    val installedFamilyPackageCount = remember(familyPackageNames, installRevision) {
        familyPackageNames.count { SectionalPackages.isInstalled(context, it) }
    }
    val sourceZooms = tiles.map { it.zoom }.distinct().sorted()
    val renderedPackages = tiles.mapNotNull { it.mapView.packageName }.distinct().sorted()
    val familyStatus = remember(installingPackage, installedFamilyPackageCount, familyPackageNames, selectedMap.mapView.chartFamily) {
        when {
            installingPackage != null -> "Installing ${installingPackage}..."
            installedFamilyPackageCount == familyPackageNames.size -> "Local ${selectedMap.mapView.chartFamily.name}"
            installedFamilyPackageCount > 0 -> "Partial ${selectedMap.mapView.chartFamily.name}"
            else -> "Package missing"
        }
    }
    val trayOptions = remember(selectedMap.id, fixture.mapViews) {
        val secTarget = fixture.mapViews.firstOrNull { it.mapView.chartFamily == MapChartFamily.Sec }
        val tacTarget = fixture.mapViews.firstOrNull { it.mapView.chartFamily == MapChartFamily.Tac }
        val enrLTarget = fixture.mapViews.firstOrNull { it.mapView.chartFamily == MapChartFamily.EnrL }
        val enrHTarget = fixture.mapViews.firstOrNull { it.mapView.chartFamily == MapChartFamily.EnrH }
        listOf(
            ChartTrayOption("sec", "SECTIONAL", "SEC", secTarget != null) { secTarget?.let { onSelectMapId(it.id) } },
            ChartTrayOption("tac", "TAC", "TAC", tacTarget != null) { tacTarget?.let { onSelectMapId(it.id) } },
            ChartTrayOption("enr-l", "IFR-LOW", "IFR L", enrLTarget != null) { enrLTarget?.let { onSelectMapId(it.id) } },
            ChartTrayOption("enr-h", "IFR-HIGH", "IFR H", enrHTarget != null) { enrHTarget?.let { onSelectMapId(it.id) } },
        )
    }
    val selectedLauncher = trayOptions.firstOrNull { option ->
        when (option.id) {
            "sec" -> selectedMap.mapView.chartFamily == MapChartFamily.Sec
            "tac" -> selectedMap.mapView.chartFamily == MapChartFamily.Tac
            "enr-l" -> selectedMap.mapView.chartFamily == MapChartFamily.EnrL
            "enr-h" -> selectedMap.mapView.chartFamily == MapChartFamily.EnrH
            else -> false
        }
    } ?: trayOptions.first()
    val tileRects = remember(tiles, density) {
        tiles.associate { tile ->
            val leftPx = with(density) { tile.leftPx.dp.roundToPx() }
            val topPx = with(density) { tile.topPx.dp.roundToPx() }
            val rightPx = with(density) { (tile.leftPx + tile.sizePx).dp.roundToPx() }
            val bottomPx = with(density) { (tile.topPx + tile.sizePx).dp.roundToPx() }
            Triple(tile.zoom, tile.x, tile.yTms) to TileRect(
                leftPx = leftPx,
                topPx = topPx,
                widthPx = rightPx - leftPx,
                heightPx = bottomPx - topPx,
            )
        }
    }
    val situationOverlay = remember(ownship, viewport, surfaceWidthUnits, surfaceHeightUnits) {
        resolveSituationOverlay(
            ownship = ownship,
            viewport = viewport,
            widthUnits = surfaceWidthUnits,
            heightUnits = surfaceHeightUnits,
            ringCandidates = situationRingCandidates,
        )
    }
    val routeScreenSegments = remember(flightPlanRoute, viewport, surfaceWidthUnits, surfaceHeightUnits) {
        if (surfaceWidthUnits <= 0f || surfaceHeightUnits <= 0f) {
            emptyList()
        } else {
            flightPlanRoute.map { segment ->
                Pair(
                    (segment.path.ifEmpty { listOf(segment.from, segment.to) }).map { point ->
                        latLonToScreenPoint(viewport, point, surfaceWidthUnits, surfaceHeightUnits)
                    },
                    segment,
                )
            }
        }
    }

    fun syncFollowStateForViewport(nextViewport: MapViewportState) {
        if (!mapFollowUiState.following || surfaceWidthUnits <= 0f || surfaceHeightUnits <= 0f) {
            return
        }
        val overlay = resolveSituationOverlay(
            ownship = ownship,
            viewport = nextViewport,
            widthUnits = surfaceWidthUnits,
            heightUnits = surfaceHeightUnits,
            ringCandidates = situationRingCandidates,
        )
        if (overlay == null) {
            runCatching { uiSession.disengageMapFollow(nextViewport) }.onSuccess(onSessionSnapshotChange)
            return
        }
        val point = overlay.pointUnits
        if (point.x < 0f || point.x > surfaceWidthUnits || point.y < 0f || point.y > surfaceHeightUnits) {
            runCatching { uiSession.disengageMapFollow(nextViewport) }.onSuccess(onSessionSnapshotChange)
            return
        }
        runCatching {
            uiSession.setMapFollowOffset(
                nextViewport,
                (point.x - surfaceWidthUnits / 2f).toDouble(),
                (point.y - surfaceHeightUnits / 2f).toDouble(),
            )
        }.onSuccess(onSessionSnapshotChange)
    }

    fun updateViewport(nextViewport: MapViewportState, syncFollow: Boolean = true) {
        onViewportChange(nextViewport)
        if (syncFollow) {
            syncFollowStateForViewport(nextViewport)
        }
    }

    val aircraftDrawable = remember(context) { AppCompatResources.getDrawable(context, R.drawable.plan_view_icon)?.mutate() }
    val outlinePaint = remember {
        Paint().apply {
            isAntiAlias = true
            color = android.graphics.Color.argb(102, 0, 0, 0)
            style = Paint.Style.STROKE
            strokeCap = Paint.Cap.ROUND
            strokeJoin = Paint.Join.ROUND
        }
    }
    val fillPaint = remember {
        Paint().apply {
            isAntiAlias = true
            color = android.graphics.Color.WHITE
            style = Paint.Style.FILL
        }
    }
    val labelStrokePaint = remember {
        Paint().apply {
            isAntiAlias = true
            color = android.graphics.Color.argb(102, 0, 0, 0)
            style = Paint.Style.STROKE
            strokeJoin = Paint.Join.ROUND
            strokeWidth = 5f
            textAlign = Paint.Align.CENTER
            textSize = 16f
            typeface = android.graphics.Typeface.create(android.graphics.Typeface.DEFAULT_BOLD, android.graphics.Typeface.BOLD)
        }
    }
    val labelFillPaint = remember {
        Paint().apply {
            isAntiAlias = true
            color = android.graphics.Color.WHITE
            style = Paint.Style.FILL
            textAlign = Paint.Align.CENTER
            textSize = 16f
            typeface = android.graphics.Typeface.create(android.graphics.Typeface.DEFAULT_BOLD, android.graphics.Typeface.BOLD)
        }
    }
    val tileBitmaps = remember(tiles, selectedMap.id, installRevision) {
        tiles.associate { tile ->
            Triple(tile.zoom, tile.x, tile.yTms) to runCatching {
                val bytes = SectionalPackages.loadTileBytes(context, tile) ?: return@runCatching null
                val bitmap = BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
                bitmap?.asImageBitmap()
            }.getOrNull()
        }
    }
    val tileLabelPaint = remember {
        Paint().apply {
            isAntiAlias = true
            color = android.graphics.Color.WHITE
            textSize = 24f
            typeface = android.graphics.Typeface.create(android.graphics.Typeface.MONOSPACE, android.graphics.Typeface.BOLD)
        }
    }
    val tileLabelBackgroundPaint = remember {
        Paint().apply {
            isAntiAlias = true
            color = android.graphics.Color.argb(224, 14, 22, 28)
        }
    }
    val fixMarkerStrokeColor = Color(0xB3081218)
    val fixMarkerFillColor = Color(0xFF39D9FF)
    val airportMarkerStrokeColor = Color(0xB3081218)
    val airportToweredFillColor = Color(0xFF4AA3FF)
    val airportUntoweredFillColor = Color(0xFFFF4FD8)
    val vorMarkerColor = Color(0xFF4AA3FF)
    val vorMarkerStrokeColor = Color(0xD1081218)
    val fixLabelStrokePaint = remember {
        Paint().apply {
            isAntiAlias = true
            color = android.graphics.Color.argb(179, 8, 18, 24)
            style = Paint.Style.STROKE
            strokeJoin = Paint.Join.ROUND
            strokeWidth = 4f
            textAlign = Paint.Align.CENTER
            textSize = 14f
            typeface = android.graphics.Typeface.create(android.graphics.Typeface.DEFAULT_BOLD, android.graphics.Typeface.BOLD)
        }
    }
    val airportLabelStrokePaint = remember {
        Paint().apply {
            isAntiAlias = true
            color = android.graphics.Color.argb(179, 8, 18, 24)
            style = Paint.Style.STROKE
            strokeJoin = Paint.Join.ROUND
            strokeWidth = 3f
            textAlign = Paint.Align.CENTER
            textSize = 14f
            typeface = Typeface.create(Typeface.DEFAULT_BOLD, Typeface.BOLD)
        }
    }
    val vorLabelFillPaint = remember {
        Paint().apply {
            isAntiAlias = true
            color = android.graphics.Color.WHITE
            style = Paint.Style.FILL
            textAlign = Paint.Align.CENTER
            textSize = 14f
            typeface = Typeface.create(Typeface.DEFAULT_BOLD, Typeface.BOLD)
        }
    }
    val fixLabelFillPaint = remember {
        Paint().apply {
            isAntiAlias = true
            color = android.graphics.Color.WHITE
            style = Paint.Style.FILL
            textAlign = Paint.Align.CENTER
            textSize = 14f
            typeface = Typeface.create(Typeface.DEFAULT_BOLD, Typeface.BOLD)
        }
    }
    val airportToweredLabelFillPaint = remember {
        Paint().apply {
            isAntiAlias = true
            color = android.graphics.Color.WHITE
            style = Paint.Style.FILL
            textAlign = Paint.Align.CENTER
            textSize = 14f
            typeface = Typeface.create(Typeface.DEFAULT_BOLD, Typeface.BOLD)
        }
    }
    val airportUntoweredLabelFillPaint = remember {
        Paint().apply {
            isAntiAlias = true
            color = android.graphics.Color.WHITE
            style = Paint.Style.FILL
            textAlign = Paint.Align.CENTER
            textSize = 14f
            typeface = Typeface.create(Typeface.DEFAULT_BOLD, Typeface.BOLD)
        }
    }

    LaunchedEffect(selectedMap.id) { chartTrayOpen = false }
    LaunchedEffect(appCore, uiSession, plan.id, plan.version, plan.guidance, plan.resolvedLegs) {
        if (plan.resolvedLegs.isEmpty()) {
            flightPlanRoute = emptyList()
            if (guidanceGeometryKey != "") {
                guidanceGeometryKey = ""
                runCatching { uiSession.setGuidanceLegGeometry(emptyList()) }
                    .onSuccess(onSessionSnapshotChange)
                    .onFailure { Log.e("AerobagGuidance", "failed to clear guidance geometry", it) }
            }
            return@LaunchedEffect
        }
        runCatching {
            appCore.projectFlightPlanRoute(plan)
        }.onSuccess {
            flightPlanRoute = it
            val nextKey =
                it.joinToString("|") { segment ->
                    "${segment.id}:${segment.from.lat},${segment.from.lon}:${segment.to.lat},${segment.to.lon}:${segment.path.size}"
                }
            if (nextKey != guidanceGeometryKey) {
                guidanceGeometryKey = nextKey
                runCatching {
                    uiSession.setGuidanceLegGeometry(
                        it.map { segment ->
                            GuidanceLegGeometry(
                                legId = segment.id,
                                from = segment.from,
                                to = segment.to,
                                path = segment.path,
                            )
                        },
                    )
                }.onSuccess(onSessionSnapshotChange)
                    .onFailure { error -> Log.e("AerobagGuidance", "failed to set guidance geometry", error) }
            }
        }.onFailure {
            flightPlanRoute = emptyList()
            Log.e("AerobagGuidance", "failed to project flight plan route", it)
        }
    }
    LaunchedEffect(selectedMap.id, pageTrayOpen, chartTrayOpen) {
        if (!pageTrayOpen && !chartTrayOpen) {
            withFrameNanos { }
            focusRequester.requestFocus()
        }
    }
    LaunchedEffect(uiSession, mapFollowUiState.following, mapFollowTargetViewport, viewport) {
        if (mapFollowUiState.following && mapFollowTargetViewport == null) {
            runCatching { uiSession.engageMapFollow(viewport) }.onSuccess(onSessionSnapshotChange)
        }
    }
    LaunchedEffect(mapFollowUiState.following, mapFollowTargetViewport, mapGestureActive) {
        if (!mapFollowUiState.following) {
            return@LaunchedEffect
        }
        if (mapGestureActive) {
            return@LaunchedEffect
        }
        val target = mapFollowTargetViewport ?: return@LaunchedEffect
        val nextViewport = mapViewportFromCore(target)
        if (!sameMapViewport(nextViewport, viewport)) {
            onViewportChange(nextViewport)
        }
    }
    LaunchedEffect(uiSession, viewport, surfaceSize) {
        if (surfaceSize.width <= 0 || surfaceSize.height <= 0) {
            mapOverlayError = null
            return@LaunchedEffect
        }
        runCatching {
            val firstPass = uiSession.queryMapOverlay(viewport, surfaceWidthUnits.toDouble(), surfaceHeightUnits.toDouble())
            val payloads = if (firstPass.neededPointTiles.isNotEmpty()) {
                VectorTileAssets.loadPointTiles(context.applicationContext, firstPass.neededPointTiles)
            } else {
                emptyList()
            }
            if (payloads.isNotEmpty()) {
                uiSession.ingestPointTiles(payloads)
                uiSession.queryMapOverlay(viewport, surfaceWidthUnits.toDouble(), surfaceHeightUnits.toDouble())
            } else {
                firstPass
            }
        }.onSuccess { overlay ->
            committedMapOverlay = overlay
            committedOverlayViewport = viewport
            committedOverlaySurfaceUnits = OverlaySurfaceUnits(surfaceWidthUnits, surfaceHeightUnits)
            mapOverlayError = null
        }.onFailure { error ->
            if (error is CancellationException) {
                mapOverlayError = null
            } else {
                mapOverlayError = error.message ?: error::class.java.simpleName
            }
        }
    }
    val displayedOverlayFeatures = remember(
        committedMapOverlay,
        committedOverlayViewport,
        committedOverlaySurfaceUnits,
        viewport,
        surfaceWidthUnits,
        surfaceHeightUnits,
    ) {
        val baseViewport = committedOverlayViewport
        val baseSurface = committedOverlaySurfaceUnits
        if (baseViewport == null || baseSurface == null || baseSurface.width <= 0f || baseSurface.height <= 0f || surfaceWidthUnits <= 0f || surfaceHeightUnits <= 0f) {
            committedMapOverlay.visibleFeatures
        } else {
            committedMapOverlay.visibleFeatures.map { feature ->
                transformVisibleFeature(
                    feature = feature,
                    fromViewport = baseViewport,
                    fromSurface = baseSurface,
                    toViewport = viewport,
                    toSurface = OverlaySurfaceUnits(surfaceWidthUnits, surfaceHeightUnits),
                )
            }
        }
    }
    val hardwareZoomState = rememberUpdatedState(
        HardwareZoomState(
            selectedMap = selectedMap.mapView,
            viewport = viewport,
            widthUnits = surfaceWidthUnits,
            heightUnits = surfaceHeightUnits,
            pageTrayOpen = pageTrayOpen,
            chartTrayOpen = chartTrayOpen,
            updateViewport = { nextViewport -> updateViewport(nextViewport) },
        ),
    )
    DisposableEffect(activity) {
        if (activity != null) {
            activity.onHardwareZoomDelta = { delta ->
                val state = hardwareZoomState.value
                if (state.widthUnits == 0f || state.heightUnits == 0f || state.pageTrayOpen || state.chartTrayOpen) {
                    false
                } else {
                    state.updateViewport(
                        zoomAroundPoint(
                            viewport = state.viewport,
                            mapView = state.selectedMap,
                            anchor = ScreenPoint(state.widthUnits / 2f, state.heightUnits / 2f),
                            widthPx = state.widthUnits,
                            heightPx = state.heightUnits,
                            nextZoom = clampZoom(state.viewport.zoom + delta, state.selectedMap),
                        ),
                    )
                    true
                }
            }
        }
        onDispose {
            if (activity != null && activity.onHardwareZoomDelta != null) {
                activity.onHardwareZoomDelta = null
            }
        }
    }

    LaunchedEffect(selectedMap.id, selectedPackageName, selectedPackageInstalled) {
        if (selectedMap.mapView.storageKind != TileStorageKind.SectionalPackage) {
            return@LaunchedEffect
        }
        val packageName = selectedPackageName
        if (packageName == null || selectedPackageInstalled) {
            return@LaunchedEffect
        }
        installingPackage = packageName
        withContext(Dispatchers.IO) {
            SectionalPackages.install(context, packageName)
        }
        installRevision += 1
        installingPackage = null
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(uiTheme.controls.chartSurfaceBg)
            .onSizeChanged { surfaceSize = it }
            .focusRequester(focusRequester)
            .onPreviewKeyEvent { keyEvent ->
                if (keyEvent.nativeKeyEvent.action != AndroidKeyEvent.ACTION_DOWN ||
                    surfaceWidthUnits == 0f ||
                    surfaceHeightUnits == 0f
                ) {
                    return@onPreviewKeyEvent false
                }
                val delta = when (keyEvent.nativeKeyEvent.keyCode) {
                    AndroidKeyEvent.KEYCODE_EQUALS,
                    AndroidKeyEvent.KEYCODE_PLUS,
                    AndroidKeyEvent.KEYCODE_NUMPAD_ADD,
                    -> 0.35
                    AndroidKeyEvent.KEYCODE_MINUS,
                    AndroidKeyEvent.KEYCODE_NUMPAD_SUBTRACT,
                    -> -0.35
                    else -> return@onPreviewKeyEvent false
                }
                updateViewport(
                    zoomAroundPoint(
                        viewport = viewportState.value,
                        mapView = selectedMap.mapView,
                        anchor = ScreenPoint(surfaceWidthUnits / 2f, surfaceHeightUnits / 2f),
                        widthPx = surfaceWidthUnits,
                        heightPx = surfaceHeightUnits,
                        nextZoom = clampZoom(viewportState.value.zoom + delta, selectedMap.mapView),
                    ),
                )
                true
            }
            .focusable()
            .pointerInput(selectedMap.mapView, surfaceSize) {
                if (surfaceWidthUnits == 0f || surfaceHeightUnits == 0f) {
                    return@pointerInput
                }
                awaitEachGesture {
                    var dragPointerId: PointerId? = null
                    var dragLastPosition: Offset? = null
                    var pinchSnapshot: net.jonh.aerobag.prototype.domain.PinchSnapshot? = null
                    var gestureViewport = viewportState.value
                    var movedViewportDuringGesture = false
                    try {
                        while (true) {
                            val event = awaitPointerEvent()
                            val pressed = event.changes.filter { it.pressed && !it.isConsumed }
                            if (pressed.isEmpty()) break
                            if (topLeftTrayOpen) {
                                pressed.forEach { it.consume() }
                                continue
                            }
                            mapGestureActive = true
                            if (pressed.size == 1) {
                                val change = pressed.first()
                                if (dragPointerId != change.id || dragLastPosition == null) {
                                    dragPointerId = change.id
                                    dragLastPosition = change.position
                                    pinchSnapshot = null
                                } else {
                                    val last = dragLastPosition ?: change.position
                                    gestureViewport = dragViewport(
                                        gestureViewport,
                                        dx = with(density) { (change.position.x - last.x).toDp().value },
                                        dy = with(density) { (change.position.y - last.y).toDp().value },
                                    )
                                    movedViewportDuringGesture = true
                                    updateViewport(gestureViewport)
                                    dragLastPosition = change.position
                                }
                                change.consume()
                            } else {
                                val first = pressed[0]
                                val second = pressed[1]
                                if (pinchSnapshot == null) {
                                    pinchSnapshot = createPinchSnapshot(
                                        viewport = gestureViewport,
                                        first = ScreenPoint(with(density) { first.position.x.toDp().value }, with(density) { first.position.y.toDp().value }),
                                        second = ScreenPoint(with(density) { second.position.x.toDp().value }, with(density) { second.position.y.toDp().value }),
                                        widthPx = surfaceWidthUnits,
                                        heightPx = surfaceHeightUnits,
                                    )
                                }
                                gestureViewport =
                                    applyPinchGesture(
                                        snapshot = pinchSnapshot,
                                        currentFirst = ScreenPoint(with(density) { first.position.x.toDp().value }, with(density) { first.position.y.toDp().value }),
                                        currentSecond = ScreenPoint(with(density) { second.position.x.toDp().value }, with(density) { second.position.y.toDp().value }),
                                        mapView = selectedMap.mapView,
                                        widthPx = surfaceWidthUnits,
                                        heightPx = surfaceHeightUnits,
                                    )
                                movedViewportDuringGesture = true
                                updateViewport(gestureViewport)
                                first.consume()
                                second.consume()
                            }
                        }
                    } finally {
                        if (movedViewportDuringGesture) {
                            syncFollowStateForViewport(gestureViewport)
                        }
                        mapGestureActive = false
                    }
                }
            }
            .pointerInteropFilter { event ->
                if (surfaceWidthUnits == 0f || surfaceHeightUnits == 0f) {
                    return@pointerInteropFilter false
                }
                if (event.action == MotionEvent.ACTION_SCROLL) {
                    val wheelDelta = event.getAxisValue(MotionEvent.AXIS_VSCROLL).takeIf { it != 0f }
                        ?: event.getAxisValue(MotionEvent.AXIS_SCROLL)
                    updateViewport(
                        zoomAroundPoint(
                            viewport = viewportState.value,
                            mapView = selectedMap.mapView,
                            anchor = ScreenPoint(surfaceWidthUnits / 2f, surfaceHeightUnits / 2f),
                            widthPx = surfaceWidthUnits,
                            heightPx = surfaceHeightUnits,
                            nextZoom = clampZoom(viewportState.value.zoom - wheelDelta * 0.28, selectedMap.mapView),
                        ),
                    )
                    true
                } else {
                    false
                }
            },
    ) {
        Canvas(modifier = Modifier.fillMaxSize()) {
            tiles.forEach { tile ->
                val tileRect = tileRects.getValue(Triple(tile.zoom, tile.x, tile.yTms))
                val bitmap = tileBitmaps.getValue(Triple(tile.zoom, tile.x, tile.yTms))
                if (bitmap != null) {
                    drawImage(
                        image = bitmap,
                        dstOffset = IntOffset(tileRect.leftPx, tileRect.topPx),
                        dstSize = IntSize(tileRect.widthPx, tileRect.heightPx),
                    )
                }
                if (debugTileLabels) {
                    val label = "z${tile.zoom} x${tile.x} y${tile.yTms}"
                    val rectLeft = tileRect.leftPx + 6f
                    val rectTop = tileRect.topPx + 6f
                    val textWidth = tileLabelPaint.measureText(label)
                    drawContext.canvas.nativeCanvas.apply {
                        drawRoundRect(
                            rectLeft,
                            rectTop,
                            rectLeft + textWidth + 16f,
                            rectTop + 30f,
                            8f,
                            8f,
                            tileLabelBackgroundPaint,
                        )
                        drawText(label, rectLeft + 8f, tileRect.topPx + 30f, tileLabelPaint)
                    }
                }
            }
        }
        if (routeScreenSegments.isNotEmpty()) {
            Canvas(modifier = Modifier.fillMaxSize()) {
                val densityScale = density.density
                routeScreenSegments.forEach { (path, segment) ->
                    path.zipWithNext().forEach { (from, to) ->
                        drawLine(
                            color = Color(0x8C000000),
                            start = Offset(from.x * densityScale, from.y * densityScale),
                            end = Offset(to.x * densityScale, to.y * densityScale),
                            strokeWidth = 7f * densityScale,
                            cap = StrokeCap.Round,
                        )
                        drawLine(
                            color = routeSegmentColor(segment.status),
                            start = Offset(from.x * densityScale, from.y * densityScale),
                            end = Offset(to.x * densityScale, to.y * densityScale),
                            strokeWidth = 3.5f * densityScale,
                            cap = StrokeCap.Round,
                        )
                    }
                }
            }
        }
        if (displayedOverlayFeatures.isNotEmpty()) {
            Canvas(modifier = Modifier.fillMaxSize()) {
                val densityScale = density.density
                fixLabelStrokePaint.textSize = 14f * densityScale
                fixLabelStrokePaint.strokeWidth = 3f * densityScale
                airportLabelStrokePaint.textSize = 14f * densityScale
                airportLabelStrokePaint.strokeWidth = 3f * densityScale
                fixLabelFillPaint.textSize = 14f * densityScale
                airportToweredLabelFillPaint.textSize = 14f * densityScale
                airportUntoweredLabelFillPaint.textSize = 14f * densityScale
                vorLabelFillPaint.textSize = 14f * densityScale
                displayedOverlayFeatures.forEach { feature ->
                    val center = Offset(feature.screenX.toFloat() * densityScale, feature.screenY.toFloat() * densityScale)
                    val isAirport = feature.styleClass == "airport" || feature.kind.equals("airport", ignoreCase = true)
                    val isVor = feature.styleClass == "nav" || feature.kind.lowercase().contains("vor")
                    if (isAirport) {
                        val airportFillColor = if (feature.towered) airportToweredFillColor else airportUntoweredFillColor
                        val airportLabelPaint = if (feature.towered) airportToweredLabelFillPaint else airportUntoweredLabelFillPaint
                        val airportRadius = 12f * densityScale
                        if (feature.fuelAvailable) {
                            val markerPath = airportFuelMarkerPath(center, densityScale)
                            drawPath(markerPath, airportFillColor)
                            drawPath(markerPath, airportMarkerStrokeColor, style = Stroke(width = 2f * densityScale))
                        } else {
                            drawCircle(airportFillColor, radius = airportRadius, center = center)
                            drawCircle(airportMarkerStrokeColor, radius = airportRadius, center = center, style = Stroke(width = 2f * densityScale))
                        }
                        feature.longestRunwayHeadingTrueDeg?.let { headingDeg ->
                            val headingRad = Math.toRadians(headingDeg)
                            val runwayHalfLength = (8f * feature.runwayLengthRatio.toFloat().coerceIn(0f, 1f)).coerceAtLeast(1.6f) * densityScale
                            val dx = kotlin.math.sin(headingRad).toFloat() * runwayHalfLength
                            val dy = (-kotlin.math.cos(headingRad)).toFloat() * runwayHalfLength
                            drawLine(
                                color = airportMarkerStrokeColor,
                                start = Offset(center.x - dx, center.y - dy),
                                end = Offset(center.x + dx, center.y + dy),
                                strokeWidth = 5f * densityScale,
                                cap = StrokeCap.Round,
                            )
                            drawLine(
                                color = Color.White,
                                start = Offset(center.x - dx, center.y - dy),
                                end = Offset(center.x + dx, center.y + dy),
                                strokeWidth = 3f * densityScale,
                                cap = StrokeCap.Round,
                            )
                        }
                        drawContext.canvas.nativeCanvas.apply {
                            val textX = center.x
                            val textY = center.y - 24f * densityScale
                            drawText(feature.label, textX, textY, airportLabelStrokePaint)
                            drawText(feature.label, textX, textY, airportLabelPaint)
                        }
                    } else if (isVor) {
                        val radius = 8f * densityScale
                        val outerHex = polygonPath(vorHexPoints(center, radius))
                        val band = vorBandPath(center, radius)
                        drawPath(band, vorMarkerColor)
                        drawPath(band, vorMarkerStrokeColor, style = Stroke(width = 1.6f * densityScale))
                        drawPath(outerHex, vorMarkerStrokeColor, style = Stroke(width = 1.6f * densityScale))
                        drawContext.canvas.nativeCanvas.apply {
                            val textY = center.y - 24f * densityScale
                            drawText(feature.label, center.x, textY, fixLabelStrokePaint)
                            drawText(feature.label, center.x, textY, vorLabelFillPaint)
                        }
                    } else {
                        val triangle = fixTrianglePath(center, 8f * densityScale)
                        drawPath(triangle, fixMarkerFillColor)
                        drawPath(triangle, fixMarkerStrokeColor, style = Stroke(width = 2.5f * densityScale))
                        drawContext.canvas.nativeCanvas.apply {
                            val textY = center.y - 15f * densityScale
                            drawText(feature.label, center.x, textY, fixLabelStrokePaint)
                            drawText(feature.label, center.x, textY, fixLabelFillPaint)
                        }
                    }
                }
            }
        }
        if (situationOverlay != null) {
            Canvas(modifier = Modifier.fillMaxSize()) {
                val densityScale = density.density
                val center = Offset(situationOverlay.pointUnits.x * densityScale, situationOverlay.pointUnits.y * densityScale)
                val ringRadius = situationOverlay.ring.radiusUnits * densityScale
                drawCircle(
                    color = Color(0x66000000),
                    radius = ringRadius,
                    center = center,
                    style = Stroke(width = 16f),
                )
                drawCircle(
                    color = Color.White,
                    radius = ringRadius,
                    center = center,
                    style = Stroke(width = 6f),
                )
                situationOverlay.ring.tickMarks.forEach { tick ->
                    val inner = Offset(tick.innerUnits.x * densityScale, tick.innerUnits.y * densityScale)
                    val outer = Offset(tick.outerUnits.x * densityScale, tick.outerUnits.y * densityScale)
                    drawLine(Color(0x66000000), inner, outer, strokeWidth = 8f)
                    drawLine(Color.White, inner, outer, strokeWidth = 6f)
                }
                drawCircle(
                    color = Color.White,
                    radius = ringRadius,
                    center = center,
                    style = Stroke(width = 6f),
                )
                if (situationOverlay.predictorUnits != null) {
                    val predictor = Offset(
                        situationOverlay.predictorUnits.x * densityScale,
                        situationOverlay.predictorUnits.y * densityScale,
                    )
                    val shaftEnd = arrowShaftEndPoint(center, predictor)
                    drawLine(Color(0x66000000), center, shaftEnd, strokeWidth = 8f)
                    drawLine(Color.White, center, shaftEnd, strokeWidth = 6f)
                    val arrow = arrowHeadPath(center, predictor)
                    drawPath(arrow, Color.White)
                    drawPath(arrow, Color(0x66000000), style = Stroke(width = 1.5f))
                }
                drawContext.canvas.nativeCanvas.apply {
                    val labelPoint = Offset(
                        situationOverlay.ring.labelPointUnits.x * densityScale,
                        situationOverlay.ring.labelPointUnits.y * densityScale,
                    )
                    save()
                    rotate(situationOverlay.ring.labelRotationDeg, labelPoint.x, labelPoint.y)
                    labelStrokePaint.textSize = 16f * densityScale
                    labelFillPaint.textSize = 16f * densityScale
                    drawText(situationOverlay.ring.labelText, labelPoint.x, labelPoint.y + labelFillPaint.textSize * 0.33f, labelStrokePaint)
                    drawText(situationOverlay.ring.labelText, labelPoint.x, labelPoint.y + labelFillPaint.textSize * 0.33f, labelFillPaint)
                    restore()
                    val iconSizePx = ThumbSize.toPx() * 0.72f
                    val left = (center.x - iconSizePx / 2f).roundToInt()
                    val top = (center.y - iconSizePx / 2f).roundToInt()
                    val drawable = aircraftDrawable
                    if (drawable != null) {
                        save()
                        rotate(situationOverlay.headingDeg, center.x, center.y)
                        drawable.setBounds(left, top, (left + iconSizePx).roundToInt(), (top + iconSizePx).roundToInt())
                        drawable.draw(this)
                        restore()
                    }
                }
            }
        }
        SituationStatusBadge(
            ownship = ownship,
            modifier = Modifier
                .align(Alignment.TopEnd)
                .padding(top = ThumbGap, end = ThumbGap),
        )

        MapTopLeftControls(
            modifier = Modifier.align(Alignment.TopStart),
            currentPage = page,
            pageTrayOpen = pageTrayOpen,
            onTogglePageTray = {
                pageTrayOpen = !pageTrayOpen
                chartTrayOpen = false
            },
            onSelectPage = {
                onSelectPage(it)
                pageTrayOpen = false
                chartTrayOpen = false
            },
            selectedLabel = selectedLauncher.launcherLabel,
            trayOptions = trayOptions,
            trayOpen = chartTrayOpen,
            onToggle = {
                chartTrayOpen = !chartTrayOpen
                pageTrayOpen = false
            },
        )

        val playbackLeftRoomUnits = surfaceWidthUnits / 2f - (ThumbSize.value * 1.5f) - (ThumbGap.value * 2f)
        val playbackBottomPadding =
            if (playbackLeftRoomUnits < ThumbSize.value * 2.8f) {
                ThumbGap + (ThumbSize * 0.67f) + ThumbGap
            } else {
                ThumbGap
            }
        PlaybackWidget(
            uiSession = uiSession,
            playbackUiState = playbackUiState,
            sourcePath = playbackSourcePath,
            onSourcePathChange = onPlaybackSourcePathChange,
            onSnapshotChange = onSessionSnapshotChange,
            modifier = Modifier
                .align(Alignment.BottomStart)
                .padding(start = ThumbGap, bottom = playbackBottomPadding),
        )

        CompactSquareButton(
            label = "CTR",
            modifier = Modifier
                .align(Alignment.BottomEnd)
                .padding(end = ThumbGap, bottom = ThumbGap)
                .size(ThumbSize),
            enabled = mapFollowUiState.canCenterHere,
            selected = mapFollowUiState.following,
            selectedColor = Color(0xFF0D6F67),
            onClick = {
                runCatching { uiSession.engageMapFollow(viewport) }.onSuccess(onSessionSnapshotChange)
            },
        )

        if (topLeftTrayOpen) {
            Scrim {
                pageTrayOpen = false
                chartTrayOpen = false
            }
        }

        NavElementDock(
            navElement = navElement,
            onClick = onOpenPlan,
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .padding(bottom = ThumbGap),
        )

        DebugDock(
            open = debugPanelOpen,
            onToggle = { debugPanelOpen = !debugPanelOpen },
            highlight = committedMapOverlay.warnings.isNotEmpty() || mapOverlayError != null,
            expandAbove = true,
            modifier = Modifier
                .align(Alignment.BottomEnd)
                .padding(end = ThumbSize + (ThumbGap * 2f)),
        ) {
            Text("up: $uptimeLabel", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
            Text("${String.format("%.3f", center.first)}/${String.format("%.3f", center.second)} z${String.format("%.2f", viewport.zoom)}", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                Checkbox(
                    checked = debugTileLabels,
                    onCheckedChange = { debugTileLabels = it },
                    modifier = Modifier.size(ThumbSize * 0.36f),
                )
                Text("tile labels", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
            }
        }
    }
}

@Composable
private fun AirportInsertPanel(
    state: AndroidAirportInsertState,
    modifier: Modifier,
    onTextChange: (String) -> Unit,
    onSubmit: () -> Unit,
    onSuggestionClick: (WaypointIdentifierSuggestion) -> Unit,
) {
    val focusRequester = remember { FocusRequester() }
    val keyboardController = LocalSoftwareKeyboardController.current
    LaunchedEffect(state.componentIndex, state.before) {
        focusRequester.requestFocus()
        keyboardController?.show()
    }
    Surface(
        modifier = modifier,
        shape = RoundedCornerShape(ThumbRadius),
        color = Color(0xF7FCF8F1),
        contentColor = Color(0xFF132129),
        border = BorderStroke(1.dp, Color(0x334E626C)),
        shadowElevation = 8.dp,
    ) {
        Column(
            modifier = Modifier.padding(ThumbGap),
            verticalArrangement = Arrangement.spacedBy(ThumbGap),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(ThumbGap),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    text = if (state.before) "INSERT BEFORE" else "INSERT AFTER",
                    style = MaterialTheme.typography.labelSmall,
                    fontWeight = FontWeight.ExtraBold,
                    color = Color(0xFF52656D),
                )
                BasicTextField(
                    value = state.airportId,
                    onValueChange = onTextChange,
                    singleLine = true,
                    keyboardOptions =
                        KeyboardOptions(
                            capitalization = KeyboardCapitalization.Characters,
                            autoCorrectEnabled = false,
                            keyboardType = KeyboardType.Password,
                            imeAction = ImeAction.Done,
                            platformImeOptions =
                                PlatformImeOptions(
                                    privateImeOptions = "com.google.android.inputmethod.latin.forceAscii",
                                ),
                        ),
                    keyboardActions = KeyboardActions(onDone = { onSubmit() }),
                    textStyle =
                        MaterialTheme.typography.headlineMedium.copy(
                            color = Color(0xFF132129),
                            fontWeight = FontWeight.ExtraBold,
                            textAlign = TextAlign.Center,
                        ),
                    modifier =
                        Modifier
                            .weight(1f)
                            .height(ThumbSize)
                            .focusRequester(focusRequester)
                            .clip(RoundedCornerShape(ThumbRadius))
                            .background(Color.White)
                            .border(1.dp, Color(0x334E626C), RoundedCornerShape(ThumbRadius))
                            .padding(horizontal = ThumbGap, vertical = ThumbSize * 0.18f),
                )
                CompactSquareButton(label = "Enter", modifier = Modifier.width(ThumbSize * 1.4f).height(ThumbSize), onClick = onSubmit)
            }
            if (state.error != null) {
                Text(
                    text = state.error,
                    style = MaterialTheme.typography.labelSmall,
                    fontWeight = FontWeight.Bold,
                    color = Color(0xFFD45A7A),
                )
            }
            if (state.loading) {
                Text("Searching...", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
            }
            Column(
                modifier = Modifier.fillMaxWidth(),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(ThumbGap / 2f),
            ) {
                state.suggestions.forEach { suggestion ->
                    val detail = "${suggestion.kind.uppercase()} ${"%.1f".format(suggestion.distanceFromAnchorNm)}nm"
                    val friendlyName = suggestion.displayName.takeIf { it.isNotBlank() }
                    MenuPanelRow(
                        label = if (friendlyName == null) {
                            "${suggestion.identifier}  $detail"
                        } else {
                            "${suggestion.identifier}  $detail\n$friendlyName"
                        },
                        active = false,
                        enabled = true,
                        width = ThumbSize * 3f,
                        onSelect = { onSuggestionClick(suggestion) },
                    )
                }
            }
        }
    }
}

@Composable
private fun FlightPlanPage(
    appCore: NativeAppCoreAdapter,
    page: AppPage,
    pageHistory: List<AppViewSnapshot>,
    uptimeLabel: String,
    navElement: NavElementUiView?,
    samplePlan: net.jonh.aerobag.prototype.domain.FlightPlan,
    planUiState: FlightPlanUiState?,
    planListState: LazyListState,
    uiTheme: UiTheme,
    onSelectPage: (AppPage) -> Unit,
    onOpenPlan: () -> Unit,
    onOpenCharts: (String?) -> Unit,
    onApplyMutation: (FlightPlanUiMutation) -> Unit,
) {
    val planWaypointTrayStart = ThumbGap + PlanArrowLane + ThumbSize * 2.5f + PlanGridGap
    val density = LocalDensity.current
    var selectedWaypointIndex by remember { mutableStateOf<Int?>(null) }
    var selectedWaypointTrayAnchor by remember { mutableStateOf<Dp?>(null) }
    var pendingSelectedRowKey by remember { mutableStateOf<String?>(null) }
    var reorderOpen by remember { mutableStateOf(false) }
    var pageTrayOpen by remember { mutableStateOf(false) }
    var debugPanelOpen by remember { mutableStateOf(false) }
    var airwayPicker by remember { mutableStateOf<AndroidAirwayPickerState?>(null) }
    var procedurePicker by remember { mutableStateOf<AndroidProcedurePickerState?>(null) }
    var airportInsert by remember { mutableStateOf<AndroidAirportInsertState?>(null) }
    var trayOpenedAtMs by remember { mutableStateOf(0L) }
    val projectedPlanUiState = requireNotNull(planUiState) { "FlightPlanPage requires core-projected FlightPlanUiState" }
    val guidance = projectedPlanUiState.guidance
    val componentViews = remember(projectedPlanUiState.components) { projectedPlanUiState.components }
    val topLevelOrderSummary = remember(componentViews) {
        componentViews.joinToString(" | ") { component ->
            val label =
                when (component.kind) {
                    RouteComponentViewKind.Waypoint -> navRefLabel(component.items.filterIsInstance<ConcretizedNavItem.Waypoint>().firstOrNull()?.navRef ?: component.precedingWaypoint ?: component.followingWaypoint ?: NavRef.Fix("?"))
                    RouteComponentViewKind.Airway,
                    RouteComponentViewKind.Procedure,
                    -> structuredComponentLabel(component)
                }
            "${component.componentIndex}:$label"
        }
    }
    val rows = remember(projectedPlanUiState.displayRows) {
        buildFlightPlanDisplayRows(projectedPlanUiState)
    }
    val blocks = remember(rows) {
        buildFlightPlanDisplayBlocks(rows)
    }
    var structuredSurfaceBounds by remember { mutableStateOf<Rect?>(null) }
    val structuredRowBounds = remember { mutableStateMapOf<String, Rect>() }
    val selectedRow = selectedWaypointIndex?.let(rows::getOrNull)
    val selectedRowBounds = selectedRow?.let { structuredRowBounds[it.id] }
    val waypointTrayStart = planWaypointTrayStart
    fun estimateTrayHeightDp(rowCount: Int): Dp =
        ThumbGap * 2 + (ThumbSize + 3.dp) * rowCount
    val waypointTrayTop =
        run {
            val defaultTop = ThumbSize + ThumbGap * 1.25f
            val anchoredTop = selectedWaypointTrayAnchor
            if (anchoredTop != null) {
                val paneTop = defaultTop + ThumbSize * 0.1f
                val estimatedRows =
                    when {
                        reorderOpen -> 2
                        airportInsert != null -> 5
                        procedurePicker != null -> {
                            val picker = procedurePicker!!
                            when {
                                picker.loading || picker.error != null -> 2
                                picker.selectedProcedureId == null -> 1 + picker.procedures.size
                                else -> 1 + (picker.options?.validChoices?.size ?: 0)
                            }
                        }
                        airwayPicker != null -> {
                            val picker = airwayPicker!!
                            when {
                                picker.loading || picker.error != null -> 2
                                picker.selectedAirwayName == null -> 1 + picker.suggestions.size
                                picker.selectedEntryIndex == null -> 1 + (picker.presentation?.points?.size ?: 0)
                                else -> 1 + (picker.presentation?.points?.size ?: 0)
                            }
                        }
                        else -> selectedRow?.actions?.size ?: 1
                    }.coerceAtLeast(1)
                val estimatedHeight = estimateTrayHeightDp(estimatedRows)
                val paneBottom = with(density) { (structuredSurfaceBounds?.height ?: 0f).toDp() } + defaultTop
                val maxTop = (paneBottom - estimatedHeight - ThumbSize * 0.1f).coerceAtLeast(paneTop)
                return@run anchoredTop.coerceIn(paneTop, maxTop)
            }
            val surfaceBounds = structuredSurfaceBounds
            val rowBounds = selectedRowBounds
            if (surfaceBounds == null || rowBounds == null) {
                defaultTop
            } else {
                val desiredTop = with(density) { (rowBounds.top - surfaceBounds.top).toDp() } + defaultTop
                val paneTop = defaultTop + ThumbSize * 0.1f
                val estimatedRows =
                    when {
                        reorderOpen -> 2
                        airportInsert != null -> 5
                        procedurePicker != null -> {
                            val picker = procedurePicker!!
                            when {
                                picker.loading || picker.error != null -> 2
                                picker.selectedProcedureId == null -> 1 + picker.procedures.size
                                else -> 1 + (picker.options?.validChoices?.size ?: 0)
                            }
                        }
                        airwayPicker != null -> {
                            val picker = airwayPicker!!
                            when {
                                picker.loading || picker.error != null -> 2
                                picker.selectedAirwayName == null -> 1 + picker.suggestions.size
                                picker.selectedEntryIndex == null -> 1 + (picker.presentation?.points?.size ?: 0)
                                else -> 1 + (picker.presentation?.points?.size ?: 0)
                            }
                        }
                        else -> selectedRow.actions.size
                    }.coerceAtLeast(1)
                val estimatedHeight = estimateTrayHeightDp(estimatedRows)
                val paneBottom = with(density) { (surfaceBounds.bottom - surfaceBounds.top).toDp() } + defaultTop
                val maxTop = (paneBottom - estimatedHeight - ThumbSize * 0.1f).coerceAtLeast(paneTop)
                desiredTop.coerceIn(paneTop, maxTop)
            }
        }
    val waypointTrayWidth = ThumbSize * 2.35f
    val structuredArrow =
        remember(rows, guidance?.activeLeg, structuredSurfaceBounds, structuredRowBounds.toMap(), density) {
            val surfaceBounds = structuredSurfaceBounds ?: return@remember null
            val activeLeg = guidance?.activeLeg ?: return@remember null
            val visibleIndices =
                rows.mapIndexedNotNull { index, row ->
                    if (structuredRowBounds.containsKey(row.id)) index else null
                }
            val firstVisibleIndex = visibleIndices.minOrNull()
            val lastVisibleIndex = visibleIndices.maxOrNull()
            val fromIndex =
                rows.indexOfFirst { row ->
                    row.rowKind == "waypoint" && navRefsEqual(row.navRef, activeLeg.from)
                }
            if (fromIndex < 0) {
                return@remember null
            }
            var toIndex = -1
            for (index in (fromIndex + 1) until rows.size) {
                val row = rows[index]
                if (row.rowKind == "waypoint" && navRefsEqual(row.navRef, activeLeg.to)) {
                    toIndex = index
                    break
                }
            }
            val lanePx = with(density) { PlanArrowLane.toPx() }
            val headLength = with(density) { 12.dp.toPx() }
            val textInsetPx = with(density) { PlanArrowButtonInset.toPx() }
            val surfaceHeight = surfaceBounds.height
            fun rowPoint(index: Int, preferBelow: Boolean): StructuredArrowEndpoint? {
                val row = rows[index]
                val indentPx = with(density) { (row.depth * 18).dp.toPx() }
                val x = lanePx + indentPx + textInsetPx
                val bounds = structuredRowBounds[row.id]
                if (bounds != null) {
                    val centerY = bounds.top - surfaceBounds.top + bounds.height / 2f
                    val clippedAbove = bounds.bottom < surfaceBounds.top
                    val clippedBelow = bounds.top > surfaceBounds.bottom
                    val clampedY =
                        when {
                            clippedAbove -> if (preferBelow) surfaceHeight else 0f
                            clippedBelow -> if (preferBelow) surfaceHeight else 0f
                            else -> centerY.coerceIn(0f, surfaceHeight)
                        }
                    return StructuredArrowEndpoint(
                        point =
                            Offset(
                                x = x,
                                y = clampedY,
                            ),
                        clipped = clampedY != centerY,
                        clippedAbove = clippedAbove || (clampedY == 0f && centerY < 0f),
                        clippedBelow = clippedBelow || (clampedY == surfaceHeight && centerY > surfaceHeight),
                    )
                }
                return when {
                    firstVisibleIndex != null && index < firstVisibleIndex ->
                        StructuredArrowEndpoint(
                            point = Offset(x = x, y = 0f),
                            clipped = true,
                            clippedAbove = true,
                            clippedBelow = false,
                        )
                    lastVisibleIndex != null && index > lastVisibleIndex ->
                        StructuredArrowEndpoint(
                            point = Offset(x = x, y = surfaceHeight),
                            clipped = true,
                            clippedAbove = false,
                            clippedBelow = true,
                        )
                    else -> null
                }
            }
            val fromEndpoint = rowPoint(fromIndex, preferBelow = false) ?: return@remember null
            val toEndpoint =
                if (toIndex >= 0) {
                    rowPoint(toIndex, preferBelow = toIndex > fromIndex)
                } else {
                    StructuredArrowEndpoint(
                        point = Offset(x = fromEndpoint.point.x, y = surfaceHeight),
                        clipped = true,
                        clippedAbove = false,
                        clippedBelow = true,
                    )
                } ?: return@remember null
            if (fromEndpoint.clipped && toEndpoint.clipped) {
                return@remember null
            }
            if (toEndpoint.clippedAbove) {
                return@remember null
            }
            val fromPoint = fromEndpoint.point
            val toPoint = toEndpoint.point
            val elbowX = lanePx * 0.25f
            StructuredArrowSpec(
                fromPoint = fromPoint,
                toPoint = toPoint,
                toClipped = toEndpoint.clipped,
                fromClippedAbove = fromEndpoint.clippedAbove,
                elbowX = elbowX,
                shaftEndX = maxOf(elbowX, toPoint.x - headLength + with(density) { 1.5.dp.toPx() }),
                headLength = headLength,
            )
        }

    fun closePanels() {
        selectedWaypointIndex = null
        selectedWaypointTrayAnchor = null
        pendingSelectedRowKey = null
        reorderOpen = false
        airwayPicker = null
        procedurePicker = null
        airportInsert = null
    }

    LaunchedEffect(airportInsert?.componentIndex, airportInsert?.before, airportInsert?.airportId, samplePlan) {
        val editor = airportInsert ?: return@LaunchedEffect
        val prefix = editor.airportId.trim().uppercase()
        if (prefix.isEmpty()) {
            airportInsert = editor.copy(loading = false, suggestions = emptyList())
            return@LaunchedEffect
        }
        airportInsert = editor.copy(loading = true)
        runCatching {
            withContext(Dispatchers.IO) {
                appCore.suggestWaypointIdentifiers(samplePlan, editor.componentIndex, editor.before, prefix, 8)
            }
        }.onSuccess { suggestions ->
            airportInsert = airportInsert?.copy(loading = false, suggestions = suggestions)
        }.onFailure { error ->
            airportInsert = airportInsert?.copy(loading = false, suggestions = emptyList(), error = error.message ?: error.toString())
        }
    }

    LaunchedEffect(rows, pendingSelectedRowKey) {
        val selectionKey = pendingSelectedRowKey ?: return@LaunchedEffect
        Log.d("AerobagReorder", "resolveSelection pendingKey=$selectionKey rows=${rows.joinToString(" | ") { "${it.selectionKey}:${it.rowKind}:${it.label}" }}")
        val nextIndex =
            rows.indexOfFirst { row ->
                row.selectionKey == selectionKey
            }
        if (nextIndex >= 0) {
            selectedWaypointIndex = nextIndex
        } else {
            selectedWaypointIndex = null
            reorderOpen = false
        }
        pendingSelectedRowKey = null
    }

    LaunchedEffect(topLevelOrderSummary) {
        Log.d("AerobagReorder", "topLevelOrder $topLevelOrderSummary")
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(uiTheme.controls.chartSurfaceBg),
    ) {
        MenuDock(
            modifier = Modifier
                .align(Alignment.TopStart)
                .padding(ThumbGap),
            launcherLabel = PageOptions.firstOrNull { it.page == page }?.launcherLabel ?: "PLN",
            open = pageTrayOpen,
            onToggle = { pageTrayOpen = !pageTrayOpen },
            style = MenuDockStyle.Compact,
            options = PageOptions.map { option ->
                MenuDockOption(option.page.name, option.label, active = option.page == page) {
                    onSelectPage(option.page)
                    pageTrayOpen = false
                }
            },
        )

        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(top = ThumbSize + ThumbGap * 2, start = ThumbGap, end = ThumbGap, bottom = ThumbSize * 2.15f),
            verticalArrangement = Arrangement.spacedBy(PlanGridGap),
        ) {
            Box(
                modifier = Modifier
                    .weight(1f)
                    .fillMaxWidth()
                    .onGloballyPositioned { coordinates ->
                        structuredSurfaceBounds = coordinates.boundsInWindow()
                    },
            ) {
                Column(
                    modifier = Modifier
                        .fillMaxSize()
                        .padding(start = PlanArrowLane),
                    verticalArrangement = Arrangement.spacedBy(PlanGridGap),
                ) {
                    PlanHeaderRow()
                    LazyColumn(
                        state = planListState,
                        modifier = Modifier.weight(1f),
                        verticalArrangement = Arrangement.spacedBy(PlanGridGap),
                    ) {
                        items(blocks.size) { blockIndex ->
                            when (val block = blocks[blockIndex]) {
                                is FlightPlanDisplayBlock.Single -> {
                                    FlightPlanDataRow(
                                        row = block.row,
                                        selected = selectedWaypointIndex == block.index,
                                        reorderOpen = reorderOpen,
                                        structuredRowBounds = structuredRowBounds,
                                        onWaypointClick = {
                                            trayOpenedAtMs = SystemClock.elapsedRealtime()
                                            selectedWaypointTrayAnchor =
                                                structuredSurfaceBounds?.let { surface ->
                                                    structuredRowBounds[block.row.id]?.top?.let { top ->
                                                        with(density) { (top - surface.top).toDp() } + (ThumbSize + ThumbGap * 1.25f)
                                                    }
                                                }
                                            selectedWaypointIndex = block.index
                                            pendingSelectedRowKey = block.row.selectionKey
                                            reorderOpen = false
                                            airwayPicker = null
                                            procedurePicker = null
                                            airportInsert = null
                                        },
                                    )
                                }

                                is FlightPlanDisplayBlock.Group -> {
                                    FlightPlanGroupBlock(
                                        header = block.header,
                                        headerSelected = selectedWaypointIndex == block.headerIndex,
                                        reorderOpen = reorderOpen,
                                        structuredRowBounds = structuredRowBounds,
                                        onHeaderClick = {
                                            trayOpenedAtMs = SystemClock.elapsedRealtime()
                                            selectedWaypointTrayAnchor =
                                                structuredSurfaceBounds?.let { surface ->
                                                    structuredRowBounds[block.header.id]?.top?.let { top ->
                                                        with(density) { (top - surface.top).toDp() } + (ThumbSize + ThumbGap * 1.25f)
                                                    }
                                                }
                                            selectedWaypointIndex = block.headerIndex
                                            pendingSelectedRowKey = block.header.selectionKey
                                            reorderOpen = false
                                            airwayPicker = null
                                            procedurePicker = null
                                            airportInsert = null
                                        },
                                        children = block.children,
                                        selectedWaypointIndex = selectedWaypointIndex,
                                        onChildClick = { childIndex ->
                                            trayOpenedAtMs = SystemClock.elapsedRealtime()
                                            selectedWaypointTrayAnchor =
                                                structuredSurfaceBounds?.let { surface ->
                                                    block.children.firstOrNull { it.first == childIndex }?.second?.id?.let { rowId ->
                                                        structuredRowBounds[rowId]?.top?.let { top ->
                                                            with(density) { (top - surface.top).toDp() } + (ThumbSize + ThumbGap * 1.25f)
                                                        }
                                                    }
                                                }
                                            selectedWaypointIndex = childIndex
                                            pendingSelectedRowKey = block.children.firstOrNull { it.first == childIndex }?.second?.selectionKey
                                            reorderOpen = false
                                            airwayPicker = null
                                            procedurePicker = null
                                            airportInsert = null
                                        },
                                    )
                                }
                            }
                        }
                    }
                }

                if (structuredArrow != null) {
                    Canvas(
                        modifier =
                            Modifier
                                .matchParentSize()
                                .zIndex(2f),
                    ) {
                        val path =
                            Path().apply {
                                moveTo(
                                    if (structuredArrow.fromClippedAbove) structuredArrow.elbowX else structuredArrow.fromPoint.x,
                                    structuredArrow.fromPoint.y,
                                )
                                if (!structuredArrow.fromClippedAbove) {
                                    lineTo(structuredArrow.elbowX, structuredArrow.fromPoint.y)
                                }
                                lineTo(structuredArrow.elbowX, structuredArrow.toPoint.y)
                                if (!structuredArrow.toClipped) {
                                    lineTo(structuredArrow.shaftEndX, structuredArrow.toPoint.y)
                                }
                            }
                        drawPath(
                            path = path,
                            color = Color(0xFFD45A7A),
                            style =
                                Stroke(
                                    width = with(density) { 3.dp.toPx() },
                                    cap = StrokeCap.Round,
                                ),
                        )
                        if (!structuredArrow.toClipped) {
                            val head =
                                Path().apply {
                                    moveTo(structuredArrow.toPoint.x, structuredArrow.toPoint.y)
                                    lineTo(
                                        structuredArrow.toPoint.x - structuredArrow.headLength,
                                        structuredArrow.toPoint.y - structuredArrow.headLength * 0.55f,
                                    )
                                    lineTo(
                                        structuredArrow.toPoint.x - structuredArrow.headLength,
                                        structuredArrow.toPoint.y + structuredArrow.headLength * 0.55f,
                                    )
                                    close()
                                }
                            drawPath(head, color = Color(0xFFD45A7A))
                        }
                    }
                }
            }
        }

        Box(
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .fillMaxWidth()
                .height(ThumbSize * 2.05f)
                .padding(start = ThumbGap, end = ThumbGap, bottom = ThumbGap),
        ) {
            Row(
                modifier = Modifier.align(Alignment.TopCenter),
                horizontalArrangement = Arrangement.spacedBy(ThumbGap),
            ) {
                CompactSquareButton(
                    label = "Next Leg",
                    modifier = Modifier.width(ThumbSize * 1.8f).height(ThumbSize),
                    enabled = guidance?.canActivateNextLeg == true,
                    onClick = { onApplyMutation(appCore.activateNextLegUi(samplePlan)) },
                )
                CompactSquareButton(
                    label = "Sequence",
                    modifier = Modifier.width(ThumbSize * 1.8f).height(ThumbSize),
                    enabled = guidance?.canSequenceActiveLeg == true,
                    onClick = { onApplyMutation(appCore.sequenceActiveLegUi(samplePlan)) },
                )
                CompactSquareButton(
                    label = "Suspend",
                    modifier = Modifier.width(ThumbSize * 1.8f).height(ThumbSize),
                    enabled = guidance?.canSuspend == true,
                    onClick = { onApplyMutation(appCore.suspendSequencingUi(samplePlan)) },
                )
                CompactSquareButton(
                    label = "Unsusp",
                    modifier = Modifier.width(ThumbSize * 1.8f).height(ThumbSize),
                    enabled = guidance?.canUnsuspend == true,
                    onClick = { onApplyMutation(appCore.unsuspendSequencingUi(samplePlan)) },
                )
            }
            NavElementDock(
                navElement = navElement,
                onClick = onOpenPlan,
                modifier = Modifier.align(Alignment.BottomCenter),
            )
        }

        DebugDock(
            open = debugPanelOpen,
            onToggle = { debugPanelOpen = !debugPanelOpen },
            modifier = Modifier.align(Alignment.BottomStart),
        ) {
            Text("up: $uptimeLabel", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
        }

        if (pageTrayOpen) {
            Scrim { pageTrayOpen = false }
        }

        if (selectedWaypointIndex != null && selectedRow != null) {
            Scrim {
                if (SystemClock.elapsedRealtime() - trayOpenedAtMs >= 150L) {
                    closePanels()
                }
            }
            if (airportInsert != null) {
                val editor = airportInsert!!
                BoxWithConstraints(
                    modifier =
                        Modifier
                            .fillMaxSize()
                            .zIndex(5f),
                ) {
                    AirportInsertPanel(
                        state = editor,
                        modifier =
                            Modifier
                                .align(Alignment.TopCenter)
                                .fillMaxWidth()
                                .padding(start = ThumbGap, top = ThumbSize, end = ThumbGap)
                                .heightIn(min = ThumbSize * 1.45f),
                        onTextChange = { value ->
                            airportInsert =
                                editor.copy(
                                    airportId = value.uppercase().filter { it in 'A'..'Z' || it in '0'..'9' }.take(8),
                                    error = null,
                                )
                        },
                        onSubmit = {
                            val airportId = editor.airportId.trim().uppercase()
                            if (airportId.isEmpty()) {
                                airportInsert = editor.copy(error = "Enter airport id")
                                return@AirportInsertPanel
                            }
                            runCatching {
                                val waypoint = appCore.resolveNavRefIdentifier(airportId)
                                appCore.insertWaypointUi(samplePlan, editor.componentIndex, editor.before, waypoint)
                            }.onSuccess { mutation ->
                                onApplyMutation(mutation)
                                closePanels()
                            }.onFailure { error ->
                                airportInsert = editor.copy(error = error.message ?: error.toString())
                            }
                        },
                        onSuggestionClick = { suggestion ->
                            runCatching {
                                appCore.insertWaypointUi(samplePlan, editor.componentIndex, editor.before, suggestion.navRef)
                            }.onSuccess { mutation ->
                                onApplyMutation(mutation)
                                closePanels()
                            }.onFailure { error ->
                                airportInsert = editor.copy(error = error.message ?: error.toString())
                            }
                        },
                    )
                }
            } else if (procedurePicker != null) {
                val picker = procedurePicker!!
                MenuPanel(
                    modifier = Modifier
                        .align(Alignment.TopStart)
                        .padding(top = waypointTrayTop, start = waypointTrayStart, end = ThumbGap)
                        .zIndex(5f),
                    width = waypointTrayWidth,
                ) {
                    MenuPanelRow(label = "APPROACH ${picker.airportId}", active = false, enabled = false, onSelect = {})
                    if (picker.error != null) {
                        MenuPanelRow(label = picker.error, active = false, enabled = false, onSelect = {})
                    }
                    if (picker.loading) {
                        MenuPanelRow(label = "Loading…", active = false, enabled = false, onSelect = {})
                    } else if (picker.selectedProcedureId == null) {
                        picker.procedures.forEach { procedure ->
                            MenuPanelRow(
                                label = procedure.procedureId,
                                active = false,
                                enabled = true,
                                onSelect = {
                                    procedurePicker = picker.copy(loading = true, error = null)
                                    runCatching {
                                        appCore.describeProcedureOptions(
                                            picker.airportId,
                                            procedure.procedureId,
                                            ProcedureKind.Approach,
                                        )
                                    }.onSuccess { options ->
                                        procedurePicker =
                                            picker.copy(
                                                loading = false,
                                                selectedProcedureId = procedure.procedureId,
                                                options = options,
                                            )
                                    }.onFailure { error ->
                                        Log.e("AerobagProcedure", "describeProcedureOptions failed airport=${picker.airportId} procedure=${procedure.procedureId}", error)
                                        procedurePicker = picker.copy(loading = false, error = error.message ?: error.toString())
                                    }
                                },
                            )
                        }
                    } else {
                        picker.options?.validChoices?.forEach { choice ->
                            MenuPanelRow(
                                label = choice.enrouteTransition ?: "No Transition",
                                active = false,
                                enabled = true,
                                onSelect = {
                                    procedurePicker = picker.copy(loading = true, error = null)
                                    runCatching {
                                        appCore.materializeProcedureSelection(
                                            picker.airportId,
                                            picker.selectedProcedureId,
                                            ProcedureKind.Approach,
                                            null,
                                            choice.enrouteTransition,
                                            picker.startComponentIndex + 1,
                                        )
                                    }.map { built ->
                                        appCore.insertProcedureMaterializedUi(
                                            samplePlan,
                                            picker.startComponentIndex,
                                            picker.endComponentIndex,
                                            built,
                                        )
                                    }.onSuccess { mutation ->
                                        onApplyMutation(mutation)
                                        closePanels()
                                    }.onFailure { error ->
                                        Log.e(
                                            "AerobagProcedure",
                                            "materialize/insert procedure failed airport=${picker.airportId} procedure=${picker.selectedProcedureId} enroute=${choice.enrouteTransition}",
                                            error,
                                        )
                                        procedurePicker = picker.copy(loading = false, error = error.message ?: error.toString())
                                    }
                                },
                            )
                        }
                        MenuPanelRow(
                            label = "Back",
                            active = false,
                            enabled = true,
                            onSelect = { procedurePicker = picker.copy(selectedProcedureId = null, options = null) },
                        )
                    }
                }
            } else if (airwayPicker != null) {
                val picker = airwayPicker!!
                MenuPanel(
                    modifier = Modifier
                        .align(Alignment.TopStart)
                        .padding(top = waypointTrayTop, start = waypointTrayStart, end = ThumbGap)
                        .zIndex(5f),
                    width = waypointTrayWidth,
                ) {
                    MenuPanelRow(
                        label = buildString {
                            append("AIRWAY ")
                            append(navRefLabel(picker.originAnchor))
                            picker.destinationAnchor?.let {
                                append(" -> ")
                                append(navRefLabel(it))
                            }
                        },
                        active = false,
                        enabled = false,
                        onSelect = {},
                    )
                    if (picker.error != null) {
                        MenuPanelRow(label = picker.error, active = false, enabled = false, onSelect = {})
                    }
                    if (picker.loading) {
                        MenuPanelRow(label = "Loading…", active = false, enabled = false, onSelect = {})
                    } else if (picker.selectedAirwayName == null) {
                        picker.suggestions.forEach { suggestion ->
                            MenuPanelRow(
                                label = suggestion.airwayName,
                                active = false,
                                enabled = true,
                                onSelect = {
                                    airwayPicker = picker.copy(loading = true, error = null)
                                    runCatching {
                                        appCore.prepareAirwayPresentationForAnchors(
                                            suggestion.airwayName,
                                            picker.originAnchor,
                                            picker.destinationAnchor,
                                        )
                                    }.onSuccess { presentation ->
                                        airwayPicker =
                                            picker.copy(
                                                loading = false,
                                                selectedAirwayName = suggestion.airwayName,
                                                presentation = presentation,
                                                selectedEntryIndex = null,
                                            )
                                    }.onFailure { error ->
                                        airwayPicker = picker.copy(loading = false, error = error.message ?: error.toString())
                                    }
                                },
                            )
                        }
                    } else if (picker.selectedEntryIndex == null) {
                        picker.presentation?.points?.forEachIndexed { index, point ->
                            MenuPanelRow(
                                label = navRefLabel(point.navRef),
                                active = index == picker.presentation.suggestedEntryIndex,
                                enabled = true,
                                onSelect = {
                                    airwayPicker = picker.copy(selectedEntryIndex = index)
                                },
                            )
                        }
                        MenuPanelRow(label = "Back", active = false, enabled = true, onSelect = { airwayPicker = picker.copy(selectedAirwayName = null, presentation = null) })
                    } else {
                        val presentation = picker.presentation
                        presentation?.points?.forEachIndexed { exitIndex, point ->
                            val isEntry = exitIndex == picker.selectedEntryIndex
                            MenuPanelRow(
                                label = navRefLabel(point.navRef),
                                active = exitIndex == presentation.suggestedExitIndex,
                                enabled = !isEntry,
                                onSelect = {
                                    if (isEntry) return@MenuPanelRow
                                    airwayPicker = picker.copy(loading = true, error = null)
                                    runCatching {
                                        val startComponentIndex = if (picker.mode == "replace" && picker.componentIndex != null) {
                                            picker.componentIndex
                                        } else {
                                            picker.startComponentIndex ?: error("missing insertion start")
                                        }
                                        appCore.materializeAirwayPresentationSelection(
                                            startComponentIndex,
                                            presentation,
                                            picker.selectedEntryIndex,
                                            exitIndex,
                                            picker.originAnchor,
                                            picker.destinationAnchor,
                                        )
                                    }.map { built ->
                                        if (picker.mode == "replace" && picker.componentIndex != null) {
                                            appCore.replaceAirwayMaterializedUi(
                                                samplePlan,
                                                picker.componentIndex,
                                                built.selection,
                                                built.airway,
                                                built.resolvedLegs,
                                            )
                                        } else {
                                            appCore.insertAirwayMaterializedUi(
                                                samplePlan,
                                                picker.startComponentIndex ?: error("missing insertion start"),
                                                picker.endComponentIndex,
                                                built.selection,
                                                built.airway,
                                                built.resolvedLegs,
                                            )
                                        }
                                    }.onSuccess { mutation ->
                                        onApplyMutation(mutation)
                                        closePanels()
                                    }.onFailure { error ->
                                        airwayPicker = picker.copy(error = error.message ?: error.toString())
                                    }
                                },
                            )
                        }
                        MenuPanelRow(label = "Back", active = false, enabled = true, onSelect = { airwayPicker = picker.copy(selectedEntryIndex = null) })
                    }
                }
            } else if (reorderOpen) {
                BoxWithConstraints(
                    modifier = Modifier
                        .fillMaxSize()
                        .zIndex(5f),
                ) {
                    val trayWidth = minOf(ThumbSize * 4f, maxWidth - waypointTrayStart - ThumbGap)
                    MenuPanel(
                        modifier = Modifier
                            .align(Alignment.TopStart)
                            .padding(top = waypointTrayTop, start = waypointTrayStart, end = ThumbGap),
                        width = trayWidth,
                    ) {
                        Column(
                            modifier = Modifier.fillMaxWidth(),
                            verticalArrangement = Arrangement.spacedBy(ThumbGap),
                            horizontalAlignment = Alignment.CenterHorizontally,
                        ) {
                            CompactSquareButton(
                                label = "Up",
                                modifier = Modifier.size(ThumbSize),
                                enabled = selectedRow.componentIndex != null && selectedRow.canReorderUp,
                                onClick = {
                                    selectedRow.componentIndex?.let {
                                        Log.d(
                                            "AerobagReorder",
                                            "request move dir=-1 selectedIndex=$selectedWaypointIndex key=${selectedRow.selectionKey} component=$it row=${selectedRow.label} orderBefore=$topLevelOrderSummary",
                                        )
                                        pendingSelectedRowKey = selectedRow.selectionKey
                                        onApplyMutation(appCore.moveComponentUi(samplePlan, it, -1))
                                    }
                                },
                            )
                            CompactSquareButton(
                                label = "Down",
                                modifier = Modifier.size(ThumbSize),
                                enabled = selectedRow.componentIndex != null && selectedRow.canReorderDown,
                                onClick = {
                                    selectedRow.componentIndex?.let {
                                        Log.d(
                                            "AerobagReorder",
                                            "request move dir=1 selectedIndex=$selectedWaypointIndex key=${selectedRow.selectionKey} component=$it row=${selectedRow.label} orderBefore=$topLevelOrderSummary",
                                        )
                                        pendingSelectedRowKey = selectedRow.selectionKey
                                        onApplyMutation(appCore.moveComponentUi(samplePlan, it, 1))
                                    }
                                },
                            )
                        }
                    }
                }
            } else {
                MenuPanel(
                    modifier = Modifier
                        .align(Alignment.TopStart)
                        .padding(top = waypointTrayTop, start = waypointTrayStart, end = ThumbGap)
                        .zIndex(5f),
                    width = waypointTrayWidth,
                ) {
                    selectedRow.actions.forEach { action ->
                        MenuPanelRow(
                            label = flightPlanActionLabel(action.id),
                            active = false,
                            enabled = action.enabled,
                            onSelect = {
                                if (!action.enabled) {
                                    return@MenuPanelRow
                                }
                                when (action.id) {
                                    FlightPlanRowActionId.ActivateLeg -> {
                                        selectedRow.legIndex?.let {
                                            onApplyMutation(appCore.activateLegUi(samplePlan, it))
                                        }
                                        closePanels()
                                    }
                                    FlightPlanRowActionId.Remove,
                                    FlightPlanRowActionId.RemoveAirway,
                                    FlightPlanRowActionId.RemoveProcedure,
                                    -> {
                                        selectedRow.componentIndex?.let {
                                            onApplyMutation(appCore.deleteComponentUi(samplePlan, it))
                                        }
                                        closePanels()
                                    }
                                    FlightPlanRowActionId.Reorder -> {
                                        reorderOpen = true
                                    }
                                    FlightPlanRowActionId.InsertBefore,
                                    FlightPlanRowActionId.InsertAfter,
                                    -> {
                                        val componentIndex = selectedRow.componentIndex ?: return@MenuPanelRow
                                        airportInsert =
                                            AndroidAirportInsertState(
                                                componentIndex = componentIndex,
                                                before = action.id == FlightPlanRowActionId.InsertBefore,
                                                airportId = "",
                                                error = null,
                                                loading = false,
                                                suggestions = emptyList(),
                                            )
                                    }
                                    FlightPlanRowActionId.AddAirway -> {
                                        airwayPicker =
                                            AndroidAirwayPickerState(
                                                loading = true,
                                                error = null,
                                                mode = "insert",
                                                componentIndex = null,
                                                startComponentIndex = selectedRow.startComponentIndex,
                                                endComponentIndex = selectedRow.endComponentIndex,
                                                originAnchor = selectedRow.originAnchor!!,
                                                destinationAnchor = selectedRow.destinationAnchor,
                                                suggestions = emptyList(),
                                                selectedAirwayName = null,
                                                presentation = null,
                                                selectedEntryIndex = null,
                                            )
                                        runCatching {
                                            appCore.suggestAirwaysNear(selectedRow.originAnchor!!)
                                        }.onSuccess { suggestions ->
                                            airwayPicker = airwayPicker?.copy(loading = false, suggestions = suggestions)
                                        }.onFailure { error ->
                                            airwayPicker = airwayPicker?.copy(loading = false, error = error.message ?: error.toString())
                                        }
                                    }
                                    FlightPlanRowActionId.ChangeAirway -> {
                                        val componentIndex = selectedRow.componentIndex ?: return@MenuPanelRow
                                        airwayPicker =
                                            AndroidAirwayPickerState(
                                                loading = true,
                                                error = null,
                                                mode = "replace",
                                                componentIndex = componentIndex,
                                                startComponentIndex = null,
                                                endComponentIndex = null,
                                                originAnchor = selectedRow.originAnchor ?: return@MenuPanelRow,
                                                destinationAnchor = selectedRow.destinationAnchor,
                                                suggestions = emptyList(),
                                                selectedAirwayName = null,
                                                presentation = null,
                                                selectedEntryIndex = null,
                                            )
                                        runCatching {
                                            appCore.suggestAirwaysNear(selectedRow.originAnchor!!)
                                        }.onSuccess { suggestions ->
                                            airwayPicker = airwayPicker?.copy(loading = false, suggestions = suggestions)
                                        }.onFailure { error ->
                                            airwayPicker = airwayPicker?.copy(loading = false, error = error.message ?: error.toString())
                                        }
                                    }
                                    FlightPlanRowActionId.SelectProcedure -> {
                                        val airportId = selectedRow.chartAirportId ?: return@MenuPanelRow
                                        val componentIndex = selectedRow.componentIndex ?: return@MenuPanelRow
                                        procedurePicker =
                                            AndroidProcedurePickerState(
                                                loading = true,
                                                error = null,
                                                airportId = airportId,
                                                startComponentIndex = componentIndex - 1,
                                                endComponentIndex = componentIndex,
                                                procedures = emptyList(),
                                                selectedProcedureId = null,
                                                options = null,
                                            )
                                        runCatching {
                                            appCore.listProcedures(airportId, ProcedureKind.Approach)
                                        }.onSuccess { procedures ->
                                            procedurePicker = procedurePicker?.copy(loading = false, procedures = procedures)
                                        }.onFailure { error ->
                                            Log.e("AerobagProcedure", "listProcedures failed airport=$airportId", error)
                                            procedurePicker = procedurePicker?.copy(loading = false, error = error.message ?: error.toString())
                                        }
                                    }
                                    FlightPlanRowActionId.Plates -> {
                                        onOpenCharts(selectedRow.chartAirportId)
                                        closePanels()
                                    }
                                    FlightPlanRowActionId.WaypointInfo,
                                    -> {}
                                }
                            }
                        )
                    }
                }
            }
        }
    }
}

@OptIn(ExperimentalComposeUiApi::class)
@Composable
private fun ChartsPage(
    page: AppPage,
    pageHistory: List<AppViewSnapshot>,
    uptimeLabel: String,
    airports: List<ChartAirport>,
    selectedAirport: ChartAirport?,
    selectedChart: ChartAsset?,
    uiTheme: UiTheme,
    ownship: OwnshipRenderState,
    navElement: NavElementUiView?,
    folderOpen: Boolean,
    viewport: net.jonh.aerobag.prototype.domain.ImageViewportState?,
    onViewportChange: (net.jonh.aerobag.prototype.domain.ImageViewportState?) -> Unit,
    onFolderOpenChange: (Boolean) -> Unit,
    onSelectPage: (AppPage) -> Unit,
    onOpenPlan: () -> Unit,
    onSelectAirport: (String) -> Unit,
    onSelectChart: (String) -> Unit,
) {
    val context = LocalContext.current
    val activity = context as? MainActivity
    val density = LocalDensity.current
    val focusRequester = remember { FocusRequester() }
    val chartLabelsById = remember(airports) {
        airports.flatMap { airport -> airport.charts }.associate { chart -> chart.id to chart.label }
    }
    var pageTrayOpen by remember { mutableStateOf(false) }
    var airportTrayOpen by remember { mutableStateOf(false) }
    var chartTrayOpen by remember { mutableStateOf(false) }
    var debugPanelOpen by remember { mutableStateOf(false) }
    var surfaceSize by remember { mutableStateOf(IntSize.Zero) }
    val sortedCharts = selectedAirport?.charts ?: emptyList()
    val overscrollPx = with(density) { ThumbSize.toPx() }
    val bitmap by produceState<androidx.compose.ui.graphics.ImageBitmap?>(initialValue = null, selectedChart?.id, selectedChart?.assetPath) {
        val chart = selectedChart
        val path = chart?.assetPath
        value = if (path == null) {
            null
        } else {
            withContext(Dispatchers.IO) {
                runCatching {
                    val localFile = java.io.File(context.filesDir, path)
                    val inputStream =
                        if (localFile.isFile) {
                            localFile.inputStream()
                        } else {
                            val chartBytes = ChartPackages.loadChartBytes(context, chart) ?: context.assets.open(path).use { it.readBytes() }
                            chartBytes.inputStream()
                        }
                    inputStream.use { stream ->
                        BitmapFactory.decodeStream(stream)?.asImageBitmap()
                    }
                }.getOrNull()
            }
        }
    }
    val viewportState = rememberUpdatedState(viewport)
    val imageWidthPx = bitmap?.width?.toFloat() ?: 0f
    val imageHeightPx = bitmap?.height?.toFloat() ?: 0f
    val trayOpen = pageTrayOpen || airportTrayOpen || chartTrayOpen

    LaunchedEffect(bitmap, surfaceSize) {
        val currentBitmap = bitmap
        if (currentBitmap != null && surfaceSize.width > 0 && surfaceSize.height > 0 && viewport == null) {
            onViewportChange(createInitialImageViewport(
                imageWidthPx = currentBitmap.width.toFloat(),
                imageHeightPx = currentBitmap.height.toFloat(),
                viewportWidthPx = surfaceSize.width.toFloat(),
                viewportHeightPx = surfaceSize.height.toFloat(),
            ))
        }
    }
    LaunchedEffect(selectedChart?.id, trayOpen, folderOpen) {
        if (!trayOpen && !folderOpen) {
            withFrameNanos { }
            focusRequester.requestFocus()
        }
    }
    DisposableEffect(activity, selectedChart?.id, surfaceSize, bitmap, viewportState.value, trayOpen, folderOpen) {
        if (activity != null) {
            activity.onHardwareZoomDelta = { delta ->
                val currentState = viewportState.value
                if (bitmap == null || currentState == null || trayOpen || folderOpen) {
                    false
                } else {
                    onViewportChange(
                        zoomImageAroundPoint(
                            state = currentState,
                            anchorX = surfaceSize.width / 2f,
                            anchorY = surfaceSize.height / 2f,
                            nextZoom = currentState.zoom + delta.toFloat(),
                            imageWidthPx = imageWidthPx,
                            imageHeightPx = imageHeightPx,
                            viewportWidthPx = surfaceSize.width.toFloat(),
                            viewportHeightPx = surfaceSize.height.toFloat(),
                            overscrollPx = overscrollPx,
                        ),
                    )
                    true
                }
            }
        }
        onDispose {
            if (activity != null && activity.onHardwareZoomDelta != null) {
                activity.onHardwareZoomDelta = null
            }
        }
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(uiTheme.controls.chartSurfaceBg)
            .onSizeChanged { surfaceSize = it }
            .focusRequester(focusRequester)
            .focusable()
            .onPreviewKeyEvent { event ->
                if (bitmap == null || viewportState.value == null || trayOpen || folderOpen || event.nativeKeyEvent.action != AndroidKeyEvent.ACTION_DOWN) {
                    return@onPreviewKeyEvent false
                }
                val nextZoom = when (event.nativeKeyEvent.keyCode) {
                    AndroidKeyEvent.KEYCODE_PLUS,
                    AndroidKeyEvent.KEYCODE_EQUALS,
                    AndroidKeyEvent.KEYCODE_NUMPAD_ADD -> (viewportState.value?.zoom ?: 1f) + 0.3f
                    AndroidKeyEvent.KEYCODE_MINUS,
                    AndroidKeyEvent.KEYCODE_NUMPAD_SUBTRACT -> (viewportState.value?.zoom ?: 1f) - 0.3f
                    else -> return@onPreviewKeyEvent false
                }
                onViewportChange(zoomImageAroundPoint(
                    state = viewportState.value ?: return@onPreviewKeyEvent false,
                    anchorX = surfaceSize.width / 2f,
                    anchorY = surfaceSize.height / 2f,
                    nextZoom = nextZoom,
                    imageWidthPx = imageWidthPx,
                    imageHeightPx = imageHeightPx,
                    viewportWidthPx = surfaceSize.width.toFloat(),
                    viewportHeightPx = surfaceSize.height.toFloat(),
                    overscrollPx = overscrollPx,
                ))
                true
            }
            .pointerInput(bitmap, surfaceSize, trayOpen) {
                if (bitmap == null || viewportState.value == null || trayOpen || folderOpen) {
                    return@pointerInput
                }
                detectTapGestures(
                    onDoubleTap = { tap ->
                        onViewportChange(zoomImageAroundPoint(
                            state = viewportState.value ?: return@detectTapGestures,
                            anchorX = tap.x,
                            anchorY = tap.y,
                            nextZoom = (viewportState.value?.zoom ?: 1f) + 0.75f,
                            imageWidthPx = imageWidthPx,
                            imageHeightPx = imageHeightPx,
                            viewportWidthPx = surfaceSize.width.toFloat(),
                            viewportHeightPx = surfaceSize.height.toFloat(),
                            overscrollPx = overscrollPx,
                        ))
                    },
                )
            }
            .pointerInput(bitmap, surfaceSize, trayOpen) {
                if (bitmap == null || viewportState.value == null || trayOpen || folderOpen) {
                    return@pointerInput
                }
                detectTransformGestures { centroid, pan, zoom, _ ->
                    val current = viewportState.value ?: return@detectTransformGestures
                    var next = zoomImageAroundPoint(
                        state = current,
                        anchorX = centroid.x,
                        anchorY = centroid.y,
                        nextZoom = current.zoom * zoom,
                        imageWidthPx = imageWidthPx,
                        imageHeightPx = imageHeightPx,
                        viewportWidthPx = surfaceSize.width.toFloat(),
                        viewportHeightPx = surfaceSize.height.toFloat(),
                        overscrollPx = overscrollPx,
                    )
                    next = dragImageViewport(
                        state = next,
                        dxPx = pan.x,
                        dyPx = pan.y,
                        imageWidthPx = imageWidthPx,
                        imageHeightPx = imageHeightPx,
                        viewportWidthPx = surfaceSize.width.toFloat(),
                        viewportHeightPx = surfaceSize.height.toFloat(),
                        overscrollPx = overscrollPx,
                    )
                    onViewportChange(next)
                }
            }
            .pointerInteropFilter { event ->
                if (bitmap == null || viewportState.value == null || trayOpen || folderOpen) {
                    return@pointerInteropFilter false
                }
                if (event.actionMasked == MotionEvent.ACTION_DOWN) {
                    focusRequester.requestFocus()
                }
                if (event.action == MotionEvent.ACTION_SCROLL) {
                    val wheelDelta = event.getAxisValue(MotionEvent.AXIS_VSCROLL).takeIf { it != 0f }
                        ?: event.getAxisValue(MotionEvent.AXIS_SCROLL)
                    onViewportChange(zoomImageAroundPoint(
                        state = viewportState.value ?: return@pointerInteropFilter false,
                        anchorX = surfaceSize.width / 2f,
                        anchorY = surfaceSize.height / 2f,
                        nextZoom = (viewportState.value?.zoom ?: 1f) - wheelDelta * 0.18f,
                        imageWidthPx = imageWidthPx,
                        imageHeightPx = imageHeightPx,
                        viewportWidthPx = surfaceSize.width.toFloat(),
                        viewportHeightPx = surfaceSize.height.toFloat(),
                        overscrollPx = overscrollPx,
                    ))
                    true
                } else {
                    false
                }
            },
    ) {
        if (folderOpen) {
            PlateFolderGrid(
                modifier = Modifier.fillMaxSize(),
                charts = sortedCharts,
                selectedChartId = selectedChart?.id,
                uiTheme = uiTheme,
                onSelectChart = {
                    onSelectChart(it)
                },
            )
        } else {
            Canvas(modifier = Modifier.fillMaxSize()) {
                val currentViewport = viewport
                val currentBitmap = bitmap
                if (currentViewport != null && currentBitmap != null) {
                    val displaySize = imageDisplaySize(
                        imageWidthPx = currentBitmap.width.toFloat(),
                        imageHeightPx = currentBitmap.height.toFloat(),
                        viewportWidthPx = surfaceSize.width.toFloat(),
                        viewportHeightPx = surfaceSize.height.toFloat(),
                        zoom = currentViewport.zoom,
                    )
                    drawImage(
                        image = currentBitmap,
                        dstOffset = IntOffset(currentViewport.leftPx.roundToInt(), currentViewport.topPx.roundToInt()),
                        dstSize = IntSize(displaySize.widthPx.roundToInt(), displaySize.heightPx.roundToInt()),
                    )
                    drawRect(
                        color = Color(0x14000000),
                        topLeft = Offset(currentViewport.leftPx, currentViewport.topPx),
                        size = Size(displaySize.widthPx, displaySize.heightPx),
                        style = Stroke(width = 1.dp.toPx()),
                    )
                }
            }
        }

        SituationStatusBadge(
            ownship = ownship,
            modifier = Modifier
                .align(Alignment.TopEnd)
                .padding(top = ThumbGap, end = ThumbGap),
        )

        ChartViewerSelectors(
            modifier = Modifier.align(Alignment.TopStart),
            currentPage = page,
            airports = airports,
            selectedAirport = selectedAirport,
            selectedChart = selectedChart,
            folderOpen = folderOpen,
            pageTrayOpen = pageTrayOpen,
            airportTrayOpen = airportTrayOpen,
            chartTrayOpen = chartTrayOpen,
            onTogglePageTray = {
                pageTrayOpen = !pageTrayOpen
                airportTrayOpen = false
                chartTrayOpen = false
            },
            onSelectPage = {
                onSelectPage(it)
                pageTrayOpen = false
                airportTrayOpen = false
                chartTrayOpen = false
            },
            onToggleAirportTray = {
                airportTrayOpen = !airportTrayOpen
                pageTrayOpen = false
                chartTrayOpen = false
            },
            onToggleChartTray = {
                chartTrayOpen = !chartTrayOpen
                pageTrayOpen = false
                airportTrayOpen = false
            },
            onToggleFolder = {
                onFolderOpenChange(!folderOpen)
                pageTrayOpen = false
                airportTrayOpen = false
                chartTrayOpen = false
            },
            onSelectAirport = {
                onSelectAirport(it)
                airportTrayOpen = false
            },
            onSelectChart = {
                onSelectChart(it)
                chartTrayOpen = false
            },
        )

        if (trayOpen) {
            Scrim {
                pageTrayOpen = false
                airportTrayOpen = false
                chartTrayOpen = false
            }
        }

        NavElementDock(
            navElement = navElement,
            onClick = onOpenPlan,
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .padding(bottom = ThumbGap),
        )

        DebugDock(
            open = debugPanelOpen,
            onToggle = { debugPanelOpen = !debugPanelOpen },
            modifier = Modifier.align(Alignment.BottomStart),
        ) {
            Text("up: $uptimeLabel", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
            Text(viewport?.let { "z${String.format("%.2f", it.zoom)}" } ?: "viewport (none)", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
        }

    }
}

@Composable
private fun MapTopLeftControls(
    modifier: Modifier = Modifier,
    currentPage: AppPage,
    pageTrayOpen: Boolean,
    onTogglePageTray: () -> Unit,
    onSelectPage: (AppPage) -> Unit,
    selectedLabel: String,
    trayOptions: List<ChartTrayOption>,
    trayOpen: Boolean,
    onToggle: () -> Unit,
) {
    Row(
        modifier = modifier.padding(ThumbGap),
        horizontalArrangement = Arrangement.spacedBy(ThumbGap),
        verticalAlignment = Alignment.Top,
    ) {
        MenuDock(
            launcherLabel = PageOptions.firstOrNull { it.page == currentPage }?.launcherLabel ?: "CHT",
            open = pageTrayOpen,
            onToggle = onTogglePageTray,
            style = MenuDockStyle.Compact,
            options = PageOptions.map { option ->
                MenuDockOption(option.page.name, option.label, active = option.page == currentPage) { onSelectPage(option.page) }
            },
        )
        MenuDock(
            launcherLabel = selectedLabel,
            open = trayOpen,
            onToggle = onToggle,
            style = MenuDockStyle.Compact,
            options = trayOptions.map { option ->
                MenuDockOption(option.id, option.label, active = option.launcherLabel == selectedLabel, enabled = option.available) { option.select?.invoke() }
            },
        )
    }
}

@Composable
private fun ChartViewerSelectors(
    modifier: Modifier = Modifier,
    currentPage: AppPage,
    airports: List<ChartAirport>,
    selectedAirport: ChartAirport?,
    selectedChart: ChartAsset?,
    folderOpen: Boolean,
    pageTrayOpen: Boolean,
    airportTrayOpen: Boolean,
    chartTrayOpen: Boolean,
    onTogglePageTray: () -> Unit,
    onSelectPage: (AppPage) -> Unit,
    onToggleAirportTray: () -> Unit,
    onToggleChartTray: () -> Unit,
    onToggleFolder: () -> Unit,
    onSelectAirport: (String) -> Unit,
    onSelectChart: (String) -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val trayOpen = pageTrayOpen || airportTrayOpen || chartTrayOpen
    Row(
        modifier = modifier.padding(ThumbGap),
        horizontalArrangement = Arrangement.spacedBy(ThumbGap),
        verticalAlignment = Alignment.Top,
    ) {
        MenuDock(
            launcherLabel = PageOptions.firstOrNull { it.page == currentPage }?.launcherLabel ?: "PLT",
            open = pageTrayOpen,
            onToggle = onTogglePageTray,
            style = MenuDockStyle.Compact,
            options = PageOptions.map { option ->
                MenuDockOption(option.page.name, option.label, active = option.page == currentPage) { onSelectPage(option.page) }
            },
        )

        MenuDock(
            launcherLabel = selectedAirport?.label ?: "---",
            open = airportTrayOpen,
            onToggle = onToggleAirportTray,
            style = MenuDockStyle.PlateAirport,
            options = airports.map { airport ->
                MenuDockOption(airport.id, airport.label, active = airport.id == selectedAirport?.id) { onSelectAirport(airport.id) }
            },
        )

        MenuDock(
            launcherLabel = selectedChart?.label ?: "---",
            open = chartTrayOpen,
            onToggle = onToggleChartTray,
            style = MenuDockStyle.PlateWide,
            options = (selectedAirport?.charts ?: emptyList()).map { chart ->
                MenuDockOption(
                    chart.id,
                    chart.label,
                    active = chart.id == selectedChart?.id,
                    accentColor = plateFolderColor(uiTheme, chart.folderCategory),
                ) { onSelectChart(chart.id) }
            },
        )

        CompactSquareButton(
            label = "FLDR",
            modifier = Modifier.size(ThumbSize),
            enabled = !trayOpen && !folderOpen,
            onClick = onToggleFolder,
        )
    }
}

@Composable
private fun PlateFolderGrid(
    modifier: Modifier = Modifier,
    charts: List<ChartAsset>,
    selectedChartId: String?,
    uiTheme: UiTheme,
    onSelectChart: (String) -> Unit,
) {
    val context = LocalContext.current
    LazyVerticalGrid(
        columns = GridCells.Adaptive(minSize = PlateFolderTileWidth),
        modifier = modifier.padding(top = ThumbSize + (ThumbGap * 2f), start = FolderThumbGutter, end = FolderThumbGutter, bottom = FolderThumbGutter),
        horizontalArrangement = Arrangement.spacedBy(FolderThumbGutter),
        verticalArrangement = Arrangement.spacedBy(FolderThumbGutter),
    ) {
        lazyGridItems(charts, key = { it.id }) { chart ->
            val thumbnail by produceState<androidx.compose.ui.graphics.ImageBitmap?>(initialValue = null, chart.id, chart.thumbnailSourceAssetPath) {
                value = chart.thumbnailSourceAssetPath?.let {
                    withContext(Dispatchers.IO) {
                        runCatching {
                            val bytes = ChartPackages.loadThumbnailBytes(context, chart) ?: return@runCatching null
                            BitmapFactory.decodeByteArray(bytes, 0, bytes.size)?.asImageBitmap()
                        }.getOrNull()
                    }
                }
            }
            Surface(
                modifier = Modifier
                    .width(PlateFolderTileWidth)
                    .height(PlateFolderTileHeight)
                    .border(
                        width = if (chart.id == selectedChartId) 2.dp else 1.dp,
                        color = if (chart.id == selectedChartId) MaterialTheme.colorScheme.primary else Color(0x26132129),
                        shape = RoundedCornerShape(ThumbRadius),
                    )
                    .clickable { onSelectChart(chart.id) },
                shape = RoundedCornerShape(ThumbRadius),
                color = uiTheme.plateFolder.thumbnailBg,
                shadowElevation = 2.dp,
            ) {
                Box {
                    if (thumbnail != null) {
                        androidx.compose.foundation.Image(
                            bitmap = thumbnail!!,
                            contentDescription = null,
                            modifier = Modifier.fillMaxSize(),
                        )
                    }
                    Box(
                        modifier = Modifier
                            .align(Alignment.BottomStart)
                            .fillMaxWidth()
                            .background(plateFolderColor(uiTheme, chart.folderCategory))
                            .padding(horizontal = 6.dp, vertical = 5.dp),
                    ) {
                        Text(
                            text = chart.label,
                            style = MaterialTheme.typography.labelSmall,
                            color = Color(0xFFFFF7EF),
                            maxLines = 3,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun MenuDock(
    modifier: Modifier = Modifier,
    launcherLabel: String,
    open: Boolean,
    onToggle: () -> Unit,
    style: MenuDockStyle,
    options: List<MenuDockOption>,
) {
    val density = LocalDensity.current
    val configuration = LocalConfiguration.current
    var anchorTopPx by remember { mutableStateOf(0f) }
    val screenHeightPx = with(density) { configuration.screenHeightDp.dp.toPx() }
    val trayOffsetPx = with(density) { (ThumbSize + ThumbGap).toPx() }
    val trayBottomMarginPx = with(density) { ThumbGap.toPx() }
    val trayMaxHeight = with(density) {
        ((screenHeightPx - anchorTopPx - trayOffsetPx - trayBottomMarginPx).coerceAtLeast(ThumbSize.toPx())).toDp()
    }
    val launcherAccentColor = options.firstOrNull { it.active }?.accentColor
    Box(
        modifier = modifier
            .width(style.buttonWidth)
            .height(ThumbSize)
            .wrapContentSize(unbounded = true, align = Alignment.TopStart),
    ) {
        CompactSquareButton(
            label = launcherLabel,
            maxLines = style.launcherMaxLines,
            enabled = true,
            accentColor = launcherAccentColor,
            modifier = Modifier
                .width(style.buttonWidth)
                .height(ThumbSize)
                .align(Alignment.TopStart)
                .onGloballyPositioned { coordinates ->
                    anchorTopPx = coordinates.boundsInWindow().top
                },
            onClick = onToggle,
        )
        if (open) {
            Popup(
                offset = IntOffset(0, trayOffsetPx.roundToInt()),
            ) {
                MenuPanel(
                    modifier = Modifier
                        .width(style.trayWidth)
                        .heightIn(max = trayMaxHeight),
                ) {
                    LazyColumn(verticalArrangement = Arrangement.spacedBy(3.dp)) {
                        lazyColumnItems(options) { option ->
                            MenuPanelRow(
                                label = option.label,
                                active = option.active,
                                enabled = option.enabled,
                                accentColor = option.accentColor,
                                width = style.trayWidth,
                                onSelect = option.onSelect,
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun MenuPanel(
    modifier: Modifier = Modifier,
    width: Dp = Dp.Unspecified,
    content: @Composable ColumnScope.() -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    Card(
        modifier = modifier.then(if (width != Dp.Unspecified) Modifier.width(width) else Modifier),
        shape = RoundedCornerShape(ThumbRadius + 2.dp),
        colors = CardDefaults.cardColors(
            containerColor = uiTheme.controls.panelBg,
            contentColor = uiTheme.controls.panelFg,
        ),
        border = BorderStroke(2.dp, uiTheme.controls.panelBorder),
    ) {
        Column(
            modifier = Modifier.padding(3.dp),
            verticalArrangement = Arrangement.spacedBy(3.dp),
            content = content,
        )
    }
}

@Composable
private fun MenuPanelRow(
    label: String,
    active: Boolean,
    enabled: Boolean,
    accentColor: Color? = null,
    width: Dp = Dp.Unspecified,
    onSelect: () -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val rowShape = RoundedCornerShape(ThumbRadius)
    val rowBackground = when {
        !enabled -> uiTheme.controls.panelBg
        active -> lerp(uiTheme.controls.buttonBg, Color.White, 0.18f)
        else -> uiTheme.controls.buttonBg
    }
    val rowTextColor = when {
        !enabled -> uiTheme.controls.panelMuted.copy(alpha = 0.7f)
        else -> uiTheme.controls.buttonFg
    }
    Box(
        modifier = Modifier
            .then(if (width != Dp.Unspecified) Modifier.width(width) else Modifier.fillMaxWidth())
            .height(ThumbSize)
            .clip(rowShape)
            .background(rowBackground)
            .clickable(
                enabled = enabled,
                indication = null,
                interactionSource = remember { MutableInteractionSource() },
            ) {
                onSelect()
            },
        contentAlignment = Alignment.CenterStart,
    ) {
        if (accentColor != null) {
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(ThumbSize / 2f)
                    .align(Alignment.BottomStart)
                    .background(accentColor.copy(alpha = if (enabled) 1f else 0.45f)),
            )
        }
        Text(
            text = label,
            modifier = Modifier.padding(horizontal = 12.dp),
            style = MaterialTheme.typography.labelLarge,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
            color = rowTextColor,
        )
    }
}

@Composable
private fun NavElementDock(
    navElement: NavElementUiView?,
    modifier: Modifier = Modifier,
    onClick: (() -> Unit)? = null,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val shape = RoundedCornerShape(ThumbRadius * 0.9f)
    Surface(
        modifier =
            modifier
                .width(ThumbSize * 3f)
                .height(ThumbSize * 0.67f)
                .then(
                    if (onClick != null) {
                        Modifier.clickable(
                            indication = null,
                            interactionSource = remember { MutableInteractionSource() },
                        ) { onClick() }
                    } else {
                        Modifier
                    },
                ),
        shape = shape,
        color = uiTheme.controls.panelFg,
        contentColor = Color.White,
        border = BorderStroke(1.dp, uiTheme.controls.panelBorder),
        shadowElevation = 6.dp,
    ) {
        Column(
            modifier = Modifier.fillMaxSize().padding(horizontal = ThumbSize * 0.14f),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.Center,
        ) {
            Box(
                modifier = Modifier.weight(1f).fillMaxWidth(),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    text = navElement?.activeLegSummary.orEmpty(),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    style = MaterialTheme.typography.labelSmall,
                    fontWeight = FontWeight.Bold,
                    color = Color.White,
                )
            }
            Canvas(
                modifier = Modifier
                    .weight(1f)
                    .fillMaxWidth(),
            ) {
                val unit = size.width / 4.5f
                val centerX = 2.25f * unit
                val baselineY = size.height * 0.5f
                val dotXs = listOf(0.25f, 1.25f, 3.25f, 4.25f).map { it * unit }
                val dotRadius = unit * 0.04375f
                val pointerPosition = navElement?.cdiIndicatorDots
                val fullScaleDots = 2f
                val offscaleDots = 2.1f
                val clampedPointerPosition = pointerPosition?.coerceIn(-fullScaleDots, fullScaleDots)
                val pointerX = clampedPointerPosition?.let { (it + 2.25f) * unit }
                val offscaleDirection = when {
                    pointerPosition == null || abs(pointerPosition) <= offscaleDots -> null
                    pointerPosition > 0f -> 1
                    else -> -1
                }
                val offscaleReadout = navElement?.cdiOffscaleReadout
                val offscaleReadoutDotIndex = when {
                    offscaleReadout == null -> null
                    offscaleDirection == 1 -> 2
                    offscaleDirection == -1 -> 1
                    else -> null
                }
                val triangleHalfWidth = unit * 0.25f
                drawPath(
                    path =
                        Path().apply {
                            moveTo(centerX - triangleHalfWidth, size.height)
                            lineTo(centerX + triangleHalfWidth, size.height)
                            lineTo(centerX, 0f)
                            close()
                        },
                    color = Color.White,
                )
                dotXs.forEachIndexed { index, x ->
                    if (index == offscaleReadoutDotIndex) {
                        return@forEachIndexed
                    }
                    drawCircle(
                        color = Color.White,
                        radius = dotRadius,
                        center = Offset(x, baselineY),
                        style = Stroke(width = unit * 0.05f),
                    )
                }
                if (offscaleDirection != null) {
                    val baseX = (2.25f + offscaleDirection * offscaleDots) * unit
                    val tipX = if (offscaleDirection > 0) size.width else 0f
                    drawPath(
                        path =
                            Path().apply {
                                moveTo(baseX, size.height * 0.18f)
                                lineTo(baseX, size.height * 0.82f)
                                lineTo(tipX, baselineY)
                                close()
                            },
                        color = uiTheme.controls.cdiPointer,
                    )
                    if (offscaleReadout != null && offscaleReadoutDotIndex != null) {
                        val textPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
                            color = uiTheme.controls.cdiPointer.toArgb()
                            textAlign = Paint.Align.CENTER
                            textSize = size.height * 0.8f
                            typeface = Typeface.create(Typeface.DEFAULT, Typeface.BOLD)
                        }
                        val strokePaint = Paint(textPaint).apply {
                            style = Paint.Style.STROKE
                            strokeWidth = size.height * 0.14f
                            strokeJoin = Paint.Join.ROUND
                            color = Color(0xD1081218).toArgb()
                        }
                        val textY = baselineY - (textPaint.descent() + textPaint.ascent()) / 2f
                        drawContext.canvas.nativeCanvas.drawText(
                            offscaleReadout,
                            dotXs[offscaleReadoutDotIndex],
                            textY,
                            strokePaint,
                        )
                        drawContext.canvas.nativeCanvas.drawText(
                            offscaleReadout,
                            dotXs[offscaleReadoutDotIndex],
                            textY,
                            textPaint,
                        )
                    }
                } else if (pointerX != null) {
                    drawLine(
                        color = uiTheme.controls.cdiPointer,
                        start = Offset(pointerX, 0f),
                        end = Offset(pointerX, size.height),
                        strokeWidth = unit * 0.14f,
                        cap = StrokeCap.Round,
                    )
                }
            }
        }
    }
}

@Composable
private fun PlaybackWidget(
    uiSession: NativeUiSession,
    playbackUiState: PlaybackUiState,
    sourcePath: String,
    onSourcePathChange: (String) -> Unit,
    onSnapshotChange: (UiSessionSnapshot) -> Unit,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    val devServerBaseUrl = remember(context) {
        loadAndroidDevServerBaseUrl(context.applicationContext)
    }
    val scope = rememberCoroutineScope()
    var isBusy by remember { mutableStateOf(false) }
    var scrubCursorSeconds by remember { mutableStateOf<Double?>(null) }
    var seekJob by remember { mutableStateOf<Job?>(null) }
    val durationSeconds = playbackUiState.durationSeconds.coerceAtLeast(0.0)
    val committedCursorSeconds = playbackUiState.cursorSeconds.coerceIn(0.0, durationSeconds.takeIf { it > 0.0 } ?: 0.0)
    val cursorSeconds = (scrubCursorSeconds ?: committedCursorSeconds).coerceIn(0.0, durationSeconds.takeIf { it > 0.0 } ?: 0.0)
    val summary = playbackUiState.titleLabel
    val panelShape = RoundedCornerShape(ThumbRadius * 0.9f)
    Surface(
        modifier =
            modifier
                .widthIn(min = ThumbSize * 2.8f, max = ThumbSize * 5.2f),
        shape = panelShape,
        color = Color(0xF0FCF8F1),
        contentColor = Color(0xFF132129),
        border = BorderStroke(1.dp, Color(0x334E626C)),
        shadowElevation = 6.dp,
    ) {
        Box {
            Box(
                modifier =
                    Modifier
                        .matchParentSize()
                        .consumePointerGestures(),
            )
            Column(
                modifier = Modifier.padding(ThumbSize * 0.12f),
                verticalArrangement = Arrangement.spacedBy(ThumbSize * 0.08f),
            ) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(ThumbSize * 0.08f),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        text = summary,
                        modifier = Modifier.weight(1f),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        style = MaterialTheme.typography.labelSmall,
                        fontWeight = FontWeight.Bold,
                        color = Color(0xFF132129),
                    )
                    Text(
                        text = "${String.format("%.1f", playbackUiState.rate)}x",
                        style = MaterialTheme.typography.labelSmall,
                        fontWeight = FontWeight.Bold,
                        color = Color(0xFF52656D),
                    )
                }
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(ThumbSize * 0.08f),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    BasicTextField(
                        value = sourcePath,
                        onValueChange = onSourcePathChange,
                        singleLine = true,
                        textStyle =
                            MaterialTheme.typography.labelSmall.copy(
                                color = Color(0xFF132129),
                                fontSize = 11.sp,
                            ),
                        modifier =
                            Modifier
                                .weight(1f)
                                .height(ThumbSize * 0.42f)
                                .clip(RoundedCornerShape(ThumbRadius * 0.55f))
                                .background(Color.White)
                                .border(1.dp, Color(0x24132129), RoundedCornerShape(ThumbRadius * 0.55f))
                                .padding(horizontal = ThumbSize * 0.1f, vertical = ThumbSize * 0.11f),
                    )
                    PlaybackSmallButton(
                        label = "LOAD",
                        enabled = !isBusy && sourcePath.isNotBlank(),
                        onClick = {
                            scope.launch {
                                isBusy = true
                                try {
                                    val traceJson =
                                        withContext(Dispatchers.IO) {
                                            URL(resolvePlaybackTraceUrl(sourcePath, devServerBaseUrl)).readText()
                                        }
                                    onSnapshotChange(uiSession.loadPlaybackTrace(sourcePath, traceJson))
                                } catch (error: Throwable) {
                                    Log.e("AerobagPlayback", "trace load failed: $sourcePath", error)
                                } finally {
                                    isBusy = false
                                }
                            }
                        },
                    )
                }
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(ThumbSize * 0.08f),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    PlaybackSmallButton(
                        label = "",
                        icon = if (playbackUiState.status == PlaybackStatus.Playing) PlaybackButtonIcon.Pause else PlaybackButtonIcon.Play,
                        enabled = playbackUiState.status != PlaybackStatus.Empty,
                        onClick = {
                            scope.launch {
                                runCatching {
                                    if (playbackUiState.status == PlaybackStatus.Playing) {
                                        uiSession.pausePlayback(System.currentTimeMillis().toDouble())
                                    } else {
                                        uiSession.playPlayback(System.currentTimeMillis().toDouble())
                                    }
                                }.onSuccess(onSnapshotChange)
                                    .onFailure { Log.e("AerobagPlayback", "play/pause failed", it) }
                            }
                        },
                    )
                    Text(
                        text = "SPD",
                        style = MaterialTheme.typography.labelSmall,
                        fontWeight = FontWeight.Bold,
                        color = Color(0xFF52656D),
                    )
                    PlaybackRateRail(
                        value = playbackUiState.rate.toFloat().coerceIn(0.25f, 11f),
                        enabled = playbackUiState.status != PlaybackStatus.Empty,
                        modifier = Modifier.weight(1f).height(ThumbSize * 0.42f),
                        onValueChange = { nextRate ->
                            scope.launch {
                                runCatching {
                                    uiSession.setPlaybackRate(nextRate.toDouble(), System.currentTimeMillis().toDouble())
                                }.onSuccess(onSnapshotChange)
                                    .onFailure { Log.e("AerobagPlayback", "rate change failed", it) }
                            }
                        },
                    )
                }
                PlaybackOverview(
                    playbackUiState = playbackUiState,
                    cursorSeconds = cursorSeconds,
                    durationSeconds = durationSeconds,
                    onScrub = { nextCursorSeconds, finished ->
                        scrubCursorSeconds = nextCursorSeconds
                        seekJob?.cancel()
                        seekJob = scope.launch {
                            runCatching {
                                uiSession.seekPlayback(nextCursorSeconds, System.currentTimeMillis().toDouble())
                            }.onSuccess {
                                onSnapshotChange(it)
                                if (finished) {
                                    scrubCursorSeconds = null
                                }
                            }.onFailure {
                                if (it !is CancellationException) {
                                    Log.e("AerobagPlayback", "seek failed", it)
                                }
                            }
                        }
                    },
                )
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                ) {
                    Text(playbackUiState.cursorLabel, style = MaterialTheme.typography.labelSmall, fontWeight = FontWeight.Bold, color = Color(0xFF52656D))
                    Text(playbackUiState.durationLabel, style = MaterialTheme.typography.labelSmall, fontWeight = FontWeight.Bold, color = Color(0xFF52656D))
                }
            }
        }
    }
}

@Composable
private fun PlaybackSmallButton(
    label: String,
    enabled: Boolean,
    onClick: () -> Unit,
    icon: PlaybackButtonIcon? = null,
) {
    Surface(
        modifier =
            Modifier
                .height(ThumbSize * 0.42f)
                .then(if (icon == null) Modifier.widthIn(min = ThumbSize * 0.86f) else Modifier.width(ThumbSize * 0.42f))
                .then(
                    if (enabled) {
                        Modifier.clickable(
                            indication = null,
                            interactionSource = remember { MutableInteractionSource() },
                        ) { onClick() }
                    } else {
                        Modifier.alpha(0.45f)
                    },
                ),
        shape = RoundedCornerShape(ThumbRadius * 0.55f),
        color = Color(0xFF132129),
        contentColor = Color.White,
        border = BorderStroke(1.dp, Color(0x24132129)),
    ) {
        Box(
            contentAlignment = Alignment.Center,
            modifier = if (icon == null) Modifier.padding(horizontal = ThumbSize * 0.14f) else Modifier.fillMaxSize(),
        ) {
            if (icon == null) {
                Text(
                    text = label,
                    style = MaterialTheme.typography.labelSmall,
                    fontWeight = FontWeight.ExtraBold,
                    color = Color.White,
                )
            } else {
                PlaybackButtonIconCanvas(icon = icon)
            }
        }
    }
}

private enum class PlaybackButtonIcon {
    Play,
    Pause,
}

@Composable
private fun PlaybackButtonIconCanvas(icon: PlaybackButtonIcon) {
    Canvas(modifier = Modifier.fillMaxSize()) {
        when (icon) {
            PlaybackButtonIcon.Play -> {
                val iconWidth = size.width * 0.34f
                val iconHeight = size.height * 0.44f
                val left = size.width * 0.5f - iconWidth * 0.38f
                val top = size.height * 0.5f - iconHeight * 0.5f
                val path =
                    Path().apply {
                        moveTo(left, top)
                        lineTo(left, top + iconHeight)
                        lineTo(left + iconWidth, top + iconHeight * 0.5f)
                        close()
                    }
                drawPath(path = path, color = Color.White)
            }
            PlaybackButtonIcon.Pause -> {
                val barWidth = size.width * 0.12f
                val barHeight = size.height * 0.44f
                val gap = size.width * 0.08f
                val left = size.width * 0.5f - gap * 0.5f - barWidth
                val top = size.height * 0.5f - barHeight * 0.5f
                drawRoundRect(
                    color = Color.White,
                    topLeft = Offset(left, top),
                    size = Size(barWidth, barHeight),
                    cornerRadius = androidx.compose.ui.geometry.CornerRadius(barWidth * 0.28f, barWidth * 0.28f),
                )
                drawRoundRect(
                    color = Color.White,
                    topLeft = Offset(left + barWidth + gap, top),
                    size = Size(barWidth, barHeight),
                    cornerRadius = androidx.compose.ui.geometry.CornerRadius(barWidth * 0.28f, barWidth * 0.28f),
                )
            }
        }
    }
}

private fun Modifier.consumePointerGestures(): Modifier =
    pointerInput(Unit) {
        awaitEachGesture {
            while (true) {
                val event = awaitPointerEvent()
                event.changes.forEach { it.consume() }
                if (event.changes.none { it.pressed }) {
                    break
                }
            }
        }
    }

@Composable
private fun PlaybackRateRail(
    value: Float,
    enabled: Boolean,
    modifier: Modifier = Modifier,
    onValueChange: (Float) -> Unit,
) {
    val shape = RoundedCornerShape(ThumbRadius * 0.55f)
    var railSize by remember { mutableStateOf(IntSize.Zero) }
    fun rateForX(x: Float): Float {
        val width = railSize.width.toFloat().coerceAtLeast(1f)
        val ratio = x.coerceIn(0f, width) / width
        val rawRate = 0.25f + ratio * (11f - 0.25f)
        return (kotlin.math.round(rawRate / 0.25f) * 0.25f).coerceIn(0.25f, 11f)
    }
    Surface(
        modifier =
            modifier
                .clip(shape)
                .background(Color.White)
                .border(1.dp, Color(0x24132129), shape)
                .alpha(if (enabled) 1f else 0.45f)
                .onSizeChanged { railSize = it }
                .pointerInput(enabled, railSize) {
                    awaitEachGesture {
                        var activePointer: PointerId? = null
                        while (true) {
                            val event = awaitPointerEvent()
                            val change =
                                if (activePointer == null) {
                                    event.changes.firstOrNull { it.pressed }?.also { activePointer = it.id }
                                } else {
                                    event.changes.firstOrNull { it.id == activePointer }
                                } ?: break
                            if (enabled && railSize.width > 0) {
                                onValueChange(rateForX(change.position.x))
                            }
                            change.consume()
                            if (!change.pressed) {
                                break
                            }
                        }
                    }
                },
        shape = shape,
        color = Color.White,
        contentColor = Color(0xFF132129),
    ) {
        Canvas(modifier = Modifier.fillMaxSize().padding(horizontal = ThumbSize * 0.09f, vertical = ThumbSize * 0.12f)) {
            val centerY = size.height * 0.5f
            val trackHeight = 3.dp.toPx()
            val knobRadius = 6.dp.toPx()
            val progress = ((value - 0.25f) / (11f - 0.25f)).coerceIn(0f, 1f)
            val knobX = knobRadius + progress * (size.width - knobRadius * 2f).coerceAtLeast(1f)
            drawLine(
                color = Color(0x26132129),
                start = Offset(knobRadius, centerY),
                end = Offset(size.width - knobRadius, centerY),
                strokeWidth = trackHeight,
                cap = StrokeCap.Round,
            )
            drawLine(
                color = Color(0xFF132129),
                start = Offset(knobRadius, centerY),
                end = Offset(knobX, centerY),
                strokeWidth = trackHeight,
                cap = StrokeCap.Round,
            )
            drawCircle(
                color = Color(0xFF132129),
                radius = knobRadius,
                center = Offset(knobX, centerY),
            )
            drawCircle(
                color = Color.White,
                radius = knobRadius * 0.45f,
                center = Offset(knobX, centerY),
            )
        }
    }
}

@Composable
private fun PlaybackOverview(
    playbackUiState: PlaybackUiState,
    cursorSeconds: Double,
    durationSeconds: Double,
    onScrub: (Double, Boolean) -> Unit,
) {
    val shape = RoundedCornerShape(ThumbRadius * 0.45f)
    var overviewSize by remember { mutableStateOf(IntSize.Zero) }
    Box(
        modifier =
            Modifier
                .fillMaxWidth()
                .height(ThumbSize * 0.56f)
                .clip(shape)
                .background(Color(0xD1FFFFFF))
                .border(1.dp, Color(0x1F132129), shape)
                .onSizeChanged { overviewSize = it }
                .pointerInput(durationSeconds, overviewSize) {
                    awaitEachGesture {
                        var activePointer: PointerId? = null
                        val width = overviewSize.width.toFloat().coerceAtLeast(1f)
                        while (true) {
                            val event = awaitPointerEvent()
                            val change =
                                if (activePointer == null) {
                                    event.changes.firstOrNull { it.pressed }?.also { activePointer = it.id }
                                } else {
                                    event.changes.firstOrNull { it.id == activePointer }
                                } ?: break
                            val finished = !change.pressed
                            if (durationSeconds > 0.0 && overviewSize.width > 0) {
                                val nextCursorSeconds = (change.position.x.coerceIn(0f, width) / width.toDouble()) * durationSeconds
                                onScrub(nextCursorSeconds, finished)
                            }
                            change.consume()
                            if (finished) {
                                break
                            }
                        }
                    }
                },
    ) {
        Canvas(modifier = Modifier.fillMaxSize()) {
            val knobRadius = 7.dp.toPx()
            val usableWidth = (size.width - knobRadius * 2f).coerceAtLeast(1f)
            fun drawProfile(samples: List<Double?>, color: Color) {
                val usable = samples.mapIndexedNotNull { index, value -> value?.let { index to it } }
                if (usable.isEmpty()) {
                    return
                }
                val lastIndex = (samples.size - 1).coerceAtLeast(1)
                val path = Path()
                usable.forEachIndexed { pointIndex, (sampleIndex, value) ->
                    val x = knobRadius + (sampleIndex.toFloat() / lastIndex.toFloat()) * usableWidth
                    val y = size.height - (value.toFloat().coerceIn(0f, 1f) * size.height)
                    if (pointIndex == 0) {
                        path.moveTo(x, y)
                    } else {
                        path.lineTo(x, y)
                    }
                }
                drawPath(
                    path = path,
                    color = color,
                    style = Stroke(width = 1.6.dp.toPx(), cap = StrokeCap.Round),
                )
            }
            drawProfile(playbackUiState.altitudeProfileNorm, Color(0xCC0D6F67))
            drawProfile(playbackUiState.speedProfileNorm, Color(0xEBD45A7A))
            if (durationSeconds > 0.0) {
                playbackUiState.gapSpans.forEach { gap ->
                    val startRatio = (gap.startSeconds / durationSeconds).coerceIn(0.0, 1.0).toFloat()
                    val endRatio = (gap.endSeconds / durationSeconds).coerceIn(0.0, 1.0).toFloat()
                    val startX = knobRadius + startRatio * usableWidth
                    val endX = knobRadius + endRatio * usableWidth
                    if (endX > startX) {
                        drawRect(
                            color = Color(0x18132129),
                            topLeft = Offset(startX, 0f),
                            size = Size(endX - startX, size.height),
                        )
                        var hatchX = startX - size.height
                        val hatchSpacing = 6.dp.toPx()
                        clipRect(left = startX, top = 0f, right = endX, bottom = size.height) {
                            while (hatchX < endX) {
                                drawLine(
                                    color = Color(0x66132129),
                                    start = Offset(hatchX, size.height),
                                    end = Offset(hatchX + size.height, 0f),
                                    strokeWidth = 1.dp.toPx(),
                                )
                                hatchX += hatchSpacing
                            }
                        }
                    }
                }
            }
            val ratio = if (durationSeconds > 0.0) (cursorSeconds / durationSeconds).coerceIn(0.0, 1.0).toFloat() else 0f
            val cursorX = knobRadius + ratio * usableWidth
            drawLine(
                color = Color(0x85132129),
                start = Offset(cursorX, 0f),
                end = Offset(cursorX, size.height),
                strokeWidth = 1.dp.toPx(),
            )
            drawCircle(
                color = Color(0xFF132129),
                radius = knobRadius,
                center = Offset(cursorX, size.height - 1.dp.toPx()),
            )
            drawCircle(
                color = Color(0xF0FCF8F1),
                radius = knobRadius,
                center = Offset(cursorX, size.height - 1.dp.toPx()),
                style = Stroke(width = 1.5.dp.toPx()),
            )
        }
    }
}

private fun loadAndroidDevServerBaseUrl(context: Context): String =
    runCatching {
        context.assets.open("fixtures/android-dev-server-base-url.txt")
            .bufferedReader()
            .use { it.readText().trim() }
            .takeIf { it.isNotBlank() }
    }.getOrNull() ?: DefaultAndroidDevServerBaseUrl

private fun resolvePlaybackTraceUrl(sourcePath: String, devServerBaseUrl: String): String =
    when {
        sourcePath.startsWith("http://") || sourcePath.startsWith("https://") -> sourcePath
        sourcePath.startsWith("/") -> "$devServerBaseUrl$sourcePath"
        else -> "$devServerBaseUrl/$sourcePath"
    }

@Composable
private fun PlanHeaderRow() {
    Row(horizontalArrangement = Arrangement.spacedBy(PlanGridGap)) {
        PlanCell("Waypoint", Modifier.width(ThumbSize * 2.5f), isHeader = true)
        PlanCell("Dist (nm)", Modifier.weight(1f), isHeader = true)
        PlanCell("ETE (h:m)", Modifier.weight(1f), isHeader = true)
        PlanCell("Course (°)", Modifier.weight(1f), isHeader = true)
    }
}

private fun concretizedNavItemLabel(item: ConcretizedNavItem): String = when (item) {
    is ConcretizedNavItem.Waypoint -> navRefLabel(item.navRef)
    is ConcretizedNavItem.Discontinuity -> item.label
}

private fun structuredComponentLabel(component: RouteComponentUiView): String =
    if (component.kind == RouteComponentViewKind.Airway) {
        component.summary.substringBefore("(").trim()
    } else {
        component.summary
    }

private fun componentWaypointNavRef(component: RouteComponentUiView?): NavRef? {
    val item = component?.items?.firstOrNull()
    return if (item is ConcretizedNavItem.Waypoint) item.navRef else null
}

private fun navRefsEqual(left: NavRef?, right: NavRef?): Boolean = when {
    left == null || right == null -> false
    left is NavRef.Airport && right is NavRef.Airport -> left.code == right.code
    left is NavRef.Navaid && right is NavRef.Navaid -> left.code == right.code
    left is NavRef.Fix && right is NavRef.Fix -> left.code == right.code
    left is NavRef.LatLon && right is NavRef.LatLon -> left.lat == right.lat && left.lon == right.lon
    else -> false
}

private fun navRefSelectionKey(navRef: NavRef?): String = when (navRef) {
    is NavRef.Airport -> "airport:${navRef.code}"
    is NavRef.Navaid -> "navaid:${navRef.code}"
    is NavRef.Fix -> "fix:${navRef.code}"
    is NavRef.LatLon -> "latlon:${navRef.lat},${navRef.lon}"
    null -> "none"
}

private fun buildFlightPlanDisplayRows(planUiState: FlightPlanUiState): List<FlightPlanDisplayRow> =
    planUiState.displayRows.mapIndexed { index, row ->
        FlightPlanDisplayRow(
            id = when (row.rowKind) {
                FlightPlanDisplayRowKind.Waypoint -> if (row.depth == 0) "component:${row.componentIndex ?: index}" else "item:${row.componentIndex ?: "x"}:${row.label}:$index"
                FlightPlanDisplayRowKind.Group -> "group:${row.componentIndex ?: index}"
                FlightPlanDisplayRowKind.Discontinuity -> "disc:${row.componentIndex ?: "x"}:$index"
            },
            selectionKey = selectionKeyForDisplayRow(row, index),
            label = row.label,
            rowKind =
                when (row.rowKind) {
                    FlightPlanDisplayRowKind.Waypoint -> "waypoint"
                    FlightPlanDisplayRowKind.Group -> "group"
                    FlightPlanDisplayRowKind.Discontinuity -> "discontinuity"
                },
            componentKind = row.componentKind,
            componentIndex = row.componentIndex,
            legIndex = row.legIndex,
            distanceNm = row.distanceNm,
            courseDeg = row.courseDeg,
            chartAirportId = row.chartAirportId,
            navRef = row.navRef,
            symbolFeature = row.symbolFeature,
            depth = row.depth,
            active = row.active,
            canAddAirwayAfter = row.canAddAirwayAfter,
            canAddProcedureBefore = row.canAddProcedureBefore,
            canChangeAirway = row.canChangeAirway,
            canRemoveComponent = row.canRemoveComponent,
            canReorderComponent = row.canReorderComponent,
            canReorderUp = row.canReorderUp,
            canReorderDown = row.canReorderDown,
            actions = row.actions,
            startComponentIndex = row.startComponentIndex,
            endComponentIndex = row.endComponentIndex,
            originAnchor = row.originAnchor,
            destinationAnchor = row.destinationAnchor,
        )
    }

private fun selectionKeyForDisplayRow(row: FlightPlanDisplayRowUiView, index: Int): String =
    when (row.rowKind) {
        FlightPlanDisplayRowKind.Waypoint ->
            if (row.depth == 0) {
                "waypoint:${navRefSelectionKey(row.navRef)}"
            } else {
                "child:${row.componentKind?.name ?: "row"}:${navRefSelectionKey(row.navRef)}:$index"
            }
        FlightPlanDisplayRowKind.Group ->
            "group:${row.componentKind?.name ?: "group"}:${row.label}:${navRefSelectionKey(row.originAnchor)}:${navRefSelectionKey(row.destinationAnchor)}"
        FlightPlanDisplayRowKind.Discontinuity ->
            "disc:${row.componentKind?.name ?: "row"}:$index"
    }

private fun flightPlanActionLabel(actionId: FlightPlanRowActionId): String = when (actionId) {
    FlightPlanRowActionId.ActivateLeg -> "Activate Leg"
    FlightPlanRowActionId.Remove -> "Remove"
    FlightPlanRowActionId.InsertBefore -> "Insert Before"
    FlightPlanRowActionId.InsertAfter -> "Insert After"
    FlightPlanRowActionId.Reorder -> "Reorder"
    FlightPlanRowActionId.WaypointInfo -> "Waypoint Info"
    FlightPlanRowActionId.AddAirway -> "Add Airway"
    FlightPlanRowActionId.SelectProcedure -> "Select Procedure"
    FlightPlanRowActionId.Plates -> "Plates"
    FlightPlanRowActionId.ChangeAirway -> "Change Airway"
    FlightPlanRowActionId.RemoveAirway -> "Remove Airway"
    FlightPlanRowActionId.RemoveProcedure -> "Remove Procedure"
}

private fun buildFlightPlanDisplayBlocks(rows: List<FlightPlanDisplayRow>): List<FlightPlanDisplayBlock> {
    val blocks = mutableListOf<FlightPlanDisplayBlock>()
    var index = 0
    while (index < rows.size) {
        val row = rows[index]
        if (row.rowKind == "group") {
            val children = mutableListOf<Pair<Int, FlightPlanDisplayRow>>()
            var childIndex = index + 1
            while (childIndex < rows.size && rows[childIndex].depth > 0) {
                children += childIndex to rows[childIndex]
                childIndex += 1
            }
            blocks += FlightPlanDisplayBlock.Group(
                headerIndex = index,
                header = row,
                children = children,
            )
            index = childIndex
        } else {
            blocks += FlightPlanDisplayBlock.Single(index = index, row = row)
            index += 1
        }
    }
    return blocks
}

private fun navRefLabel(ref: NavRef): String = when (ref) {
    is NavRef.Airport -> ref.code
    is NavRef.Navaid -> ref.code
    is NavRef.Fix -> ref.code
    is NavRef.LatLon -> "${"%.3f".format(ref.lat)},${"%.3f".format(ref.lon)}"
}

private fun airportFuelMarkerPath(center: Offset, scale: Float): Path {
    val circleRadius = 12f * scale
    val tabHalf = 4f * scale
    val tabOuter = 17f * scale
    val arcJoin = 11.314f * scale
    val circleBounds = Rect(
        left = center.x - circleRadius,
        top = center.y - circleRadius,
        right = center.x + circleRadius,
        bottom = center.y + circleRadius,
    )
    return Path().apply {
        moveTo(center.x - tabHalf, center.y - tabOuter)
        lineTo(center.x + tabHalf, center.y - tabOuter)
        lineTo(center.x + tabHalf, center.y - arcJoin)
        arcTo(circleBounds, -70.5288f, 51.0576f, false)
        lineTo(center.x + tabOuter, center.y - tabHalf)
        lineTo(center.x + tabOuter, center.y + tabHalf)
        lineTo(center.x + arcJoin, center.y + tabHalf)
        arcTo(circleBounds, 19.4712f, 51.0576f, false)
        lineTo(center.x + tabHalf, center.y + tabOuter)
        lineTo(center.x - tabHalf, center.y + tabOuter)
        lineTo(center.x - tabHalf, center.y + arcJoin)
        arcTo(circleBounds, 109.4712f, 51.0576f, false)
        lineTo(center.x - tabOuter, center.y + tabHalf)
        lineTo(center.x - tabOuter, center.y - tabHalf)
        lineTo(center.x - arcJoin, center.y - tabHalf)
        arcTo(circleBounds, 199.4712f, 51.0576f, false)
        close()
    }
}

@Composable
private fun PlanWaypointSymbol(
    feature: net.jonh.aerobag.prototype.domain.NavSymbolFeature?,
    modifier: Modifier = Modifier,
) {
    if (feature == null) {
        return
    }
    Canvas(modifier = modifier.size(ThumbSize * 0.78f)) {
        val scale = size.minDimension / 40f
        val center = Offset(size.width / 2f, size.height / 2f)
        val fixMarkerStrokeColor = Color(0xB3081218)
        val fixMarkerFillColor = Color(0xFF39D9FF)
        val airportMarkerStrokeColor = Color(0xB3081218)
        val airportFillColor = Color(0xFFFF4FD8)
        val vorMarkerColor = Color(0xFF4AA3FF)
        val isAirport = feature.styleClass == "airport" || feature.kind.equals("airport", ignoreCase = true)
        val isVor = feature.styleClass == "nav" || feature.kind.contains("vor", ignoreCase = true)
        when {
            isAirport -> {
                val airportRadius = 12f * scale
                if (feature.fuelAvailable) {
                    val markerPath = airportFuelMarkerPath(center, scale)
                    drawPath(markerPath, airportFillColor)
                    drawPath(markerPath, airportMarkerStrokeColor, style = Stroke(width = 2f * scale))
                } else {
                    drawCircle(airportFillColor, radius = airportRadius, center = center)
                    drawCircle(airportMarkerStrokeColor, radius = airportRadius, center = center, style = Stroke(width = 2f * scale))
                }
                feature.longestRunwayHeadingTrueDeg?.let { heading ->
                    val runwayHalfLength = 8f * feature.runwayLengthRatio.coerceAtLeast(0.2).toFloat() * scale
                    rotate(heading.toFloat(), center) {
                        drawLine(
                            color = airportMarkerStrokeColor,
                            start = Offset(center.x, center.y - runwayHalfLength),
                            end = Offset(center.x, center.y + runwayHalfLength),
                            strokeWidth = 5f * scale,
                            cap = StrokeCap.Round,
                        )
                        drawLine(
                            color = Color.White,
                            start = Offset(center.x, center.y - runwayHalfLength),
                            end = Offset(center.x, center.y + runwayHalfLength),
                            strokeWidth = 3f * scale,
                            cap = StrokeCap.Round,
                        )
                    }
                }
            }

            isVor -> {
                val radius = 8f * scale
                val outerHex = polygonPath(vorHexPoints(center, radius))
                val band = vorBandPath(center, radius)
                drawPath(band, vorMarkerColor)
                drawPath(band, fixMarkerStrokeColor, style = Stroke(width = 1.6f * scale))
                drawPath(outerHex, fixMarkerStrokeColor, style = Stroke(width = 1.6f * scale))
            }

            else -> {
                val triangle = fixTrianglePath(center, 8f * scale)
                drawPath(triangle, fixMarkerFillColor)
                drawPath(triangle, fixMarkerStrokeColor, style = Stroke(width = 2.5f * scale))
            }
        }
    }
}

@Composable
private fun FlightPlanDataRow(
    row: FlightPlanDisplayRow,
    selected: Boolean,
    reorderOpen: Boolean = false,
    modifier: Modifier = Modifier,
    structuredRowBounds: MutableMap<String, Rect>? = null,
    onWaypointClick: () -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val childRow = row.depth > 0
    val targetIndent = ThumbSize * (row.depth * 0.5f)
    val indent by animateDpAsState(targetValue = targetIndent, label = "planRowIndent")
    val rowOpacity by animateFloatAsState(
        targetValue = if (reorderOpen && childRow) 0.72f else 1f,
        label = "planRowOpacity",
    )
    val labelScaleY by animateFloatAsState(
        targetValue = if (reorderOpen && childRow) 0.72f else 1f,
        label = "planRowLabelScaleY",
    )
    val cellHeight by animateDpAsState(
        targetValue = if (reorderOpen && childRow) ThumbSize * 0.34f else ThumbSize,
        label = "planRowCellHeight",
    )
    val rowBoundsModifier =
        if (structuredRowBounds != null) {
            rememberStructuredRowBounds(row.id, structuredRowBounds)
        } else {
            Modifier
        }
    val defaultButtonColor =
        when {
            row.rowKind == "group" -> uiTheme.controls.headerButton
            else -> uiTheme.controls.buttonBg
        }
    val selectedButtonColor =
        when {
            row.active -> Color(0xFF9B3A88)
            else -> Color(
                red = uiTheme.controls.buttonBg.red * 0.74f,
                green = uiTheme.controls.buttonBg.green * 0.74f,
                blue = uiTheme.controls.buttonBg.blue * 0.74f,
                alpha = uiTheme.controls.buttonBg.alpha,
            )
        }
    Row(modifier = modifier.then(rowBoundsModifier), horizontalArrangement = Arrangement.spacedBy(PlanGridGap)) {
        Box(modifier = Modifier.width(ThumbSize * 2.5f).height(cellHeight)) {
            CompactSquareButton(
                label = row.label,
                modifier =
                    Modifier
                        .height(cellHeight)
                        .width(ThumbSize * 2.5f - indent)
                        .align(Alignment.CenterEnd)
                        .alpha(rowOpacity),
                centered = false,
                textStartPadding = 10.dp,
                backgroundColor = defaultButtonColor,
                selected = selected,
                selectedColor = selectedButtonColor,
                textModifier =
                    Modifier
                        .padding(end = ThumbSize * 0.78f)
                        .graphicsLayer {
                            scaleY = labelScaleY
                            transformOrigin = TransformOrigin(0f, 0.5f)
                        },
                onClick = onWaypointClick,
            )
            PlanWaypointSymbol(
                feature = row.symbolFeature,
                modifier = Modifier
                    .align(Alignment.CenterEnd)
                    .padding(end = ThumbSize * 0.12f)
                    .alpha(rowOpacity),
            )
        }
        PlanCell(formatPlanDistance(row.distanceNm), Modifier.weight(1f), cellHeight = cellHeight, alpha = rowOpacity)
        PlanCell("—", Modifier.weight(1f), cellHeight = cellHeight, alpha = rowOpacity)
        PlanCell(formatPlanCourse(row.courseDeg), Modifier.weight(1f), cellHeight = cellHeight, alpha = rowOpacity)
    }
}

@Composable
private fun FlightPlanGroupBlock(
    header: FlightPlanDisplayRow,
    headerSelected: Boolean,
    reorderOpen: Boolean,
    structuredRowBounds: MutableMap<String, Rect>? = null,
    onHeaderClick: () -> Unit,
    children: List<Pair<Int, FlightPlanDisplayRow>>,
    selectedWaypointIndex: Int?,
    onChildClick: (Int) -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val groupOverhang = 8.dp
    val cornerRadius = ThumbRadius + 2.dp
    Column(
        verticalArrangement = Arrangement.spacedBy(PlanGridGap),
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = PlanGridGap / 2)
            .drawBehind {
                val overhangPx = groupOverhang.toPx()
                val radiusPx = cornerRadius.toPx()
                drawRoundRect(
                    color = uiTheme.controls.panelBg.copy(alpha = 0.82f),
                    topLeft = Offset(-overhangPx, 0f),
                    size = Size(size.width + overhangPx, size.height),
                    cornerRadius = androidx.compose.ui.geometry.CornerRadius(radiusPx, radiusPx),
                )
                drawRoundRect(
                    color = uiTheme.controls.panelBorder.copy(alpha = 0.95f),
                    topLeft = Offset(-overhangPx, 0f),
                    size = Size(size.width + overhangPx, size.height),
                    cornerRadius = androidx.compose.ui.geometry.CornerRadius(radiusPx, radiusPx),
                    style = Stroke(width = 2.dp.toPx()),
                )
            }
            .padding(top = 8.dp, end = 8.dp, bottom = 8.dp),
    ) {
        FlightPlanDataRow(
            row = header,
            selected = headerSelected,
            reorderOpen = reorderOpen,
            structuredRowBounds = structuredRowBounds,
            onWaypointClick = onHeaderClick,
        )
        children.forEach { (childIndex, childRow) ->
            FlightPlanDataRow(
                row = childRow,
                selected = selectedWaypointIndex == childIndex,
                reorderOpen = reorderOpen,
                structuredRowBounds = structuredRowBounds,
                onWaypointClick = { onChildClick(childIndex) },
            )
        }
    }
}

@Composable
private fun PlanCell(value: String, modifier: Modifier, isHeader: Boolean = false, cellHeight: Dp? = null, alpha: Float = 1f) {
    val uiTheme = LocalAerobagUiTheme.current
    val resolvedCellHeight = cellHeight ?: if (isHeader) ThumbSize * 0.5f else ThumbSize
    Box(
        modifier = modifier
            .height(resolvedCellHeight)
            .alpha(alpha)
            .background(uiTheme.controls.panelBg, RoundedCornerShape(ThumbRadius))
            .border(1.dp, uiTheme.controls.panelBorder, RoundedCornerShape(ThumbRadius))
            .padding(horizontal = 10.dp),
        contentAlignment = Alignment.CenterStart,
    ) {
        Text(
            value,
            style = if (isHeader) MaterialTheme.typography.labelMedium else MaterialTheme.typography.bodyMedium,
            color = if (isHeader) uiTheme.controls.panelMuted else uiTheme.controls.panelFg,
            fontWeight = if (isHeader) FontWeight.Bold else FontWeight.Medium,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
private fun DebugDock(
    open: Boolean,
    onToggle: () -> Unit,
    highlight: Boolean = false,
    expandAbove: Boolean = false,
    modifier: Modifier = Modifier,
    content: @Composable ColumnScope.() -> Unit,
) {
    Box(modifier = modifier.padding(ThumbGap)) {
        CompactSquareButton(
            label = "DBG",
            modifier = Modifier
                .align(if (expandAbove) Alignment.BottomEnd else Alignment.BottomStart)
                .size(ThumbSize),
            selected = highlight,
            selectedColor = Color(0xFFB85C00),
            onClick = onToggle,
        )

        AnimatedVisibility(
            visible = open,
            modifier = Modifier
                .align(if (expandAbove) Alignment.BottomEnd else Alignment.BottomStart)
                .padding(
                    bottom = ThumbSize + ThumbGap,
                ),
            enter = slideInVertically(initialOffsetY = { it / 3 }) + fadeIn(),
            exit = slideOutVertically(targetOffsetY = { it / 3 }) + fadeOut(),
        ) {
            Card(modifier = Modifier.width(ThumbSize * 4f)) {
                Column(
                    modifier = Modifier.padding(horizontal = 10.dp, vertical = 8.dp),
                    verticalArrangement = Arrangement.spacedBy(2.dp),
                    content = content,
                )
            }
        }
    }
}

private fun pageLabel(page: AppPage): String = PageOptions.firstOrNull { it.page == page }?.launcherLabel ?: page.name.uppercase()

@Composable
private fun rememberUptimeLabel(sessionStartElapsedMs: Long): String {
    val nowMs by produceState(initialValue = SystemClock.elapsedRealtime(), sessionStartElapsedMs) {
        while (true) {
            value = SystemClock.elapsedRealtime()
            delay(1000)
        }
    }
    return formatUptimeLabel(nowMs - sessionStartElapsedMs)
}

private fun formatUptimeLabel(elapsedMs: Long): String {
    val totalSeconds = (elapsedMs / 1000).coerceAtLeast(0)
    val hours = totalSeconds / 3600
    val minutes = (totalSeconds % 3600) / 60
    val seconds = totalSeconds % 60
    return if (hours > 0) {
        "%d:%02d:%02d".format(hours, minutes, seconds)
    } else {
        "%d:%02d".format(minutes, seconds)
    }
}

private fun formatSnapshot(snapshot: AppViewSnapshot): String {
    return formatSnapshot(snapshot, emptyMap())
}

private fun formatSnapshot(snapshot: AppViewSnapshot, chartLabelsById: Map<String, String>): String {
    val label = pageLabel(snapshot.page)
    if (snapshot.page == AppPage.Map) {
        val family = when (snapshot.selectedMapId.substringBefore(':')) {
            "sec" -> "SEC"
            "tac" -> "TAC"
            "enr-l" -> "IFR L"
            "enr-h" -> "IFR H"
            else -> ""
        }
        return if (family.isBlank()) label else "$label-$family"
    }
    if (snapshot.page != AppPage.Charts) {
        return label
    }
    if (snapshot.chartFolderOpen) {
        return "$label-FLDR"
    }
    val suffixSource = snapshot.selectedChartLabel
        .ifBlank { chartLabelsById[snapshot.selectedChartId].orEmpty() }
        .ifBlank { snapshot.selectedChartId }
    val suffix = suffixSource.takeLast(3).uppercase()
    return if (suffix.isBlank()) label else "$label-$suffix"
}

private fun formatPageStack(
    pageHistory: List<AppViewSnapshot>,
    currentPage: AppPage,
    selectedMapId: String = "",
    selectedAirportId: String = "",
    selectedChartId: String = "",
    selectedChartLabel: String = "",
    chartFolderOpen: Boolean = false,
    chartLabelsById: Map<String, String> = emptyMap(),
): String = (
    listOf(
        AppViewSnapshot(
        page = currentPage,
        selectedMapId = selectedMapId,
        mapViewport = MapViewportState(0.0, 0.0, 0.0),
        selectedAirportId = selectedAirportId,
        selectedChartId = selectedChartId,
        selectedChartLabel = selectedChartLabel,
        recentAirportIds = emptyList(),
        chartViewport = null,
        chartFolderOpen = chartFolderOpen,
    )
    ) + pageHistory.asReversed()
).joinToString(" > ") { formatSnapshot(it, chartLabelsById) }

@Composable
private fun ToolbarButton(label: String, modifier: Modifier = Modifier, onClick: () -> Unit) {
    CompactSquareButton(label = label, modifier = modifier.size(ThumbSize), onClick = onClick)
}

@Composable
private fun CompactSquareButton(
    label: String,
    modifier: Modifier = Modifier,
    maxLines: Int = 1,
    enabled: Boolean = true,
    selected: Boolean = false,
    backgroundColor: Color? = null,
    selectedColor: Color? = null,
    accentColor: Color? = null,
    centered: Boolean = true,
    textStartPadding: Dp = 0.dp,
    textModifier: Modifier = Modifier,
    onDisabledClick: (() -> Unit)? = null,
    onClick: () -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    Surface(
        modifier = modifier.then(
            if (enabled) {
                Modifier.pointerInput(onClick) {
                    awaitEachGesture {
                        var activePointer: PointerId? = null
                        var moved = false
                        while (true) {
                            val event = awaitPointerEvent()
                            if (activePointer == null) {
                                val downChange = event.changes.firstOrNull { it.pressed } ?: continue
                                activePointer = downChange.id
                                downChange.consume()
                                continue
                            }
                            val change = event.changes.firstOrNull { it.id == activePointer } ?: break
                            if (change.positionChanged()) {
                                moved = true
                            }
                            change.consume()
                            if (!change.pressed) {
                                if (!moved) {
                                    onClick()
                                }
                                break
                            }
                        }
                    }
                }
            } else {
                Modifier.pointerInput(onDisabledClick) {
                    if (onDisabledClick == null) {
                        return@pointerInput
                    }
                    awaitEachGesture {
                        var activePointer: PointerId? = null
                        var moved = false
                        while (true) {
                            val event = awaitPointerEvent()
                            if (activePointer == null) {
                                val downChange = event.changes.firstOrNull { it.pressed } ?: continue
                                activePointer = downChange.id
                                downChange.consume()
                                continue
                            }
                            val change = event.changes.firstOrNull { it.id == activePointer } ?: break
                            if (change.positionChanged()) {
                                moved = true
                            }
                            change.consume()
                            if (!change.pressed) {
                                if (!moved) {
                                    onDisabledClick()
                                }
                                break
                            }
                        }
                    }
                }
            }
        ),
        shape = RoundedCornerShape(ThumbRadius),
        color = if (selected) selectedColor ?: uiTheme.controls.buttonBg.copy(alpha = 0.9f) else backgroundColor ?: uiTheme.controls.buttonBg,
        contentColor = uiTheme.controls.buttonFg,
        shadowElevation = 2.dp,
    ) {
        Box(
            modifier = Modifier.fillMaxSize(),
            contentAlignment = if (centered) Alignment.Center else Alignment.CenterStart,
        ) {
            if (accentColor != null) {
                Box(
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(ThumbSize / 2f)
                        .align(Alignment.BottomStart)
                        .background(accentColor.copy(alpha = if (enabled) 1f else 0.45f)),
                )
            }
            Text(
                text = label,
                modifier = (if (centered) Modifier else Modifier.padding(start = textStartPadding, end = 8.dp)).then(textModifier),
                style = MaterialTheme.typography.labelSmall,
                maxLines = maxLines,
                overflow = TextOverflow.Clip,
            )
            if (!enabled) {
                Box(
                    modifier = Modifier
                        .matchParentSize()
                        .background(Color(0x42000000)),
                )
            }
        }
    }
}

@Composable
private fun Scrim(onDismiss: () -> Unit) {
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Color(0x3D0A1014))
            .clickable(
                indication = null,
                interactionSource = remember { MutableInteractionSource() },
            ) { onDismiss() },
    ) {}
}
