// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later
package org.aerobag.app

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.focusable
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.input.key.*
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.TextButton
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.layout.boundsInRoot
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.positionOnScreen
import androidx.compose.ui.layout.positionInWindow
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupProperties
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import org.aerobag.app.generated.UiGuidedTour
import org.aerobag.app.generated.UiTourAction
import org.aerobag.app.generated.UiTourPlacement
import org.aerobag.app.generated.UiTourPresentation
import kotlin.math.roundToInt

internal val LocalGuidedTourFeedback = staticCompositionLocalOf<(Long, String) -> Unit> { { _, _ -> } }
internal val LocalGuidedTour = staticCompositionLocalOf<UiGuidedTour?> { null }
internal val LocalGuidedTourAnchors = staticCompositionLocalOf<TourAnchors?> { null }
internal class TourAnchors {
    val bounds = mutableStateMapOf<String, Rect>()
    val menus = mutableStateMapOf<Any, TourMenu>()
}

internal data class TourMenu(val position: Offset, val content: @Composable () -> Unit)

/** MenuDock uses the tour host's app plane during a tour. A platform popup
 * window would otherwise appear above the transparent input blocker. */
@Composable
internal fun TourAwarePopup(position: Offset = Offset.Zero, offset: IntOffset = IntOffset.Zero,
    onDismiss: () -> Unit, properties: PopupProperties = PopupProperties(focusable=true), content: @Composable () -> Unit) {
    val registry = LocalGuidedTourAnchors.current
    if (registry == null) {
        Popup(offset=offset,onDismissRequest=onDismiss,properties=properties,content=content)
    } else {
        val owner=remember { Any() }
        val latest=rememberUpdatedState(content)
        val render = remember { movableContentOf { latest.value() } }
        DisposableEffect(registry,owner,position,offset) {
            registry.menus[owner]=TourMenu(position+Offset(offset.x.toFloat(),offset.y.toFloat()),render)
            onDispose { registry.menus.remove(owner) }
        }
    }
}

/** Geometry follows the real controls, including ordinary (non-E2E) builds. */
@OptIn(ExperimentalComposeUiApi::class)
@Composable
internal fun Modifier.guidedTourAnchor(id: String): Modifier {
    val registry = LocalGuidedTourAnchors.current ?: return this
    DisposableEffect(registry, id) { onDispose { registry.bounds.remove(id) } }
    return onGloballyPositioned { registry.bounds[id] = it.boundsInWindow().translate(it.positionOnScreen() - it.positionInWindow()) }
}

private val tourAnchorIds = mapOf(
    "cdi" to listOf("parity:nav-cdi"), "home" to listOf("tour:home"),
    "chart-plate" to listOf("tour:chart-plate"),
    "base-map" to listOf("parity:chart-family-button"), "base-map-menu" to listOf("parity:chart-family-button:tray"),
    "layers" to listOf("parity:layers-button"), "layers-menu" to listOf("parity:layers-button:tray"),
    "route-entry" to listOf("parity:plan-append-route-input"), "plan-row" to listOf("tour:plan-row"),
    "plan-row-activate" to listOf("parity:plan-row-action:activate_leg"),
    "plan-remove-all-above" to listOf("parity:plan-row-action:remove_all_above"),
    "plan-add-airway" to listOf("parity:plan-row-action:add_airway"), "plan-find-route" to listOf("parity:plan-row-action:find_route"),
    "plate-airport-menu" to listOf("parity:plate-airport-button:tray", "parity:plate-airport-button"),
    "plate-folder" to listOf("parity:plate-folder-button"), "plate-folder-content" to listOf("tour:plate-folder"),
    "home-guided-tour" to listOf("parity:home-button:GuidedTour"),
    "home-cloud" to listOf("parity:home-button:Cloud"),
    "home-about" to listOf("parity:home-button:About"), "home-settings" to listOf("parity:home-button:Settings"), "home-status" to listOf("parity:home-button:DataStatus"),
    "airway-picker" to listOf("tour:airway-picker"), "flight-plan" to listOf("tour:flight-plan"), "map" to listOf("parity:map-surface"),
    "routing-summary" to listOf("parity:airway-routing-summary"), "routing-route" to listOf("parity:airway-routing-summary"),
    "routing-gnss" to listOf("parity:airway-routing-control-gnss"), "routing-apply" to listOf("parity:airway-routing-control-apply_route"),
    "estimate" to listOf("parity:plan-estimate-mode"), "aircraft-models" to listOf("parity:altitude-planner-control:aircraft"),
    "use-model" to listOf("parity:altitude-planner-wind-action:ready_forecast"), "altitude-table" to listOf("tour:altitude-table"),
    "inspector" to listOf("parity:map-selection-tray"), "inspector-weather" to listOf("parity:map-selection-action:wx"),
    "inspector-supplement" to listOf("parity:map-selection-action:csup"), "inspector-plates" to listOf("parity:map-selection-action:plates"),
    "inspector-insert" to listOf("parity:map-selection-action:insert"), "weather" to listOf("tour:weather"),
    "notams" to listOf("tour:notams"), "airport-info" to listOf("tour:airport-info"),
    "plan-preview" to listOf("parity:ownship-source:__direct_situation__"),
    "inspector-spot" to listOf("tour:inspector-spot"), "map-spot-marker" to listOf("tour:map-spot-marker"),
    "ownship-menu" to listOf("parity:ownship-launcher:tray"), "preview-controls" to listOf("parity:ownship-launcher:tray"),
    "center" to listOf("parity:center-here-button"), "orientation" to listOf("parity:map-orientation-button"),
    "cloud-create" to listOf("parity:cloud-action:begin_create"),
    "offline-regions" to listOf("tour:offline-regions"), "offline-northwest" to listOf("parity:offline-region:nw"),
    "offline-products" to listOf("tour:offline-products"), "offline-help" to listOf("tour:offline-help"),
    "offline-help-panel" to listOf("tour:offline-help-panel"), "offline-apply" to listOf("parity:offline-sync-button"),
)

