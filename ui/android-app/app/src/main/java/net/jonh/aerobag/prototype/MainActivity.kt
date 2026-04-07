package net.jonh.aerobag.prototype

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
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.Card
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
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import net.jonh.aerobag.prototype.domain.ChartAirport
import net.jonh.aerobag.prototype.domain.ChartAsset
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

private enum class AppPage {
    Map,
    Plan,
    Charts,
}

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
    var page by remember { mutableStateOf(AppPage.Map) }
    var selectedMapId by remember { mutableStateOf(fixture.mapViews.first().id) }
    val selectedMap = remember(selectedMapId, fixture.mapViews) {
        fixture.mapViews.find { it.id == selectedMapId } ?: fixture.mapViews.first()
    }
    var mapViewport by remember { mutableStateOf(createInitialViewport(selectedMap.mapView)) }
    var selectedAirportId by remember { mutableStateOf(fixture.chartPage.initialAirportId.ifEmpty { fixture.chartPage.airports.firstOrNull()?.id.orEmpty() }) }
    var selectedChartId by remember { mutableStateOf(fixture.chartPage.initialChartId.ifEmpty { fixture.chartPage.airports.firstOrNull()?.charts?.firstOrNull()?.id.orEmpty() }) }
    val selectedAirport = remember(selectedAirportId, fixture.chartPage.airports) {
        fixture.chartPage.airports.find { it.id == selectedAirportId } ?: fixture.chartPage.airports.firstOrNull()
    }
    val selectedChart = remember(selectedAirport, selectedChartId) {
        selectedAirport?.charts?.find { it.id == selectedChartId } ?: selectedAirport?.charts?.firstOrNull()
    }
    val legSummary = remember(fixture.samplePlan) {
        fixture.samplePlan.legs.firstOrNull()?.let { "K${it.fromAirport} -> K${it.toAirport} CRS 342" } ?: "NO LEG"
    }

    LaunchedEffect(selectedMap.id) {
        mapViewport = preserveViewportForMap(mapViewport, selectedMap.mapView)
    }

    when (page) {
        AppPage.Map -> MapExplorerPage(
            fixture = fixture,
            selectedMapId = selectedMapId,
            viewport = mapViewport,
            onViewportChange = { mapViewport = it },
            onSelectMapId = { selectedMapId = it },
            onOpenPlan = { page = AppPage.Plan },
            legSummary = legSummary,
        )
        AppPage.Plan -> FlightPlanPage(
            legSummary = legSummary,
            samplePlan = fixture.samplePlan,
            onBack = { page = AppPage.Map },
            onOpenCharts = { page = AppPage.Charts },
        )
        AppPage.Charts -> ChartsPage(
            airports = fixture.chartPage.airports,
            selectedAirport = selectedAirport,
            selectedChart = selectedChart,
            onBack = { page = AppPage.Plan },
            onSelectAirport = { airportId ->
                selectedAirportId = airportId
                selectedChartId = fixture.chartPage.airports.find { it.id == airportId }?.charts?.firstOrNull()?.id.orEmpty()
            },
            onSelectChart = { selectedChartId = it },
        )
    }
}

