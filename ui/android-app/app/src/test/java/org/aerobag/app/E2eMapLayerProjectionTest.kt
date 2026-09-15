// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import org.aerobag.app.domain.MapLayerId
import org.aerobag.app.domain.UiMapLayerOption
import org.aerobag.app.domain.UiMapLayerState
import org.aerobag.app.domain.UiMapLayerToggleState
import org.junit.Assert.assertEquals
import org.junit.Test

class E2eMapLayerProjectionTest {
    private val visible = UiMapLayerToggleState(visible = true, enabled = true)
    private val hidden = UiMapLayerToggleState(visible = false, enabled = true)
    private val disabled = UiMapLayerToggleState(visible = false, enabled = false)
    private val layers = UiMapLayerState(
        options = MapLayerId.entries.map { UiMapLayerOption(it, it.name) },
        worldBasemap = visible,
        vectors = hidden,
        metars = disabled,
        nexrad = visible,
        traffic = hidden,
        terrainWarning = disabled,
        offlineRegions = visible,
    )

    @Test
    fun publishesEveryCoreOptionWithItsCurrentToggleState() {
        val entries = buildMapLayersProjectionState(layers).split("|")
        assertEquals(layers.options.size, entries.size)
        layers.options.zip(entries).forEach { (option, entry) ->
            val state = layers.toggleState(option.layerId)
            assertEquals("${option.layerId.name}:visible:${state.visible}:enabled:${state.enabled}", entry)
        }
    }

    @Test
    fun followsOptionMembershipOrderingAndStateChangesWithoutRetainingOldEntries() {
        val changed = layers.copy(
            options = listOf(UiMapLayerOption(MapLayerId.Vectors, "Vectors")),
            vectors = visible,
        )
        assertEquals("Vectors:visible:true:enabled:true", buildMapLayersProjectionState(changed))
        assertEquals("", buildMapLayersProjectionState(changed.copy(options = emptyList())))
    }
}