/** A structural input plane: no child's local zIndex can escape the app plane.
 * Page-local state, popups and effects have a disposable demo lifetime. Core
 * session state and the saved view stay above this host. Key the mode, not the
 * step, so unknown future pages get cleanup without per-menu tour hooks. */
@Composable
internal fun GuidedTourHost(
    tour: UiGuidedTour?, busy: Boolean, error: String?, onAction: (UiTourAction) -> Unit,
    modifier: Modifier = Modifier, onSceneError: (Long, String) -> Unit = { _, _ -> }, content: @Composable () -> Unit,
) = key(tour != null) {
    val registry = remember { TourAnchors() }
    CompositionLocalProvider(LocalGuidedTourFeedback provides onSceneError, LocalGuidedTour provides tour, LocalGuidedTourAnchors provides if (tour != null) registry else null) {
        Box(modifier.fillMaxSize()) {
            Box(Modifier.fillMaxSize()) {
                Box(Modifier.fillMaxSize()) { content() }
                registry.menus.forEach { (owner,menu) ->
                    key(owner) {
                        Layout(content=menu.content, modifier=Modifier.fillMaxSize()) { measurables, constraints ->
                            val children=measurables.map { it.measure(constraints.copy(minWidth=0,minHeight=0)) }
                            layout(constraints.maxWidth,constraints.maxHeight) {
                                children.forEach { child -> child.place(
                                    menu.position.x.roundToInt().coerceIn(0,(constraints.maxWidth-child.width).coerceAtLeast(0)),
                                    menu.position.y.roundToInt().coerceIn(0,(constraints.maxHeight-child.height).coerceAtLeast(0)),
                                ) }
                            }
                        }
                    }
                }
            }
            if (tour != null) {
                BackHandler { onAction(UiTourAction.Close) }
                TourOverlay(tour, registry, busy, error, onAction)
            }
        }
    }
}

