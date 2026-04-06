package net.jonh.aerobag.prototype

import android.graphics.BitmapFactory
import android.os.Bundle
import android.view.KeyEvent as AndroidKeyEvent
import android.view.MotionEvent
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.Image
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
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
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
import kotlin.math.min
import net.jonh.aerobag.prototype.domain.MapView
import net.jonh.aerobag.prototype.domain.MapViewportState
import net.jonh.aerobag.prototype.domain.ScreenPoint
import net.jonh.aerobag.prototype.domain.SampleData
import net.jonh.aerobag.prototype.domain.applyPinchGesture
import net.jonh.aerobag.prototype.domain.clampZoom
import net.jonh.aerobag.prototype.domain.createInitialViewport
import net.jonh.aerobag.prototype.domain.createPinchSnapshot
import net.jonh.aerobag.prototype.domain.dragViewport
import net.jonh.aerobag.prototype.domain.renderTiles
import net.jonh.aerobag.prototype.domain.tileAssetPath
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
    var viewport by remember { mutableStateOf(createInitialViewport(fixture.mapView)) }
    var interactionLabel by remember { mutableStateOf("idle") }
    var surfaceSize by remember { mutableStateOf(IntSize.Zero) }
    val focusRequester = remember { FocusRequester() }
    val viewportState = rememberUpdatedState(viewport)
    val center = remember(viewport) { viewportCenterLatLon(viewport) }
    val surfaceWidthUnits = remember(surfaceSize, density) {
        with(density) { surfaceSize.width.toDp().value }
    }
    val surfaceHeightUnits = remember(surfaceSize, density) {
        with(density) { surfaceSize.height.toDp().value }
    }
    val tiles = remember(viewport, surfaceSize, fixture.mapView) {
        if (surfaceSize.width == 0 || surfaceSize.height == 0) {
            emptyList()
        } else {
            renderTiles(
                mapView = fixture.mapView,
                viewport = viewport,
                widthPx = surfaceWidthUnits,
                heightPx = surfaceHeightUnits,
            )
        }
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

    LaunchedEffect(Unit) {
        focusRequester.requestFocus()
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
                    mapView = fixture.mapView,
                    anchor = ScreenPoint(surfaceWidthUnits / 2f, surfaceHeightUnits / 2f),
                    widthPx = surfaceWidthUnits,
                    heightPx = surfaceHeightUnits,
                    nextZoom = clampZoom(viewportState.value.zoom + delta, fixture.mapView),
                )
                interactionLabel = "key ${if (delta > 0) "+" else "-"}"
                true
            }
            .pointerInput(fixture.mapView, surfaceSize) {
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
                                mapView = fixture.mapView,
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
                        mapView = fixture.mapView,
                        anchor = ScreenPoint(surfaceWidthUnits / 2f, surfaceHeightUnits / 2f),
                        widthPx = surfaceWidthUnits,
                        heightPx = surfaceHeightUnits,
                        nextZoom = clampZoom(
                            viewportState.value.zoom - wheelDelta * 0.28,
                            fixture.mapView,
                        ),
                    )
                    interactionLabel = "wheel ${"%.2f".format(wheelDelta)}"
                    true
                } else {
                    false
                }
            },
    ) {
        val tileBitmaps = remember(tiles) {
            tiles.associate { tile ->
                Triple(tile.zoom, tile.x, tile.yTms) to runCatching {
                    context.assets.open(tileAssetPath(fixture.mapView, tile)).use { stream ->
                        BitmapFactory.decodeStream(stream)?.asImageBitmap()
                    }
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
            }
        }

        Card(
            modifier = Modifier
                .align(Alignment.TopStart)
                .padding(18.dp),
        ) {
            Column(
                modifier = Modifier.padding(18.dp),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                Text(
                    text = "Avare Android Prototype",
                    style = MaterialTheme.typography.labelLarge,
                    color = Color(0xFF0D6F67),
                )
                Text(
                    text = fixture.mapView.chartName,
                    style = MaterialTheme.typography.headlineMedium,
                    fontWeight = FontWeight.Bold,
                )
                Text(
                    text = "Drag to pan. Pinch to zoom. For emulator debugging in this setup, use keyboard +/-.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = Color(0xFF52656D),
                )
                Text(
                    text = "Input $interactionLabel",
                    style = MaterialTheme.typography.labelMedium,
                    color = Color(0xFF52656D),
                )
            }
        }

        Card(
            modifier = Modifier
                .align(Alignment.BottomStart)
                .padding(18.dp),
        ) {
            Column(
                modifier = Modifier.padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
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
            .padding(12.dp),
    ) {
        Text(
            text = label,
            style = MaterialTheme.typography.labelSmall,
            color = Color(0xFF52656D),
        )
        Text(
            text = value,
            style = MaterialTheme.typography.bodyLarge,
            fontWeight = FontWeight.Bold,
        )
    }
}
