// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.ui.geometry.Offset
import org.aerobag.app.domain.FlightPlanRouteDistanceAnnotation
import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.hypot

internal data class RouteDistancePillLayout(
    val annotation: FlightPlanRouteDistanceAnnotation,
    val center: Offset,
    val widthPx: Float,
    val rotationDegrees: Float,
)

internal fun layoutRouteDistancePills(
    annotations: List<FlightPlanRouteDistanceAnnotation>,
    screenPaths: List<List<Offset>>,
    visibleFeatureIds: Set<String>,
    measurePillWidth: (String) -> Float,
): List<RouteDistancePillLayout> = buildList {
    annotations.forEach { annotation ->
        if (annotation.requiredFeatureIds.any { it !in visibleFeatureIds }) return@forEach
        val path = buildList {
            annotation.segmentIndexes.forEachIndexed { index, segmentIndex ->
                val points = screenPaths.getOrNull(segmentIndex).orEmpty()
                addAll(if (index == 0) points else points.drop(1))
            }
        }
        if (path.size < 2) return@forEach
        val segmentLengths = path.zipWithNext { first, second ->
            hypot((second.x - first.x).toDouble(), (second.y - first.y).toDouble()).toFloat()
        }
        val pathLength = segmentLengths.sum()
        val widthPx = measurePillWidth(annotation.text)
        if (pathLength < widthPx * annotation.minimumPathToPillWidthRatio.toFloat()) return@forEach

        val anchorDistance = widthPx * annotation.minimumPathToPillWidthRatio.toFloat() / 2f
        var traversed = 0f
        var center = path.first()
        var rotationDegrees = 0f
        for (index in segmentLengths.indices) {
            val length = segmentLengths[index]
            if (length > 0f && traversed <= anchorDistance && traversed + length >= anchorDistance) {
                val fraction = (anchorDistance - traversed) / length
                center = Offset(
                    x = path[index].x + (path[index + 1].x - path[index].x) * fraction,
                    y = path[index].y + (path[index + 1].y - path[index].y) * fraction,
                )
                var deltaX = path[index + 1].x - path[index].x
                var deltaY = path[index + 1].y - path[index].y
                if (deltaY < 0f || (abs(deltaY) < 1e-6f && deltaX < 0f)) {
                    deltaX = -deltaX
                    deltaY = -deltaY
                }
                rotationDegrees = Math.toDegrees(atan2(deltaY.toDouble(), deltaX.toDouble())).toFloat()
                break
            }
            traversed += length
        }
        add(RouteDistancePillLayout(annotation, center, widthPx, rotationDegrees))
    }
}
