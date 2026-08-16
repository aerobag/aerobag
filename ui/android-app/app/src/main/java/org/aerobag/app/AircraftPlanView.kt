// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.vector.PathParser

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
    drawPath(path, Color(0xFFE6E6E6))
    drawPath(
        path,
        Color.Black,
        style = Stroke(width = 1.1f, join = androidx.compose.ui.graphics.StrokeJoin.Round),
    )
    drawContext.canvas.restore()
}
