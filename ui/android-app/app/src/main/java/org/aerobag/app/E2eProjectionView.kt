// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.annotation.IdRes
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex

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
    Spacer(
        modifier = modifier
            .requiredSize(1.dp)
            .zIndex(Float.MAX_VALUE)
            .testTag("org.aerobag.app:id/$resourceName")
            .semantics {
                testTagsAsResourceId = true
                stateDescription = state
            },
    )
}
