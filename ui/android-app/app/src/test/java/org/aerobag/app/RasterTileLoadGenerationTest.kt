// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertTrue
import org.junit.Test

class RasterTileLoadGenerationTest {
    @Test
    fun successorGenerationCannotBeConsumedThroughRetiredQueue() {
        val retired = RasterTileLoadGeneration()
        val current = RasterTileLoadGeneration()

        retired.close()
        assertTrue(retired.requests.trySend(request(1L)).isFailure)
        val currentRequestId = current.beginRequest()
        assertTrue(current.isCurrentRequest(currentRequestId))
        assertFalse(current.isCurrentRequest(currentRequestId + 1L))
        assertTrue(current.requests.trySend(request(currentRequestId)).isSuccess)
        assertFalse(retired.requests.tryReceive().isSuccess)
        assertTrue(current.requests.tryReceive().isSuccess)
        assertNotSame(retired.bitmapCache, current.bitmapCache)
    }

    private fun request(id: Long) = RasterTileLoadRequest(
        id = id,
        mapId = "sec:nw",
        zoom = 8.0,
        centerLat = 47.0,
        centerLon = -122.0,
        visibleTiles = emptyList(),
        missingTiles = emptyList(),
        pageTilePaintTiming = null,
    )
}
