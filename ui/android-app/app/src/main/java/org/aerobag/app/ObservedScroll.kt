// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.foundation.ScrollState
import androidx.compose.foundation.rememberOverscrollEffect
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyGridScope
import androidx.compose.foundation.lazy.grid.LazyGridState
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import java.util.concurrent.atomic.AtomicLong

private val nextScrollIdentity = AtomicLong()

@Composable
private fun Modifier.observeScroll(
    orientation: String, sample: () -> String,
): Modifier {
    if (!BuildConfig.AEROBAG_E2E_ENABLED) return this
    val id = remember { nextScrollIdentity.incrementAndGet() }
    return e2eIndexedGeometry("parity:scroll:$id") {
        "kind:scroll:orientation:$orientation:${sample()}:enabled:true"
    }
}

@Composable
internal fun Modifier.observedVerticalScroll(state: ScrollState): Modifier {
    val overscroll = rememberOverscrollEffect()
    val observed = if (BuildConfig.AEROBAG_E2E_ENABLED) {
        observeScroll("vertical") {
            "position:${state.value}:backward:${state.canScrollBackward}:forward:${state.canScrollForward}" +
                ":moving:${state.isScrollInProgress || overscroll?.isInProgress == true}"
        }
    } else this
    return observed.verticalScroll(state, overscrollEffect = overscroll)
}

@Composable
internal fun Modifier.observedHorizontalScroll(state: ScrollState): Modifier {
    val overscroll = rememberOverscrollEffect()
    val observed = if (BuildConfig.AEROBAG_E2E_ENABLED) {
        observeScroll("horizontal") {
            "position:${state.value}:backward:${state.canScrollBackward}:forward:${state.canScrollForward}" +
                ":moving:${state.isScrollInProgress || overscroll?.isInProgress == true}"
        }
    } else this
    return observed.horizontalScroll(state, overscrollEffect = overscroll)
}

@Composable
internal fun ObservedLazyColumn(
    modifier: Modifier = Modifier,
    state: LazyListState = rememberLazyListState(),
    contentPadding: PaddingValues = PaddingValues(0.dp),
    reverseLayout: Boolean = false,
    verticalArrangement: Arrangement.Vertical = if (reverseLayout) Arrangement.Bottom else Arrangement.Top,
    horizontalAlignment: Alignment.Horizontal = Alignment.Start,
    userScrollEnabled: Boolean = true,
    content: LazyListScope.() -> Unit,
) {
    val overscroll = rememberOverscrollEffect()
    val observed = if (BuildConfig.AEROBAG_E2E_ENABLED) {
        modifier.observeScroll("vertical") {
            "position:${state.firstVisibleItemIndex},${state.firstVisibleItemScrollOffset}" +
                ":backward:${userScrollEnabled && state.canScrollBackward}:forward:${userScrollEnabled && state.canScrollForward}" +
                ":moving:${state.isScrollInProgress || overscroll?.isInProgress == true}"
        }
    } else modifier
    LazyColumn(
        modifier = observed,
        overscrollEffect = overscroll,
        state = state, contentPadding = contentPadding, reverseLayout = reverseLayout,
        verticalArrangement = verticalArrangement, horizontalAlignment = horizontalAlignment,
        userScrollEnabled = userScrollEnabled, content = content,
    )
}

@Composable
internal fun ObservedLazyVerticalGrid(
    columns: GridCells,
    modifier: Modifier = Modifier,
    state: LazyGridState = rememberLazyGridState(),
    horizontalArrangement: Arrangement.Horizontal = Arrangement.Start,
    verticalArrangement: Arrangement.Vertical = Arrangement.Top,
    userScrollEnabled: Boolean = true,
    content: LazyGridScope.() -> Unit,
) {
    val overscroll = rememberOverscrollEffect()
    val observed = if (BuildConfig.AEROBAG_E2E_ENABLED) {
        modifier.observeScroll("vertical") {
            "position:${state.firstVisibleItemIndex},${state.firstVisibleItemScrollOffset}" +
                ":backward:${userScrollEnabled && state.canScrollBackward}:forward:${userScrollEnabled && state.canScrollForward}" +
                ":moving:${state.isScrollInProgress || overscroll?.isInProgress == true}"
        }
    } else modifier
    LazyVerticalGrid(
        columns = columns,
        modifier = observed,
        overscrollEffect = overscroll,
        state = state, horizontalArrangement = horizontalArrangement,
        verticalArrangement = verticalArrangement, userScrollEnabled = userScrollEnabled, content = content,
    )
}
