// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.foundation.Canvas
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.vector.PathParser
import org.aerobag.app.domain.AircraftSymbolUiView

private const val AircraftPlanViewCanonicalWidth = 100f

@Composable
internal fun rememberAircraftPlanViewPath(pathData: String): Path? =
    remember(pathData) {
        pathData.takeIf(String::isNotBlank)?.let { encoded ->
            PathParser().parsePathString(encoded).toPath()
        }
    }

internal fun DrawScope.drawAircraftPlanView(
    path: Path,
    center: Offset,
    headingDeg: Float,
    wingspanPx: Float,
) {
    val scale = wingspanPx / AircraftPlanViewCanonicalWidth
    drawContext.canvas.save()
    drawContext.canvas.translate(center.x, center.y)
    drawContext.canvas.rotate(headingDeg)
    drawContext.canvas.scale(scale, scale)
    drawPath(
        path,
        Color.Black,
        style = Stroke(
            width = 3.3f,
            cap = androidx.compose.ui.graphics.StrokeCap.Round,
            join = androidx.compose.ui.graphics.StrokeJoin.Round,
        ),
    )
    drawPath(path, Color(0xFFE6E6E6))
    drawContext.canvas.restore()
}

@Composable
internal fun AircraftPlanViewIcon(
    symbol: AircraftSymbolUiView,
    modifier: Modifier = Modifier,
) {
    val path = rememberAircraftPlanViewPath(symbol.pathData)
    Canvas(modifier = modifier) {
        path?.let {
            drawAircraftPlanView(
                path = it,
                center = center,
                headingDeg = symbol.rotationDegrees,
                wingspanPx = size.minDimension * 0.82f,
            )
        }
    }
}
