package net.jonh.aerobag.prototype.domain

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Test
import java.io.File
import java.nio.file.Files
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream

class PackageZipStoreTest {
    @Test
    fun readEntryBytesCachesZipIdentityAndReadsEntries() {
        createZip(
            "tiles/0/10/198/650.webp" to "tile-a".encodeToByteArray(),
            "tiles/0/10/198/651.webp" to "tile-b".encodeToByteArray(),
        ).use { tempZip ->
            val zipFile = tempZip.zipFile

            val first = PackageZipStore.readEntryBytes(zipFile, "tiles/0/10/198/650.webp")
            val firstIdentity = PackageZipStore.debugIdentity(zipFile)
            val second = PackageZipStore.readEntryBytes(zipFile, "tiles/0/10/198/651.webp")
            val secondIdentity = PackageZipStore.debugIdentity(zipFile)

            assertEquals("tile-a", first?.decodeToString())
            assertEquals("tile-b", second?.decodeToString())
            assertEquals(firstIdentity, secondIdentity)
        }
    }

    @Test
    fun readEntryBytesReloadsCacheWhenZipChanges() {
        createZip("root" to "old".encodeToByteArray()).use { tempZip ->
            val zipFile = tempZip.zipFile

            assertEquals("old", PackageZipStore.readEntryBytes(zipFile, "root")?.decodeToString())
            val firstIdentity = PackageZipStore.debugIdentity(zipFile)

            rewriteZip(zipFile, "root" to "new".encodeToByteArray())

            assertEquals("new", PackageZipStore.readEntryBytes(zipFile, "root")?.decodeToString())
            assertNotEquals(firstIdentity, PackageZipStore.debugIdentity(zipFile))
        }
    }

    @Test
    fun missingEntryReturnsNullWithoutThrowing() {
        createZip("root" to "ok".encodeToByteArray()).use { tempZip ->
            val zipFile = tempZip.zipFile

            assertNull(PackageZipStore.readEntryBytes(zipFile, "missing"))
        }
    }

    private fun createZip(vararg entries: Pair<String, ByteArray>): TempZip {
        val directory = Files.createTempDirectory("package-zip-store-test").toFile()
        val zipFile = File(directory, "package.zip")
        writeZip(zipFile, *entries)
        return TempZip(directory, zipFile)
    }

    private fun rewriteZip(zipFile: File, vararg entries: Pair<String, ByteArray>) {
        writeZip(zipFile, *entries)
        zipFile.setLastModified(System.currentTimeMillis() + 1_000)
        PackageZipStore.invalidate(zipFile)
    }

    private fun writeZip(zipFile: File, vararg entries: Pair<String, ByteArray>) {
        ZipOutputStream(zipFile.outputStream().buffered()).use { zip ->
            for ((name, bytes) in entries) {
                zip.putNextEntry(ZipEntry(name))
                zip.write(bytes)
                zip.closeEntry()
            }
        }
    }

    private data class TempZip(
        private val directory: File,
        val zipFile: File,
    ) : AutoCloseable {
        override fun close() {
            PackageZipStore.invalidate(zipFile)
            directory.deleteRecursively()
        }
    }
}
