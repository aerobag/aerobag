// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.domain

import kotlinx.serialization.decodeFromString
import kotlinx.serialization.json.Json
import org.aerobag.app.generated.NexradOverlayQueryResult
import org.aerobag.app.generated.NexradOverlayStatus
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
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
              "stats": {
                "source_tile_count": 0,
                "render_piece_count": 0,
                "split_count": 0,
                "max_affine_error_px": 0.0,
                "level_pixel_span_px": 0.0,
                "max_level_pixel_stretch_px": 0.0,
                "max_stack_depth": 0,
                "res": 4
              },
              "animation": {
                "phase": "idle",
                "frame_count": 0,
                "age_labels": [],
                "age_summary": "---"
              }
            }
            """.trimIndent(),
        )

        assertTrue(result.status is NexradOverlayStatus.Ready)
        val status = result.status as NexradOverlayStatus.Ready
        assertEquals(96, status.count)
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
              "stats": {
                "source_tile_count": 0,
                "render_piece_count": 0,
                "split_count": 0,
                "max_affine_error_px": 0.0,
                "level_pixel_span_px": 0.0,
                "max_level_pixel_stretch_px": 0.0,
                "max_stack_depth": 0
              },
              "animation": {
                "phase": "idle",
                "frame_count": 0,
                "age_labels": [],
                "age_summary": "---",
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
              "stats": {
                "source_tile_count": 0,
                "render_piece_count": 0,
                "split_count": 0,
                "max_affine_error_px": 0.0,
                "level_pixel_span_px": 0.0,
                "max_level_pixel_stretch_px": 0.0,
                "max_stack_depth": 0
              },
              "animation": {
                "phase": "idle",
                "frame_count": 0,
                "age_labels": [],
                "age_summary": "---"
              }
            }
            """.trimIndent(),
        )

        assertTrue(result.status is NexradOverlayStatus.Unavailable)
        val status = result.status as NexradOverlayStatus.Unavailable
        assertEquals("missing product", status.reason)
    }
}
