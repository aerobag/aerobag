package net.jonh.aerobag.prototype

import android.graphics.BitmapFactory
import android.graphics.Paint
import android.os.Bundle
import android.view.KeyEvent as AndroidKeyEvent
import android.view.MotionEvent
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.focusable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.wrapContentWidth
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.horizontalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.input.pointer.PointerId
import androidx.compose.ui.input.pointer.pointerInteropFilter
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import net.jonh.aerobag.prototype.domain.MapView
import net.jonh.aerobag.prototype.domain.MapViewportState
import net.jonh.aerobag.prototype.domain.ScreenPoint
import net.jonh.aerobag.prototype.domain.SectionalPackages
import net.jonh.aerobag.prototype.domain.SampleData
import net.jonh.aerobag.prototype.domain.TileStorageKind
import net.jonh.aerobag.prototype.domain.applyPinchGesture
import net.jonh.aerobag.prototype.domain.clampZoom
import net.jonh.aerobag.prototype.domain.createInitialViewport
import net.jonh.aerobag.prototype.domain.createPinchSnapshot
import net.jonh.aerobag.prototype.domain.dragViewport
import net.jonh.aerobag.prototype.domain.renderTiles
import net.jonh.aerobag.prototype.domain.viewportCenterLatLon
import net.jonh.aerobag.prototype.domain.zoomAroundPoint

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            MaterialTheme {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = Color(0xFFF3EFE4),
                ) {
                    MapExplorerScreen()
                }
            }
        }
    }
}

