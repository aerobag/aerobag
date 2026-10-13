// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.junit4.createComposeRule
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.SupervisorJob
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
class RetainedRuntimeCompositionTest {
    private val effects = StandardTestDispatcher()
    @get:Rule val compose = createComposeRule(effectContext = effects)

    @Test fun adoptionSurvivesEffectRestartAndRemountWithoutRecreatingTheSession() {
        val owner = RetainedRuntime<Int>(CoroutineScope(SupervisorJob() + effects)) { }
        val mounted = mutableStateOf(true)
        val progressRevision = mutableStateOf(1)
        val release = CompletableDeferred<Unit>()
        var loads = 0
        var landed: Int? = null
        try {
            compose.setContent {
                if (mounted.value) {
                    LaunchedEffect(progressRevision.value) {
                        owner.resetFor("same-download")
                        landed = owner.await { loads++; release.await(); loads }.getOrThrow()
                    }
                }
            }
            effects.scheduler.runCurrent()
            compose.runOnIdle { progressRevision.value++ }
            effects.scheduler.runCurrent()
            compose.runOnIdle { mounted.value = false }
            compose.runOnIdle { mounted.value = true }
            effects.scheduler.runCurrent()
            compose.runOnIdle {
                assertEquals(1, loads)
                assertNull(landed)
                release.complete(Unit)
            }
            effects.scheduler.advanceUntilIdle()
            compose.runOnIdle { assertEquals(1, landed); mounted.value = false }
            compose.runOnIdle { mounted.value = true }
            effects.scheduler.advanceUntilIdle()
            compose.runOnIdle {
                assertEquals(1, loads)
                assertEquals(1, landed)
            }
        } finally {
            release.complete(Unit)
            owner.close()
            effects.scheduler.advanceUntilIdle()
        }
    }
}
