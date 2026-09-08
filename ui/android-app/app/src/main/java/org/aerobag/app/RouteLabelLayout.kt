// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import kotlin.math.*

// Pointer-rate mirror of app_core::ui_geometry::route_labels; shared conformance
// vectors fence placement, clipping and the protected-label collision fallback.
internal data class RouteLabelLayout(val anchor: Offset, val baseline: Offset,
    val bounds: Rect, val leader: Offset?)
internal data class RouteLabelCandidate(val from: Offset, val to: Offset, val label: String, val important: Boolean)

internal fun routeLabelIndices(legs: List<RouteLabelCandidate>, width: Float, height: Float): List<Int> {
    val result = mutableListOf<Int>()
    var index = 0
    while (index < legs.size) {
        val first = index++
        if (!legs[first].important) { result.add(first); continue }
        while (index < legs.size && legs[index].important && legs[index].label == legs[first].label &&
            legs[index-1].to == legs[index].from) index++
        var best: Int? = null; var length = -1f
        for (member in first until index) {
            val leg = legs[member]
            val interval = visibleInterval(leg.from,leg.to,width,height) ?: continue
            val visibleLength = (leg.to-leg.from).getDistance()*(interval.second-interval.first)
            if (visibleLength > length+1e-4f) { length = visibleLength; best = member }
        }
        best?.let { result.add(it) }
    }
    return result.sortedByDescending { legs[it].important }
}

internal fun routeLabelBounds(baseline: Offset, textWidth: Float) = Rect(
    baseline.x-textWidth/2f-4f, baseline.y-17f, baseline.x+textWidth/2f+4f, baseline.y+7f)

private fun visibleInterval(from: Offset, to: Offset, width: Float, height: Float): Pair<Float,Float>? {
    var start = 0f; var end = 1f
    for ((origin, delta, limit) in listOf(Triple(from.x,to.x-from.x,width), Triple(from.y,to.y-from.y,height))) {
        if (abs(delta) < 1e-9f) {
            if (origin < 0f || origin > limit) return null
        } else {
            val a = -origin/delta; val b = (limit-origin)/delta
            start = max(start,min(a,b)); end = min(end,max(a,b))
        }
    }
    return if (start <= end) Pair(start,end) else null
}

/** All dimensions are in density-independent pixels in the displayed map frame. */
internal fun routeLabelLayout(from: Offset, to: Offset, width: Float, height: Float,
    textWidth: Float, occupied: List<Rect>, important: Boolean): RouteLabelLayout? {
    if (width < 32f || height < 40f) return null
    val (start,end) = visibleInterval(from,to,width,height) ?: return null
    val fraction = if (important) (start+end)/2f else 0.5f
    val anchor = from+(to-from)*fraction
    val preferred = anchor-Offset(0f,10f)
    fun overlap(bounds: Rect) = occupied.sumOf { other ->
        (max(0f,min(bounds.right,other.right)-max(bounds.left,other.left)) *
            max(0f,min(bounds.bottom,other.bottom)-max(bounds.top,other.top))).toDouble()
    }
    var baseline = preferred
    if (important) {
        val inset = min(textWidth/2f+12f,width/2f); val stepX = max(textWidth+12f,32f)
        val columns = ceil(width/stepX).toInt(); val rows = ceil(height/32f).toInt()
        var bestOverlap = Double.POSITIVE_INFINITY; var bestDistance = Float.POSITIVE_INFINITY
        for (row in -rows..rows) for (column in -columns..columns) {
            val candidate = Offset((preferred.x+column*stepX).coerceIn(inset,width-inset),
                (preferred.y+row*32f).coerceIn(25f,height-15f))
            val area = overlap(routeLabelBounds(candidate,textWidth))
            val distance = (candidate-preferred).getDistanceSquared()
            if (area < bestOverlap || (area == bestOverlap && distance < bestDistance)) {
                bestOverlap = area; bestDistance = distance; baseline = candidate
            }
        }
    } else {
        val bounds = routeLabelBounds(baseline,textWidth)
        if (bounds.left < 8f || bounds.right > width-8f || bounds.top < 8f || bounds.bottom > height-8f || overlap(bounds)>0) return null
    }
    val bounds = routeLabelBounds(baseline,textWidth)
    val leader = if (abs(baseline.x-preferred.x)>1f || abs(baseline.y-preferred.y)>1f)
        Offset(anchor.x.coerceIn(bounds.left,bounds.right),anchor.y.coerceIn(bounds.top,bounds.bottom)) else null
    return RouteLabelLayout(anchor,baseline,bounds,leader)
}
