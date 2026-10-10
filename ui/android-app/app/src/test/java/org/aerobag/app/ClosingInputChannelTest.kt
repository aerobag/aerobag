// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ClosingInputChannelTest {
    @Test fun closeWhileProducerIsBackpressuredIsNormalCompletion() = runBlocking {
        val channel = ClosingInputChannel<Int>(1)
        assertTrue(channel.send(1))
        val blocked = async(start = CoroutineStart.UNDISPATCHED) { channel.send(2) }
        assertFalse(blocked.isCompleted)
        channel.close()
        assertFalse(blocked.await())
        assertFalse(channel.send(3))
    }
}
