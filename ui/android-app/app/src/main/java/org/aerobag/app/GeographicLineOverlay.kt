// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.State
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import org.aerobag.app.domain.MapDisplayFrame
import org.aerobag.app.domain.UiTheme
import org.aerobag.app.generated.GeographicLineAnnotation

@Composable
internal fun GeographicLineOverlay(annotation: GeographicLineAnnotation?, frame: State<MapDisplayFrame>, theme: UiTheme) {
    if (annotation == null) return
    val scale = LocalDensity.current.density
    val paint = remember(scale) { android.graphics.Paint(android.graphics.Paint.ANTI_ALIAS_FLAG).apply {
        textSize = 16f * scale
        textAlign = android.graphics.Paint.Align.CENTER
        typeface = android.graphics.Typeface.DEFAULT_BOLD
    } }
    Canvas(Modifier.fillMaxSize().testTag("altitude-intercept-arc")) {
        val current = frame.value
        val path = Path()
        annotation.points.forEachIndexed { index, point ->
            val screen = current.latLonToScreen(point.lat, point.lon)
            if (index == 0) path.moveTo(screen.x, screen.y) else path.lineTo(screen.x, screen.y)
        }
        drawPath(path, theme.flightPlanRoute.contrast, style = Stroke(width = 6f * scale))
        drawPath(path, theme.aviation.altitudeIntercept, style = Stroke(width = 3f * scale))
        val label = current.latLonToScreen(annotation.labelPosition.lat, annotation.labelPosition.lon)
        rotate((annotation.labelBearingDeg - current.viewport.rotationDeg).toFloat(), Offset(label.x, label.y)) {
            paint.style = android.graphics.Paint.Style.STROKE
            paint.strokeWidth = 3f * scale
            paint.color = theme.aviation.trafficContrast.toArgb()
            drawContext.canvas.nativeCanvas.drawText(annotation.label, label.x, label.y - 10f * scale, paint)
            paint.style = android.graphics.Paint.Style.FILL
            paint.color = theme.aviation.altitudeIntercept.toArgb()
            drawContext.canvas.nativeCanvas.drawText(annotation.label, label.x, label.y - 10f * scale, paint)
        }
    }
}
