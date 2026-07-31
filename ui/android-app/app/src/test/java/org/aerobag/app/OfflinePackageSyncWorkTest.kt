// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import java.io.IOException
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class OfflinePackageSyncWorkTest {
    @Test
    fun contentRangeMustBeginAtRequestedResumeOffset() {
        assertTrue(contentRangeStartsAt("bytes 1000-1999/3000", 1_000))
        assertFalse(contentRangeStartsAt("bytes 0-1999/3000", 1_000))
        assertFalse(contentRangeStartsAt("bytes */3000", 1_000))
        assertFalse(contentRangeStartsAt(null, 1_000))
    }

    @Test
    fun retryPolicyRetriesTransientFailuresButNotMissingOrCorruptArtifacts() {
        assertTrue(packageDownloadErrorIsRetryable(IOException("socket closed")))
        assertTrue(packageDownloadErrorIsRetryable(PackageHttpStatusException(503, "https://example.test/a")))
        assertTrue(packageDownloadErrorIsRetryable(PackageHttpStatusException(429, "https://example.test/a")))
        assertFalse(packageDownloadErrorIsRetryable(PackageHttpStatusException(404, "https://example.test/a")))
        assertFalse(packageDownloadErrorIsRetryable(IllegalStateException("checksum mismatch")))
    }

    @Test
    fun notificationProgressCountsCompletedAndActivePackagesOnce() {
        val command = OfflinePackagesControllerCommandWire.Sync(
            packageSourceBaseUrl = "https://example.test/packages",
            packagedArtifactRoot = "immutable/packaged/",
            plan = PackageManagementPlanWire(fetch = listOf("A", "B")),
            bundle = BundleManifestWire(
                packages = listOf(
                    packageArtifact("A", 10_000_000),
                    packageArtifact("B", 20_000_000),
                ),
            ),
        )
        val record = DurableOfflinePackageSyncRecord(
            id = "sync-1",
            command = command,
            phase = DurableOfflinePackageSyncPhase.Running,
            progress = OfflinePackagesSyncProgressWire(
                plannedFetchArtifactIds = setOf("A", "B"),
                completedFetchArtifactIds = setOf("A"),
                activeFetchBytesByArtifactId = mapOf("B" to 5_000_000),
            ),
            message = "Downloading",
            createdAtEpochMs = 1,
            updatedAtEpochMs = 2,
        )

        assertEquals(30_000_000, offlinePackageSyncTotalBytes(command))
        assertEquals(15_000_000, offlinePackageSyncDownloadedBytes(record))
    }

    @Test
    fun durableRecordRoundTripsTheCoreTransferCommand() {
        val record = DurableOfflinePackageSyncRecord(
            id = "sync-2",
            command = OfflinePackagesControllerCommandWire.Sync(
                packageSourceBaseUrl = "https://example.test/packages",
                packagedArtifactRoot = "immutable/packaged/",
                plan = PackageManagementPlanWire(
                    fetch = listOf("A"),
                    gc = listOf("old-A.zip"),
                ),
                bundle = BundleManifestWire(
                    packages = listOf(packageArtifact("A", 42)),
                ),
            ),
            phase = DurableOfflinePackageSyncPhase.Queued,
            progress = OfflinePackagesSyncProgressWire(
                plannedFetchArtifactIds = setOf("A"),
            ),
            message = "Waiting",
            createdAtEpochMs = 1,
            updatedAtEpochMs = 1,
        )

        val encoded = PackageManagementJson.encodeToString(
            DurableOfflinePackageSyncRecord.serializer(),
            record,
        )
        val decoded = PackageManagementJson.decodeFromString(
            DurableOfflinePackageSyncRecord.serializer(),
            encoded,
        )

        assertEquals(record, decoded)
    }

    private fun packageArtifact(
        id: String,
        sizeBytes: Long,
    ): BundlePackageArtifactWire =
        BundlePackageArtifactWire(
            id = id,
            familyId = "sec",
            regionId = "nw",
            filename = "$id.zip",
            relativePath = "$id.zip",
            sizeBytes = sizeBytes,
        )
}
