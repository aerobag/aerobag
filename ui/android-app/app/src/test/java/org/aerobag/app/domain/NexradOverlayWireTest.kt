// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.domain

import kotlinx.serialization.decodeFromString
import kotlinx.serialization.json.Json
import org.aerobag.app.generated.NexradOverlayQueryResult
import org.aerobag.app.generated.NexradOverlayStatusState
import org.junit.Assert.assertEquals
import org.junit.Test

class NexradOverlayWireTest {
    private val json = Json {
        ignoreUnknownKeys = true
    }

    @Test
    fun decodesReadyStatusFromCoreStateTaggedObject() {
        val result = json.decodeFromString<NexradOverlayQueryResult>(
            """
            {
              "status": { "state": "ready", "count": 96 },
              "tiles": [],
              "stats": { "res": 4 }
            }
            """.trimIndent(),
        )

        assertEquals(NexradOverlayStatusState.Ready, result.status.state)
        assertEquals(96, result.status.count)
        assertEquals(4, result.stats.res)
    }

    @Test
    fun decodesAbsoluteAnimationDeadlineAsLong() {
        val deadline = 4_102_444_800_123L
        val result = json.decodeFromString<NexradOverlayQueryResult>(
            """
            {
              "status": { "state": "ready", "count": 1 },
              "tiles": [],
              "stats": {},
              "animation": {
                "next_update_epoch_ms": $deadline
              }
            }
            """.trimIndent(),
        )

        assertEquals(deadline, result.animation.nextUpdateEpochMs)
    }

    @Test
    fun decodesUnavailableStatusFromCoreStateTaggedObject() {
        val result = json.decodeFromString<NexradOverlayQueryResult>(
            """
            {
              "status": { "state": "unavailable", "reason": "missing product" },
              "tiles": [],
              "stats": {}
            }
            """.trimIndent(),
        )

        assertEquals(NexradOverlayStatusState.Unavailable, result.status.state)
        assertEquals("missing product", result.status.reason)
    }
}
