package net.jonh.aerobag.prototype

import android.content.Context
import android.graphics.BitmapFactory
import android.graphics.Paint
import android.os.Bundle
import android.view.KeyEvent as AndroidKeyEvent
import android.view.MotionEvent
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.focusable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.wrapContentWidth
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.pointer.PointerId
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.pointerInteropFilter
import androidx.compose.ui.input.pointer.positionChanged
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import net.jonh.aerobag.prototype.domain.ChartAirport
import net.jonh.aerobag.prototype.domain.ChartAsset
import net.jonh.aerobag.prototype.domain.ChartPackages
import net.jonh.aerobag.prototype.domain.MapChartFamily
import net.jonh.aerobag.prototype.domain.MapView
import net.jonh.aerobag.prototype.domain.MapViewportState
import net.jonh.aerobag.prototype.domain.ScreenPoint
import net.jonh.aerobag.prototype.domain.SectionalPackages
import net.jonh.aerobag.prototype.domain.SampleData
import net.jonh.aerobag.prototype.domain.TileStorageKind
import net.jonh.aerobag.prototype.domain.applyPinchGesture
import net.jonh.aerobag.prototype.domain.clampZoom
import net.jonh.aerobag.prototype.domain.createInitialImageViewport
import net.jonh.aerobag.prototype.domain.createInitialViewport
import net.jonh.aerobag.prototype.domain.createPinchSnapshot
import net.jonh.aerobag.prototype.domain.dragImageViewport
import net.jonh.aerobag.prototype.domain.dragViewport
import net.jonh.aerobag.prototype.domain.imageDisplaySize
import net.jonh.aerobag.prototype.domain.preserveViewportForMap
import net.jonh.aerobag.prototype.domain.renderTiles
import net.jonh.aerobag.prototype.domain.viewportCenterLatLon
import net.jonh.aerobag.prototype.domain.zoomAroundPoint
import net.jonh.aerobag.prototype.domain.zoomImageAroundPoint
import kotlin.math.roundToInt

private val ThumbSize = 56.dp
private val ThumbGap = 5.6.dp
private val ThumbRadius = 10.dp
private const val UiPrefsName = "aerobag_ui"
private const val UiPrefsPageKey = "page"
private const val UiPrefsSelectedAirportKey = "selected_airport_id"
private const val UiPrefsSelectedChartKey = "selected_chart_id"
private const val UiPrefsRecentAirportsKey = "recent_airport_ids"

private enum class AppPage {
    Map,
    Plan,
    Charts,
}

private data class PageTrayOption(
    val page: AppPage,
    val label: String,
    val launcherLabel: String,
)

private data class MenuDockOption(
    val key: String,
    val label: String,
    val enabled: Boolean = true,
    val onSelect: () -> Unit,
)

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