@OptIn(ExperimentalComposeUiApi::class)
@Composable
private fun MapExplorerPage(
    fixture: net.jonh.aerobag.prototype.domain.ContentFixture,
    selectedMapId: String,
    viewport: MapViewportState,
    onViewportChange: (MapViewportState) -> Unit,
    onSelectMapId: (String) -> Unit,
    onOpenPlan: () -> Unit,
    legSummary: String,
) {
    val context = LocalContext.current
    val density = LocalDensity.current
    var chartTrayOpen by remember { mutableStateOf(false) }
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
    val familyPackageNames = remember(selectedFamilyMapViews) {
        selectedFamilyMapViews.mapNotNull { it.mapView.packageName }.distinct()
    }
    val installedPackageCount = remember(familyPackageNames, installRevision) {
        familyPackageNames.count { SectionalPackages.isInstalled(context, it) }
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

    LaunchedEffect(selectedMap.mapView.chartFamily, familyPackageNames, installedPackageCount) {
        if (selectedMap.mapView.storageKind != TileStorageKind.SectionalPackage) {
            return@LaunchedEffect
        }
        val missingPackages = familyPackageNames.filterNot { SectionalPackages.isInstalled(context, it) }
        if (missingPackages.isEmpty()) {
            return@LaunchedEffect
        }
        installingPackage = missingPackages.first()
        withContext(Dispatchers.IO) {
            for (packageName in missingPackages) {
                SectionalPackages.install(context, packageName)
            }
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
                        val pressed = event.changes.filter { it.pressed }
                        if (pressed.isEmpty()) break
                        if (chartTrayOpen) {
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
        if (chartTrayOpen) {
            Scrim { chartTrayOpen = false }
        }

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

        MapTray(
            modifier = Modifier.align(Alignment.TopStart),
            selectedLabel = selectedLauncher.launcherLabel,
            trayOptions = trayOptions,
            trayOpen = chartTrayOpen,
            onToggle = { chartTrayOpen = !chartTrayOpen },
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

        OutlinedButton(
            onClick = { debugTileLabels = !debugTileLabels },
            modifier = Modifier
                .align(Alignment.BottomStart)
                .padding(ThumbGap)
                .height(ThumbSize * 0.7f),
        ) {
            Text(if (debugTileLabels) "DBG ON" else "DBG", style = MaterialTheme.typography.labelSmall)
        }

        if (installingPackage != null || selectedMap.mapView.storageKind == TileStorageKind.SectionalPackage) {
            Card(
                modifier = Modifier
                    .align(Alignment.BottomEnd)
                    .padding(ThumbGap),
            ) {
                Text(
                    text = when {
                        installingPackage != null -> "Installing ${installingPackage}…"
                        installedPackageCount == familyPackageNames.size -> "Local ${selectedMap.mapView.chartFamily.name}"
                        else -> "Package missing"
                    },
                    modifier = Modifier.padding(horizontal = 10.dp, vertical = 8.dp),
                    style = MaterialTheme.typography.labelSmall,
                    color = Color(0xFF52656D),
                )
            }
        }
    }
}

@Composable
private fun FlightPlanPage(
    legSummary: String,
    samplePlan: net.jonh.aerobag.prototype.domain.FlightPlan,
    onBack: () -> Unit,
    onOpenCharts: () -> Unit,
) {
    var selectedWaypointIndex by remember { mutableStateOf<Int?>(null) }
    val rows = remember(samplePlan) {
        samplePlan.legs.mapIndexed { index, leg ->
            FlightPlanRow(
                waypoint = "K${leg.toAirport}",
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
        ToolbarButton(label = "MAP", modifier = Modifier.align(Alignment.TopStart).padding(ThumbGap), onClick = onBack)

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
    airports: List<ChartAirport>,
    selectedAirport: ChartAirport?,
    selectedChart: ChartAsset?,
    onBack: () -> Unit,
    onSelectAirport: (String) -> Unit,
    onSelectChart: (String) -> Unit,
) {
    val context = LocalContext.current
    val density = LocalDensity.current
    var airportTrayOpen by remember { mutableStateOf(false) }
    var chartTrayOpen by remember { mutableStateOf(false) }
    var surfaceSize by remember { mutableStateOf(IntSize.Zero) }
    val overscrollPx = with(density) { ThumbSize.toPx() }
    val bitmap by produceState<androidx.compose.ui.graphics.ImageBitmap?>(initialValue = null, selectedChart?.assetPath) {
        value = selectedChart?.assetPath?.let { path ->
            withContext(Dispatchers.IO) {
                runCatching {
                    context.assets.open(path).use { stream ->
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
            airports = airports,
            selectedAirport = selectedAirport,
            selectedChart = selectedChart,
            airportTrayOpen = airportTrayOpen,
            chartTrayOpen = chartTrayOpen,
            onToggleAirportTray = {
                airportTrayOpen = !airportTrayOpen
                chartTrayOpen = false
            },
            onToggleChartTray = {
                chartTrayOpen = !chartTrayOpen
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

        ToolbarButton(label = "PLAN", modifier = Modifier.align(Alignment.TopEnd).padding(ThumbGap), onClick = onBack)
    }
}

@Composable
private fun MapTray(
    modifier: Modifier = Modifier,
    selectedLabel: String,
    trayOptions: List<ChartTrayOption>,
    trayOpen: Boolean,
    onToggle: () -> Unit,
) {
    Card(
        modifier = modifier
            .padding(ThumbGap)
            .widthIn(min = ThumbSize, max = ThumbSize * 2.6f),
    ) {
        Column(
            modifier = Modifier.padding(ThumbGap),
            verticalArrangement = Arrangement.spacedBy(ThumbGap),
        ) {
            CompactSquareButton(label = selectedLabel, modifier = Modifier.size(ThumbSize), onClick = onToggle)
            AnimatedVisibility(
                visible = trayOpen,
                enter = slideInHorizontally(initialOffsetX = { -it / 3 }) + fadeIn(),
                exit = slideOutHorizontally(targetOffsetX = { -it / 3 }) + fadeOut(),
            ) {
                Column(verticalArrangement = Arrangement.spacedBy(ThumbGap)) {
                    trayOptions.forEach { option ->
                        OutlinedButton(
                            onClick = option.select ?: {},
                            enabled = option.available,
                            modifier = Modifier.fillMaxWidth().height(ThumbSize),
                        ) {
                            Text(option.label, style = MaterialTheme.typography.labelLarge)
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun ChartViewerSelectors(
    modifier: Modifier = Modifier,
    airports: List<ChartAirport>,
    selectedAirport: ChartAirport?,
    selectedChart: ChartAsset?,
    airportTrayOpen: Boolean,
    chartTrayOpen: Boolean,
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
        SelectorDock(
            label = selectedAirport?.label ?: "---",
            buttonWidth = ThumbSize * 1.2f,
            open = airportTrayOpen,
            onToggle = onToggleAirportTray,
        ) {
            airports.forEach { airport ->
                OutlinedButton(
                    onClick = { onSelectAirport(airport.id) },
                    modifier = Modifier.fillMaxWidth().height(ThumbSize),
                ) {
                    Text(airport.label, style = MaterialTheme.typography.labelLarge)
                }
            }
        }

        SelectorDock(
            label = selectedChart?.label ?: "---",
            buttonWidth = ThumbSize * 2.6f,
            open = chartTrayOpen,
            onToggle = onToggleChartTray,
        ) {
            selectedAirport?.charts?.forEach { chart ->
                OutlinedButton(
                    onClick = { onSelectChart(chart.id) },
                    modifier = Modifier.fillMaxWidth().height(ThumbSize),
                ) {
                    Text(chart.label, style = MaterialTheme.typography.labelLarge, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
        }
    }
}

@Composable
private fun SelectorDock(
    label: String,
    buttonWidth: androidx.compose.ui.unit.Dp,
    open: Boolean,
    onToggle: () -> Unit,
    content: @Composable () -> Unit,
) {
    Card {
        Column(
            modifier = Modifier.padding(ThumbGap),
            verticalArrangement = Arrangement.spacedBy(ThumbGap),
        ) {
            Button(
                onClick = onToggle,
                modifier = Modifier.width(buttonWidth).height(ThumbSize),
                shape = RoundedCornerShape(ThumbRadius),
            ) {
                Text(label, maxLines = 1, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.labelMedium)
            }
            AnimatedVisibility(
                visible = open,
                enter = slideInHorizontally(initialOffsetX = { -it / 3 }) + fadeIn(),
                exit = slideOutHorizontally(targetOffsetX = { -it / 3 }) + fadeOut(),
            ) {
                Column(verticalArrangement = Arrangement.spacedBy(ThumbGap), content = { content() })
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
private fun CompactSquareButton(label: String, modifier: Modifier = Modifier, onClick: () -> Unit) {
    Surface(
        modifier = modifier.clickable(onClick = onClick),
        shape = RoundedCornerShape(ThumbRadius),
        color = MaterialTheme.colorScheme.primary,
        contentColor = MaterialTheme.colorScheme.onPrimary,
        shadowElevation = 2.dp,
    ) {
        Box(contentAlignment = Alignment.Center) {
            Text(
                text = label,
                style = MaterialTheme.typography.labelSmall,
                maxLines = 1,
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