@OptIn(ExperimentalComposeUiApi::class)
@Composable
private fun TourOverlay(tour: UiGuidedTour, registry: TourAnchors, busy: Boolean, error: String?, onAction: (UiTourAction) -> Unit) {
    val color = Color(0xffed7100)
    val focus = remember { List(4) { FocusRequester() } }
    val overlayFocus = remember { FocusRequester() }
    var focused by remember { mutableIntStateOf(-1) }
    var panelBounds by remember { mutableStateOf(Rect.Zero) }
    var rootScreenOrigin by remember { mutableStateOf(Offset.Zero) }
    val targets = tour.targets.mapNotNull { target ->
        (if (target == "aircraft-model-option" && tour.optionUid != null) listOf("parity:altitude-planner-option:aircraft:${tour.optionUid}") else tourAnchorIds[target] ?: listOf(target)).firstNotNullOfOrNull { registry.bounds[it]?.takeIf { b -> b.width > 0 && b.height > 0 }?.translate(-rootScreenOrigin) }
    }
    val preparing = busy || (error == null && targets.size < tour.targets.size)
    val handleKey: (KeyEvent) -> Boolean = handler@{ event ->
        val key = when (event.key) {
            Key.Enter, Key.NumPadEnter -> "enter"
            else -> event.nativeKeyEvent.unicodeChar.toChar().lowercase()
        }
        val shortcut = tour.shortcuts.firstOrNull { it.key == key }
        if (shortcut != null) {
            val enabled = when (shortcut.action) {
                UiTourAction.Next -> !preparing
                UiTourAction.Back -> tour.backEnabled && !busy
                UiTourAction.Close -> true
                else -> !busy
            }
            if (enabled && event.type == KeyEventType.KeyDown && event.nativeKeyEvent.repeatCount == 0 &&
                !event.isCtrlPressed && !event.isAltPressed && !event.isMetaPressed) onAction(shortcut.action)
            // Consume release too, so Enter cannot activate the focused Close button.
            return@handler true
        }
        when(event.key) {
            Key.Escape -> { if(event.type==KeyEventType.KeyUp) onAction(UiTourAction.Close); true }
            Key.Tab -> { if(event.type==KeyEventType.KeyDown) { val enabled=listOf(0) + (if(tour.backEnabled && !busy) listOf(1) else emptyList()) + (if(!preparing) listOf(2) else emptyList()) + (if(tour.restartLabel != null && !busy) listOf(3) else emptyList()); val index=enabled.indexOf(focused); focused=if(index < 0) { if(event.isShiftPressed) enabled.last() else enabled.first() } else enabled[(index+if(event.isShiftPressed) enabled.size-1 else 1)%enabled.size];focus[focused].requestFocus() };true }
            Key.Spacebar -> false
            else -> true
        }
    }
    val currentKeyHandler = rememberUpdatedState(handleKey)
    val activity = LocalContext.current as? MainActivity
    DisposableEffect(activity) {
        // A touch or a platform child view can move focus outside Compose.
        // While modal, the tour owns window keyboard input as well as pointers.
        val handler: (android.view.KeyEvent) -> Boolean = { currentKeyHandler.value(KeyEvent(it)) }
        activity?.onGuidedTourKeyEvent = handler
        onDispose { if (activity?.onGuidedTourKeyEvent === handler) activity.onGuidedTourKeyEvent = null }
    }
    BoxWithConstraints(Modifier.fillMaxSize().onPreviewKeyEvent(handleKey)
        .focusRequester(overlayFocus).focusable().onGloballyPositioned { rootScreenOrigin = it.positionOnScreen() }.testTag("guided-tour-scrim")) {
        // Material buttons may refuse keyboard focus in touch mode. The tour
        // itself owns focus until the user tabs to one of its controls.
        LaunchedEffect(tour.generation) { focused=-1; overlayFocus.requestFocus() }
        // This full-screen sibling owns every pointer outside the panel. It is
        // transparent: there is deliberately no dimming background.
        Box(Modifier.fillMaxSize().pointerInput(Unit) {
            awaitEachGesture { do { val event = awaitPointerEvent(); event.changes.forEach { it.consume() } } while (event.changes.any { it.pressed }) }
        })
        val density = LocalDensity.current
        val widthPx = with(density) { maxWidth.toPx() }
        val heightPx = with(density) { maxHeight.toPx() }
        val pw = if (panelBounds.width > 0) panelBounds.width else with(density) { 360.dp.toPx() }
        val ph = if (panelBounds.height > 0) panelBounds.height else with(density) { 260.dp.toPx() }
        val margin = with(density) { 12.dp.toPx() }
        val autoPosition = listOf(Offset(margin, margin), Offset((widthPx-pw-margin).coerceAtLeast(margin), margin),
            Offset(margin, (heightPx-ph-margin).coerceAtLeast(margin)), Offset((widthPx-pw-margin).coerceAtLeast(margin), (heightPx-ph-margin).coerceAtLeast(margin)))
            .minBy { point -> targets.sumOf { target -> val overlap = target.intersect(Rect(point.x, point.y, point.x+pw, point.y+ph)); (overlap.width.coerceAtLeast(0f)*overlap.height.coerceAtLeast(0f)).toDouble() } }
        val position = when (tour.placement) {
            UiTourPlacement.Auto -> autoPosition
            UiTourPlacement.TopRight -> Offset((widthPx-pw-margin).coerceAtLeast(margin), margin)
            UiTourPlacement.BottomRight -> Offset((widthPx-pw-margin).coerceAtLeast(margin), (heightPx-ph-margin).coerceAtLeast(margin))
            UiTourPlacement.Center -> Offset((widthPx-pw)/2, (heightPx-ph)/2)
        }
        Canvas(Modifier.fillMaxSize()) {
            targets.forEach { target ->
                val end = Offset(panelBounds.center.x.coerceIn(target.left,target.right), panelBounds.center.y.coerceIn(target.top,target.bottom))
                val start = Offset(end.x.coerceIn(panelBounds.left,panelBounds.right), end.y.coerceIn(panelBounds.top,panelBounds.bottom))
                val delta = end-start
                val length = delta.getDistance()
                listOf(Color(0xff38210e) to 9.dp, Color.White to 7.dp, color to 4.dp).forEach { (strokeColor, strokeWidth) ->
                    val width = strokeWidth.toPx()
                    val padding = 4.dp.toPx()
                    drawRoundRect(strokeColor, target.topLeft-Offset(padding,padding), target.size.copy(width=target.width+2*padding,height=target.height+2*padding), style=Stroke(width))
                    drawLine(strokeColor,start,end,width,cap=androidx.compose.ui.graphics.StrokeCap.Round)
                    if (length > 1f) {
                        val unit=delta/length; val side=Offset(-unit.y,unit.x)
                        for (direction in listOf(-1f,1f)) drawLine(strokeColor,end,end-unit*16.dp.toPx()+side*(8.dp.toPx()*direction),width,cap=androidx.compose.ui.graphics.StrokeCap.Round)
                    }
                }
            }
        }
        val panelModifier = Modifier
            .onGloballyPositioned { panelBounds = it.boundsInWindow().translate(it.positionOnScreen() - it.positionInWindow() - rootScreenOrigin) }
            .e2eIndexedElement("parity:guided-tour-panel", "text:${android.net.Uri.encode(tour.title + "\n" + tour.body)}:state:${tour.stepId}:position:${tour.position}:anchors:${targets.size}")
        Column(Modifier.offset { IntOffset(position.x.roundToInt(),position.y.roundToInt()) }
            .widthIn(max=minOf(380.dp,maxWidth-24.dp)).fillMaxWidth().heightIn(max=maxHeight-24.dp).then(panelModifier)
            .background(Color(0xff17242c),RoundedCornerShape(14.dp)).border(2.dp,color,RoundedCornerShape(14.dp))
            .padding(14.dp)) {
            Column(Modifier.weight(1f,fill=false).verticalScroll(rememberScrollState())) {
                if (tour.presentation == UiTourPresentation.TitleCard) {
                    Box(Modifier.fillMaxWidth().heightIn(min=160.dp).testTag("guided-tour-title-content"),contentAlignment=Alignment.Center) {
                        Text(tour.title,color=Color.White,fontSize=28.sp,textAlign=androidx.compose.ui.text.style.TextAlign.Center,
                            modifier=Modifier.testTag("guided-tour-title"))
                    }
                } else {
                    Text("${tour.chapter} · ${tour.position} / ${tour.total}",color=color,fontSize=12.sp)
                    Text(tour.title,color=Color.White,fontSize=20.sp,modifier=Modifier.padding(vertical=8.dp))
                    if (tour.body.isNotEmpty()) Text(tour.body,color=Color.White,fontSize=15.sp)
                }
                error?.let { Text(it,color=Color(0xffff9c9c),modifier=Modifier.padding(top=8.dp)) }
            }
            Box(Modifier.align(Alignment.Start)) { TourRestartButton(tour,busy,focus,onAction) }
            TourNavigationButtons(tour,busy,preparing,focus,onAction)
        }
    }
}

