// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.ExperimentalTestApi
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
@OptIn(ExperimentalTestApi::class, ExperimentalCoroutinesApi::class)
class KeyedResourceStateTest {
    private val effects = StandardTestDispatcher()
    @get:Rule val compose = createComposeRule(effectContext = effects)

    @Test
    fun replacementNeverPublishesThePreviousResourcesValueEvenBeforeTheNewEffectRuns() {
        val selected = mutableStateOf("first")
        val mounted = mutableStateOf(true)
        val pending = mapOf("first" to CompletableDeferred<String>(), "second" to CompletableDeferred())
        val commits = mutableListOf<Pair<String, String?>>()
        compose.setContent {
            if (mounted.value) {
                val id = selected.value
                val resource by produceKeyedResourceState<String>(id) { value = pending.getValue(id).await() }
                val rendered = resource
                SideEffect { commits.add(id to rendered) }
            }
        }
        compose.runOnIdle { pending.getValue("first").complete("first pixels") }
        effects.scheduler.advanceUntilIdle()
        compose.runOnIdle { assertEquals("first" to "first pixels", commits.last()) }
        compose.runOnIdle { selected.value = "second" }
        compose.runOnIdle {
            assertEquals("second" to null, commits.last())
            assertTrue(commits.none { it.first == "second" && it.second == "first pixels" })
            pending.getValue("second").complete("second pixels")
        }
        effects.scheduler.advanceUntilIdle()
        compose.runOnIdle { assertEquals("second" to "second pixels", commits.last()); mounted.value = false }
        compose.runOnIdle { mounted.value = true }
        effects.scheduler.advanceUntilIdle()
        compose.runOnIdle { assertEquals("second" to "second pixels", commits.last()) }
    }
}