private fun initialMapId(fixture: net.jonh.aerobag.prototype.domain.ContentFixture): String {
    val targetFamily = fixture.mapView.chartFamily
    val targetPackage = fixture.mapView.packageName
    return fixture.mapViews.firstOrNull {
        it.mapView.chartFamily == targetFamily && it.mapView.packageName == targetPackage
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
}

@Composable
private fun AerobagApp() {
    val context = LocalContext.current
    val fixture = remember(context) { SampleData.load(context.applicationContext) }
    val prefs = remember(context) { context.applicationContext.getSharedPreferences(UiPrefsName, Context.MODE_PRIVATE) }
    val initialRecentAirportIds = remember(fixture.chartPage.airports) {
        mergeRecentAirportIds(fixture.chartPage.airports, readRecentAirportIds(context.applicationContext))
    }
    var page by remember {
        mutableStateOf(
            runCatching { AppPage.valueOf(prefs.getString(UiPrefsPageKey, AppPage.Map.name) ?: AppPage.Map.name) }
                .getOrDefault(AppPage.Map),
        )
    }
    var selectedMapId by remember { mutableStateOf(initialMapId(fixture)) }
    var recentAirportIds by remember { mutableStateOf(initialRecentAirportIds) }
    val selectedMap = remember(selectedMapId, fixture.mapViews) {
        fixture.mapViews.find { it.id == selectedMapId } ?: fixture.mapViews.first()
    }
    var mapViewport by remember { mutableStateOf(createInitialViewport(selectedMap.mapView)) }
    var selectedAirportId by remember {
        mutableStateOf(
            resolveAirportId(
                fixture.chartPage.airports,
                prefs.getString(UiPrefsSelectedAirportKey, null),
                initialRecentAirportIds,
            ),
        )
    }
    var selectedChartId by remember {
        mutableStateOf(
            resolveChartId(
                fixture.chartPage.airports,
                resolveAirportId(
                    fixture.chartPage.airports,
                    prefs.getString(UiPrefsSelectedAirportKey, null),
                    initialRecentAirportIds,
                ),
                prefs.getString(UiPrefsSelectedChartKey, null),
            ),
        )
    }
    val orderedChartAirports = remember(recentAirportIds, fixture.chartPage.airports) {
        orderAirportsByRecency(fixture.chartPage.airports, recentAirportIds)
    }
    val selectedAirport = remember(selectedAirportId, orderedChartAirports) {
        orderedChartAirports.find { it.id == selectedAirportId } ?: orderedChartAirports.firstOrNull()
    }
    val selectedChart = remember(selectedAirport, selectedChartId) {
        selectedAirport?.charts?.find { it.id == selectedChartId } ?: selectedAirport?.charts?.firstOrNull()
    }

    LaunchedEffect(fixture.chartPage.airports, recentAirportIds, selectedAirportId, selectedChartId) {
        val normalizedRecentAirportIds = mergeRecentAirportIds(fixture.chartPage.airports, recentAirportIds)
        if (normalizedRecentAirportIds != recentAirportIds) {
            recentAirportIds = normalizedRecentAirportIds
            return@LaunchedEffect
        }
        val normalizedAirportId = resolveAirportId(fixture.chartPage.airports, selectedAirportId, normalizedRecentAirportIds)
        if (normalizedAirportId != selectedAirportId) {
            selectedAirportId = normalizedAirportId
            return@LaunchedEffect
        }
        val normalizedChartId = resolveChartId(fixture.chartPage.airports, normalizedAirportId, selectedChartId)
        if (normalizedChartId != selectedChartId) {
            selectedChartId = normalizedChartId
        }
    }

    LaunchedEffect(page, selectedAirportId, selectedChartId, recentAirportIds) {
        writeUiPrefs(context.applicationContext, page, selectedAirportId, selectedChartId, recentAirportIds)
    }
    val legSummary = remember(fixture.samplePlan) {
        fixture.samplePlan.legs.firstOrNull()?.let { "${it.fromAirport} -> ${it.toAirport} CRS 342" } ?: "NO LEG"
    }

    LaunchedEffect(selectedMap.id) {
        mapViewport = preserveViewportForMap(mapViewport, selectedMap.mapView)
    }

    when (page) {
        AppPage.Map -> MapExplorerPage(
            page = page,
            fixture = fixture,
            selectedMapId = selectedMapId,
            viewport = mapViewport,
            onViewportChange = { mapViewport = it },
            onSelectMapId = { selectedMapId = it },
            onSelectPage = { page = it },
            onOpenPlan = { page = AppPage.Plan },
            legSummary = legSummary,
        )
        AppPage.Plan -> FlightPlanPage(
            page = page,
            legSummary = legSummary,
            samplePlan = fixture.samplePlan,
            onSelectPage = { page = it },
            onOpenCharts = { page = AppPage.Charts },
        )
        AppPage.Charts -> ChartsPage(
            page = page,
            airports = orderedChartAirports,
            selectedAirport = selectedAirport,
            selectedChart = selectedChart,
            onSelectPage = { page = it },
            onSelectAirport = { airportId ->
                selectedAirportId = airportId
                recentAirportIds = moveAirportToFront(recentAirportIds, airportId, fixture.chartPage.airports)
                selectedChartId = fixture.chartPage.airports.find { it.id == airportId }?.charts?.firstOrNull()?.id.orEmpty()
            },
            onSelectChart = { selectedChartId = it },
        )
    }
}

@OptIn(ExperimentalComposeUiApi::class)
@Composable
private fun MapExplorerPage(
    page: AppPage,
    fixture: net.jonh.aerobag.prototype.domain.ContentFixture,
    selectedMapId: String,
    viewport: MapViewportState,
    onViewportChange: (MapViewportState) -> Unit,
    onSelectMapId: (String) -> Unit,
    onSelectPage: (AppPage) -> Unit,
    onOpenPlan: () -> Unit,
    legSummary: String,
) {
    val context = LocalContext.current
    val density = LocalDensity.current
    var pageTrayOpen by remember { mutableStateOf(false) }
    var chartTrayOpen by remember { mutableStateOf(false) }
    var debugPanelOpen by remember { mutableStateOf(false) }
    var debugTileLabels by remember { mutableStateOf(false) }
    var surfaceSize by remember { mutableStateOf(IntSize.Zero) }
    var installingPackage by remember { mutableStateOf<String?>(null) }
    var installRevision by remember { mutableStateOf(0) }
    val selectedMap = remember(selectedMapId, fixture.mapViews) {
        fixture.mapViews.find { it.id == selectedMapId } ?: fixture.mapViews.first()
    }
    val selectedFamilyMapViews = remember(selectedMap, fixture.mapViews) {
        fixture.mapViews.filter { it.mapView.chartFamily == selectedMap.mapView.chartFamily }
    }
    val familyPackageNames = remember(selectedFamilyMapViews) {
        selectedFamilyMapViews.mapNotNull { it.mapView.packageName }.distinct()
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
        val sectionalTarget = fixture.mapViews.firstOrNull { it.mapView.chartFamily == MapChartFamily.Sectional }
        val tacTarget = fixture.mapViews.firstOrNull { it.mapView.chartFamily == MapChartFamily.Tac }
        val ifrLowTarget = fixture.mapViews.firstOrNull { it.mapView.chartFamily == MapChartFamily.IfrLow }
        val ifrHighTarget = fixture.mapViews.firstOrNull { it.mapView.chartFamily == MapChartFamily.IfrHigh }
        listOf(
            ChartTrayOption("sectional", "SECTIONAL", "SEC", sectionalTarget != null) { sectionalTarget?.let { onSelectMapId(it.id) } },
            ChartTrayOption("tac", "TAC", "TAC", tacTarget != null) { tacTarget?.let { onSelectMapId(it.id) } },
            ChartTrayOption("ifr_low", "IFR-LOW", "IFR L", ifrLowTarget != null) { ifrLowTarget?.let { onSelectMapId(it.id) } },
            ChartTrayOption("ifr_high", "IFR-HIGH", "IFR H", ifrHighTarget != null) { ifrHighTarget?.let { onSelectMapId(it.id) } },
        )
    }
    val selectedLauncher = trayOptions.firstOrNull { option ->
        when (option.id) {
            "sectional" -> selectedMap.mapView.chartFamily == MapChartFamily.Sectional
            "tac" -> selectedMap.mapView.chartFamily == MapChartFamily.Tac
            "ifr_low" -> selectedMap.mapView.chartFamily == MapChartFamily.IfrLow
            "ifr_high" -> selectedMap.mapView.chartFamily == MapChartFamily.IfrHigh
            else -> false
        }
    } ?: trayOptions.first()
    val tileRects = remember(tiles, density) {
        val columns = tiles.groupBy { it.x }.mapValues { (_, entries) ->
            with(density) { entries.minOf { it.leftPx.dp.roundToPx() } }
        }.toList().sortedBy { it.second }
        val rows = tiles.groupBy { it.yTms }.mapValues { (_, entries) ->
            with(density) { entries.minOf { it.topPx.dp.roundToPx() } }
        }.toList().sortedBy { it.second }
        val columnRects = columns.mapIndexed { index, (x, leftPx) ->
            val rightPx = if (index + 1 < columns.size) columns[index + 1].second else with(density) {
                val sample = tiles.first { it.x == x }
                (sample.leftPx + sample.sizePx).dp.roundToPx()
            }
            x to (leftPx to (rightPx - leftPx))
        }.toMap()
        val rowRects = rows.mapIndexed { index, (yTms, topPx) ->
            val bottomPx = if (index + 1 < rows.size) rows[index + 1].second else with(density) {
                val sample = tiles.first { it.yTms == yTms }
                (sample.topPx + sample.sizePx).dp.roundToPx()
            }
            yTms to (topPx to (bottomPx - topPx))
        }.toMap()
        tiles.associate { tile ->
            val (leftPx, widthPx) = columnRects.getValue(tile.x)
            val (topPx, heightPx) = rowRects.getValue(tile.yTms)
            Triple(tile.zoom, tile.x, tile.yTms) to TileRect(leftPx, topPx, widthPx, heightPx)
        }
    }
    val tileBitmaps = remember(tiles, selectedMap.id, installRevision) {
        tiles.associate { tile ->
            Triple(tile.zoom, tile.x, tile.yTms) to runCatching {
                SectionalPackages.loadTileBytes(context, tile)
                    ?.let { BitmapFactory.decodeByteArray(it, 0, it.size)?.asImageBitmap() }
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

    LaunchedEffect(selectedMap.id) { chartTrayOpen = false }

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
            .background(
                brush = Brush.verticalGradient(
                    colors = listOf(Color(0xFFF6F1E7), Color(0xFFDCE4E4)),
                ),
            )
            .onSizeChanged { surfaceSize = it }
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
                } else {
                    drawRect(
                        color = Color(0x12000000),
                        topLeft = Offset(tileRect.leftPx.toFloat(), tileRect.topPx.toFloat()),
                        size = Size(tileRect.widthPx.toFloat(), tileRect.heightPx.toFloat()),
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

        CompactSquareButton(
            label = "DBG",
            modifier = Modifier
                .align(Alignment.BottomStart)
                .padding(ThumbGap)
                .size(ThumbSize),
            onClick = { debugPanelOpen = !debugPanelOpen },
        )

        AnimatedVisibility(
            visible = debugPanelOpen,
            modifier = Modifier
                .align(Alignment.BottomStart)
                .padding(start = ThumbSize + ThumbGap * 2, bottom = ThumbGap),
            enter = slideInHorizontally(initialOffsetX = { -it / 3 }) + fadeIn(),
            exit = slideOutHorizontally(targetOffsetX = { -it / 3 }) + fadeOut(),
        ) {
            Card(
                modifier = Modifier.width(ThumbSize * 4f),
            ) {
                Column(
                    modifier = Modifier.padding(horizontal = 10.dp, vertical = 8.dp),
                    verticalArrangement = Arrangement.spacedBy(2.dp),
                ) {
                    Text("family ${selectedLauncher.launcherLabel}", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
                    Text("${String.format("%.3f", center.first)}/${String.format("%.3f", center.second)} z${String.format("%.2f", viewport.zoom)}", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
                    Text("tiles ${tiles.size}", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
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
    }
}

@Composable
private fun FlightPlanPage(
    page: AppPage,
    legSummary: String,
    samplePlan: net.jonh.aerobag.prototype.domain.FlightPlan,
    onSelectPage: (AppPage) -> Unit,
    onOpenCharts: () -> Unit,
) {
    var selectedWaypointIndex by remember { mutableStateOf<Int?>(null) }
    var pageTrayOpen by remember { mutableStateOf(false) }
    val rows = remember(samplePlan) {
        samplePlan.legs.mapIndexed { index, leg ->
            FlightPlanRow(
                waypoint = leg.toAirport,
                distance = if (index == 0) "18.4" else "11.2",
                ete = if (index == 0) "0:07" else "0:04",
                course = if (index == 0) "342" else "161",
            )
        }
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(
                brush = Brush.verticalGradient(
                    colors = listOf(Color(0xFFF7F2E9), Color(0xFFECE7DB)),
                ),
            ),
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
            width = ThumbSize * 2.4f,
            options = PageOptions.map { option ->
                MenuDockOption(option.page.name, option.label) {
                    onSelectPage(option.page)
                    pageTrayOpen = false
                }
            },
        )

        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(top = ThumbSize + ThumbGap * 2, start = ThumbGap, end = ThumbGap, bottom = ThumbSize),
            verticalArrangement = Arrangement.spacedBy(1.dp),
        ) {
            PlanHeaderRow()
            rows.forEachIndexed { index, row ->
                FlightPlanDataRow(row = row, onWaypointClick = { selectedWaypointIndex = index })
            }
        }

        Text(
            text = legSummary,
            modifier = Modifier.align(Alignment.BottomCenter).padding(bottom = ThumbGap),
            style = MaterialTheme.typography.labelMedium,
            color = Color(0xFF52656D),
        )

        if (selectedWaypointIndex != null) {
            Scrim { selectedWaypointIndex = null }
            Card(
                modifier = Modifier
                    .align(Alignment.TopStart)
                    .padding(top = ThumbSize + ThumbGap * 2, start = ThumbSize * 2.6f + ThumbGap * 2, end = ThumbGap),
            ) {
                Column(
                    modifier = Modifier.padding(ThumbGap),
                    verticalArrangement = Arrangement.spacedBy(ThumbGap),
                ) {
                    listOf("Remove", "Insert", "Reorder", "Waypoint Info", "Add Airway", "Select Procedure", "Charts").forEach { action ->
                        OutlinedButton(
                            onClick = {
                                if (action == "Charts") {
                                    onOpenCharts()
                                }
                                selectedWaypointIndex = null
                            },
                            modifier = Modifier.fillMaxWidth().height(ThumbSize),
                        ) {
                            Text(action, style = MaterialTheme.typography.labelLarge)
                        }
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
    airports: List<ChartAirport>,
    selectedAirport: ChartAirport?,
    selectedChart: ChartAsset?,
    onSelectPage: (AppPage) -> Unit,
    onSelectAirport: (String) -> Unit,
    onSelectChart: (String) -> Unit,
) {
    val context = LocalContext.current
    val density = LocalDensity.current
    var pageTrayOpen by remember { mutableStateOf(false) }
    var airportTrayOpen by remember { mutableStateOf(false) }
    var chartTrayOpen by remember { mutableStateOf(false) }
    var surfaceSize by remember { mutableStateOf(IntSize.Zero) }
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
    var viewport by remember(selectedChart?.id, surfaceSize) { mutableStateOf<net.jonh.aerobag.prototype.domain.ImageViewportState?>(null) }
    val viewportState = rememberUpdatedState(viewport)
    val imageWidthPx = bitmap?.width?.toFloat() ?: 0f
    val imageHeightPx = bitmap?.height?.toFloat() ?: 0f
    val trayOpen = airportTrayOpen || chartTrayOpen

    LaunchedEffect(bitmap, surfaceSize) {
        val currentBitmap = bitmap
        if (currentBitmap != null && surfaceSize.width > 0 && surfaceSize.height > 0) {
            viewport = createInitialImageViewport(
                imageWidthPx = currentBitmap.width.toFloat(),
                imageHeightPx = currentBitmap.height.toFloat(),
                viewportWidthPx = surfaceSize.width.toFloat(),
                viewportHeightPx = surfaceSize.height.toFloat(),
            )
        }
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(
                brush = Brush.verticalGradient(
                    colors = listOf(Color(0xFFF6F1E7), Color(0xFFDCE4E4)),
                ),
            )
            .onSizeChanged { surfaceSize = it }
            .focusable()
            .onPreviewKeyEvent { event ->
                if (bitmap == null || viewportState.value == null || trayOpen || event.nativeKeyEvent.action != AndroidKeyEvent.ACTION_DOWN) {
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
                viewport = zoomImageAroundPoint(
                    state = viewportState.value ?: return@onPreviewKeyEvent false,
                    anchorX = surfaceSize.width / 2f,
                    anchorY = surfaceSize.height / 2f,
                    nextZoom = nextZoom,
                    imageWidthPx = imageWidthPx,
                    imageHeightPx = imageHeightPx,
                    viewportWidthPx = surfaceSize.width.toFloat(),
                    viewportHeightPx = surfaceSize.height.toFloat(),
                    overscrollPx = overscrollPx,
                )
                true
            }
            .pointerInput(bitmap, surfaceSize, trayOpen) {
                if (bitmap == null || viewportState.value == null || trayOpen) {
                    return@pointerInput
                }
                detectTapGestures(
                    onDoubleTap = { tap ->
                        viewport = zoomImageAroundPoint(
                            state = viewportState.value ?: return@detectTapGestures,
                            anchorX = tap.x,
                            anchorY = tap.y,
                            nextZoom = (viewportState.value?.zoom ?: 1f) + 0.75f,
                            imageWidthPx = imageWidthPx,
                            imageHeightPx = imageHeightPx,
                            viewportWidthPx = surfaceSize.width.toFloat(),
                            viewportHeightPx = surfaceSize.height.toFloat(),
                            overscrollPx = overscrollPx,
                        )
                    },
                )
            }
            .pointerInput(bitmap, surfaceSize, trayOpen) {
                if (bitmap == null || viewportState.value == null || trayOpen) {
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
                    viewport = next
                }
            }
            .pointerInteropFilter { event ->
                if (bitmap == null || viewportState.value == null || trayOpen) {
                    return@pointerInteropFilter false
                }
                if (event.action == MotionEvent.ACTION_SCROLL) {
                    val wheelDelta = event.getAxisValue(MotionEvent.AXIS_VSCROLL).takeIf { it != 0f }
                        ?: event.getAxisValue(MotionEvent.AXIS_SCROLL)
                    viewport = zoomImageAroundPoint(
                        state = viewportState.value ?: return@pointerInteropFilter false,
                        anchorX = surfaceSize.width / 2f,
                        anchorY = surfaceSize.height / 2f,
                        nextZoom = (viewportState.value?.zoom ?: 1f) - wheelDelta * 0.18f,
                        imageWidthPx = imageWidthPx,
                        imageHeightPx = imageHeightPx,
                        viewportWidthPx = surfaceSize.width.toFloat(),
                        viewportHeightPx = surfaceSize.height.toFloat(),
                        overscrollPx = overscrollPx,
                    )
                    true
                } else {
                    false
                }
            },
    ) {
        if (trayOpen) {
            Scrim {
                airportTrayOpen = false
                chartTrayOpen = false
            }
        }

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

        ChartViewerSelectors(
            modifier = Modifier.align(Alignment.TopStart),
            currentPage = page,
            airports = airports,
            selectedAirport = selectedAirport,
            selectedChart = selectedChart,
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
            onSelectAirport = {
                onSelectAirport(it)
                airportTrayOpen = false
            },
            onSelectChart = {
                onSelectChart(it)
                chartTrayOpen = false
            },
        )

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
            width = ThumbSize * 2.4f,
            options = PageOptions.map { option ->
                MenuDockOption(option.page.name, option.label) { onSelectPage(option.page) }
            },
        )
        MenuDock(
            launcherLabel = selectedLabel,
            buttonWidth = ThumbSize,
            open = trayOpen,
            onToggle = onToggle,
            width = ThumbSize * 2.4f,
            options = trayOptions.map { option ->
                MenuDockOption(option.id, option.label, option.available) { option.select?.invoke() }
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
    pageTrayOpen: Boolean,
    airportTrayOpen: Boolean,
    chartTrayOpen: Boolean,
    onTogglePageTray: () -> Unit,
    onSelectPage: (AppPage) -> Unit,
    onToggleAirportTray: () -> Unit,
    onToggleChartTray: () -> Unit,
    onSelectAirport: (String) -> Unit,
    onSelectChart: (String) -> Unit,
) {
    Row(
        modifier = modifier.padding(ThumbGap),
        horizontalArrangement = Arrangement.spacedBy(ThumbGap),
        verticalAlignment = Alignment.Top,
    ) {
        MenuDock(
            launcherLabel = PageOptions.firstOrNull { it.page == currentPage }?.launcherLabel ?: "PLT",
            open = pageTrayOpen,
            onToggle = onTogglePageTray,
            width = ThumbSize * 2.4f,
            options = PageOptions.map { option ->
                MenuDockOption(option.page.name, option.label) { onSelectPage(option.page) }
            },
        )

        MenuDock(
            launcherLabel = selectedAirport?.label ?: "---",
            buttonWidth = ThumbSize,
            open = airportTrayOpen,
            onToggle = onToggleAirportTray,
            width = ThumbSize * 2.4f,
            options = airports.map { airport ->
                MenuDockOption(airport.id, airport.label) { onSelectAirport(airport.id) }
            },
        )

        MenuDock(
            launcherLabel = selectedChart?.label ?: "---",
            buttonWidth = ThumbSize * 3f,
            open = chartTrayOpen,
            onToggle = onToggleChartTray,
            width = ThumbSize * 4f,
            launcherMaxLines = 2,
            options = (selectedAirport?.charts ?: emptyList()).map { chart ->
                MenuDockOption(chart.id, chart.label) { onSelectChart(chart.id) }
            },
        )
    }
}

@Composable
private fun MenuDock(
    modifier: Modifier = Modifier,
    launcherLabel: String,
    open: Boolean,
    onToggle: () -> Unit,
    width: androidx.compose.ui.unit.Dp,
    buttonWidth: androidx.compose.ui.unit.Dp = ThumbSize,
    launcherMaxLines: Int = 1,
    options: List<MenuDockOption>,
) {
    Box(modifier = modifier.width(buttonWidth).height(ThumbSize)) {
        CompactSquareButton(
            label = launcherLabel,
            maxLines = launcherMaxLines,
            modifier = Modifier.width(buttonWidth).height(ThumbSize).align(Alignment.TopStart),
            onClick = onToggle,
        )
        DropdownMenu(
            expanded = open,
            onDismissRequest = onToggle,
            modifier = Modifier.width(width),
        ) {
            options.forEach { option ->
                DropdownMenuItem(
                    text = {
                        Text(
                            option.label,
                            style = MaterialTheme.typography.labelLarge,
                            maxLines = 2,
                            overflow = TextOverflow.Ellipsis,
                        )
                    },
                    enabled = option.enabled,
                    onClick = option.onSelect,
                )
            }
        }
    }
}

@Composable
private fun PlanHeaderRow() {
    Row(horizontalArrangement = Arrangement.spacedBy(1.dp)) {
        PlanCell("Waypoint", Modifier.width(ThumbSize * 2.5f), isHeader = true)
        PlanCell("Dist (nm)", Modifier.weight(1f), isHeader = true)
        PlanCell("ETE (h:m)", Modifier.weight(1f), isHeader = true)
        PlanCell("Course (°)", Modifier.weight(1f), isHeader = true)
    }
}

private data class FlightPlanRow(
    val waypoint: String,
    val distance: String,
    val ete: String,
    val course: String,
)

@Composable
private fun FlightPlanDataRow(row: FlightPlanRow, onWaypointClick: () -> Unit) {
    Row(horizontalArrangement = Arrangement.spacedBy(1.dp)) {
        Button(
            onClick = onWaypointClick,
            modifier = Modifier.width(ThumbSize * 2.5f).height(ThumbSize),
            shape = RoundedCornerShape(0.dp),
        ) {
            Text(row.waypoint, style = MaterialTheme.typography.labelLarge)
        }
        PlanCell(row.distance, Modifier.weight(1f))
        PlanCell(row.ete, Modifier.weight(1f))
        PlanCell(row.course, Modifier.weight(1f))
    }
}

@Composable
private fun PlanCell(value: String, modifier: Modifier, isHeader: Boolean = false) {
    Box(
        modifier = modifier
            .height(ThumbSize)
            .background(Color(0xFFFEFCF7))
            .border(1.dp, Color(0x1A132129))
            .padding(horizontal = 10.dp),
        contentAlignment = Alignment.CenterStart,
    ) {
        Text(
            value,
            style = if (isHeader) MaterialTheme.typography.labelMedium else MaterialTheme.typography.bodyMedium,
            color = if (isHeader) Color(0xFF52656D) else Color(0xFF132129),
            fontWeight = if (isHeader) FontWeight.Bold else FontWeight.Medium,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
private fun ToolbarButton(label: String, modifier: Modifier = Modifier, onClick: () -> Unit) {
    CompactSquareButton(label = label, modifier = modifier.size(ThumbSize), onClick = onClick)
}

@Composable
private fun CompactSquareButton(label: String, modifier: Modifier = Modifier, maxLines: Int = 1, onClick: () -> Unit) {
    Surface(
        modifier = modifier.pointerInput(onClick) {
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
        },
        shape = RoundedCornerShape(ThumbRadius),
        color = MaterialTheme.colorScheme.primary,
        contentColor = MaterialTheme.colorScheme.onPrimary,
        shadowElevation = 2.dp,
    ) {
        Box(contentAlignment = Alignment.Center) {
            Text(
                text = label,
                style = MaterialTheme.typography.labelSmall,
                maxLines = maxLines,
                overflow = TextOverflow.Clip,
            )
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
    )
}
