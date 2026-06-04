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
