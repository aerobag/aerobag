// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import org.aerobag.app.domain.UiMapLayerState

// Observe the same core-exported options and toggle state that the UI renders.
// Names come from the generated contract; there is no second layer registry.
internal fun buildMapLayersProjectionState(layers: UiMapLayerState): String =
    layers.options.joinToString("|") { option ->
        val state = layers.toggleState(option.layerId)
        "${option.layerId.name}:visible:${state.visible}:enabled:${state.enabled}"
    }
