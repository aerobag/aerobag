// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.domain

import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Test

class MapOverlayWireTest {
    @Test
    fun decodesCoreOwnedOfflineRegionSummary() {
        val overlay = Json.decodeFromString<WireMapOverlayQueryResult>(
            """
            {
              "visible_features": [],
              "visible_metars": [],
              "visible_pireps": [],
              "airspace_paths": [],
              "tfr_paths": [],
              "airspace_labels": [],
              "offline_regions": [{
                "id": "nw",
                "kind": "cycle",
                "region_id": "nw",
                "label": "Northwest",
                "color_key": "offline_region",
                "summary": [{"action": "fetch", "cycle": "2609", "count": 2}],
                "points": [],
                "label_x": 10.0,
                "label_y": 20.0
              }]
            }
            """.trimIndent(),
        )

        assertEquals(
            WireOfflineRegionSummaryEntry(action = "fetch", cycle = "2609", count = 2),
            overlay.offline_regions.single().summary.single(),
        )
    }
}
