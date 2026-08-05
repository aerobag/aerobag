// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import java.io.ByteArrayInputStream
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.fail
import org.junit.Test

class RuntimeFetchTest {
    @Test
    fun resourceResponseLimitIsEnforcedWhileStreaming() {
        assertArrayEquals(
            byteArrayOf(1, 2, 3),
            readResourceBytes(ByteArrayInputStream(byteArrayOf(1, 2, 3)), 3),
        )
        try {
            readResourceBytes(ByteArrayInputStream(byteArrayOf(1, 2, 3, 4)), 3)
            fail("oversize resource response should fail")
        } catch (_: IllegalStateException) {
        }
    }
}
