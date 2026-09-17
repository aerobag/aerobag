// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.graphics.Paint
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import org.aerobag.app.domain.CoreResourceRequest
import org.aerobag.app.domain.MapDisplayFrame
import org.aerobag.app.generated.UiGlideRing

@Composable
internal fun GlideRingOverlay(runner: UiSessionWorkRunner, displayFrame: State<MapDisplayFrame>,
    fetchResource: (CoreResourceRequest) -> ByteArray, onError: (Throwable) -> Unit) {
    var ring by remember(runner) { mutableStateOf<UiGlideRing?>(null) }
    val fetch = rememberUpdatedState(fetchResource)
    val reportError = rememberUpdatedState(onError)
    LaunchedEffect(runner) {
        while (true) {
            try { ring = runner.queryGlideRing { fetch.value(it) } }
            catch (cancel: CancellationException) { throw cancel }
            catch (error: Throwable) { ring = null; reportError.value(error) }
            delay(ring?.recheckAfterMs?.toLong() ?: 1_000L)
        }
    }
    ring?.let { GlideRingGeometry(it, displayFrame) }
}

@Composable
internal fun GlideRingGeometry(ring: UiGlideRing, displayFrame: State<MapDisplayFrame>) {
    val density = LocalDensity.current.density
    val paint = remember(density) { Paint(Paint.ANTI_ALIAS_FLAG).apply {
        textSize = 14f * density; textAlign = Paint.Align.CENTER; isFakeBoldText = true
    } }
    Canvas(Modifier.fillMaxSize().testTag("glide-ring")) {
        val frame = displayFrame.value
        ring.paths.forEach { points ->
            val path = Path()
            points.forEachIndexed { index, point ->
                val p = frame.latLonToScreen(point.lat, point.lon)
                if (index == 0) path.moveTo(p.x, p.y) else path.lineTo(p.x, p.y)
            }
            drawPath(path, Color.White, style = Stroke(width = 6f * density))
            drawPath(path, Color(0xff087c85), style = Stroke(width = 3f * density,
                pathEffect = PathEffect.dashPathEffect(floatArrayOf(9f*density, 5f*density))))
        }
        ring.labelPosition?.let { point ->
            val p = frame.latLonToScreen(point.lat, point.lon)
            fun strokedText(text: String, x: Float, y: Float) {
                paint.style = Paint.Style.STROKE; paint.strokeWidth = 4f*density; paint.color = android.graphics.Color.WHITE
                drawContext.canvas.nativeCanvas.drawText(text, x, y, paint)
                paint.style = Paint.Style.FILL; paint.color = 0xff06545b.toInt()
                drawContext.canvas.nativeCanvas.drawText(text, x, y, paint)
            }
            ring.windDirectionDegTrue?.let { direction ->
                val canvas = drawContext.canvas.nativeCanvas
                canvas.save()
                canvas.translate(p.x - 24f*density, p.y - 28f*density)
                canvas.rotate((direction - frame.viewport.rotationDeg).toFloat())
                strokedText("↑", 0f, -(paint.ascent() + paint.descent()) / 2f)
                canvas.restore()
            }
            strokedText(ring.windLabel, p.x + (if (ring.windDirectionDegTrue != null) 6f*density else 0f), p.y - 23f*density)
            strokedText(ring.speedLabel, p.x, p.y - 6f*density)
        }
    }
}
