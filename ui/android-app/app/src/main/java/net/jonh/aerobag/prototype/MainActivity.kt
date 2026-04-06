package net.jonh.aerobag.prototype

import android.graphics.BitmapFactory
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
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
import androidx.compose.ui.input.pointer.PointerId
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import kotlin.math.roundToInt
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

@Composable
private fun MapExplorerScreen() {
    val context = LocalContext.current
    val density = LocalDensity.current
    val fixture = remember(context) { SampleData.load(context.applicationContext) }
    var viewport by remember { mutableStateOf(createInitialViewport(fixture.mapView)) }
    var surfaceSize by remember { mutableStateOf(IntSize.Zero) }
    val viewportState = rememberUpdatedState(viewport)
    val center = remember(viewport) { viewportCenterLatLon(viewport) }
    val tiles = remember(viewport, surfaceSize, fixture.mapView) {
        if (surfaceSize.width == 0 || surfaceSize.height == 0) {
            emptyList()
        } else {
            renderTiles(
                mapView = fixture.mapView,
                viewport = viewport,
                widthPx = surfaceSize.width.toFloat(),
                heightPx = surfaceSize.height.toFloat(),
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
            .pointerInput(fixture.mapView, surfaceSize) {
                if (surfaceSize.width == 0 || surfaceSize.height == 0) {
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
                                    dx = change.position.x - last.x,
                                    dy = change.position.y - last.y,
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
                                    first = ScreenPoint(first.position.x, first.position.y),
                                    second = ScreenPoint(second.position.x, second.position.y),
                                    widthPx = surfaceSize.width.toFloat(),
                                    heightPx = surfaceSize.height.toFloat(),
                                )
                            }
                            viewport = applyPinchGesture(
                                snapshot = pinchSnapshot,
                                currentFirst = ScreenPoint(first.position.x, first.position.y),
                                currentSecond = ScreenPoint(second.position.x, second.position.y),
                                mapView = fixture.mapView,
                                widthPx = surfaceSize.width.toFloat(),
                                heightPx = surfaceSize.height.toFloat(),
                            )
                            dragPointerId = null
                            dragLastPosition = null
                            first.consume()
                            second.consume()
                        }
                    }
                }
            },
    ) {
        tiles.forEach { tile ->
            val bitmap = remember(tile.x, tile.yTms, tile.zoom) {
                runCatching {
                    context.assets.open(tileAssetPath(fixture.mapView, tile)).use { stream ->
                        BitmapFactory.decodeStream(stream)?.asImageBitmap()
                    }
                }.getOrNull()
            }
            if (bitmap != null) {
                Image(
                    bitmap = bitmap,
                    contentDescription = null,
                    modifier = Modifier
                        .offset { IntOffset(tile.leftPx.roundToInt(), tile.topPx.roundToInt()) }
                        .size(with(density) { tile.sizePx.toDp() }),
                )
            } else {
                Box(
                    modifier = Modifier
                        .offset { IntOffset(tile.leftPx.roundToInt(), tile.topPx.roundToInt()) }
                        .size(with(density) { tile.sizePx.toDp() })
                        .background(Color(0x12000000)),
                )
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
                    text = "Drag to pan. Pinch or use the zoom buttons to explore the tiled chart with a continuous zoom state.",
                    style = MaterialTheme.typography.bodyMedium,
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

        Column(
            modifier = Modifier
                .align(Alignment.BottomEnd)
                .padding(18.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Button(onClick = {
                if (surfaceSize.width == 0 || surfaceSize.height == 0) {
                    return@Button
                }
                viewport = zoomAroundPoint(
                    viewport = viewport,
                    mapView = fixture.mapView,
                    anchor = ScreenPoint(surfaceSize.width / 2f, surfaceSize.height / 2f),
                    widthPx = surfaceSize.width.toFloat(),
                    heightPx = surfaceSize.height.toFloat(),
                    nextZoom = clampZoom(viewport.zoom + 0.35, fixture.mapView),
                )
            }) {
                Text("+")
            }
            Button(onClick = {
                if (surfaceSize.width == 0 || surfaceSize.height == 0) {
                    return@Button
                }
                viewport = zoomAroundPoint(
                    viewport = viewport,
                    mapView = fixture.mapView,
                    anchor = ScreenPoint(surfaceSize.width / 2f, surfaceSize.height / 2f),
                    widthPx = surfaceSize.width.toFloat(),
                    heightPx = surfaceSize.height.toFloat(),
                    nextZoom = clampZoom(viewport.zoom - 0.35, fixture.mapView),
                )
            }) {
                Text("-")
            }
        }
    }
}

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
