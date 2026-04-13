package net.jonh.aerobag.prototype

import android.content.Context
import android.graphics.BitmapFactory
import android.graphics.Paint
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
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.focusable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectTransformGestures
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
import androidx.compose.foundation.lazy.items as lazyColumnItems
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items as lazyGridItems
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
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
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.draw.alpha
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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import net.jonh.aerobag.prototype.domain.ChartAirport
import net.jonh.aerobag.prototype.domain.ChartAsset
import net.jonh.aerobag.prototype.domain.ChartPackages
import net.jonh.aerobag.prototype.domain.AppState
import net.jonh.aerobag.prototype.domain.AirwayEntryCandidate
import net.jonh.aerobag.prototype.domain.AirwayExitCandidate
import net.jonh.aerobag.prototype.domain.AirwaySuggestion
import net.jonh.aerobag.prototype.domain.ConcretizedNavItem
import net.jonh.aerobag.prototype.domain.FlightPlanUiMutation
import net.jonh.aerobag.prototype.domain.FlightPlanUiState
import net.jonh.aerobag.prototype.domain.LatLonPoint
import net.jonh.aerobag.prototype.domain.MapChartFamily
import net.jonh.aerobag.prototype.domain.MapOverlayQueryResult
import net.jonh.aerobag.prototype.domain.MapView
import net.jonh.aerobag.prototype.domain.MapViewportState
import net.jonh.aerobag.prototype.domain.NativeAppCoreAdapter
import net.jonh.aerobag.prototype.domain.NativeUiSession
import net.jonh.aerobag.prototype.domain.NavRef
import net.jonh.aerobag.prototype.domain.PointTilePayload
import net.jonh.aerobag.prototype.domain.ProcedureKind
import net.jonh.aerobag.prototype.domain.ProcedureOptions
import net.jonh.aerobag.prototype.domain.ProcedureSummary
import net.jonh.aerobag.prototype.domain.RouteComponentUiView
import net.jonh.aerobag.prototype.domain.RouteComponentViewKind
import net.jonh.aerobag.prototype.domain.ScreenPoint
import net.jonh.aerobag.prototype.domain.SectionalPackages
import net.jonh.aerobag.prototype.domain.SampleData
import net.jonh.aerobag.prototype.domain.Situation
import net.jonh.aerobag.prototype.domain.SituationPosition
import net.jonh.aerobag.prototype.domain.TileStorageKind
import net.jonh.aerobag.prototype.domain.UiTheme
import net.jonh.aerobag.prototype.domain.UiThemeLoader
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
import java.util.zip.ZipInputStream
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
private val VampsPosition = LatLon(47.3648944444444, -121.980275)
private val SituationRingSizesNm = listOf(0.25, 0.5, 0.8, 1.0, 1.5, 2.0, 3.0, 5.0, 8.0, 10.0, 15.0, 20.0, 30.0, 50.0, 100.0, 150.0, 200.0)

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

