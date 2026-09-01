// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.net.Uri
import android.view.MotionEvent
import android.view.ViewTreeObserver
import androidx.annotation.IdRes
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionOnScreen
import androidx.compose.ui.input.pointer.motionEventSpy
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import java.util.concurrent.atomic.AtomicReference
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
    val owner = remember(viewId) { Any() }
    SideEffect {
        E2eProjectionRegistry.publish(resourceId, state, owner)
    }
    DisposableEffect(resourceId, owner) {
        onDispose {
            E2eProjectionRegistry.remove(resourceId, owner)
        }
    }
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
internal fun Modifier.e2eIndexedControl(
    semanticTag: String,
    state: String,
): Modifier {
    if (!BuildConfig.AEROBAG_E2E_ENABLED) return this
    val owner = remember(semanticTag) { Any() }
    val bounds = remember(semanticTag) { AtomicReference<String?>(null) }
    val view = LocalView.current
    var windowFocused by remember(view) { mutableStateOf(view.hasWindowFocus()) }
    val publishedState = "$state:window-focus:$windowFocused"
    SideEffect {
        bounds.get()?.let { positionedBounds ->
            E2eProjectionRegistry.publish(semanticTag, publishedState, owner, positionedBounds)
        }
    }
    DisposableEffect(semanticTag, owner, view) {
        val focusListener = ViewTreeObserver.OnWindowFocusChangeListener { hasFocus ->
            windowFocused = hasFocus
        }
        val viewTreeObserver = view.viewTreeObserver
        viewTreeObserver.addOnWindowFocusChangeListener(focusListener)
        onDispose {
            if (viewTreeObserver.isAlive) {
                viewTreeObserver.removeOnWindowFocusChangeListener(focusListener)
            }
            E2eProjectionRegistry.remove(semanticTag, owner)
        }
    }
    return motionEventSpy { event ->
        if (event.actionMasked == MotionEvent.ACTION_UP) {
            E2eProjectionRegistry.publishTouchReceipt(semanticTag = semanticTag)
        }
    }.onGloballyPositioned { coordinates ->
            val encoded = coordinates.toE2eBounds()
            bounds.set(encoded)
            E2eProjectionRegistry.publish(semanticTag, publishedState, owner, encoded)
        }
}

/** Indexed editable state used by the release-journey IME boundary. */
@Composable
internal fun Modifier.e2eIndexedTextControl(
    semanticTag: String,
    text: String,
    enabled: Boolean,
    focused: Boolean,
): Modifier = e2eIndexedControl(
    semanticTag = semanticTag,
    state = "kind:text:text:${Uri.encode(text)}:enabled:$enabled:focused:$focused",
)

private fun LayoutCoordinates.toE2eBounds(): String {
    val topLeft = positionOnScreen()
    return "[${topLeft.x.roundToInt()},${topLeft.y.roundToInt()}]" +
        "[${(topLeft.x + size.width).roundToInt()},${(topLeft.y + size.height).roundToInt()}]"
}
