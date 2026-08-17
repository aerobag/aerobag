// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.domain

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Test

class InstalledPackagesMetadataTest {
    @Test
    fun remotePackageFilenameCannotEscapeItsStorageDirectory() {
        val root = File("build/test-package-root")

        assertEquals(
            File(root.canonicalFile, "nav-db.zip"),
            InstalledPackages.packageFileInDirectory(root, "nav-db.zip"),
        )
        for (filename in listOf("../secret", "/tmp/secret", "nested/file.zip", "nested\\file.zip", ".", "")) {
            assertThrows(IllegalArgumentException::class.java) {
                InstalledPackages.packageFileInDirectory(root, filename)
            }
        }
    }

    @Test
    fun groupingFieldsRoundTripThroughInstalledSidecar() {
        val metadata = InstalledArtifactMetadata(
            artifactId = "NW_SEC_DETAIL_SEC1_2607",
            filename = "sec-nw.zip",
            sizeBytes = 42,
            checksumSha256 = "abc",
            familyId = "sec",
            regionId = "nw",
            chartPackageTier = "detail",
        )

        assertEquals(metadata, decodeInstalledArtifactMetadata(encodeInstalledArtifactMetadata(metadata)))
    }

    @Test
    fun legacyInstalledSidecarDecodesWithoutGrouping() {
        val metadata = decodeInstalledArtifactMetadata(
            """{"artifactId":"NAV_DB_NAV18_2608_01","filename":"nav.zip","sizeBytes":18}""",
        )

        assertEquals("NAV_DB_NAV18_2608_01", metadata.artifactId)
        assertNull(metadata.familyId)
        assertNull(metadata.regionId)
        assertNull(metadata.chartPackageTier)
    }
}
