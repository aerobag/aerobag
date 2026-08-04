// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.rotate
import org.aerobag.app.generated.manualSequenceChevronPath
import org.aerobag.app.generated.manualSequenceChevronSpacing
import kotlin.math.PI
import kotlin.math.atan2
import kotlin.math.hypot

internal data class RouteChevronPlacement(
    val center: Offset,
    val angleDegrees: Float,
)

private data class RoutePathSection(
    val start: Offset,
    val end: Offset,
    val length: Float,
)

internal fun spacedRouteChevronPlacements(
    path: List<Offset>,
    spacingPx: Float,
): List<RouteChevronPlacement> {
    if (path.size < 2 || !spacingPx.isFinite() || spacingPx <= 0f) return emptyList()

    val sections = path.zipWithNext().mapNotNull { (start, end) ->
        val length = hypot(end.x - start.x, end.y - start.y)
        if (length > 0f) RoutePathSection(start, end, length) else null
    }
    val totalLength = sections.sumOf { it.length.toDouble() }.toFloat()
    if (totalLength <= 0f) return emptyList()

    val distances = if (totalLength <= spacingPx) {
        listOf(totalLength / 2f)
    } else {
        buildList {
            var distance = spacingPx / 2f
            while (distance <= totalLength - spacingPx / 2f + 0.0001f) {
                add(distance)
                distance += spacingPx
            }
        }
    }

    var sectionIndex = 0
    var sectionStartDistance = 0f
    return distances.map { distance ->
        while (
            sectionIndex + 1 < sections.size &&
            distance > sectionStartDistance + sections[sectionIndex].length
        ) {
            sectionStartDistance += sections[sectionIndex].length
            sectionIndex += 1
        }
        val section = sections[sectionIndex]
        val fraction = (distance - sectionStartDistance) / section.length
        RouteChevronPlacement(
            center = Offset(
                x = section.start.x + (section.end.x - section.start.x) * fraction,
                y = section.start.y + (section.end.y - section.start.y) * fraction,
            ),
            angleDegrees = (
                atan2(section.end.y - section.start.y, section.end.x - section.start.x) *
                    180.0 / PI
                ).toFloat(),
        )
    }
}

internal fun DrawScope.drawFlightPlanRoutePath(
    screenPath: List<Offset>,
    style: String,
    color: Color,
    densityScale: Float,
) {
    if (screenPath.size < 2) return
    if (style == "vectors") {
        spacedRouteChevronPlacements(
            path = screenPath,
            spacingPx = manualSequenceChevronSpacing * densityScale,
        ).forEach { placement ->
            rotate(placement.angleDegrees, placement.center) {
                val chevron = manualSequenceChevronPath(placement.center, densityScale)
                drawPath(
                    path = chevron,
                    color = Color(0x8C000000),
                    style = Stroke(
                        width = 7f * densityScale,
                        cap = StrokeCap.Round,
                        join = StrokeJoin.Round,
                    ),
                )
                drawPath(
                    path = chevron,
                    color = color,
                    style = Stroke(
                        width = 3.5f * densityScale,
                        cap = StrokeCap.Round,
                        join = StrokeJoin.Round,
                    ),
                )
            }
        }
        return
    }

    val routePath = Path().apply {
        moveTo(screenPath.first().x, screenPath.first().y)
        screenPath.drop(1).forEach { point -> lineTo(point.x, point.y) }
    }
    val pathEffect = if (style == "dashed") {
        PathEffect.dashPathEffect(floatArrayOf(10f * densityScale, 8f * densityScale))
    } else {
        null
    }
    drawPath(
        path = routePath,
        color = Color(0x8C000000),
        style = Stroke(
            width = 7f * densityScale,
            cap = StrokeCap.Round,
            join = StrokeJoin.Round,
            pathEffect = pathEffect,
        ),
    )
    drawPath(
        path = routePath,
        color = color,
        style = Stroke(
            width = 3.5f * densityScale,
            cap = StrokeCap.Round,
            join = StrokeJoin.Round,
            pathEffect = pathEffect,
        ),
    )
}
