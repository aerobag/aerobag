// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.encodeToJsonElement
import org.aerobag.app.generated.UiSettingsAction
import org.aerobag.app.generated.UiSettingsPageBlock
import org.aerobag.app.generated.UiSettingsPageRow
import org.aerobag.app.generated.UiSettingsPageSection
import org.aerobag.app.generated.UiSettingsPageState
import org.aerobag.app.generated.UiSettingsRowKind
import org.aerobag.app.generated.UiSettingsSyncIndicator
import org.aerobag.app.generated.UiStatusSeverity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Test

class SettingsProjectionLandingTest {
    @Test fun controlOnlyUpdatesPreserveOtherBlocksAndFullUpdatesCarryNewCloudTone() {
        val row = UiSettingsPageRow(id = "setting", title = "Setting", kind = UiSettingsRowKind.Toggle,
            actionId = "opaque-toggle", valueId = "off", stops = emptyList())
        val section = UiSettingsPageBlock.Section(UiSettingsPageSection(id = "debug", title = "Debug",
            expanded = true, rows = listOf(row), toggleAction = UiSettingsAction("opaque-fold", "off")))
        val initial = UiSettingsPageState(title = "Settings", summary = "", blocks = listOf(
            UiSettingsPageBlock.Controls(listOf(row)), section,
        ))
        val warning = UiSettingsSyncIndicator(symbol = "cloud", helpText = "Sync failed", tone = UiStatusSeverity.Caution)
        val updatedRows = listOf(row.copy(valueId = "on", syncIndicator = warning))
        val partial = landSettingsPageState(initial, listOf("settings_page_state", "blocks", "0", "rows"),
            Json.encodeToJsonElement(updatedRows), Json)
        assertEquals(updatedRows, (partial.blocks[0] as UiSettingsPageBlock.Controls).rows)
        assertSame(section, partial.blocks[1])
        assertEquals("off", (initial.blocks[0] as UiSettingsPageBlock.Controls).rows[0].valueId)
        val full = landSettingsPageState(initial, listOf("settings_page_state"), Json.encodeToJsonElement(partial), Json)
        assertEquals(partial, full)
        assertEquals(warning, (full.blocks[0] as UiSettingsPageBlock.Controls).rows[0].syncIndicator)
    }
}
