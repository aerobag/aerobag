// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

@file:OptIn(org.aerobag.app.domain.RawUiSessionWorkApi::class)

package org.aerobag.app

import android.graphics.Paint
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.boundsInParent
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.zIndex
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.aerobag.app.domain.*
import org.aerobag.app.generated.*
import kotlin.math.*

private data class AirwayDragRequest(val phase: UiAirwayRouteDragPhase, val position: LatLonPoint,
    val radius: Double, val insert: Int, val moving: Int?)

/** At most one computation and the latest pointer position; a release replaces a pending preview. */
private class AirwayDragQueue(val runner: UiSessionWorkRunner, val editId: String) {
    var busy by mutableStateOf(false)
    var pending: AirwayDragRequest? = null
    var onResult: (UiSessionSnapshot) -> Unit = {}
    var onError: (Throwable) -> Unit = {}
    fun submit(request: AirwayDragRequest) { pending = request; drain() }
    private fun drain() {
        if (busy) return
        val request = pending ?: return
        pending = null; busy = true
        runner.submitAirwayRouteDrag(editId, request.phase, request.position, request.radius, request.insert, request.moving,
            onResult = { onResult(it); busy = false; drain() },
            onError = { pending = null; busy = false; onError(it) })
    }
}

@Composable
internal fun BoxScope.AirwayRoutingOverlay(view: UiAirwayRouting, displayFrame: State<MapDisplayFrame>, uiSession: NativeUiSession, runner: UiSessionWorkRunner,
    onViewport: (MapViewportState) -> Unit, onSnapshot: (UiSessionSnapshot) -> Unit, onError: (Throwable) -> Unit) {
    val width = displayFrame.value.widthPx
    val height = displayFrame.value.heightPx
    val density = LocalDensity.current.density
    val currentView = rememberUpdatedState(view)
    val queue = remember(view.editId, runner) { AirwayDragQueue(runner, view.editId) }
    queue.onResult = onSnapshot; queue.onError = onError
    var dragging by remember(view.editId) { mutableStateOf(false) }
    var actionBusy by remember(view.editId) { mutableStateOf(false) }
    var cursor by remember(view.editId) { mutableStateOf<Offset?>(null) }
    var panelBounds by remember { mutableStateOf<Rect?>(null) }
    LaunchedEffect(view.editId, width > 0 && height > 0) {
        if (width <= 0 || height <= 0) return@LaunchedEffect
        runCatching {
            withContext(Dispatchers.IO) {
                uiSession.airwayRoutingViewport(width.toDouble(), height.toDouble(), displayFrame.value.viewport.rotationDeg)?.let { frame ->
                    frame to uiSession.disengageMapFollow(frame)
                }
            }
        }.onSuccess { result -> result?.let { (frame, snapshot) -> onSnapshot(snapshot); onViewport(frame) } }
            .onFailure(onError)
    }
    fun screen(point: UiAirwayRoutePosition, frame: MapDisplayFrame = displayFrame.value): Offset {
        val p = frame.latLonToScreen(point.lat, point.lon)
        return Offset(p.x, p.y)
    }
    fun action(id: String) {
        if (queue.busy || dragging || actionBusy) return
        actionBusy = true
        runner.submitAirwayRoutingAction(id, { onSnapshot(it); actionBusy = false }, { actionBusy = false; onError(it) })
    }
    val selected = view.route
    val purple = Color(0xff7b26cb)
    Canvas(Modifier.fillMaxSize().zIndex(3f).pointerInput(view.editId, width, height) {
        awaitEachGesture {
            val down = awaitFirstDown(requireUnconsumed = false)
            if (queue.busy || actionBusy) return@awaitEachGesture
            val frame = displayFrame.value
            val draft = currentView.value
            fun point(p: UiAirwayRoutePosition) = screen(p, frame)
            val pin = draft.viaPoints.firstOrNull { (point(it.position) - down.position).getDistance() <= 22f * density }
            val choice = draft.route
            fun distance(a: Offset, b: Offset): Float {
                val ab = b - a
                val size = ab.x * ab.x + ab.y * ab.y
                val t = if (size == 0f) 0f else (((down.position-a).x*ab.x+(down.position-a).y*ab.y)/size).coerceIn(0f, 1f)
                return (down.position-(a+ab*t)).getDistance()
            }
            val leg = choice?.legs?.minByOrNull { distance(point(it.from), point(it.to)) }
                ?.takeIf { distance(point(it.from), point(it.to)) <= 18f*density }
            val originalHit = choice == null && draft.originalPath.zipWithNext().any { (a,b) -> distance(point(a),point(b)) <= 18f*density }
            if (pin == null && leg == null && !originalHit) return@awaitEachGesture
            val insert = pin?.index ?: leg?.viaInsertIndex ?: 0
            val moving = pin?.index
            down.consume(); dragging = true
            var last = down.position
            var finished = false
            var moved = false
            fun send(phase: UiAirwayRouteDragPhase, at: Offset) {
                val pointerFrame = displayFrame.value
                val world = pointerFrame.screenToWorld(ScreenPoint(at.x, at.y))
                val latLon = worldToLatLon(world.x, world.y)
                val nearWorld = pointerFrame.screenToWorld(ScreenPoint(at.x+26f*density, at.y))
                val near = worldToLatLon(nearWorld.x, nearWorld.y)
                val radius = hypot((near.first-latLon.first)*60, (near.second-latLon.second)*60*cos(latLon.first*PI/180)).coerceAtLeast(0.01)
                queue.submit(AirwayDragRequest(phase, LatLonPoint(latLon.first, latLon.second), radius, insert, moving))
            }
            try {
                while (true) {
                    val event = awaitPointerEvent()
                    val change = event.changes.firstOrNull { it.id == down.id } ?: break
                    last = change.position
                    if (!change.pressed) {
                        change.consume()
                        dragging = false
                        if (moved) send(UiAirwayRouteDragPhase.Commit, last)
                        else pin?.removeAction?.let { action(it.actionId) }
                        finished = true; break
                    }
                    change.consume()
                    if ((last-down.position).getDistance() > viewConfiguration.touchSlop) moved = true
                    if (moved) { cursor = last; send(UiAirwayRouteDragPhase.Preview, last) }
                }
            } finally {
                if (!finished && moved) send(UiAirwayRouteDragPhase.Cancel, last)
                dragging = false; cursor = null
            }
        }
    }) {
        view.originalPath.zipWithNext().forEach { (a,b) -> drawLine(Color.Gray, screen(a), screen(b), 3f*density,
            pathEffect = PathEffect.dashPathEffect(floatArrayOf(5f*density,6f*density))) }
        selected?.legs?.forEach { leg ->
            val a = screen(leg.from); val b = screen(leg.to)
            drawLine(Color.White, a, b, 8f*density)
            drawLine(purple, a, b, 4f*density,
                pathEffect = if (leg.direct) PathEffect.dashPathEffect(floatArrayOf(7f*density,6f*density)) else null)
        }
        val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply { textSize=13f*density; textAlign=Paint.Align.CENTER; isFakeBoldText=true }
        val occupied = mutableListOf<Rect>()
        panelBounds?.let { occupied.add(Rect(it.left/density,it.top/density,it.right/density,it.bottom/density)) }
        fun reserve(text: String, at: UiAirwayRoutePosition, offset: Float) {
            occupied.add(routeLabelBounds(screen(at)/density+Offset(0f,offset),paint.measureText(text)/density))
        }
        selected?.junctions?.forEach { reserve(it.label,it.position,-24f) }
        view.viaPoints.forEach { reserve(it.label,it.position,-18f) }
        selected?.crossings?.forEach { reserve(it.label,it.position,40f) }
        fun label(text: String, p: Offset, color: Int) {
            paint.color=android.graphics.Color.WHITE; paint.style=Paint.Style.STROKE; paint.strokeWidth=4f*density
            drawContext.canvas.nativeCanvas.drawText(text,p.x,p.y,paint)
            paint.color=color; paint.style=Paint.Style.FILL
            drawContext.canvas.nativeCanvas.drawText(text,p.x,p.y,paint)
        }
        val labelCandidates = selected?.legs?.map { RouteLabelCandidate(screen(it.from)/density,screen(it.to)/density,it.label,it.highestMea) }.orEmpty()
        routeLabelIndices(labelCandidates,width/density,height/density).forEach { index ->
            val leg = selected!!.legs[index]; val candidate = labelCandidates[index]
            val layout = routeLabelLayout(candidate.from,candidate.to,width/density,height/density,
                paint.measureText(leg.label)/density,occupied,leg.highestMea) ?: return@forEach
            occupied.add(layout.bounds)
            layout.leader?.let { drawLine(Color(0xff941426),layout.anchor*density,it*density,1.5f*density) }
            label(leg.label,layout.baseline*density,if (leg.highestMea) 0xff941426.toInt() else 0xff37204e.toInt())
        }
        selected?.crossings?.forEach { crossing ->
            val p = screen(crossing.position)
            drawLine(Color(0xff941426), p+Offset(0f,16f*density), p+Offset(0f,26f*density), 2f*density)
            label(crossing.label, p+Offset(0f,40f*density), 0xff941426.toInt())
        }
        view.viaPoints.forEach { pin ->
            val p=screen(pin.position)
            drawCircle(purple, 13f*density,p); drawCircle(Color.White,9f*density,p)
            label(pin.label,p-Offset(0f,18f*density),0xff37204e.toInt())
        }
        selected?.junctions?.forEach { junction ->
            val p = screen(junction.position)
            drawCircle(Color.White, 14f*density, p)
            label(junction.label, p-Offset(0f,24f*density), 0xff37204e.toInt())
        }
        cursor?.let { drawCircle(purple,14f*density,it,style=androidx.compose.ui.graphics.drawscope.Stroke(2f*density)) }
        view.dragTarget?.let { drawCircle(Color(0xffffdc54),8f*density,screen(it)) }
    }
    // Reuse the standard navigation glyphs; geographic placement follows the
    // live display frame, while glyphs and identifiers remain upright.
    selected?.junctions?.forEach { junction ->
        PlanWaypointSymbol(junction.symbolFeature,
            Modifier.offset {
                val p = screen(junction.position)
                IntOffset((p.x-20f*density).roundToInt(), (p.y-20f*density).roundToInt())
            }.size(40.dp).zIndex(3f).testTag("airway-route-junction-${junction.nodeId}"))
    }
    // The drawing owns hit testing; these shared indexed controls expose the same
    // pins to accessibility and geometry-based journeys without stealing drags.
    view.viaPoints.forEach { pin ->
        Box(Modifier.offset {
            val p = screen(pin.position)
            IntOffset((p.x-11f*density).roundToInt(), (p.y-11f*density).roundToInt())
        }.size(22.dp).zIndex(3f)
            .e2eIndexedControl(semanticTag="airway-route-via-${pin.index}",
                enabled=pin.removeAction.enabled && !queue.busy && !dragging && !actionBusy,
                text=pin.removeAction.label)
            .semantics { onClick(pin.removeAction.label) { action(pin.removeAction.actionId); true } })
    }
    val trayWidth = ((ThumbSize + 8.dp) * view.controls.size + 6.dp)
        .coerceAtMost(with(LocalDensity.current) { (width-24f*density).coerceAtLeast(1f).toDp() })
    MenuPanel(Modifier.align(Alignment.BottomCenter).padding(start=ThumbGap, end=ThumbGap, bottom=ThumbSize + ThumbGap*2)
        .onGloballyPositioned { panelBounds = it.boundsInParent() }.zIndex(4f), width=trayWidth) {
        Text(view.title, modifier=Modifier.padding(horizontal=5.dp))
        view.route?.let { Text(it.summary, modifier=Modifier.padding(horizontal=5.dp)
            .e2eIndexedControl(semanticTag="parity:airway-routing-summary", enabled=false, text=it.summary),
            style=androidx.compose.material3.MaterialTheme.typography.bodySmall) }
        view.dragLabel.ifEmpty { view.message }.takeIf { it.isNotEmpty() }?.let {
            Text(it,modifier=Modifier.padding(horizontal=5.dp),style=androidx.compose.material3.MaterialTheme.typography.bodySmall)
        }
        Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState())) {
            view.controls.forEach { control ->
                val button = control.button
                SelectedControlHighlightFrame(selected=control.selected,modifier=Modifier.size(ThumbSize+8.dp)) {
                    CompactSquareButton(label=button.label,modifier=Modifier.fillMaxSize(),
                        enabled=button.enabled && !queue.busy && !dragging && !actionBusy,
                        selected=control.selected,actionSymbolId=control.symbolId,
                        testTag=button.testId,onClick={action(button.actionId)})
                }
            }
        }
    }
}
