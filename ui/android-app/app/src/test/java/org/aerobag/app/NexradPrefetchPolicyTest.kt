// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import org.aerobag.app.generated.NexradOverlayCacheResource
import org.junit.Assert.assertEquals
import org.junit.Assert.fail
import org.junit.Test

class NexradPrefetchPolicyTest {
    @Test
    fun failedBackgroundFrameDoesNotSuppressLaterFrames() = runBlocking {
        val resources = listOf(
            resource("old", "/old.png"),
            resource("old-duplicate", "/old.png"),
            resource("selected", "/selected.png"),
        )
        val attempted = mutableListOf<String>()
        val failed = mutableListOf<String>()

        val completed = prefetchNexradCacheResourcesBestEffort(
            resources = resources,
            fetch = { resource ->
                attempted += resource.src
                if (resource.src == "/old.png") {
                    error("retired frame")
                }
            },
            reportFailure = { resource, _ -> failed += resource.src },
        )

        assertEquals(listOf("/old.png", "/selected.png"), attempted)
        assertEquals(listOf("/old.png"), failed)
        assertEquals(setOf("/selected.png"), completed)
    }

    @Test
    fun completedResourcesAreNotFetchedAgainUntilTheyLeaveAndReenterThePlan() {
        val tracker = NexradPrefetchTracker()
        val old = resource("old", "/old.png")
        val current = resource("current", "/current.png")

        assertEquals(listOf(old, current), tracker.pending(listOf(old, current)))
        tracker.recordCompleted(listOf(old.src, current.src))
        assertEquals(emptyList<NexradOverlayCacheResource>(), tracker.pending(listOf(old, current)))

        assertEquals(emptyList<NexradOverlayCacheResource>(), tracker.pending(listOf(current)))
        assertEquals(listOf(old), tracker.pending(listOf(old, current)))
    }

    @Test
    fun cancellationStillStopsPrefetch() = runBlocking {
        try {
            prefetchNexradCacheResourcesBestEffort(
                resources = listOf(resource("frame", "/frame.png")),
                fetch = { throw CancellationException("stopped") },
                reportFailure = { _, _ -> error("cancellation must not be reported as a fetch failure") },
            )
            fail("expected cancellation")
        } catch (_: CancellationException) {
            // Expected: cancellation controls coroutine lifetime, unlike an individual fetch fault.
        }
    }

    private fun resource(frame: String, src: String) = NexradOverlayCacheResource(
        frameVersion = frame,
        src = src,
    )
}
