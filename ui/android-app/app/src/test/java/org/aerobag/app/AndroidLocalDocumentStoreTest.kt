// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.content.Context
import android.util.AtomicFile
import androidx.test.core.app.ApplicationProvider
import java.io.File
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.util.concurrent.CompletableFuture
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
class AndroidLocalDocumentStoreTest {
    @Test fun separateStoresSerializeTheSameTemporaryFileIncludingCleanup() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val beforeRename = CountDownLatch(1)
        val releaseRename = CountDownLatch(1)
        val old = AndroidLocalDocumentStore(context) { source, target ->
            beforeRename.countDown()
            check(releaseRename.await(5, TimeUnit.SECONDS)) { "test did not release rename" }
            Files.move(source.toPath(), target.toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING)
        }
        val replacement = AndroidLocalDocumentStore(context)
        val firstResult = CompletableFuture<Unit>()
        val secondResult = CompletableFuture<Unit>()
        fun writer(store: AndroidLocalDocumentStore, bytes: String, done: CompletableFuture<Unit>) = Thread {
            try {
                store.writeDocument("core-settings-v1.json", bytes.toByteArray())
                done.complete(Unit)
            } catch (error: Throwable) { done.completeExceptionally(error) }
        }
        val first = writer(old, "old", firstResult)
        val second = writer(replacement, "new", secondResult)
        first.start()
        try {
            assertTrue(beforeRename.await(5, TimeUnit.SECONDS))
            second.start()
            val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(5)
            // Wait for a JVM monitor receipt, not an arbitrary timing grace period.
            while (second.state != Thread.State.BLOCKED && !secondResult.isDone) {
                check(System.nanoTime() < deadline) { "second writer neither blocked nor completed" }
                Thread.yield()
            }
            assertFalse("second writer must wait for first writer's rename AND cleanup", secondResult.isDone)
        } finally {
            releaseRename.countDown()
            first.join(5_000)
            second.join(5_000)
        }
        firstResult.get(5, TimeUnit.SECONDS)
        secondResult.get(5, TimeUnit.SECONDS)
        assertEquals("new", replacement.readDocument("core-settings-v1.json")!!.decodeToString())
        assertFalse(File(context.filesDir, ".core-settings-v1.json.pending").exists())
    }

    @Test fun queuedWritesFromRetiredSessionCannotOverwriteReplacementSettings() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val oldSession = AndroidLocalDocumentStore(context)
        val key = "core-settings-v1.json"
        oldSession.writeDocument(key, "old settings".toByteArray())
        // JNI callbacks can retain the store after the native session is destroyed.
        val queuedWrite = { oldSession.writeDocument(key, "late old settings".toByteArray()) }
        oldSession.close()
        val replacement = AndroidLocalDocumentStore(context)
        assertEquals("old settings", replacement.readDocument(key)!!.decodeToString())
        replacement.writeDocument(key, "new settings".toByteArray())
        assertThrows(IllegalStateException::class.java) { queuedWrite() }
        assertThrows(IllegalStateException::class.java) { oldSession.writeDocument(key, null) }
        assertThrows(IllegalStateException::class.java) { oldSession.clearSettings() }
        assertThrows(IllegalStateException::class.java) { oldSession.readDocument(key) }
        oldSession.close()
        assertEquals("new settings", replacement.readDocument(key)!!.decodeToString())
    }

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
