// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.domain

import org.junit.Assert.assertEquals
import org.junit.Test

class RasterMapUiStateTest {
    @Test
    fun decodesCoreOwnedNoneFamilyId() {
        val state = decodeRasterMapUiStateForTesting(
            """
            {
              "selected_map_id": "none",
              "selected_map_label": "NONE",
              "selected_family_id": "none",
              "selected_family_label": "NONE",
              "selected_family_launcher_label": "NONE",
              "min_zoom": 0.0,
              "max_zoom": 12.0,
              "initial_viewport": {
                "lat": 47.6,
                "lon": -122.3,
                "zoom": 8.0
              },
              "family_options": [
                {
                  "id": "none",
                  "label": "NONE",
                  "launcher_label": "NONE",
                  "enabled": true,
                  "active": true
                }
              ]
            }
            """.trimIndent(),
        )

        assertEquals("none", state.selectedMapId)
        assertEquals("none", state.selectedFamilyId)
        assertEquals("none", state.familyOptions.single().id)
        assertEquals(true, state.familyOptions.single().active)
    }
}
