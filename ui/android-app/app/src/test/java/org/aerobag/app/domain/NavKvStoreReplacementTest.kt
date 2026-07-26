package org.aerobag.app.domain

import java.io.File
import java.lang.reflect.Proxy
import java.nio.file.Files
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class NavKvStoreReplacementTest {
    @Test
    fun failedOptionalSessionResourceResumesWithEmptyPayload() {
        val directory = Files.createTempDirectory("nav-kv-optional-resource-test").toFile()
        val artifact = createArtifact(directory, "nav.zip", "page")
        val bridge = FakeNavKvBridge()
        val store = NavKvStore.openInstalledArtifacts(listOf(artifact), "", bridge.nativeBridge)
        var operationCalls = 0
        var ingestedBytes: ByteArray? = null

        try {
            val outcome = store.runPagedSessionOperation(
                fetchSessionResource = { error("terrain tile is unavailable") },
                ingestSessionResource = { _, bytes -> ingestedBytes = bytes },
                operation = {
                    operationCalls += 1
                    if (operationCalls == 1) {
                        """{"state":"need_resources","resources":[{"id":"terrain/source/missing","source":{"kind":"unavailable","message":"missing"},"optional":true}]}"""
                    } else {
                        """{"state":"complete","result":"inspector"}"""
                    }
                },
            )

            assertEquals("inspector", outcome.result.jsonPrimitive.content)
            assertEquals(2, operationCalls)
            assertArrayEquals(ByteArray(0), ingestedBytes)
        } finally {
            store.close()
            PackageZipStore.invalidate(artifact.file)
            directory.deleteRecursively()
        }
    }

    @Test
    fun replacingInstalledArtifactsCommitsCoreTransactionBeforeRetiringOldNavDb() {
        val directory = Files.createTempDirectory("nav-kv-replacement-test").toFile()
        val oldArtifact = createArtifact(directory, "old-nav.zip", "old-page")
        val newArtifact = createArtifact(directory, "new-nav.zip", "new-page")
        val bridge = FakeNavKvBridge()
        val store = NavKvStore.openInstalledArtifacts(listOf(oldArtifact), "", bridge.nativeBridge)

        try {
            store.attachToSession(42L)
            assertEquals("old-page", readPageValue(store))

            val outcome = store.replaceInstalledArtifacts(listOf(newArtifact), "", 42L)
            PackageZipStore.invalidate(oldArtifact.file)
            assertTrue(oldArtifact.file.delete())

            assertEquals("adopted", outcome.result.jsonObject.getValue("disposition").jsonPrimitive.content)
            assertEquals("new-page", readPageValue(store))
            assertEquals(listOf(101L to 42L), bridge.attachments)
            assertTrue(bridge.advances.size >= 2)
            assertTrue(bridge.advances.all { it == (102L to 42L) })
            assertEquals(listOf(101L), bridge.destroyedHandles)
        } finally {
            store.close()
            PackageZipStore.invalidate(newArtifact.file)
            directory.deleteRecursively()
        }

        assertEquals(listOf(101L, 102L), bridge.destroyedHandles)
    }

    @Test
    fun rejectedCandidateKeepsOldBackendAndDestroysCandidate() {
        val directory = Files.createTempDirectory("nav-kv-rejection-test").toFile()
        val oldArtifact = createArtifact(directory, "old-nav.zip", "old-page")
        val newArtifact = createArtifact(directory, "new-nav.zip", "new-page")
        val bridge = FakeNavKvBridge().apply { advanceDisposition = "rejected" }
        val store = NavKvStore.openInstalledArtifacts(listOf(oldArtifact), "", bridge.nativeBridge)

        try {
            store.attachToSession(42L)
            assertEquals("old-page", readPageValue(store))

            val outcome = store.replaceInstalledArtifacts(listOf(newArtifact), "", 42L)

            assertEquals("rejected", outcome.result.jsonObject.getValue("disposition").jsonPrimitive.content)
            assertEquals("old-page", readPageValue(store))
            assertEquals(listOf(102L), bridge.destroyedHandles)
            assertTrue(oldArtifact.file.isFile)
        } finally {
            store.close()
            PackageZipStore.invalidate(oldArtifact.file)
            PackageZipStore.invalidate(newArtifact.file)
            directory.deleteRecursively()
        }

        assertEquals(listOf(102L, 101L), bridge.destroyedHandles)
    }

    @Test
    fun candidatePageIoDoesNotBlockOldGenerationReaders() {
        val directory = Files.createTempDirectory("nav-kv-concurrent-replacement-test").toFile()
        val oldArtifact = createArtifact(directory, "old-nav.zip", "old-page")
        val newArtifact = createArtifact(directory, "new-nav.zip", "new-page")
        val bridge = FakeNavKvBridge().apply { blockCandidatePageInsert = true }
        val store = NavKvStore.openInstalledArtifacts(listOf(oldArtifact), "", bridge.nativeBridge)
        val executor = Executors.newSingleThreadExecutor()

        try {
            store.attachToSession(42L)
            assertEquals("old-page", readPageValue(store))
            val replacement = executor.submit<PagedSessionOperationResult> {
                store.replaceInstalledArtifacts(listOf(newArtifact), "", 42L)
            }
            assertTrue(bridge.candidatePageInsertStarted.await(2, TimeUnit.SECONDS))

            assertEquals("old-page", readPageValue(store))

            bridge.releaseCandidatePageInsert.countDown()
            assertEquals(
                "adopted",
                replacement.get(2, TimeUnit.SECONDS)
                    .result.jsonObject.getValue("disposition").jsonPrimitive.content,
            )
            assertEquals("new-page", readPageValue(store))
        } finally {
            bridge.releaseCandidatePageInsert.countDown()
            executor.shutdownNow()
            store.close()
            PackageZipStore.invalidate(oldArtifact.file)
            PackageZipStore.invalidate(newArtifact.file)
            directory.deleteRecursively()
        }
    }

    private fun readPageValue(store: NavKvStore): String =
        store.runCoreOperationElement(buildJsonObject { put("kind", "test") }).jsonPrimitive.content

    private fun createArtifact(directory: File, filename: String, page: String): InstalledPackageArtifact {
        val file = File(directory, filename)
        ZipOutputStream(file.outputStream().buffered()).use { zip ->
            zip.putNextEntry(ZipEntry("had/pages/1.bin"))
            zip.write(page.encodeToByteArray())
            zip.closeEntry()
        }
        return InstalledPackageArtifact(
            artifactId = filename.removeSuffix(".zip"),
            filename = filename,
            file = file,
            sizeBytes = file.length(),
        )
    }

    private class FakeNavKvBridge {
        private val json = Json { ignoreUnknownKeys = true }
        private var nextControllerHandle = 1L
        private var nextNavKvHandle = 101L
        private val controllerFilenames = mutableMapOf<Long, String>()
        private val navKvFilenames = mutableMapOf<Long, String>()
        private val pageValues = mutableMapOf<Long, String>()

        val attachments = mutableListOf<Pair<Long, Long>>()
        val advances = mutableListOf<Pair<Long, Long>>()
        val destroyedHandles = mutableListOf<Long>()
        var advanceDisposition = "adopted"
        var blockCandidatePageInsert = false
        val candidatePageInsertStarted = CountDownLatch(1)
        val releaseCandidatePageInsert = CountDownLatch(1)

        val nativeBridge: NativeBridge = Proxy.newProxyInstance(
            NativeBridge::class.java.classLoader,
            arrayOf(NativeBridge::class.java),
        ) { _, method, args ->
            when (method.name) {
                "navDbOpenControllerCreateFromInstalledArtifacts" -> {
                    val artifacts = json.parseToJsonElement(args!![0] as String).jsonArray
                    val filename = artifacts.single().jsonObject.getValue("filename").jsonPrimitive.content
                    nextControllerHandle++.also { controllerFilenames[it] = filename }
                }
                "navDbOpenControllerStep" -> "{\"state\":\"complete\"}"
                "navDbOpenControllerFinish" -> {
                    val controllerHandle = args!![0] as Long
                    val filename = controllerFilenames.getValue(controllerHandle)
                    val navKvHandle = nextNavKvHandle++
                    navKvFilenames[navKvHandle] = filename
                    """{"nav_kv_handle":$navKvHandle,"open_result":{"selected_package_id":"$filename","selected_filename":"$filename"}}"""
                }
                "navDbOpenControllerDestroy" -> Unit
                "attachNavKvStoreToSession" -> {
                    attachments += (args!![0] as Long) to (args[1] as Long)
                    Unit
                }
                "advanceNavKvStoreInSessionJson" -> {
                    val handle = args!![0] as Long
                    val sessionHandle = args[1] as Long
                    advances += handle to sessionHandle
                    if (advanceDisposition == "adopted" && pageValues[handle] == null) {
                        """{"state":"need_resources","resources":[{"id":"nav_kv/page/1","source":{"kind":"nav_kv_member","member_path":"had/pages/1.bin"},"optional":false}]}"""
                    } else {
                        """{"state":"complete","result":{"disposition":"$advanceDisposition"},"invalidations":["nav_data"]}"""
                    }
                }
                "coreHadOperation" -> {
                    val handle = args!![0] as Long
                    pageValues[handle]?.let { value ->
                        "{\"state\":\"complete\",\"result\":\"$value\"}"
                    } ?: """{"state":"need_resources","resources":[{"id":"nav_kv/page/1","source":{"kind":"nav_kv_member","member_path":"had/pages/1.bin"},"optional":false}]}"""
                }
                "navKvInsertResource" -> {
                    val handle = args!![0] as Long
                    assertTrue(navKvFilenames.containsKey(handle))
                    if (blockCandidatePageInsert && handle != 101L) {
                        candidatePageInsertStarted.countDown()
                        assertTrue(releaseCandidatePageInsert.await(2, TimeUnit.SECONDS))
                    }
                    pageValues[handle] = (args[2] as ByteArray).decodeToString()
                    Unit
                }
                "navKvDestroy" -> {
                    destroyedHandles += args!![0] as Long
                    Unit
                }
                "equals" -> false
                "hashCode" -> 0
                "toString" -> "NavKvStoreReplacementTestBridge"
                else -> error("unexpected NativeBridge call in NavKvStoreReplacementTest: ${method.name}")
            }
        } as NativeBridge
    }
}
