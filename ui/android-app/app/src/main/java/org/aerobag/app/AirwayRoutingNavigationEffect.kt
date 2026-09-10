// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue

/** Only a newly activated editor requests navigation; mounting existing state does not. */
@Composable
internal fun AirwayRoutingNavigationEffect(activeEditId: String?, onOpenMap: () -> Unit) {
    var observedEditId by remember { mutableStateOf(activeEditId) }
    val currentOpenMap by rememberUpdatedState(onOpenMap)
    LaunchedEffect(activeEditId) {
        val newlyOpened = activeEditId != null && activeEditId != observedEditId
        observedEditId = activeEditId
        if (newlyOpened) currentOpenMap()
    }
}
