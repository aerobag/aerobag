// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.async
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test

class BackgroundSessionCommandTest {
    @Test fun heldNativeCommandDoesNotBlockUiAndPublishesOnItsCaller() = runBlocking {
        val caller = Thread.currentThread()
        val entered = CompletableDeferred<Unit>()
        val release = CountDownLatch(1)
        var published = false
        val command = async {
            deliverBackgroundSessionCommand({
                assertNotSame("Native locks must not block the UI dispatcher", caller, Thread.currentThread())
                entered.complete(Unit)
                check(release.await(5, TimeUnit.SECONDS)) { "test did not release native command" }
                42
            }, {
                assertSame(caller, Thread.currentThread())
                assertEquals(42, it)
                published = true
            }, { throw it })
        }
        try {
            entered.await()
            // This continuation must run while native work is still held.
            assertFalse(published)
        } finally { release.countDown() }
        assertTrue(command.await())
        assertTrue(published)
    }

    @Test fun failureReturnsToCallerAndIsNotRetried() = runBlocking {
        val caller = Thread.currentThread()
        val failure = IllegalStateException("native failure")
        var calls = 0
        assertFalse(deliverBackgroundSessionCommand({ calls++; throw failure },
            { fail("failed commands cannot publish") }, {
                assertSame(caller, Thread.currentThread())
                // Coroutine stack-trace recovery may copy the exception.
                assertEquals(failure.javaClass, it.javaClass)
                assertEquals(failure.message, it.message)
            }))
        assertEquals(1, calls)
    }

    @Test fun cancellationDoesNotPublishOrReportAUserError() = runBlocking {
        val entered = CompletableDeferred<Unit>()
        val release = CountDownLatch(1)
        val command = async {
            deliverBackgroundSessionCommand({
                entered.complete(Unit)
                check(release.await(5, TimeUnit.SECONDS))
            }, { fail("cancelled command published") }, { fail("cancellation is not an error") })
        }
        entered.await()
        command.cancel()
        release.countDown()
        command.cancelAndJoin()
    }
}
