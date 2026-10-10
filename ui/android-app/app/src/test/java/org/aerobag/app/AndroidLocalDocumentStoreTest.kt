// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.content.Context
import android.util.AtomicFile
import androidx.test.core.app.ApplicationProvider
import java.io.File
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
class AndroidLocalDocumentStoreTest {
    @Test fun oldFilesBinaryReplacementDeletionAndRestart() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val file = File(context.filesDir, "core-settings-v1.json")
        file.writeText("old settings")
        val store = AndroidLocalDocumentStore(context)
        assertEquals("old settings", store.readDocument(file.name)!!.decodeToString())
        val bytes = byteArrayOf(0, -1, 4, 42)
        store.writeDocument(file.name, bytes)
        assertArrayEquals(bytes, AndroidLocalDocumentStore(context).readDocument(file.name))
        store.writeDocument(file.name, null)
        assertNull(store.readDocument(file.name))
    }

    @Test fun interruptedReplacementKeepsPreviousDocument() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val store = AndroidLocalDocumentStore(context)
        store.writeDocument("interrupted.bin", "old".toByteArray())
        val file = AtomicFile(File(context.filesDir, "interrupted.bin"))
        file.startWrite().use { it.write("torn replacement".toByteArray()) }
        assertEquals("old", AndroidLocalDocumentStore(context).readDocument("interrupted.bin")!!.decodeToString())
    }

    @Test fun filesystemErrorsPropagateToTheNativeHost() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val store = AndroidLocalDocumentStore(context)
        File(context.filesDir, "blocked.bin").mkdirs()
        assertThrows(Exception::class.java) { store.writeDocument("blocked.bin", byteArrayOf(1)) }
        assertThrows(Exception::class.java) { store.readDocument("blocked.bin") }
        assertThrows(IllegalArgumentException::class.java) { store.readDocument("../outside") }
    }
}
