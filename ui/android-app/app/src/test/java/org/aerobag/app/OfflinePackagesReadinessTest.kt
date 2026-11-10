// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.view.View
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.test.click
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performTouchInput
import androidx.test.core.app.ApplicationProvider
import org.aerobag.app.domain.UiThemeLoader
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w400dp-h800dp")
class OfflinePackagesReadinessTest {
    @get:Rule val compose = createComposeRule()

    @Test fun catalogCompletionAndSyncEnablementFollowRenderedStateThroughRefreshAndRemount() {
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        val window = View(ApplicationProvider.getApplicationContext()).apply { layout(0, 0, 400, 800) }
        val fetchedAt = mutableStateOf(100L)
        val loading = mutableStateOf(false)
        val operationActive = mutableStateOf(false)
        val mounted = mutableStateOf(true)
        var syncs = 0
        compose.setContent {
            CompositionLocalProvider(LocalAerobagUiTheme provides theme, LocalView provides window) {
                if (mounted.value) OfflinePackagesPanel(
                    uiState = OfflinePackagesUiStateWire(), storageCapacityLabel = null,
                    syncMessage = null, cancelRequested = false, showSimulatedClockButtons = false,
                    packageSourceBaseUrl = "http://fixture/packages", onPackageSourceBaseUrlChange = {},
                    onRefreshLibrary = { loading.value = true; operationActive.value = true },
                    libraryRefreshInFlight = loading.value, libraryFetchedAtEpochMs = fetchedAt.value,
                    operationInFlight = operationActive.value,
                    packageSourceEditable = !loading.value, refreshEnabled = !loading.value,
                    refreshCancelEnabled = loading.value, syncEnabled = !loading.value,
                    syncCancelEnabled = false, plannerInteractionsEnabled = true,
                    onCancelRefresh = {}, onRowClick = {}, onClockClick = {},
                    onSync = { syncs++ }, onCancelOperation = {}, syncInFlight = false,
                    closeEnabled = false, onClose = {}, showCloseButton = false,
                )
            }
        }
        fun frame() = compose.runOnIdle { window.drawObservationFrame() }
        fun catalog() = E2eProjectionRegistry.read("parity:offline-packages-panel")!!
        fun sync() = E2eProjectionRegistry.read("parity:offline-sync-button")!!
        frame()
        val original = catalog()
        compose.onNodeWithTag("parity:offline-refresh-button").performTouchInput { click() }
        compose.runOnIdle {
            assertEquals("old frame does not prove refresh completion", original, catalog())
        }
        frame()
        assertTrue(catalog().state.contains("catalog-fetched-at:100:catalog-loading:true"))
        assertTrue(sync().state.contains("enabled:false"))

        // Core has applied the new library, but platform cleanup still owns the operation.
        compose.runOnIdle { fetchedAt.value = 200L; loading.value = false }
        frame()
        assertTrue(catalog().state.contains("catalog-fetched-at:200:catalog-loading:false:operation-active:true"))
        assertTrue("do not offer an action its handler will ignore", sync().state.contains("enabled:false"))
        compose.onNodeWithTag("parity:offline-sync-button").performTouchInput { click() }
        compose.runOnIdle { assertEquals(0, syncs); operationActive.value = false }
        frame()
        assertTrue(sync().state.contains("enabled:true"))
        compose.onNodeWithTag("parity:offline-sync-button").performTouchInput { click() }
        compose.runOnIdle { assertEquals(1, syncs); mounted.value = false }
        frame()
        assertNull(E2eProjectionRegistry.read("parity:offline-packages-panel"))
        compose.runOnIdle { mounted.value = true }
        frame()
        assertTrue(catalog().state.contains("catalog-fetched-at:200"))
        compose.onNodeWithTag("parity:offline-sync-button").performTouchInput { click() }
        compose.runOnIdle { assertEquals(2, syncs) }
    }
}
