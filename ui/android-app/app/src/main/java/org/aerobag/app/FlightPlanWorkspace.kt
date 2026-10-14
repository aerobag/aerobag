// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.tween
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.layer.drawLayer
import androidx.compose.ui.graphics.rememberGraphicsLayer
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import org.aerobag.app.domain.FlightPlanControlUiView
import org.aerobag.app.domain.FlightPlanWorkspaceUi
import org.aerobag.app.generated.FlightPlanControlId
import org.aerobag.app.generated.FlightPlanSlot

@Composable
internal fun FlightPlanSlotLabel(
    workspace: FlightPlanWorkspaceUi,
    slot: FlightPlanSlot,
    modifier: Modifier = Modifier,
) {
    val theme = LocalAerobagUiTheme.current.flightPlanRoute
    val active = slot == FlightPlanSlot.Active
    FittedSingleLineText(
        text = if (active) workspace.activeLabel else workspace.standbyLabel,
        modifier = modifier,
        style = MaterialTheme.typography.titleMedium.copy(fontWeight = FontWeight.ExtraBold),
        textAlign = TextAlign.Start,
        color = if (active) theme.remaining else theme.standby,
        outlineColor = theme.contrast,
        outlineOffsets = listOf(IntOffset(-2, 0), IntOffset(2, 0), IntOffset(0, -2), IntOffset(0, 2)),
    )
}

@Composable
internal fun FlightPlanWorkspaceButton(
    control: FlightPlanControlUiView,
    onControl: (FlightPlanControlId) -> Unit,
) {
    val context = LocalContext.current
    CompactSquareButton(
        label = control.label,
        showLabel = false,
        actionSymbolId = control.symbolId,
        modifier = Modifier.size(ThumbSize),
        enabled = control.enabled,
        testTag = "parity:plan-control:${control.id.name}",
        onDisabledClick = control.disabledReason?.let { reason -> { showDisabledActionToast(context, reason) } },
        onClick = { onControl(control.id) },
    )
}

@Composable
internal fun FlightPlanWorkspaceBand(
    workspace: FlightPlanWorkspaceUi,
    onControl: (FlightPlanControlId) -> Unit,
) {
    val standby = workspace.openSlot == FlightPlanSlot.Active
    Column(Modifier.fillMaxWidth()) {
        Row(
            Modifier.fillMaxWidth().padding(vertical = ThumbGap),
            horizontalArrangement = Arrangement.spacedBy(ThumbGap),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f).height(ThumbSize), verticalArrangement = Arrangement.Center) {
                val slot = if (standby) FlightPlanSlot.Standby else FlightPlanSlot.Active
                FlightPlanSlotLabel(workspace, slot, Modifier.fillMaxWidth()
                    .e2eIndexedControl("parity:plan-collapsed-open", enabled = true)
                    .clickable { onControl(if (standby) FlightPlanControlId.OpenStandbyPlan else FlightPlanControlId.OpenActivePlan) })
                val summary = if (standby) workspace.standbySummary else workspace.activeSummary
                SelectionContainer {
                    Text(summary, modifier = Modifier.e2eIndexedLabel("parity:plan-collapsed-summary", summary),
                        style = MaterialTheme.typography.titleMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
            workspace.controls.forEach { FlightPlanWorkspaceButton(it, onControl) }
        }
    }
}

@Composable
internal fun FlightPlanWorkspaceLayout(
    slot: FlightPlanSlot,
    modifier: Modifier = Modifier,
    band: @Composable () -> Unit,
    content: @Composable ColumnScope.() -> Unit,
) {
    val active = slot == FlightPlanSlot.Active
    val expansion by animateFloatAsState(if (active) 1f else 0f,
        animationSpec = tween(320, easing = FastOutSlowInEasing), label = "plan-expansion")
    val activeDrawing = rememberGraphicsLayer()
    val standbyDrawing = rememberGraphicsLayer()
    val currentDrawing = if (active) activeDrawing else standbyDrawing
    val closingDrawing = if (active) standbyDrawing else activeDrawing
    val theme = LocalAerobagUiTheme.current.flightPlanRoute
    val liveOpacity = if (active) expansion else 1f - expansion
    val moving = liveOpacity < 1f
    BoxWithConstraints(modifier.clipToBounds()
        .e2eIndexedElement("parity:plan-workspace-transition:${if (moving) "running" else "idle"}")
        .pointerInput(moving) {
            if (moving) awaitPointerEventScope {
                while (true) awaitPointerEvent(PointerEventPass.Initial).changes.forEach { it.consume() }
            }
        }) {
        val bandHeight = minOf(ThumbSize + ThumbGap * 2, (maxHeight - 8.dp).coerceAtLeast(0.dp) / 2)
        val expandedHeight = (maxHeight - bandHeight - 8.dp).coerceAtLeast(0.dp)
        // Animate the actual boundary, not a fraction of the newly sized frame:
        // Active's extra navigation buttons can change the footer height.
        val dividerY by animateDpAsState(if (active) expandedHeight else bandHeight,
            animationSpec = tween(320, easing = FastOutSlowInEasing), label = "plan-boundary")
        val standbyTop = dividerY + 8.dp
        val editorHeightPx = with(LocalDensity.current) { expandedHeight.roundToPx() }
        // The closing grid is a retained display list, not another editor. Keying
        // the live subtree prevents it from reusing/mutating the old draw nodes.
        if (moving) {
            FlightPlanEditorRegion(
                top = if (active) standbyTop else 0.dp,
                height = if (active) maxHeight - standbyTop else dividerY,
                editorHeightPx = editorHeightPx,
                modifier = Modifier.graphicsLayer { alpha = (1f - liveOpacity * 2).coerceAtLeast(0f) }.clearAndSetSemantics {},
            ) {
                Canvas(Modifier.fillMaxSize()) { drawLayer(closingDrawing) }
            }
        }
        key(slot) {
            FlightPlanEditorRegion(
                top = if (active) 0.dp else standbyTop,
                height = if (active) dividerY else maxHeight - standbyTop,
                editorHeightPx = editorHeightPx,
                modifier = Modifier.e2eIndexedElement("parity:plan-workspace-editor-clip")
                    .graphicsLayer { alpha = liveOpacity },
            ) {
                Column(Modifier.fillMaxSize().drawWithContent {
                    currentDrawing.record { this@drawWithContent.drawContent() }
                    drawLayer(currentDrawing)
                }, verticalArrangement = Arrangement.spacedBy(PlanGridGap)) { content() }
            }
        }
        Box(Modifier.offset(y = if (active) standbyTop else 0.dp).fillMaxWidth()
            .height(bandHeight).graphicsLayer { alpha = (liveOpacity * 2 - 1f).coerceAtLeast(0f) }) { band() }
        Box(Modifier.offset(y = dividerY).fillMaxWidth().height(8.dp)
            .e2eIndexedElement("parity:plan-workspace-divider")
            .background(theme.contrast).padding(2.dp).background(theme.standby))
    }
}

@Composable
private fun FlightPlanEditorRegion(
    top: androidx.compose.ui.unit.Dp,
    height: androidx.compose.ui.unit.Dp,
    editorHeightPx: Int,
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    Layout(content = content, modifier = modifier.offset(y = top).fillMaxWidth().height(height).clipToBounds()) { measurables, constraints ->
        val child = measurables.single().measure(constraints.copy(minHeight = editorHeightPx, maxHeight = editorHeightPx))
        layout(constraints.maxWidth, constraints.maxHeight) { child.place(0, 0) }
    }
}
