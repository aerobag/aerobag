// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.view.View
import androidx.annotation.IdRes
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.view.ViewCompat

/** Stable, indexed read-only state for release journeys; never rendered in ordinary builds. */
@Composable
internal fun E2eProjectionView(
    @IdRes viewId: Int,
    state: String,
    modifier: Modifier = Modifier,
) {
    if (!BuildConfig.AEROBAG_E2E_ENABLED) return
    AndroidView(
        factory = { context ->
            View(context).apply {
                id = viewId
                importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_YES
            }
        },
        update = { view -> ViewCompat.setStateDescription(view, state) },
        modifier = modifier,
    )
}
