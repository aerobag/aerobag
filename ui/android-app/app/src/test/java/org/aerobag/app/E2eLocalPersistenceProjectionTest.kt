// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.view.View
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.test.core.app.ApplicationProvider
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
@OptIn(ExperimentalTestApi::class, ExperimentalCoroutinesApi::class)
class E2eLocalPersistenceProjectionTest {
    private val effects = StandardTestDispatcher()
    @get:Rule val compose = createComposeRule(effectContext = effects)
    private val id = "org.aerobag.app:id/e2e_local_persistence_projection"

    @Test fun revisionOnlyNotificationsRefreshReceiptsAndUnmountUnsubscribes() {
        val window = View(ApplicationProvider.getApplicationContext()).apply { layout(0, 0, 200, 400) }
        val mounted = mutableStateOf(true)
        val owner = Any()
        var notify: ((Long) -> Unit)? = null
        var status = "pending"
        var closed = false
        compose.setContent {
            CompositionLocalProvider(LocalView provides window) {
                if (mounted.value) E2eLocalPersistenceProjection(owner, { listener ->
                    notify = listener
                    listener(1L)
                    AutoCloseable { closed = true }
                }) { status }
            }
        }
        effects.scheduler.advanceUntilIdle()
        compose.waitForIdle()
        effects.scheduler.advanceUntilIdle()
        compose.runOnIdle {
            window.drawObservationFrame()
            assertEquals("pending", E2eProjectionRegistry.read(id)!!.state)
            status = "saved"
            notify!!(2L)
        }
        effects.scheduler.advanceUntilIdle()
        compose.waitForIdle()
        effects.scheduler.advanceUntilIdle()
        compose.runOnIdle {
            window.drawObservationFrame()
            assertEquals("saved", E2eProjectionRegistry.read(id)!!.state)
            mounted.value = false
        }
        effects.scheduler.advanceUntilIdle()
        compose.runOnIdle { assertTrue(closed); assertNull(E2eProjectionRegistry.read(id)) }
    }

    @Test fun pendingReplacementAndRemountNeverBorrowAnOldSavedReceipt() {
        assertTrue("this suite requires the real E2E publisher", BuildConfig.AEROBAG_E2E_ENABLED)
        val window = View(ApplicationProvider.getApplicationContext()).apply { layout(0, 0, 200, 400) }
        val revision = mutableStateOf(1L)
        val mounted = mutableStateOf(true)
        val reads = mutableListOf<CompletableDeferred<String>>()
        val owner = Any()
        compose.setContent {
            CompositionLocalProvider(LocalView provides window) {
                if (mounted.value) E2eLocalPersistenceProjection(owner, revision.value) {
                    CompletableDeferred<String>().also { reads.add(it) }.await()
                }
            }
        }
        effects.scheduler.advanceUntilIdle()
        compose.runOnIdle { window.drawObservationFrame(); assertNull(E2eProjectionRegistry.read(id)) }
        reads.last().complete("saved revision 1")
        effects.scheduler.advanceUntilIdle()
        compose.runOnIdle {
            window.drawObservationFrame()
            assertEquals("saved revision 1", E2eProjectionRegistry.read(id)!!.state)
            revision.value = 2L
        }
        effects.scheduler.advanceUntilIdle()
        compose.runOnIdle { window.drawObservationFrame(); assertNull(E2eProjectionRegistry.read(id)) }
        reads.last().complete("pending revision 2")
        effects.scheduler.advanceUntilIdle()
        compose.runOnIdle {
            window.drawObservationFrame()
            assertEquals("pending revision 2", E2eProjectionRegistry.read(id)!!.state)
            mounted.value = false
        }
        compose.runOnIdle { assertNull(E2eProjectionRegistry.read(id)); mounted.value = true }
        effects.scheduler.advanceUntilIdle()
        compose.runOnIdle { window.drawObservationFrame(); assertNull(E2eProjectionRegistry.read(id)) }
        reads.last().complete("saved revision 2")
        effects.scheduler.advanceUntilIdle()
        compose.runOnIdle {
            window.drawObservationFrame()
            assertEquals("saved revision 2", E2eProjectionRegistry.read(id)!!.state)
        }
    }
}