@Composable
private fun TourRestartButton(tour: UiGuidedTour, busy: Boolean, focus: List<FocusRequester>, onAction: (UiTourAction) -> Unit) {
    tour.restartLabel?.let { label ->
        TextButton(onClick={onAction(UiTourAction.Restart)},enabled=!busy,
            modifier=Modifier.focusRequester(focus[3]).e2eIndexedControl("parity:guided-tour-restart",enabled=!busy,text=label)) {
            Text(label,color=Color(0xff50d9ef))
        }
    }
}

@Composable
private fun TourNavigationButtons(tour: UiGuidedTour, busy: Boolean, preparing: Boolean, focus: List<FocusRequester>, onAction: (UiTourAction) -> Unit) {
    Row(Modifier.fillMaxWidth().padding(top=8.dp),horizontalArrangement=Arrangement.SpaceBetween,verticalAlignment=Alignment.CenterVertically) {
        Button(onClick={onAction(UiTourAction.Close)},modifier=Modifier.focusRequester(focus[0]).e2eIndexedControl("parity:guided-tour-close",enabled=true,text=tour.closeLabel)) { Text(tour.closeLabel) }
        Button(onClick={onAction(UiTourAction.Back)},enabled=tour.backEnabled&&!busy,modifier=Modifier.focusRequester(focus[1]).e2eIndexedControl("parity:guided-tour-back",enabled=tour.backEnabled&&!busy,text="Back")) { Text("Back") }
        Button(onClick={onAction(UiTourAction.Next)},enabled=!preparing,modifier=Modifier.focusRequester(focus[2]).e2eIndexedControl("parity:guided-tour-next",enabled=!preparing,text=if(preparing) "Loading…" else tour.nextLabel)) { Text(if(preparing) "Loading…" else tour.nextLabel) }
    }
}