@OptIn(ExperimentalComposeUiApi::class)
@Composable
private fun MapExplorerScreen() {
    val context = LocalContext.current
    val density = LocalDensity.current
    val fixture = remember(context) { SampleData.load(context.applicationContext) }
    val coroutineScope = rememberCoroutineScope()
    var selectedMapId by remember { mutableStateOf(fixture.mapViews.first().id) }
    val selectedMap = remember(selectedMapId, fixture.mapViews) {
        fixture.mapViews.find { it.id == selectedMapId } ?: fixture.mapViews.first()
    }
    var viewport by remember(selectedMap.id) { mutableStateOf(createInitialViewport(selectedMap.mapView)) }
    var interactionLabel by remember { mutableStateOf("idle") }
    var debugTileLabels by remember { mutableStateOf(true) }
    var surfaceSize by remember { mutableStateOf(IntSize.Zero) }
    var installingPackage by remember { mutableStateOf<String?>(null) }
    var installRevision by remember { mutableStateOf(0) }
    val focusRequester = remember { FocusRequester() }
    val viewportState = rememberUpdatedState(viewport)
    val center = remember(viewport) { viewportCenterLatLon(viewport) }
    val surfaceWidthUnits = remember(surfaceSize, density) {
        with(density) { surfaceSize.width.toDp().value }
    }
    val surfaceHeightUnits = remember(surfaceSize, density) {
        with(density) { surfaceSize.height.toDp().value }
    }
    val tiles = remember(viewport, surfaceSize, selectedMap.mapView) {
        if (surfaceSize.width == 0 || surfaceSize.height == 0) {
            emptyList()
        } else {
            renderTiles(
                mapView = selectedMap.mapView,
                viewport = viewport,
                widthPx = surfaceWidthUnits,
                heightPx = surfaceHeightUnits,
            )
        }
    }
    val isInstalled = remember(selectedMap.id, installRevision) {
        val packageName = selectedMap.mapView.packageName
        packageName != null && SectionalPackages.isInstalled(context, packageName)
    }

    val tileRects = remember(tiles, density) {
        val columns = tiles
            .groupBy { it.x }
            .mapValues { (_, entries) -> with(density) { entries.minOf { it.leftPx.dp.roundToPx() } } }
            .toList()
            .sortedBy { it.second }
        val rows = tiles
            .groupBy { it.yTms }
            .mapValues { (_, entries) -> with(density) { entries.minOf { it.topPx.dp.roundToPx() } } }
            .toList()
            .sortedBy { it.second }

        val columnRects = columns.mapIndexed { index, (x, leftPx) ->
            val rightPx = if (index + 1 < columns.size) {
                columns[index + 1].second
            } else {
                with(density) {
                    val sample = tiles.first { it.x == x }
                    (sample.leftPx + sample.sizePx).dp.roundToPx()
                }
            }
            x to (leftPx to (rightPx - leftPx))
        }.toMap()

        val rowRects = rows.mapIndexed { index, (yTms, topPx) ->
            val bottomPx = if (index + 1 < rows.size) {
                rows[index + 1].second
            } else {
                with(density) {
                    val sample = tiles.first { it.yTms == yTms }
                    (sample.topPx + sample.sizePx).dp.roundToPx()
                }
            }
            yTms to (topPx to (bottomPx - topPx))
        }.toMap()

        tiles.associate { tile ->
            val (leftPx, widthPx) = columnRects.getValue(tile.x)
            val (topPx, heightPx) = rowRects.getValue(tile.yTms)
            Triple(tile.zoom, tile.x, tile.yTms) to TileRect(
                leftPx = leftPx,
                topPx = topPx,
                widthPx = widthPx,
                heightPx = heightPx,
            )
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

    LaunchedEffect(Unit) {
        focusRequester.requestFocus()
    }

    LaunchedEffect(selectedMap.id) {
        viewport = createInitialViewport(selectedMap.mapView)
        interactionLabel = "idle"
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .focusRequester(focusRequester)
            .focusable()
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

                    viewport = zoomAroundPoint(
                        viewport = viewportState.value,
                        mapView = selectedMap.mapView,
                        anchor = ScreenPoint(surfaceWidthUnits / 2f, surfaceHeightUnits / 2f),
                        widthPx = surfaceWidthUnits,
                        heightPx = surfaceHeightUnits,
                        nextZoom = clampZoom(viewportState.value.zoom + delta, selectedMap.mapView),
                    )
                interactionLabel = "key ${if (delta > 0) "+" else "-"}"
                true
            }
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
                        if (pressed.isEmpty()) {
                            break
                        }

                        if (pressed.size == 1) {
                            val change = pressed.first()
                            if (dragPointerId != change.id || dragLastPosition == null) {
                                dragPointerId = change.id
                                dragLastPosition = change.position
                                pinchSnapshot = null
                            } else {
                                val last = dragLastPosition ?: change.position
                                    viewport = dragViewport(
                                        viewportState.value,
                                        dx = with(density) { (change.position.x - last.x).toDp().value },
                                        dy = with(density) { (change.position.y - last.y).toDp().value },
                                )
                                interactionLabel = "drag"
                                dragLastPosition = change.position
                            }
                            change.consume()
                        } else {
                            val first = pressed[0]
                            val second = pressed[1]
                            if (pinchSnapshot == null) {
                                pinchSnapshot = createPinchSnapshot(
                                    viewport = viewportState.value,
                                    first = ScreenPoint(
                                        with(density) { first.position.x.toDp().value },
                                        with(density) { first.position.y.toDp().value },
                                    ),
                                    second = ScreenPoint(
                                        with(density) { second.position.x.toDp().value },
                                        with(density) { second.position.y.toDp().value },
                                    ),
                                    widthPx = surfaceWidthUnits,
                                    heightPx = surfaceHeightUnits,
                                )
                            }
                            viewport = applyPinchGesture(
                                    snapshot = pinchSnapshot,
                                    currentFirst = ScreenPoint(
                                        with(density) { first.position.x.toDp().value },
                                        with(density) { first.position.y.toDp().value },
                                    ),
                                currentSecond = ScreenPoint(
                                    with(density) { second.position.x.toDp().value },
                                    with(density) { second.position.y.toDp().value },
                                ),
                                    mapView = selectedMap.mapView,
                                    widthPx = surfaceWidthUnits,
                                    heightPx = surfaceHeightUnits,
                                )
                            interactionLabel = "pinch"
                            dragPointerId = null
                            dragLastPosition = null
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
                    viewport = zoomAroundPoint(
                        viewport = viewportState.value,
                        mapView = selectedMap.mapView,
                        anchor = ScreenPoint(surfaceWidthUnits / 2f, surfaceHeightUnits / 2f),
                        widthPx = surfaceWidthUnits,
                        heightPx = surfaceHeightUnits,
                        nextZoom = clampZoom(
                            viewportState.value.zoom - wheelDelta * 0.28,
                            selectedMap.mapView,
                        ),
                    )
                    interactionLabel = "wheel ${"%.2f".format(wheelDelta)}"
                    true
                } else {
                    false
                }
            },
    ) {
        val tileBitmaps = remember(tiles, selectedMap.id, installRevision) {
            tiles.associate { tile ->
                Triple(tile.zoom, tile.x, tile.yTms) to runCatching {
                    SectionalPackages.loadTileBytes(context, selectedMap.mapView, tile)
                        ?.let { BitmapFactory.decodeByteArray(it, 0, it.size)?.asImageBitmap() }
                }.getOrNull()
            }
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
                        size = androidx.compose.ui.geometry.Size(
                            tileRect.widthPx.toFloat(),
                            tileRect.heightPx.toFloat(),
                        ),
                    )
                }
                if (debugTileLabels) {
                    val label = "z${tile.zoom} x${tile.x} y${tile.yTms}"
                    val padding = 8f
                    val baseline = tileRect.topPx + 30f
                    val textWidth = tileLabelPaint.measureText(label)
                    val rectLeft = tileRect.leftPx + 6f
                    val rectTop = tileRect.topPx + 6f
                    val rectRight = rectLeft + textWidth + padding * 2f
                    val rectBottom = rectTop + 30f
                    drawContext.canvas.nativeCanvas.apply {
                        drawRoundRect(
                            rectLeft,
                            rectTop,
                            rectRight,
                            rectBottom,
                            8f,
                            8f,
                            tileLabelBackgroundPaint,
                        )
                        drawText(label, rectLeft + padding, baseline, tileLabelPaint)
                    }
                }
            }
        }

        Card(
            modifier = Modifier
                .align(Alignment.TopStart)
                .padding(8.dp)
                .widthIn(max = 250.dp),
        ) {
            Column(
                modifier = Modifier.padding(horizontal = 10.dp, vertical = 8.dp),
                verticalArrangement = Arrangement.spacedBy(5.dp),
            ) {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .horizontalScroll(rememberScrollState()),
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    fixture.mapViews.forEach { mapOption ->
                        val isSelected = mapOption.id == selectedMap.id
                        if (isSelected) {
                            Button(onClick = { }) {
                                Text(
                                    text = mapOption.regionId.uppercase(),
                                    style = MaterialTheme.typography.labelMedium,
                                )
                            }
                        } else {
                            OutlinedButton(onClick = { selectedMapId = mapOption.id }) {
                                Text(
                                    text = mapOption.regionId.uppercase(),
                                    style = MaterialTheme.typography.labelMedium,
                                )
                            }
                        }
                    }
                }
                OutlinedButton(onClick = { debugTileLabels = !debugTileLabels }) {
                    Text(
                        text = if (debugTileLabels) "Hide labels" else "Show labels",
                        style = MaterialTheme.typography.labelMedium,
                    )
                }
                if (selectedMap.mapView.storageKind == TileStorageKind.SectionalPackage) {
                    val packageName = selectedMap.mapView.packageName.orEmpty()
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.spacedBy(10.dp),
                    ) {
                        OutlinedButton(
                            onClick = {
                                coroutineScope.launch {
                                    installingPackage = packageName
                                    withContext(Dispatchers.IO) {
                                        SectionalPackages.install(context, packageName)
                                    }
                                    installRevision += 1
                                    installingPackage = null
                                }
                            },
                            enabled = !isInstalled && installingPackage == null,
                        ) {
                            Text(
                                when {
                                    isInstalled -> "Installed"
                                    installingPackage == packageName -> "Installing…"
                                    else -> "Install ${packageName}"
                                },
                            )
                        }
                        Text(
                            text = if (isInstalled) "Local package ready" else "Not installed yet",
                            style = MaterialTheme.typography.labelSmall,
                            color = Color(0xFF52656D),
                            modifier = Modifier.align(Alignment.CenterVertically),
                        )
                    }
                }
                Text(
                    text = "${selectedMap.mapView.chartName} • $interactionLabel",
                    style = MaterialTheme.typography.labelSmall,
                    color = Color(0xFF52656D),
                )
            }
        }

        Card(
            modifier = Modifier
                .align(Alignment.BottomStart)
                .padding(8.dp)
                .widthIn(max = 150.dp),
        ) {
            Column(
                modifier = Modifier.padding(8.dp),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                MetricRow("Latitude", "%.4f".format(center.first))
                MetricRow("Longitude", "%.4f".format(center.second))
                MetricRow("Zoom", "%.2f".format(viewport.zoom))
            }
        }

    }
}

private data class TileRect(
    val leftPx: Int,
    val topPx: Int,
    val widthPx: Int,
    val heightPx: Int,
)

@Composable
private fun MetricRow(label: String, value: String) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .border(1.dp, Color(0x1F182128), MaterialTheme.shapes.medium)
            .padding(horizontal = 10.dp, vertical = 8.dp),
    ) {
        Text(
            text = label,
            style = MaterialTheme.typography.labelSmall,
            color = Color(0xFF52656D),
        )
        Text(
            text = value,
            style = MaterialTheme.typography.bodyMedium,
            fontWeight = FontWeight.Bold,
        )
    }
}
