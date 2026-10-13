// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.async
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.withContext
import org.junit.Assert.*
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class RetainedRuntimeTest {
    @Test fun replacementWaitsForUninterruptibleStartupAndDisposesItBeforeLoading() = runTest {
        val events = mutableListOf<String>()
        val owner = RetainedRuntime<String>(backgroundScope) { events.add("close:$it") }
        val releaseNativeCall = CompletableDeferred<Unit>()
        owner.begin {
            events.add("load:old")
            // Native startup cannot cooperate with coroutine cancellation.
            withContext(NonCancellable) { releaseNativeCall.await() }
            "old"
        }
        runCurrent()
        owner.reset()
        val replacement = async {
            owner.await { events.add("load:new"); "new" }
        }
        try {
            runCurrent()
            assertEquals(listOf("load:old"), events)
            assertNull(owner.result)
        } finally {
            releaseNativeCall.complete(Unit)
        }
        assertEquals("new", replacement.await().getOrThrow())
        assertEquals(listOf("load:old", "close:old", "load:new"), events)
        assertEquals("new", owner.result!!.getOrThrow())
        owner.reset()
    }

    @Test fun repeatedPackageAdoptionJoinsTheSameStartupEvenAfterCallerCancellation() = runTest {
        var loads = 0
        val closed = mutableListOf<Int>()
        val owner = RetainedRuntime<Int>(backgroundScope) { closed.add(it) }
        val release = CompletableDeferred<Unit>()
        suspend fun adopt(): Result<Int> {
            owner.resetFor("download-1")
            return owner.await {
                val number = ++loads
                withContext(NonCancellable) { release.await() }
                number
            }
        }
        val first = async { adopt() }
        runCurrent()
        first.cancel()
        val remounted = async { adopt() }
        runCurrent()
        release.complete(Unit)
        assertEquals(1, remounted.await().getOrThrow())
        assertEquals(1, loads)
        assertTrue(closed.isEmpty())
        assertEquals(1, adopt().getOrThrow())
        owner.reset()
    }

    @Test fun failedStartupStaysFailedUntilExplicitResetAndDifferentAdoptionReplacesIt() = runTest {
        val closed = mutableListOf<Int>()
        val owner = RetainedRuntime<Int>(backgroundScope) { closed.add(it) }
        val failure = IllegalStateException("bad nav database")
        assertSame(failure, owner.await { throw failure }.exceptionOrNull())
        assertSame(failure, owner.await { fail("must not retry"); 0 }.exceptionOrNull())
        owner.resetFor("download-1")
        assertEquals(1, owner.await { 1 }.getOrThrow())
        owner.resetFor("download-2")
        assertEquals(2, owner.await { 2 }.getOrThrow())
        assertEquals(listOf(1), closed)
        owner.reset()
    }

    @Test fun repeatedResetsCannotSkipTheStillRunningPredecessor() = runTest {
        val events = mutableListOf<String>()
        val owner = RetainedRuntime<String>(backgroundScope) { events.add("close:$it") }
        val release = CompletableDeferred<Unit>()
        owner.begin {
            events.add("load:old")
            withContext(NonCancellable) { release.await() }
            "old"
        }
        runCurrent()
        owner.reset()
        owner.begin { events.add("load:skipped"); "skipped" }
        runCurrent()
        owner.reset()
        val latest = owner.begin { events.add("load:latest"); "latest" }
        try {
            runCurrent()
            assertEquals(listOf("load:old"), events)
        } finally {
            release.complete(Unit)
        }
        assertEquals("latest", latest.await().getOrThrow())
        assertEquals(listOf("load:old", "close:old", "load:latest"), events)
        owner.reset()
        runCurrent()
        assertEquals("close:latest", events.last())
    }

    @Test fun closingTheOwnerRetiresNativeStartupWithoutPublishingIt() = runTest {
        val closed = mutableListOf<String>()
        val owner = RetainedRuntime<String>(backgroundScope) { closed.add(it) }
        val release = CompletableDeferred<Unit>()
        owner.begin {
            withContext(NonCancellable) { release.await() }
            "old"
        }
        runCurrent()
        owner.close()
        assertThrows(IllegalStateException::class.java) { owner.begin { "forbidden" } }
        release.complete(Unit)
        runCurrent()
        assertNull(owner.result)
        assertEquals(listOf("old"), closed)
    }
}
