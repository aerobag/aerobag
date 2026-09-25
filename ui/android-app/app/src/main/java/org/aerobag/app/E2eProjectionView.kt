// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.net.Uri
import android.view.ViewTreeObserver
import android.view.View
import androidx.annotation.IdRes
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInWindow
import androidx.compose.ui.layout.positionOnScreen
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import java.util.concurrent.atomic.AtomicReference
import java.util.IdentityHashMap
import kotlin.math.roundToInt

/** Stable, indexed read-only state for release journeys; never rendered in ordinary builds. */
@OptIn(ExperimentalComposeUiApi::class)
@Composable
internal fun E2eProjectionView(
    @IdRes viewId: Int,
    state: String,
    modifier: Modifier = Modifier,
) {
    if (!BuildConfig.AEROBAG_E2E_ENABLED) return
    val resourceName = LocalContext.current.resources.getResourceEntryName(viewId)
    val resourceId = "org.aerobag.app:id/$resourceName"
    ObserveRenderedFrame(resourceId) { E2eProjectionSnapshot(state, null, 0) }
    Spacer(
        modifier = modifier
            .requiredSize(1.dp)
            .zIndex(Float.MAX_VALUE)
            .testTag(resourceId)
            .semantics {
                testTagsAsResourceId = true
                stateDescription = state
            },
    )
}

/** Indexed geometry and state for a real Compose control used by release journeys. */
@OptIn(ExperimentalComposeUiApi::class)
@Composable
internal fun Modifier.e2eIndexedGeometry(
    semanticTag: String,
    state: String,
): Modifier = e2eIndexedGeometry(semanticTag) { state }

@Composable
internal fun Modifier.e2eIndexedGeometry(
    semanticTag: String,
    sampleState: () -> String,
): Modifier {
    if (!BuildConfig.AEROBAG_E2E_ENABLED) return this
    val coordinates = remember(semanticTag) { AtomicReference<LayoutCoordinates?>(null) }
    val view = LocalView.current
    ObserveRenderedFrame(semanticTag) {
        coordinates.get()?.takeIf { it.isAttached }?.let {
            E2eProjectionSnapshot("${sampleState()}:window-focus:${view.hasWindowFocus()}", it.toE2eBounds(), 0)
        }
    }
    return onGloballyPositioned { coordinates.set(it) }
}

/** Data is sampled after layout by its real rendering window, not by the IPC reader. */
@Composable
private fun ObserveRenderedFrame(tag: String, sample: () -> E2eProjectionSnapshot?) {
    val view = LocalView.current
    val owner = remember(tag, view) { Any() }
    val currentSample = rememberUpdatedState(sample)
    DisposableEffect(tag, owner, view) {
        val window = RenderedObservationWindows.acquire(view)
        window.sources[owner] = tag to { currentSample.value() }
        onDispose { RenderedObservationWindows.release(view, owner, tag) }
    }
    SideEffect { view.invalidate() }
}

private object RenderedObservationWindows {
    private val windows = IdentityHashMap<View, Window>()

    fun acquire(view: View): Window = windows.getOrPut(view) { Window(view) }

    fun release(view: View, owner: Any, tag: String) {
        val window = windows[view] ?: return
        window.sources.remove(owner)
        E2eProjectionRegistry.remove(tag, owner)
        if (window.sources.isEmpty()) {
            window.close()
            windows.remove(view)
        } else view.invalidate()
    }

    class Window(private val view: View) : ViewTreeObserver.OnPreDrawListener,
        ViewTreeObserver.OnWindowFocusChangeListener {
        val sources = mutableMapOf<Any, Pair<String, () -> E2eProjectionSnapshot?>>()
        private var previous = emptyMap<Any, String>()
        init {
            view.viewTreeObserver.addOnPreDrawListener(this)
            view.viewTreeObserver.addOnWindowFocusChangeListener(this)
        }
        override fun onPreDraw(): Boolean {
            val frame = sources.mapNotNull { (owner, source) ->
                source.second()?.let { owner to (source.first to it) }
            }.toMap()
            E2eProjectionRegistry.replaceFrame(previous, frame)
            previous = frame.mapValues { it.value.first }
            // Edge effects advance during drawing, including their final frame.
            // Request a final publication if drawing changed sampled readiness.
            view.post {
                if (sources.any { (owner, source) -> source.second() != frame[owner]?.second }) view.invalidate()
            }
            return true
        }
        override fun onWindowFocusChanged(hasFocus: Boolean) { view.invalidate() }
        fun close() {
            E2eProjectionRegistry.replaceFrame(previous, emptyMap())
            if (view.viewTreeObserver.isAlive) {
                view.viewTreeObserver.removeOnPreDrawListener(this)
                view.viewTreeObserver.removeOnWindowFocusChangeListener(this)
            }
        }
    }
}

/** One semantic identity for Compose tests and indexed release-journey input. */
@Composable
internal fun Modifier.e2eIndexedElement(
    semanticTag: String,
    state: String = "enabled:true",
): Modifier = e2eIndexedGeometry(
    semanticTag = semanticTag,
    state = state,
).testTag(semanticTag).guidedTourAnchor(semanticTag)

@Composable
internal fun Modifier.e2eIndexedLabel(semanticTag: String, text: String): Modifier =
    e2eIndexedElement(semanticTag, "text:${Uri.encode(text)}:enabled:true")

/** Standard state contract for an indexed interactive control. */
@Composable
internal fun Modifier.e2eIndexedControl(
    semanticTag: String,
    enabled: Boolean,
    selected: Boolean = false,
    checked: Boolean? = null,
    text: String? = null,
    state: String? = null,
): Modifier = e2eIndexedElement(
    semanticTag = semanticTag,
    state = buildString {
        append("enabled:").append(enabled)
        append(":selected:").append(selected)
        checked?.let { append(":checked:").append(it) }
        text?.let { append(":text:").append(Uri.encode(it)) }
        state?.let { append(":state:").append(Uri.encode(it)) }
    },
)

/** Indexed editable state used by the release-journey IME boundary. */
@Composable
internal fun Modifier.e2eIndexedTextControl(
    semanticTag: String,
    text: String,
    enabled: Boolean,
    focused: Boolean,
): Modifier = e2eIndexedElement(
    semanticTag = semanticTag,
    state = "kind:text:text:${Uri.encode(text)}:enabled:$enabled:focused:$focused",
)

/** A rendered page marker, published only after Compose has positioned the page root. */
@Composable
internal fun Modifier.e2ePageRoot(semanticTag: String): Modifier =
    e2eIndexedElement(
        semanticTag = semanticTag,
        state = "enabled:true",
    )

private fun LayoutCoordinates.toE2eBounds(): String {
    val clippedWindowBounds = boundsInWindow()
    val screenOffset = positionOnScreen() - positionInWindow()
    val left = clippedWindowBounds.left + screenOffset.x
    val top = clippedWindowBounds.top + screenOffset.y
    val right = clippedWindowBounds.right + screenOffset.x
    val bottom = clippedWindowBounds.bottom + screenOffset.y
    return "[${left.roundToInt()},${top.roundToInt()}]" +
        "[${right.roundToInt()},${bottom.roundToInt()}]"
}
