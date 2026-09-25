// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.click
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.unit.dp
import androidx.test.core.app.ApplicationProvider
import org.aerobag.app.domain.UiDataStatusBox
import org.aerobag.app.domain.UiDataStatusState
import org.aerobag.app.domain.UiStatusAction
import org.aerobag.app.domain.UiStatusActionStyle
import org.aerobag.app.domain.UiStatusSeverity
import org.aerobag.app.domain.UiThemeLoader
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w600dp-h800dp")
class DataStatusBadgeInputTest {
    @get:Rule val compose = createComposeRule()

    @Test
    fun physicalInboxTapUsesProductionTagsAndUnmountRemovesPopupOnNavigation() {
        val actions = mutableListOf<String>()
        mountBadge(actions)
        repeat(2) {
            tap("parity:data-status-launcher")
            compose.onNodeWithTag("parity:data-status-box-service:unread").assertIsDisplayed()
            compose.onNodeWithTag("parity:data-status-action-service:unread-service:inbox")
                .assertTextEquals("READ NOTIFICATIONS")
            if (BuildConfig.AEROBAG_E2E_ENABLED) {
                compose.runOnIdle {
                    drawObservationWindows()
                    assertTrue(E2eProjectionRegistry.read("parity:data-status-action-service:unread-service:inbox")!!.state
                        .contains("text:READ%20NOTIFICATIONS"))
                }
            }
            tap("parity:data-status-action-service:unread-service:inbox")
            compose.onNodeWithTag("parity:data-status-panel").assertDoesNotExist()
            compose.onNodeWithTag("parity:data-status-box-service:unread").assertDoesNotExist()
            compose.onNodeWithTag("parity:data-status-action-service:unread-service:inbox").assertDoesNotExist()
            tap("test:back-to-chart")
            compose.onNodeWithTag("parity:data-status-panel").assertDoesNotExist()
        }
        compose.runOnIdle { assertEquals(listOf("service:inbox", "service:inbox"), actions) }
    }

    @Test
    fun physicalHushTapKeepsPopupOpenAndDisabledActionDoesNothing() {
        val actions = mutableListOf<String>()
        mountBadge(actions)
        tap("parity:data-status-launcher")
        tap("parity:data-status-action-data:stale-status:hush")
        compose.onNodeWithTag("parity:data-status-panel").assertIsDisplayed()
        compose.onNodeWithTag("parity:data-status-action-data:stale-status:disabled").assertIsNotEnabled()
        tap("parity:data-status-action-data:stale-status:disabled")
        compose.onNodeWithTag("parity:data-status-panel").assertIsDisplayed()
        compose.runOnIdle { assertEquals(listOf("status:hush"), actions) }
    }

    private fun mountBadge(actions: MutableList<String>) {
        val mounted = mutableStateOf(true)
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        compose.setContent {
            CompositionLocalProvider(LocalAerobagUiTheme provides theme) {
                Box(Modifier.size(400.dp, 600.dp)) {
                    if (mounted.value) {
                        val open = remember { mutableStateOf(false) }
                        DataStatusBadge(
                            modifier = Modifier.align(Alignment.TopEnd),
                            dataStatusState = UiDataStatusState(
                                boxes = listOf(
                                    box("service:unread", "Service", listOf(
                                        UiStatusAction("service:inbox", "Read notifications", true, UiStatusActionStyle.Normal),
                                    )),
                                    box("data:stale", "Data", listOf(
                                        UiStatusAction("status:hush", "Hush", true, UiStatusActionStyle.Hush),
                                        UiStatusAction("status:disabled", "Retry", false, UiStatusActionStyle.Normal),
                                    )),
                                ),
                                launcherCount = "1", launcherSeverity = UiStatusSeverity.Caution,
                            ),
                            open = open.value,
                            onToggle = { open.value = !open.value },
                            onDismiss = { open.value = false },
                            onAction = {
                                actions.add(it)
                                if (it == "service:inbox") mounted.value = false
                            },
                        )
                    } else {
                        CompactSquareButton(
                            label = "Back to chart",
                            modifier = Modifier.size(160.dp, 48.dp),
                            testTag = "test:back-to-chart",
                            onClick = { mounted.value = true },
                        )
                    }
                }
            }
        }
    }

    private fun box(id: String, label: String, actions: List<UiStatusAction>) = UiDataStatusBox(
        id = id, label = label, value = "1", severity = UiStatusSeverity.Caution,
        drivesCaution = true, detail = "Status detail", hushed = false, actions = actions,
    )

    private fun tap(tag: String) = compose.onNodeWithTag(tag).performTouchInput { click() }
}
