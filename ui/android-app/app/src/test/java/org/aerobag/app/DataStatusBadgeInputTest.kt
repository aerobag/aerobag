// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.State
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.pointer.pointerInteropFilter
import androidx.compose.ui.platform.testTag
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

    @OptIn(ExperimentalComposeUiApi::class)
    @Test
    fun physicalBadgeTapAndStatusReplacementDoNotReachMapGestures() {
        val boxes = mutableStateOf(listOf(box("loading", "Loading", emptyList())))
        val open = mutableStateOf(false)
        var mapTaps = 0
        val mapActivity = mutableListOf<Boolean>()
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        compose.setContent {
            CompositionLocalProvider(LocalAerobagUiTheme provides theme) {
                MapSurfaceLayers(
                    modifier = Modifier.size(400.dp, 600.dp).testTag("test:map")
                        .mapGestureInput(
                            enabled = !open.value,
                            onTap = { mapTaps++ }, onGesture = {}, onTransform = { _, _ -> },
                            onActiveChange = { mapActivity += it }, onFinished = {},
                        )
                        .pointerInteropFilter { false },
                    mapContent = {},
                    controls = {
                        DataStatusBadge(
                            modifier = Modifier.align(Alignment.TopEnd),
                            dataStatusState = UiDataStatusState(boxes.value, boxes.value.size.toString(), UiStatusSeverity.Caution),
                            open = open.value,
                            onToggle = { open.value = !open.value }, onDismiss = { open.value = false },
                        )
                    },
                )
            }
        }
        tap("parity:data-status-launcher")
        compose.onNodeWithTag("parity:data-status-panel").assertIsDisplayed()
        compose.runOnIdle { open.value = false }
        compose.onNodeWithTag("parity:data-status-launcher").performTouchInput { down(center) }
        compose.runOnIdle {
            boxes.value = listOf(box("service:unread", "Service", emptyList()), box("data", "Data", emptyList()))
        }
        compose.onNodeWithTag("parity:data-status-launcher").performTouchInput { up() }
        compose.onNodeWithTag("parity:data-status-panel").assertIsDisplayed()
        compose.runOnIdle {
            assertEquals(0, mapTaps)
            assertTrue("badge input must not begin a map gesture: $mapActivity", mapActivity.isEmpty())
            open.value = false
        }
        compose.onNodeWithTag("test:map").performTouchInput { click(center) }
        compose.runOnIdle { assertEquals(1, mapTaps) }
    }

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

    @Test
    fun clearingWarningsDuringPhysicalScrollClampsTheListAndLeavesInboxClickable() {
        val service = box("service:unread", "Service", listOf(
            UiStatusAction("service:inbox", "Read notifications", true, UiStatusActionStyle.Normal),
        ))
        val boxes = mutableStateOf((0..5).map { box("loading:$it", "Loading $it", emptyList()) } + service)
        val actions = mutableListOf<String>()
        mountBadge(actions, boxes)
        tap("parity:data-status-launcher")
        fun scrollState(): String {
            drawObservationWindows()
            return E2eProjectionRegistry.readPrefix("parity:scroll:").single().second.state
        }
        compose.runOnIdle { assertTrue(scrollState().contains("position:0,0:backward:false:forward:true")) }
        compose.onNodeWithTag("parity:data-status-panel").performTouchInput {
            down(Offset(center.x, height * 0.8f))
            moveTo(Offset(center.x, height * 0.3f), delayMillis = 100)
        }
        compose.runOnIdle {
            val state = scrollState()
            assertTrue("physical gesture actually moved: $state", state.contains(":backward:true"))
            assertTrue(state.contains(":moving:true"))
            boxes.value = listOf(service)
        }
        compose.onNodeWithTag("parity:data-status-panel").performTouchInput { up() }
        compose.runOnIdle {
            val state = scrollState()
            assertTrue("offset legitimately returns to zero: $state",
                state.contains("position:0,0:backward:false:forward:false:moving:false"))
        }
        compose.onNodeWithTag("parity:data-status-action-service:unread-service:inbox").assertIsDisplayed()
        tap("parity:data-status-action-service:unread-service:inbox")
        compose.runOnIdle { assertEquals(listOf("service:inbox"), actions) }
        compose.onNodeWithTag("parity:data-status-panel").assertDoesNotExist()
    }

    private fun mountBadge(actions: MutableList<String>, boxes: State<List<UiDataStatusBox>>? = null) {
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
                                boxes = boxes?.value ?: listOf(
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
