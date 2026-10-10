// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import java.io.IOException
import java.util.concurrent.CountDownLatch
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import org.junit.Assert.*
import org.junit.Test

class DuplexStreamPumpTest {
    private class Stream(private val blockConnect: Boolean = false, private val blockWrite: Boolean = false) : BlockingDuplexStream {
        val connectEntered = CountDownLatch(1)
        val readEntered = CountDownLatch(1)
        val writeEntered = CountDownLatch(1)
        val closed = CountDownLatch(1)
        val connectReturned = CountDownLatch(1)
        val readReturned = CountDownLatch(1)
        val writeReturned = CountDownLatch(1)
        val reads = LinkedBlockingQueue<ByteArray>()
        val writes = LinkedBlockingQueue<ByteArray>()
        override fun connect() {
            connectEntered.countDown()
            try { if (blockConnect) closed.await() } finally { connectReturned.countDown() }
        }
        override fun read(buffer: ByteArray): Int {
            readEntered.countDown()
            try {
                val bytes = reads.take()
                if (bytes.isEmpty()) return -1
                bytes.copyInto(buffer)
                return bytes.size
            } finally { readReturned.countDown() }
        }
        override fun write(bytes: ByteArray) {
            writeEntered.countDown()
            try {
                if (blockWrite) { closed.await(); throw IOException("write aborted") }
                writes.put(bytes.copyOf())
            } finally { writeReturned.countDown() }
        }
        override fun close() { closed.countDown(); reads.offer(byteArrayOf()) }
    }

    private sealed interface Notice {
        data class Connected(val id: Long) : Notice
        class Received(val id: Long, val bytes: ByteArray) : Notice
        data class Written(val id: Long, val write: Long) : Notice
        data class Failed(val id: Long, val operation: StreamOperation) : Notice
    }
    private class Consumer : DuplexStreamCallbacks {
        val notices = LinkedBlockingQueue<Notice>()
        override fun connected(connectionId: Long) { notices.put(Notice.Connected(connectionId)) }
        override fun received(connectionId: Long, bytes: ByteArray) { notices.put(Notice.Received(connectionId, bytes)) }
        override fun written(connectionId: Long, writeId: Long) { notices.put(Notice.Written(connectionId, writeId)) }
        override fun failed(connectionId: Long, operation: StreamOperation, permissionDenied: Boolean) {
            notices.put(Notice.Failed(connectionId, operation))
        }
        fun next(): Notice = checkNotNull(notices.poll(5, TimeUnit.SECONDS)) { "IO callback did not arrive" }
    }
    private fun await(latch: CountDownLatch) { assertTrue("IO worker did not reach gate", latch.await(5, TimeUnit.SECONDS)) }

    @Test
    fun transfersOwnedBytesAndConfirmsOnlyCompletedWrites() {
        val stream = Stream()
        val consumer = Consumer()
        DuplexStreamPump(7, stream, consumer).use { pump ->
            pump.start()
            assertEquals(Notice.Connected(7), consumer.next())
            stream.reads.put(byteArrayOf(1, 2, 3))
            val first = consumer.next() as Notice.Received
            stream.reads.put(byteArrayOf(4, 5, 6))
            val second = consumer.next() as Notice.Received
            assertArrayEquals(byteArrayOf(1, 2, 3), first.bytes)
            assertArrayEquals(byteArrayOf(4, 5, 6), second.bytes)
            assertEquals(7L, first.id)
            pump.write(42, byteArrayOf(10, 11))
            assertEquals(Notice.Written(7, 42), consumer.next())
            assertArrayEquals(byteArrayOf(10, 11), stream.writes.poll())
        }
        await(stream.closed)
    }

    @Test
    fun closeAbortsBlockedConnectWithoutAConnectOrWriteCallback() {
        val stream = Stream(blockConnect = true)
        val consumer = Consumer()
        val pump = DuplexStreamPump(1, stream, consumer)
        pump.start()
        await(stream.connectEntered)
        pump.close()
        await(stream.connectReturned)
        assertTrue(consumer.notices.isEmpty())
    }

    @Test
    fun closeAbortsReadAndWriteWithoutWaitingForPeerOrReportingSuccessfulWrite() {
        val stream = Stream(blockWrite = true)
        val consumer = Consumer()
        val pump = DuplexStreamPump(1, stream, consumer)
        pump.start()
        assertEquals(Notice.Connected(1), consumer.next())
        await(stream.readEntered)
        pump.write(2, byteArrayOf(3))
        await(stream.writeEntered)
        pump.close()
        await(stream.readReturned)
        await(stream.writeReturned)
        assertTrue(consumer.notices.isEmpty())
    }

    @Test
    fun EOFReportsFailureAndClosesWithoutRetrying() {
        val stream = Stream()
        val consumer = Consumer()
        DuplexStreamPump(3, stream, consumer).use { pump ->
            pump.start()
            assertEquals(Notice.Connected(3), consumer.next())
            stream.reads.put(byteArrayOf())
            assertEquals(Notice.Failed(3, StreamOperation.Read), consumer.next())
            await(stream.closed)
        }
    }

    @Test
    fun immediateConnectFailureCannotRaceWorkerSubmissionOrLoseItsNotification() {
        repeat(50) { attempt ->
            val closed = CountDownLatch(1)
            val stream = object : BlockingDuplexStream {
                override fun connect() { throw IOException("synthetic failure") }
                override fun read(buffer: ByteArray): Int = error("not connected")
                override fun write(bytes: ByteArray) { error("not connected") }
                override fun close() { closed.countDown() }
            }
            val consumer = Consumer()
            DuplexStreamPump(attempt.toLong(), stream, consumer).use { pump ->
                pump.start()
                assertEquals(Notice.Failed(attempt.toLong(), StreamOperation.Connect), consumer.next())
                await(closed)
            }
        }
    }

    @Test
    fun failedConsumerStillClosesTheSocket() {
        val closed = CountDownLatch(1)
        val stream = object : BlockingDuplexStream {
            override fun connect() { throw SecurityException("permission revoked") }
            override fun read(buffer: ByteArray): Int = error("not connected")
            override fun write(bytes: ByteArray) { error("not connected") }
            override fun close() { closed.countDown() }
        }
        val failed = CountDownLatch(1)
        val callbacks = object : DuplexStreamCallbacks {
            override fun connected(connectionId: Long) { error("not connected") }
            override fun received(connectionId: Long, bytes: ByteArray) { error("not connected") }
            override fun written(connectionId: Long, writeId: Long) { error("not connected") }
            override fun failed(connectionId: Long, operation: StreamOperation, permissionDenied: Boolean) {
                assertTrue(permissionDenied)
                failed.countDown()
                error("consumer failed")
            }
        }
        DuplexStreamPump(9, stream, callbacks).use { pump ->
            pump.start()
            await(failed)
            await(closed)
        }
    }

    @Test
    fun writeQueueCannotGrowBehindBlockedSocket() {
        val stream = Stream(blockWrite = true)
        val consumer = Consumer()
        DuplexStreamPump(4, stream, consumer).use { pump ->
            pump.start()
            assertEquals(Notice.Connected(4), consumer.next())
            pump.write(1, byteArrayOf(1))
            await(stream.writeEntered)
            pump.write(2, byteArrayOf(2))
            assertEquals(Notice.Failed(4, StreamOperation.Write), consumer.next())
            await(stream.closed)
            await(stream.writeReturned)
            assertTrue(consumer.notices.isEmpty())
        }
    }
}