private data class FlightPlanDisplayRow(
    val id: String,
    val label: String,
    val rowKind: String,
    val componentKind: RouteComponentViewKind? = null,
    val componentIndex: Int? = null,
    val legIndex: Int? = null,
    val chartAirportId: String? = null,
    val navRef: NavRef? = null,
    val depth: Int = 0,
    val active: Boolean = false,
    val canAddAirwayAfter: Boolean = false,
    val canAddProcedureBefore: Boolean = false,
    val canChangeAirway: Boolean = false,
    val canRemoveComponent: Boolean = false,
    val canReorderComponent: Boolean = false,
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
    val entries: List<AirwayEntryCandidate>,
    val selectedEntryIndex: Int?,
    val exits: List<AirwayExitCandidate>,
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

private fun orderAirportsByRecency(
    airports: List<ChartAirport>,
    recentAirportIds: List<String>,
): List<ChartAirport> {
    val airportById = airports.associateBy { it.id }
    return recentAirportIds.mapNotNull(airportById::get)
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

private fun plateFolderCategoryOrder(category: String): Int = when (category) {
    "airport-diagram" -> 0
    "csup" -> 1
    "takeoff-mins" -> 2
    "approach" -> 3
    "departure" -> 4
    "star" -> 5
    else -> 6
}

private fun sortChartsForFolder(charts: List<ChartAsset>): List<ChartAsset> =
    charts.sortedWith(compareBy<ChartAsset>({ plateFolderCategoryOrder(it.folderCategory) }, { it.label }))

private fun plateFolderColor(uiTheme: UiTheme, category: String): Color =
    uiTheme.plateFolder.labelColors[category] ?: uiTheme.plateFolder.labelColors["other"] ?: Color(0xFF52656D)

private fun demoSituation(): Situation =
    Situation(
        position = SituationPosition.LatLon(VampsPosition.lat, VampsPosition.lon),
        orientationDeg = 135.0,
        speedKt = 105.0,
    )

private fun ensureNavDbPath(context: Context): String {
    val target = java.io.File(context.filesDir, "nav-db/main.db")
    val needsRefresh =
        !target.isFile ||
            runCatching {
                target.inputStream().use { input ->
                    input.readNBytes(16).decodeToString() != "SQLite format 3\u0000"
                }
            }.getOrDefault(true)
    if (needsRefresh) {
        target.parentFile?.mkdirs()
        context.assets.open("nav-db/main.db").use { input ->
            target.outputStream().use { output -> input.copyTo(output) }
        }
    }
    return target.absolutePath
}

private fun buildSeededDevPlan(
    adapter: NativeAppCoreAdapter,
    dbPath: String,
    plan: net.jonh.aerobag.prototype.domain.FlightPlan,
): FlightPlanUiMutation {
    return runCatching {
        val originAnchor = NavRef.Airport("KRNT")
        val destinationAnchor = NavRef.Airport("KUAO")
        val airwayName = "V23"
        val branches = adapter.loadAirwayBranches(dbPath, airwayName)
        val presentation =
            adapter.prepareAirwayPresentation(
                airwayName,
                branches,
                adapter.resolveNavRefPosition(dbPath, originAnchor),
                adapter.resolveNavRefPosition(dbPath, destinationAnchor),
            )
        val entryIndex = presentation.points.indexOfFirst { navRefLabel(it.navRef) == "SEA" }
        require(entryIndex >= 0) { "failed to seed V23 airway: SEA not found in presentation" }
        val exitIndex = presentation.points.indexOfFirst { navRefLabel(it.navRef) == "RAWER" }
        require(exitIndex > entryIndex) { "failed to seed V23 airway: RAWER not found in presentation" }
        val entry = AirwayEntryCandidate(
            airwayName = presentation.airwayName,
            branchKey = presentation.branchKey,
            branchPointIndex = presentation.points[entryIndex].branchPointIndex,
            sequence = presentation.points[entryIndex].sequence,
            navRef = presentation.points[entryIndex].navRef,
            distanceFromAnchorNm = 0.0,
            previousNavRef = presentation.points.getOrNull(entryIndex - 1)?.navRef,
            nextNavRef = presentation.points.getOrNull(entryIndex + 1)?.navRef,
        )
        val exit = AirwayExitCandidate(
            airwayName = presentation.airwayName,
            branchKey = presentation.branchKey,
            branchPointIndex = presentation.points[exitIndex].branchPointIndex,
            sequence = presentation.points[exitIndex].sequence,
            navRef = presentation.points[exitIndex].navRef,
            legOffsetFromEntry = exitIndex - entryIndex,
            isEntry = false,
            distanceFromTargetNm = null,
        )
        val selection =
            net.jonh.aerobag.prototype.domain.AirwayAutoSelection(
                airwayName = presentation.airwayName,
                branchKey = presentation.branchKey,
                entry = entry,
                exit = exit,
                originDistanceNm = 0.0,
                destinationDistanceNm = 0.0,
                totalAnchorDistanceNm = 0.0,
            )
        val airway =
            net.jonh.aerobag.prototype.domain.AirwaySegment(
                name = presentation.airwayName,
                branchKey = presentation.branchKey,
                entry = entry.navRef,
                exit = exit.navRef,
            )
        val resolvedLegs =
            presentation.points
                .subList(entryIndex, exitIndex + 1)
                .zipWithNext()
                .mapIndexed { index, (fromPoint, toPoint) ->
                    net.jonh.aerobag.prototype.domain.ResolvedLeg(
                        id = "airway:${presentation.airwayName}:${presentation.branchKey}:$index",
                        from = fromPoint.navRef,
                        to = toPoint.navRef,
                        source =
                            net.jonh.aerobag.prototype.domain.ResolvedLegSource.RouteComponent(
                                componentIndex = 1,
                            ),
                    )
                }
        adapter.insertAirwayMaterializedUi(
            plan = plan,
            startComponentIndex = 0,
            endComponentIndex = 1,
            selection = selection,
            airway = airway,
            resolvedLegs = resolvedLegs,
        )
    }.getOrElse {
        Log.e("AerobagSeed", "buildSeededDevPlan fell back to sample plan", it)
        FlightPlanUiMutation(
            plan = plan,
            uiState = adapter.buildFlightPlanUi(plan),
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

@Composable
private fun SituationStatusBadge(
    situation: Situation,
    modifier: Modifier = Modifier,
) {
    val tone = when (situation.position) {
        SituationPosition.Unknown -> Triple("Location Unknown", Color(0xFFB3261E), "unknown")
        is SituationPosition.FlightPlanLocation -> Triple("Simulated Position", Color(0xFFB1591A), "simulated")
        is SituationPosition.LatLon -> Triple("Live Position", Color(0xFF2A4F66), "live")
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
    situation: Situation,
    viewport: MapViewportState,
    widthUnits: Float,
    heightUnits: Float,
): SituationOverlay? {
    if (widthUnits <= 0f || heightUnits <= 0f) return null
    val position = when (val current = situation.position) {
        SituationPosition.Unknown -> return null
        is SituationPosition.LatLon -> LatLon(current.lat, current.lon)
        is SituationPosition.FlightPlanLocation -> LatLon(current.lat, current.lon)
    }
    val point = latLonToScreen(position.lat, position.lon, viewport, widthUnits, heightUnits)
    val heading = (situation.orientationDeg ?: 0.0).toFloat()
    val predictor = situation.speedKt?.let { speedKt ->
        val ahead = projectAhead(position.lat, position.lon, heading.toDouble(), speedKt / 60.0)
        latLonToScreen(ahead.lat, ahead.lon, viewport, widthUnits, heightUnits)
    }
    return SituationOverlay(
        pointUnits = point,
        headingDeg = heading,
        predictorUnits = predictor,
        ring = selectSituationRing(position, viewport, widthUnits, heightUnits),
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
): SituationRing {
    val center = latLonToScreen(position.lat, position.lon, viewport, widthUnits, heightUnits)
    val smaller = minOf(widthUnits, heightUnits)
    val minDiameter = smaller * 0.5f
    val maxDiameter = smaller * 0.8f
    val targetDiameter = smaller * 0.65f
    val best = SituationRingSizesNm
        .map { radiusNm ->
            val edge = projectAhead(position.lat, position.lon, 90.0, radiusNm)
            val edgePoint = latLonToScreen(edge.lat, edge.lon, viewport, widthUnits, heightUnits)
            val radiusUnits = hypot(edgePoint.x - center.x, edgePoint.y - center.y)
            val diameterUnits = radiusUnits * 2f
            val outOfBounds = when {
                diameterUnits < minDiameter -> minDiameter - diameterUnits
                diameterUnits > maxDiameter -> diameterUnits - maxDiameter
                else -> 0f
            }
            val score = if (outOfBounds > 0f) 10000f + outOfBounds else kotlin.math.abs(diameterUnits - targetDiameter)
            Triple(radiusNm, radiusUnits, score)
        }
        .minBy { it.third }
    val labelPoint = pointOnCircle(center, best.second + 16f, -45f)
    return SituationRing(
        radiusUnits = best.second,
        tickMarks = buildSituationTickMarks(center, best.second),
        labelPointUnits = labelPoint,
        labelRotationDeg = 45f,
        labelText = formatRingDistance(best.first),
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

private fun formatRingDistance(radiusNm: Double): String =
    if (radiusNm % 1.0 == 0.0) "${radiusNm.toInt()}nm" else "${radiusNm}nm"

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
    val appCore = remember(fixture.catalogJson, fixture.chartCatalogJson) { NativeAppCoreAdapter(fixture.catalogJson, fixture.chartCatalogJson) }
    val navDbPath = remember(context) { ensureNavDbPath(context.applicationContext) }
    val initialPlanMutation = remember(appCore, navDbPath, fixture.samplePlan) {
        buildSeededDevPlan(appCore, navDbPath, fixture.samplePlan)
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
    var planUiState by remember(uiSession) { mutableStateOf(initialPlanMutation.uiState) }
    val appState = sessionSnapshot.appState
    val currentPlan = appState.activePlan ?: initialPlanMutation.plan
    val chartCatalog = uiSession.chartCatalog
    val derivedChartPageState = sessionSnapshot.chartPageState
    val selectedMap = remember(selectedMapId, fixture.mapViews) {
        fixture.mapViews.find { it.id == selectedMapId } ?: fixture.mapViews.first()
    }
    var mapViewport by remember { mutableStateOf(createInitialSituationViewport(selectedMap.mapView)) }
    var chartViewport by remember { mutableStateOf<net.jonh.aerobag.prototype.domain.ImageViewportState?>(null) }
    var chartFolderOpen by remember { mutableStateOf(false) }
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
        sessionSnapshot = uiSession.setSituation(demoSituation())
    }
    LaunchedEffect(appCore, currentPlan) {
        planUiState = appCore.buildFlightPlanUi(currentPlan)
    }
    val legSummary = remember(currentPlan) {
        currentPlan.legs.firstOrNull()?.let { "${navRefLabel(it.from)} -> ${navRefLabel(it.to)} CRS 342" } ?: "NO LEG"
    }

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
                    page = page,
                    pageHistory = pageHistory,
                    uptimeLabel = uptimeLabel,
                    fixture = fixture,
                    uiSession = uiSession,
                    uiTheme = uiTheme,
                    situation = appState.situation,
                    selectedMapId = selectedMapId,
                    viewport = mapViewport,
                    onViewportChange = { mapViewport = it },
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
                    legSummary = legSummary,
                )
            }
            AppPage.Plan -> {
                FlightPlanPage(
                    appCore = appCore,
                    navDbPath = navDbPath,
                    page = page,
                    pageHistory = pageHistory,
                    uptimeLabel = uptimeLabel,
                    legSummary = legSummary,
                    samplePlan = currentPlan,
                    planUiState = planUiState,
                    uiTheme = uiTheme,
                    onSelectPage = ::navigateToPage,
                    onOpenCharts = { airportId -> if (airportId != null) openChartsForAirport(airportId) },
                    onApplyMutation = { mutation ->
                        sessionSnapshot = uiSession.replaceFlightPlan(mutation.plan)
                        planUiState = mutation.uiState
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
                    situation = appState.situation,
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
    page: AppPage,
    pageHistory: List<AppViewSnapshot>,
    uptimeLabel: String,
    fixture: net.jonh.aerobag.prototype.domain.ContentFixture,
    uiSession: NativeUiSession,
    uiTheme: UiTheme,
    situation: Situation,
    selectedMapId: String,
    viewport: MapViewportState,
    onViewportChange: (MapViewportState) -> Unit,
    onSelectMapId: (String) -> Unit,
    onSelectPage: (AppPage) -> Unit,
    onOpenPlan: () -> Unit,
    legSummary: String,
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
    var installingPackage by remember { mutableStateOf<String?>(null) }
    var installRevision by remember { mutableStateOf(0) }
    var motionDragActive by remember { mutableStateOf(false) }
    var motionDragLastX by remember { mutableStateOf(0f) }
    var motionDragLastY by remember { mutableStateOf(0f) }
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
    val situationOverlay = remember(situation, viewport, surfaceWidthUnits, surfaceHeightUnits) {
        resolveSituationOverlay(
            situation = situation,
            viewport = viewport,
            widthUnits = surfaceWidthUnits,
            heightUnits = surfaceHeightUnits,
        )
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
            textAlign = Paint.Align.LEFT
            textSize = 14f
            typeface = android.graphics.Typeface.create(android.graphics.Typeface.DEFAULT_BOLD, android.graphics.Typeface.BOLD)
        }
    }
    val vorLabelFillPaint = remember {
        Paint().apply {
            isAntiAlias = true
            color = android.graphics.Color.rgb(74, 163, 255)
            style = Paint.Style.FILL
            textAlign = Paint.Align.CENTER
            textSize = 14f
            typeface = android.graphics.Typeface.create(android.graphics.Typeface.DEFAULT_BOLD, android.graphics.Typeface.BOLD)
        }
    }
    val fixLabelFillPaint = remember {
        Paint().apply {
            isAntiAlias = true
            color = android.graphics.Color.rgb(57, 217, 255)
            style = Paint.Style.FILL
            textAlign = Paint.Align.CENTER
            textSize = 14f
            typeface = android.graphics.Typeface.create(android.graphics.Typeface.DEFAULT_BOLD, android.graphics.Typeface.BOLD)
        }
    }
    val airportToweredLabelFillPaint = remember {
        Paint().apply {
            isAntiAlias = true
            color = android.graphics.Color.rgb(74, 163, 255)
            style = Paint.Style.FILL
            textAlign = Paint.Align.LEFT
            textSize = 14f
            typeface = android.graphics.Typeface.create(android.graphics.Typeface.DEFAULT_BOLD, android.graphics.Typeface.BOLD)
        }
    }
    val airportUntoweredLabelFillPaint = remember {
        Paint().apply {
            isAntiAlias = true
            color = android.graphics.Color.rgb(255, 79, 216)
            style = Paint.Style.FILL
            textAlign = Paint.Align.LEFT
            textSize = 14f
            typeface = android.graphics.Typeface.create(android.graphics.Typeface.DEFAULT_BOLD, android.graphics.Typeface.BOLD)
        }
    }

    LaunchedEffect(selectedMap.id) { chartTrayOpen = false }
    LaunchedEffect(selectedMap.id, pageTrayOpen, chartTrayOpen) {
        if (!pageTrayOpen && !chartTrayOpen) {
            withFrameNanos { }
            focusRequester.requestFocus()
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
    DisposableEffect(activity, selectedMap.mapView, surfaceWidthUnits, surfaceHeightUnits, viewport, pageTrayOpen, chartTrayOpen) {
        if (activity != null) {
            activity.onHardwareZoomDelta = { delta ->
                if (surfaceWidthUnits == 0f || surfaceHeightUnits == 0f || pageTrayOpen || chartTrayOpen) {
                    false
                } else {
                    onViewportChange(
                        zoomAroundPoint(
                            viewport = viewport,
                            mapView = selectedMap.mapView,
                            anchor = ScreenPoint(surfaceWidthUnits / 2f, surfaceHeightUnits / 2f),
                            widthPx = surfaceWidthUnits,
                            heightPx = surfaceHeightUnits,
                            nextZoom = clampZoom(viewport.zoom + delta, selectedMap.mapView),
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
                onViewportChange(
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
                    while (true) {
                        val event = awaitPointerEvent()
                        val pressed = event.changes.filter { it.pressed && !it.isConsumed }
                        if (pressed.isEmpty()) break
                        if (topLeftTrayOpen) {
                            pressed.forEach { it.consume() }
                            continue
                        }
                        if (pressed.size == 1) {
                            val change = pressed.first()
                            if (dragPointerId != change.id || dragLastPosition == null) {
                                dragPointerId = change.id
                                dragLastPosition = change.position
                                pinchSnapshot = null
                            } else {
                                val last = dragLastPosition ?: change.position
                                onViewportChange(
                                    dragViewport(
                                        viewportState.value,
                                        dx = with(density) { (change.position.x - last.x).toDp().value },
                                        dy = with(density) { (change.position.y - last.y).toDp().value },
                                    ),
                                )
                                dragLastPosition = change.position
                            }
                            change.consume()
                        } else {
                            val first = pressed[0]
                            val second = pressed[1]
                            if (pinchSnapshot == null) {
                                pinchSnapshot = createPinchSnapshot(
                                    viewport = viewportState.value,
                                    first = ScreenPoint(with(density) { first.position.x.toDp().value }, with(density) { first.position.y.toDp().value }),
                                    second = ScreenPoint(with(density) { second.position.x.toDp().value }, with(density) { second.position.y.toDp().value }),
                                    widthPx = surfaceWidthUnits,
                                    heightPx = surfaceHeightUnits,
                                )
                            }
                            onViewportChange(
                                applyPinchGesture(
                                    snapshot = pinchSnapshot,
                                    currentFirst = ScreenPoint(with(density) { first.position.x.toDp().value }, with(density) { first.position.y.toDp().value }),
                                    currentSecond = ScreenPoint(with(density) { second.position.x.toDp().value }, with(density) { second.position.y.toDp().value }),
                                    mapView = selectedMap.mapView,
                                    widthPx = surfaceWidthUnits,
                                    heightPx = surfaceHeightUnits,
                                ),
                            )
                            first.consume()
                            second.consume()
                        }
                    }
                }
            }
            .pointerInteropFilter { event ->
                if (surfaceWidthUnits == 0f || surfaceHeightUnits == 0f) {
                    return@pointerInteropFilter false
                }
                when (event.actionMasked) {
                    MotionEvent.ACTION_DOWN -> {
                        if (topLeftTrayOpen) {
                            return@pointerInteropFilter false
                        }
                        focusRequester.requestFocus()
                        motionDragActive = true
                        motionDragLastX = event.x
                        motionDragLastY = event.y
                        true
                    }
                    MotionEvent.ACTION_MOVE -> {
                        if (!motionDragActive || topLeftTrayOpen) {
                            return@pointerInteropFilter false
                        }
                        val dxPx = event.x - motionDragLastX
                        val dyPx = event.y - motionDragLastY
                        onViewportChange(
                            dragViewport(
                                viewportState.value,
                                dx = with(density) { dxPx.toDp().value },
                                dy = with(density) { dyPx.toDp().value },
                            ),
                        )
                        motionDragLastX = event.x
                        motionDragLastY = event.y
                        true
                    }
                    MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                        val wasDragging = motionDragActive
                        motionDragActive = false
                        wasDragging
                    }
                }
                if (event.action == MotionEvent.ACTION_SCROLL) {
                    val wheelDelta = event.getAxisValue(MotionEvent.AXIS_VSCROLL).takeIf { it != 0f }
                        ?: event.getAxisValue(MotionEvent.AXIS_SCROLL)
                    onViewportChange(
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
                        drawCircle(airportFillColor, radius = airportRadius, center = center)
                        drawCircle(airportMarkerStrokeColor, radius = airportRadius, center = center, style = Stroke(width = 2f * densityScale))
                        if (feature.fuelAvailable) {
                            val tabHalf = 4f * densityScale
                            val tabInset = 11f * densityScale
                            drawRect(
                                color = airportFillColor,
                                topLeft = Offset(center.x - tabHalf, center.y - 17f * densityScale),
                                size = Size(tabHalf * 2f, 6f * densityScale),
                            )
                            drawRect(
                                color = airportFillColor,
                                topLeft = Offset(center.x + tabInset, center.y - tabHalf),
                                size = Size(6f * densityScale, tabHalf * 2f),
                            )
                            drawRect(
                                color = airportFillColor,
                                topLeft = Offset(center.x - tabHalf, center.y + tabInset),
                                size = Size(tabHalf * 2f, 6f * densityScale),
                            )
                            drawRect(
                                color = airportFillColor,
                                topLeft = Offset(center.x - 17f * densityScale, center.y - tabHalf),
                                size = Size(6f * densityScale, tabHalf * 2f),
                            )
                            drawRect(
                                color = airportMarkerStrokeColor,
                                topLeft = Offset(center.x - tabHalf, center.y - 17f * densityScale),
                                size = Size(tabHalf * 2f, 6f * densityScale),
                                style = Stroke(width = 2f * densityScale),
                            )
                            drawRect(
                                color = airportMarkerStrokeColor,
                                topLeft = Offset(center.x + tabInset, center.y - tabHalf),
                                size = Size(6f * densityScale, tabHalf * 2f),
                                style = Stroke(width = 2f * densityScale),
                            )
                            drawRect(
                                color = airportMarkerStrokeColor,
                                topLeft = Offset(center.x - tabHalf, center.y + tabInset),
                                size = Size(tabHalf * 2f, 6f * densityScale),
                                style = Stroke(width = 2f * densityScale),
                            )
                            drawRect(
                                color = airportMarkerStrokeColor,
                                topLeft = Offset(center.x - 17f * densityScale, center.y - tabHalf),
                                size = Size(6f * densityScale, tabHalf * 2f),
                                style = Stroke(width = 2f * densityScale),
                            )
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
                            val textX = center.x + 18f * densityScale
                            val textY = center.y + 5f * densityScale
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
                            val textY = center.y + 20f * densityScale
                            drawText(feature.label, center.x, textY, fixLabelStrokePaint)
                            drawText(feature.label, center.x, textY, vorLabelFillPaint)
                        }
                    } else {
                        val triangle = fixTrianglePath(center, 8f * densityScale)
                        drawPath(triangle, fixMarkerFillColor)
                        drawPath(triangle, fixMarkerStrokeColor, style = Stroke(width = 2.5f * densityScale))
                        drawContext.canvas.nativeCanvas.apply {
                            val textY = center.y + 20f * densityScale
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
            situation = situation,
            modifier = Modifier
                .align(Alignment.TopEnd)
                .padding(top = ThumbGap, end = ThumbGap),
        )

        if (topLeftTrayOpen) {
            Scrim {
                pageTrayOpen = false
                chartTrayOpen = false
            }
        }

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

        Button(
            onClick = onOpenPlan,
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .padding(bottom = ThumbGap)
                .width(ThumbSize * 3f)
                .height(ThumbSize * 0.67f),
            shape = RoundedCornerShape(ThumbRadius),
        ) {
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                Text(legSummary, maxLines = 1, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.labelSmall)
                Text("° ° ^| ° °", style = MaterialTheme.typography.labelSmall)
            }
        }

        DebugDock(
            open = debugPanelOpen,
            onToggle = { debugPanelOpen = !debugPanelOpen },
            highlight = committedMapOverlay.warnings.isNotEmpty() || mapOverlayError != null,
            modifier = Modifier.align(Alignment.BottomStart),
        ) {
            Text("page ${pageLabel(page)}", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
            Text("up $uptimeLabel", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
            Text("stack ${formatPageStack(pageHistory, page)}", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
            Text("family ${selectedLauncher.launcherLabel}", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
            Text("${String.format("%.3f", center.first)}/${String.format("%.3f", center.second)} z${String.format("%.2f", viewport.zoom)}", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
            Text("tiles ${tiles.size}", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
            Text("vec pts=${committedMapOverlay.visibleFeatures.size} need=${committedMapOverlay.neededPointTiles.size} warn=${committedMapOverlay.warnings.size}", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
            if (mapOverlayError != null) {
                Text("fatal $mapOverlayError", style = MaterialTheme.typography.labelSmall, color = Color(0xFFB85C00))
            }
            Text("src z ${if (sourceZooms.isNotEmpty()) sourceZooms.joinToString(", ") else "(none)"}", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
            Text("pkg ${if (renderedPackages.isNotEmpty()) renderedPackages.joinToString(", ") else "(none)"}", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
            Text("maps ${selectedFamilyMapViews.joinToString(", ") { it.id }}", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
            Text(familyStatus, style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
            Text(if (debugTileLabels) "debugTiles=on" else "debugTiles=off", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
            OutlinedButton(
                onClick = { debugTileLabels = !debugTileLabels },
                modifier = Modifier.fillMaxWidth().height(ThumbSize * 0.7f),
            ) {
                Text(if (debugTileLabels) "DBG TILES ON" else "DBG TILES", style = MaterialTheme.typography.labelSmall)
            }
        }
    }
}

@Composable
private fun FlightPlanPage(
    appCore: NativeAppCoreAdapter,
    navDbPath: String,
    page: AppPage,
    pageHistory: List<AppViewSnapshot>,
    uptimeLabel: String,
    legSummary: String,
    samplePlan: net.jonh.aerobag.prototype.domain.FlightPlan,
    planUiState: FlightPlanUiState?,
    uiTheme: UiTheme,
    onSelectPage: (AppPage) -> Unit,
    onOpenCharts: (String?) -> Unit,
    onApplyMutation: (FlightPlanUiMutation) -> Unit,
) {
    val planWaypointTrayStart = ThumbSize * 2.6f + ThumbGap * 2
    val density = LocalDensity.current
    var selectedWaypointIndex by remember { mutableStateOf<Int?>(null) }
    var reorderOpen by remember { mutableStateOf(false) }
    var pageTrayOpen by remember { mutableStateOf(false) }
    var debugPanelOpen by remember { mutableStateOf(false) }
    var airwayPicker by remember { mutableStateOf<AndroidAirwayPickerState?>(null) }
    var procedurePicker by remember { mutableStateOf<AndroidProcedurePickerState?>(null) }
    val guidance = planUiState?.guidance
    val componentViews = remember(samplePlan, planUiState?.components) {
        planUiState?.components ?: buildLegacyComponentViews(samplePlan)
    }
    val rows = remember(samplePlan, planUiState, componentViews) {
        buildFlightPlanDisplayRows(samplePlan, planUiState, componentViews)
    }
    val blocks = remember(rows) {
        buildFlightPlanDisplayBlocks(rows)
    }
    var structuredSurfaceBounds by remember { mutableStateOf<Rect?>(null) }
    val structuredRowBounds = remember { mutableStateMapOf<String, Rect>() }
    val selectedRow = selectedWaypointIndex?.let(rows::getOrNull)
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
        reorderOpen = false
        airwayPicker = null
        procedurePicker = null
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(uiTheme.controls.chartSurfaceBg),
    ) {
        if (pageTrayOpen) {
            Scrim { pageTrayOpen = false }
        }

        MenuDock(
            modifier = Modifier
                .align(Alignment.TopStart)
                .padding(ThumbGap)
                .zIndex(4f),
            launcherLabel = PageOptions.firstOrNull { it.page == page }?.launcherLabel ?: "PLN",
            open = pageTrayOpen,
            onToggle = { pageTrayOpen = !pageTrayOpen },
            blocked = selectedWaypointIndex != null,
            onBlockedClick = {
                selectedWaypointIndex = null
                reorderOpen = false
            },
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
                .padding(top = ThumbSize + ThumbGap * 2, start = ThumbGap, end = ThumbGap, bottom = ThumbSize * 1.35f),
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
                        modifier = Modifier.weight(1f),
                        verticalArrangement = Arrangement.spacedBy(PlanGridGap),
                    ) {
                        items(blocks.size) { blockIndex ->
                            when (val block = blocks[blockIndex]) {
                                is FlightPlanDisplayBlock.Single -> {
                                    FlightPlanDataRow(
                                        row = block.row,
                                        selected = selectedWaypointIndex == block.index,
                                        structuredRowBounds = structuredRowBounds,
                                        onWaypointClick = {
                                            selectedWaypointIndex = block.index
                                            reorderOpen = false
                                            airwayPicker = null
                                            procedurePicker = null
                                        },
                                    )
                                }

                                is FlightPlanDisplayBlock.Group -> {
                                    FlightPlanGroupBlock(
                                        header = block.header,
                                        headerSelected = selectedWaypointIndex == block.headerIndex,
                                        structuredRowBounds = structuredRowBounds,
                                        onHeaderClick = {
                                            selectedWaypointIndex = block.headerIndex
                                            reorderOpen = false
                                            airwayPicker = null
                                            procedurePicker = null
                                        },
                                        children = block.children,
                                        selectedWaypointIndex = selectedWaypointIndex,
                                        onChildClick = { childIndex ->
                                            selectedWaypointIndex = childIndex
                                            reorderOpen = false
                                            airwayPicker = null
                                            procedurePicker = null
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
                .height(ThumbSize * 1.2f)
                .padding(start = ThumbGap, end = ThumbGap, bottom = ThumbGap),
        ) {
            Row(
                modifier = Modifier.align(Alignment.Center),
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
            Text(
                text =
                    guidance?.let {
                        buildString {
                            append(it.sequencingMode.name.uppercase())
                            it.activeLeg?.let { leg ->
                                append(" · ")
                                append(navRefLabel(leg.from))
                                append(" -> ")
                                append(navRefLabel(leg.to))
                            }
                        }
                    } ?: legSummary,
                modifier = Modifier.align(Alignment.BottomCenter),
                style = MaterialTheme.typography.labelMedium,
                color = Color(0xFF52656D),
            )
        }

        DebugDock(
            open = debugPanelOpen,
            onToggle = { debugPanelOpen = !debugPanelOpen },
            modifier = Modifier.align(Alignment.BottomStart),
        ) {
            Text("page ${pageLabel(page)}", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
            Text("up $uptimeLabel", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
            Text("stack ${formatPageStack(pageHistory, page)}", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
            Text("components ${componentViews.size}", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
            Text("rows ${rows.size}", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
        }

        if (selectedWaypointIndex != null && selectedRow != null) {
            Scrim {
                closePanels()
            }
            if (procedurePicker != null) {
                val picker = procedurePicker!!
                MenuPanel(
                    modifier = Modifier
                        .align(Alignment.TopStart)
                        .padding(top = ThumbSize + ThumbGap * 2, start = planWaypointTrayStart, end = ThumbGap)
                        .zIndex(5f),
                    width = Dp.Unspecified,
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
                                            navDbPath,
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
                                            navDbPath,
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
                        .padding(top = ThumbSize + ThumbGap * 2, start = planWaypointTrayStart, end = ThumbGap)
                        .zIndex(5f),
                    width = Dp.Unspecified,
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
                                        appCore.listAirwayEntryCandidates(navDbPath, suggestion.airwayName, picker.originAnchor)
                                    }.onSuccess { entries ->
                                        airwayPicker =
                                            picker.copy(
                                                loading = false,
                                                selectedAirwayName = suggestion.airwayName,
                                                entries = entries,
                                                selectedEntryIndex = null,
                                                exits = emptyList(),
                                            )
                                    }.onFailure { error ->
                                        airwayPicker = picker.copy(loading = false, error = error.message ?: error.toString())
                                    }
                                },
                            )
                        }
                    } else if (picker.selectedEntryIndex == null) {
                        picker.entries.forEachIndexed { index, entry ->
                            MenuPanelRow(
                                label = navRefLabel(entry.navRef),
                                active = index == picker.entries.indexOfFirst { it.branchPointIndex == entry.branchPointIndex && it.sequence == entry.sequence },
                                enabled = true,
                                onSelect = {
                                    val destination = picker.destinationAnchor
                                    if (destination == null) {
                                        airwayPicker = picker.copy(error = "airway insertion needs a following waypoint")
                                        return@MenuPanelRow
                                    }
                                    airwayPicker = picker.copy(loading = true, error = null)
                                    runCatching {
                                        appCore.listAirwayExitCandidates(navDbPath, picker.selectedAirwayName, entry, destination)
                                    }.onSuccess { exits ->
                                        airwayPicker =
                                            picker.copy(
                                                loading = false,
                                                selectedEntryIndex = index,
                                                exits = exits,
                                            )
                                    }.onFailure { error ->
                                        airwayPicker = picker.copy(loading = false, error = error.message ?: error.toString())
                                    }
                                },
                            )
                        }
                        MenuPanelRow(label = "Back", active = false, enabled = true, onSelect = { airwayPicker = picker.copy(selectedAirwayName = null, entries = emptyList()) })
                    } else {
                        val entry = picker.entries[picker.selectedEntryIndex]
                        picker.exits.forEach { exit ->
                            MenuPanelRow(
                                label = navRefLabel(exit.navRef),
                                active = false,
                                enabled = !exit.isEntry,
                                onSelect = {
                                    if (exit.isEntry) return@MenuPanelRow
                                    runCatching {
                                        if (picker.mode == "replace" && picker.componentIndex != null) {
                                            appCore.replaceAirwayFromSelectionUi(
                                                navDbPath,
                                                samplePlan,
                                                picker.componentIndex,
                                                entry,
                                                exit,
                                            )
                                        } else {
                                            appCore.insertAirwayFromSelectionUi(
                                                navDbPath,
                                                samplePlan,
                                                picker.startComponentIndex ?: error("missing insertion start"),
                                                picker.endComponentIndex ?: error("missing insertion end"),
                                                entry,
                                                exit,
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
                        MenuPanelRow(label = "Back", active = false, enabled = true, onSelect = { airwayPicker = picker.copy(selectedEntryIndex = null, exits = emptyList()) })
                    }
                }
            } else if (reorderOpen) {
                BoxWithConstraints(
                    modifier = Modifier
                        .fillMaxSize()
                        .zIndex(5f),
                ) {
                    val trayWidth = minOf(ThumbSize * 4f, maxWidth - planWaypointTrayStart - ThumbGap)
                    val trayHeight = maxHeight - (ThumbSize + ThumbGap * 2) - ThumbSize
                    MenuPanel(
                        modifier = Modifier
                            .align(Alignment.TopStart)
                            .padding(top = ThumbSize + ThumbGap * 2, start = planWaypointTrayStart, end = ThumbGap, bottom = ThumbSize)
                            .height(trayHeight),
                        width = trayWidth,
                    ) {
                        Column(
                            modifier = Modifier.fillMaxSize(),
                            verticalArrangement = Arrangement.Center,
                            horizontalAlignment = Alignment.CenterHorizontally,
                        ) {
                            Column(verticalArrangement = Arrangement.spacedBy(ThumbGap)) {
                                CompactSquareButton(
                                    label = "Up",
                                    modifier = Modifier.size(ThumbSize),
                                    enabled = selectedRow.componentIndex != null && selectedRow.canReorderComponent,
                                    onClick = {
                                        selectedRow.componentIndex?.let {
                                            onApplyMutation(appCore.moveComponentUi(samplePlan, it, -1))
                                        }
                                        closePanels()
                                    },
                                )
                                CompactSquareButton(
                                    label = "Down",
                                    modifier = Modifier.size(ThumbSize),
                                    enabled = selectedRow.componentIndex != null && selectedRow.canReorderComponent,
                                    onClick = {
                                        selectedRow.componentIndex?.let {
                                            onApplyMutation(appCore.moveComponentUi(samplePlan, it, 1))
                                        }
                                        closePanels()
                                    },
                                )
                            }
                        }
                    }
                }
            } else {
                MenuPanel(
                    modifier = Modifier
                        .align(Alignment.TopStart)
                        .padding(top = ThumbSize + ThumbGap * 2, start = planWaypointTrayStart, end = ThumbGap)
                        .zIndex(5f),
                    width = Dp.Unspecified,
                ) {
                    val actions = when {
                        selectedRow.rowKind == "group" &&
                            selectedRow.componentKind == RouteComponentViewKind.Airway &&
                            selectedRow.componentIndex != null -> listOf(
                            "Change Airway" to selectedRow.canChangeAirway,
                            "Remove Airway" to selectedRow.canRemoveComponent,
                        )
                        selectedRow.rowKind == "group" &&
                            selectedRow.componentKind == RouteComponentViewKind.Procedure &&
                            selectedRow.componentIndex != null -> listOf(
                            "Remove Procedure" to selectedRow.canRemoveComponent,
                        )
                        selectedRow.rowKind != "waypoint" -> emptyList()
                        selectedRow.depth == 0 -> listOf(
                            "Activate Leg" to (selectedRow.legIndex != null),
                            "Remove" to (selectedRow.componentIndex != null && selectedRow.canRemoveComponent),
                            "Reorder" to (selectedRow.componentIndex != null && selectedRow.canReorderComponent),
                            "Add Airway" to (selectedRow.canAddAirwayAfter && selectedRow.originAnchor != null && selectedRow.destinationAnchor != null),
                            "Select Procedure" to (selectedRow.canAddProcedureBefore && selectedRow.componentIndex != null && selectedRow.chartAirportId != null),
                            "Charts" to (selectedRow.chartAirportId != null),
                        )
                        else -> listOf(
                            "Activate Leg" to (selectedRow.legIndex != null),
                            "Charts" to (selectedRow.chartAirportId != null),
                        )
                    }
                    actions.forEach { (action, enabled) ->
                        MenuPanelRow(
                            label = action,
                            active = false,
                            enabled = enabled,
                            onSelect = {
                                if (!enabled) {
                                    return@MenuPanelRow
                                }
                                when (action) {
                                    "Activate Leg" -> {
                                        selectedRow.legIndex?.let {
                                            onApplyMutation(appCore.activateLegUi(samplePlan, it))
                                        }
                                        closePanels()
                                    }
                                    "Remove", "Remove Airway", "Remove Procedure" -> {
                                        selectedRow.componentIndex?.let {
                                            onApplyMutation(appCore.deleteComponentUi(samplePlan, it))
                                        }
                                        closePanels()
                                    }
                                    "Reorder" -> {
                                        reorderOpen = true
                                    }
                                    "Add Airway" -> {
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
                                                entries = emptyList(),
                                                selectedEntryIndex = null,
                                                exits = emptyList(),
                                            )
                                        runCatching {
                                            appCore.suggestAirwaysNear(navDbPath, selectedRow.originAnchor!!)
                                        }.onSuccess { suggestions ->
                                            airwayPicker = airwayPicker?.copy(loading = false, suggestions = suggestions)
                                        }.onFailure { error ->
                                            airwayPicker = airwayPicker?.copy(loading = false, error = error.message ?: error.toString())
                                        }
                                    }
                                    "Change Airway" -> {
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
                                                entries = emptyList(),
                                                selectedEntryIndex = null,
                                                exits = emptyList(),
                                            )
                                        runCatching {
                                            appCore.suggestAirwaysNear(navDbPath, selectedRow.originAnchor!!)
                                        }.onSuccess { suggestions ->
                                            airwayPicker = airwayPicker?.copy(loading = false, suggestions = suggestions)
                                        }.onFailure { error ->
                                            airwayPicker = airwayPicker?.copy(loading = false, error = error.message ?: error.toString())
                                        }
                                    }
                                    "Select Procedure" -> {
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
                                            appCore.listProcedures(navDbPath, airportId, ProcedureKind.Approach)
                                        }.onSuccess { procedures ->
                                            procedurePicker = procedurePicker?.copy(loading = false, procedures = procedures)
                                        }.onFailure { error ->
                                            procedurePicker = procedurePicker?.copy(loading = false, error = error.message ?: error.toString())
                                        }
                                    }
                                    "Charts" -> {
                                        onOpenCharts(selectedRow.chartAirportId)
                                        closePanels()
                                    }
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
    situation: Situation,
    folderOpen: Boolean,
    viewport: net.jonh.aerobag.prototype.domain.ImageViewportState?,
    onViewportChange: (net.jonh.aerobag.prototype.domain.ImageViewportState?) -> Unit,
    onFolderOpenChange: (Boolean) -> Unit,
    onSelectPage: (AppPage) -> Unit,
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
    val sortedCharts = remember(selectedAirport) { sortChartsForFolder(selectedAirport?.charts ?: emptyList()) }
    val overscrollPx = with(density) { ThumbSize.toPx() }
    val bitmap by produceState<androidx.compose.ui.graphics.ImageBitmap?>(initialValue = null, selectedChart?.assetPath) {
        value = selectedChart?.assetPath?.let { path ->
            withContext(Dispatchers.IO) {
                runCatching {
                    val localFile = java.io.File(context.filesDir, path)
                    val inputStream = when {
                        localFile.isFile -> localFile.inputStream()
                        selectedChart != null -> {
                            val chartBytes = ChartPackages.loadChartBytes(context, selectedChart) ?: context.assets.open(path).use { it.readBytes() }
                            chartBytes.inputStream()
                        }
                        else -> context.assets.open(path)
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

        if (trayOpen) {
            Scrim {
                pageTrayOpen = false
                airportTrayOpen = false
                chartTrayOpen = false
            }
        }

        SituationStatusBadge(
            situation = situation,
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

        DebugDock(
            open = debugPanelOpen,
            onToggle = { debugPanelOpen = !debugPanelOpen },
            modifier = Modifier.align(Alignment.BottomStart),
        ) {
            Text("page ${pageLabel(page)}", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
            Text("up $uptimeLabel", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
            Text(
                "stack ${formatPageStack(pageHistory, page, "", selectedAirport?.id ?: "", selectedChart?.id ?: "", selectedChart?.label ?: "", folderOpen, chartLabelsById)}",
                style = MaterialTheme.typography.labelSmall,
                color = Color(0xFF52656D),
            )
            Text("apt ${selectedAirport?.label ?: "---"}", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
            Text("chart ${selectedChart?.label ?: "---"}", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
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
    val anyTrayOpen = pageTrayOpen || trayOpen
    val dismissOpenTray = {
        if (pageTrayOpen) {
            onTogglePageTray()
        } else if (trayOpen) {
            onToggle()
        }
    }
    Row(
        modifier = modifier.padding(ThumbGap),
        horizontalArrangement = Arrangement.spacedBy(ThumbGap),
        verticalAlignment = Alignment.Top,
    ) {
        MenuDock(
            launcherLabel = PageOptions.firstOrNull { it.page == currentPage }?.launcherLabel ?: "CHT",
            open = pageTrayOpen,
            onToggle = onTogglePageTray,
            blocked = anyTrayOpen && !pageTrayOpen,
            onBlockedClick = dismissOpenTray,
            style = MenuDockStyle.Compact,
            options = PageOptions.map { option ->
                MenuDockOption(option.page.name, option.label, active = option.page == currentPage) { onSelectPage(option.page) }
            },
        )
        MenuDock(
            launcherLabel = selectedLabel,
            open = trayOpen,
            onToggle = onToggle,
            blocked = anyTrayOpen && !trayOpen,
            onBlockedClick = dismissOpenTray,
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
    val dismissOpenTray = {
        when {
            pageTrayOpen -> onTogglePageTray()
            airportTrayOpen -> onToggleAirportTray()
            chartTrayOpen -> onToggleChartTray()
        }
    }
    Row(
        modifier = modifier.padding(ThumbGap),
        horizontalArrangement = Arrangement.spacedBy(ThumbGap),
        verticalAlignment = Alignment.Top,
    ) {
        MenuDock(
            launcherLabel = PageOptions.firstOrNull { it.page == currentPage }?.launcherLabel ?: "PLT",
            open = pageTrayOpen,
            onToggle = onTogglePageTray,
            blocked = trayOpen && !pageTrayOpen,
            onBlockedClick = dismissOpenTray,
            style = MenuDockStyle.Compact,
            options = PageOptions.map { option ->
                MenuDockOption(option.page.name, option.label, active = option.page == currentPage) { onSelectPage(option.page) }
            },
        )

        MenuDock(
            launcherLabel = selectedAirport?.label ?: "---",
            open = airportTrayOpen,
            onToggle = onToggleAirportTray,
            blocked = trayOpen && !airportTrayOpen,
            onBlockedClick = dismissOpenTray,
            style = MenuDockStyle.PlateAirport,
            options = airports.map { airport ->
                MenuDockOption(airport.id, airport.label, active = airport.id == selectedAirport?.id) { onSelectAirport(airport.id) }
            },
        )

        MenuDock(
            launcherLabel = selectedChart?.label ?: "---",
            open = chartTrayOpen,
            onToggle = onToggleChartTray,
            blocked = trayOpen && !chartTrayOpen,
            onBlockedClick = dismissOpenTray,
            style = MenuDockStyle.PlateWide,
            options = sortChartsForFolder(selectedAirport?.charts ?: emptyList()).map { chart ->
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
            onDisabledClick = if (trayOpen) dismissOpenTray else null,
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
    blocked: Boolean = false,
    onBlockedClick: (() -> Unit)? = null,
    style: MenuDockStyle,
    options: List<MenuDockOption>,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val density = LocalDensity.current
    val configuration = LocalConfiguration.current
    var anchorTopPx by remember { mutableStateOf(0f) }
    val screenHeightPx = with(density) { configuration.screenHeightDp.dp.toPx() }
    val trayOffsetPx = with(density) { (ThumbSize + ThumbGap).toPx() }
    val trayBottomMarginPx = with(density) { ThumbGap.toPx() }
    val trayMaxHeight = with(density) {
        ((screenHeightPx - anchorTopPx - trayOffsetPx - trayBottomMarginPx).coerceAtLeast(ThumbSize.toPx())).toDp()
    }
    Box(
        modifier = modifier
            .width(style.buttonWidth)
            .height(ThumbSize)
            .wrapContentSize(unbounded = true, align = Alignment.TopStart),
    ) {
        CompactSquareButton(
            label = launcherLabel,
            maxLines = style.launcherMaxLines,
            enabled = !blocked || open,
            onDisabledClick = if (blocked && !open) onBlockedClick else null,
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
            MenuPanel(
                modifier = Modifier
                    .align(Alignment.TopStart)
                    .padding(top = ThumbSize + ThumbGap)
                    .width(style.trayWidth)
                    .heightIn(max = trayMaxHeight)
                    .zIndex(10f),
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
            .background(rowBackground)
            .clickable(
                enabled = enabled,
                indication = null,
                interactionSource = remember { MutableInteractionSource() },
            ) {
                onSelect()
            }
            .padding(horizontal = 12.dp),
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
            style = MaterialTheme.typography.labelLarge,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
            color = rowTextColor,
        )
    }
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

private fun buildLegacyComponentViews(plan: net.jonh.aerobag.prototype.domain.FlightPlan): List<RouteComponentUiView> {
    if (plan.legs.isEmpty()) {
        return emptyList()
    }
    val waypoints = listOf(plan.legs.first().from) + plan.legs.map { it.to }
    return waypoints.mapIndexed { index, waypoint ->
        RouteComponentUiView(
            componentIndex = index,
            kind = RouteComponentViewKind.Waypoint,
            summary = navRefLabel(waypoint),
            items = listOf(ConcretizedNavItem.Waypoint(waypoint)),
            active = false,
            canAddAirwayAfter = true,
            canAddProcedureBefore = index > 0 && waypoint is NavRef.Airport,
            canChangeAirway = false,
            canRemove = true,
            canReorder = waypoints.size > 1,
            precedingWaypoint = waypoints.getOrNull(index - 1),
            followingWaypoint = waypoints.getOrNull(index + 1),
        )
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

private fun buildFlightPlanDisplayRows(
    plan: net.jonh.aerobag.prototype.domain.FlightPlan,
    planUiState: FlightPlanUiState?,
    componentViews: List<RouteComponentUiView>,
): List<FlightPlanDisplayRow> {
    return buildList {
        componentViews.forEachIndexed { index, component ->
            val chartAirportId =
                when (val navRef = componentWaypointNavRef(component)) {
                    is NavRef.Airport -> navRef.code
                    else -> null
                }
            if (component.kind == RouteComponentViewKind.Waypoint) {
                val navRef = componentWaypointNavRef(component)
                add(
                    FlightPlanDisplayRow(
                        id = "component:${component.componentIndex}",
                        label = navRef?.let(::navRefLabel) ?: component.summary,
                        rowKind = "waypoint",
                        componentKind = component.kind,
                        componentIndex = component.componentIndex,
                        legIndex = planUiState?.resolvedLegs?.firstOrNull { navRefsEqual(it.from, navRef) }?.legIndex,
                        chartAirportId = chartAirportId,
                        navRef = navRef,
                        active = component.active,
                        canAddAirwayAfter = component.canAddAirwayAfter,
                        canAddProcedureBefore = component.canAddProcedureBefore,
                        canChangeAirway = component.canChangeAirway,
                        canRemoveComponent = component.canRemove,
                        canReorderComponent = component.canReorder,
                        startComponentIndex = component.componentIndex,
                        endComponentIndex = component.componentIndex + 1,
                        originAnchor = component.precedingWaypoint ?: navRef,
                        destinationAnchor = component.followingWaypoint,
                    ),
                )
            } else {
                val originAnchor = component.precedingWaypoint
                val destinationAnchor = component.followingWaypoint
                add(
                    FlightPlanDisplayRow(
                        id = "group:${component.componentIndex}",
                        label = structuredComponentLabel(component),
                        rowKind = "group",
                        componentKind = component.kind,
                        componentIndex = component.componentIndex,
                        chartAirportId = chartAirportId,
                        active = component.active,
                        canAddAirwayAfter = component.canAddAirwayAfter,
                        canAddProcedureBefore = component.canAddProcedureBefore,
                        canChangeAirway = component.canChangeAirway,
                        canRemoveComponent = component.canRemove,
                        canReorderComponent = component.canReorder,
                        originAnchor = originAnchor,
                        destinationAnchor = destinationAnchor,
                    ),
                )
                component.items.forEach { item ->
                    when (item) {
                        is ConcretizedNavItem.Waypoint ->
                            add(
                                FlightPlanDisplayRow(
                                    id = "item:${component.componentIndex}:${navRefLabel(item.navRef)}:${size}",
                                    label = navRefLabel(item.navRef),
                                    rowKind = "waypoint",
                                    componentKind = component.kind,
                                    componentIndex = component.componentIndex,
                                    chartAirportId = (item.navRef as? NavRef.Airport)?.code,
                                    navRef = item.navRef,
                                    depth = 1,
                                    active = component.active,
                                ),
                            )

                        is ConcretizedNavItem.Discontinuity ->
                            add(
                                FlightPlanDisplayRow(
                                    id = "disc:${component.componentIndex}:${size}",
                                    label = item.label,
                                    rowKind = "discontinuity",
                                    componentKind = component.kind,
                                    componentIndex = component.componentIndex,
                                    depth = 1,
                                ),
                            )
                    }
                }
            }
        }

        val firstLeg = plan.legs.firstOrNull()
        if (firstLeg != null && none { it.navRef == firstLeg.from }) {
            add(
                0,
                FlightPlanDisplayRow(
                    id = "legacy:start",
                    label = navRefLabel(firstLeg.from),
                    rowKind = "waypoint",
                    chartAirportId = (firstLeg.from as? NavRef.Airport)?.code,
                    navRef = firstLeg.from,
                    componentIndex = 0,
                    legIndex = planUiState?.resolvedLegs?.firstOrNull { navRefsEqual(it.from, firstLeg.from) }?.legIndex,
                    startComponentIndex = 0,
                    endComponentIndex = 1,
                    originAnchor = firstLeg.from,
                    destinationAnchor = firstLeg.to,
                ),
            )
        }
    }
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

@Composable
private fun FlightPlanDataRow(
    row: FlightPlanDisplayRow,
    selected: Boolean,
    modifier: Modifier = Modifier,
    structuredRowBounds: MutableMap<String, Rect>? = null,
    onWaypointClick: () -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val indent = (row.depth * 18).dp
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
        Box(modifier = Modifier.width(ThumbSize * 2.5f).height(ThumbSize)) {
            CompactSquareButton(
                label = row.label,
                modifier = Modifier.fillMaxSize().padding(start = indent),
                centered = false,
                textStartPadding = 10.dp,
                backgroundColor = defaultButtonColor,
                selected = selected,
                selectedColor = selectedButtonColor,
                onClick = onWaypointClick,
            )
        }
        PlanCell("—", Modifier.weight(1f))
        PlanCell("—", Modifier.weight(1f))
        PlanCell("—", Modifier.weight(1f))
    }
}

@Composable
private fun FlightPlanGroupBlock(
    header: FlightPlanDisplayRow,
    headerSelected: Boolean,
    structuredRowBounds: MutableMap<String, Rect>? = null,
    onHeaderClick: () -> Unit,
    children: List<Pair<Int, FlightPlanDisplayRow>>,
    selectedWaypointIndex: Int?,
    onChildClick: (Int) -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    Column(
        verticalArrangement = Arrangement.spacedBy(PlanGridGap),
        modifier = Modifier
            .padding(PlanGridGap / 2)
            .background(
                color = uiTheme.controls.panelBg.copy(alpha = 0.82f),
                shape = RoundedCornerShape(ThumbRadius + 2.dp),
            )
            .border(
                width = 2.dp,
                color = uiTheme.controls.panelBorder.copy(alpha = 0.95f),
                shape = RoundedCornerShape(ThumbRadius + 2.dp),
            )
            .padding(8.dp),
    ) {
        FlightPlanDataRow(
            row = header,
            selected = headerSelected,
            structuredRowBounds = structuredRowBounds,
            onWaypointClick = onHeaderClick,
        )
        children.forEach { (childIndex, childRow) ->
            FlightPlanDataRow(
                row = childRow,
                selected = selectedWaypointIndex == childIndex,
                structuredRowBounds = structuredRowBounds,
                onWaypointClick = { onChildClick(childIndex) },
            )
        }
    }
}

@Composable
private fun PlanCell(value: String, modifier: Modifier, isHeader: Boolean = false) {
    val uiTheme = LocalAerobagUiTheme.current
    val cellHeight = if (isHeader) ThumbSize * 0.5f else ThumbSize
    Box(
        modifier = modifier
            .height(cellHeight)
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
    modifier: Modifier = Modifier,
    content: @Composable ColumnScope.() -> Unit,
) {
    Box(modifier = modifier.padding(ThumbGap)) {
        CompactSquareButton(
            label = "DBG",
            modifier = Modifier
                .align(Alignment.BottomStart)
                .size(ThumbSize),
            selected = highlight,
            selectedColor = Color(0xFFB85C00),
            onClick = onToggle,
        )

        AnimatedVisibility(
            visible = open,
            modifier = Modifier
                .align(Alignment.BottomStart)
                .padding(start = ThumbSize + ThumbGap, bottom = 0.dp),
            enter = slideInHorizontally(initialOffsetX = { -it / 3 }) + fadeIn(),
            exit = slideOutHorizontally(targetOffsetX = { -it / 3 }) + fadeOut(),
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
    centered: Boolean = true,
    textStartPadding: Dp = 0.dp,
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
            modifier = Modifier.fillMaxWidth(),
            contentAlignment = if (centered) Alignment.Center else Alignment.CenterStart,
        ) {
            Text(
                text = label,
                modifier = if (centered) Modifier else Modifier.padding(start = textStartPadding, end = 8.dp),
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
