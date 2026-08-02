// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class OfflinePackagePreferenceHandoffTest {
    @Test
    fun profileBlockedByPackageWorkBecomesEligibleWhenWorkFinishes() {
        val desired = "nw-and-sw"

        assertFalse(
            shouldApplySynchronizedOfflinePackagePreferences(
                offlinePackagesRouted = true,
                desiredPreferencesJson = desired,
                appliedPreferencesJson = "prior",
                operationActive = true,
            ),
        )
        assertTrue(
            shouldApplySynchronizedOfflinePackagePreferences(
                offlinePackagesRouted = true,
                desiredPreferencesJson = desired,
                appliedPreferencesJson = "prior",
                operationActive = false,
            ),
        )
        assertFalse(
            shouldApplySynchronizedOfflinePackagePreferences(
                offlinePackagesRouted = true,
                desiredPreferencesJson = desired,
                appliedPreferencesJson = desired,
                operationActive = false,
            ),
        )
    }

    @Test
    fun hiddenOfflinePackagesPageDefersProfileApplication() {
        assertFalse(
            shouldApplySynchronizedOfflinePackagePreferences(
                offlinePackagesRouted = false,
                desiredPreferencesJson = "new",
                appliedPreferencesJson = "old",
                operationActive = false,
            ),
        )
    }
}
