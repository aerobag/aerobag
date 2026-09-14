// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.State
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsNotDisplayed
import androidx.compose.ui.test.assertIsNotSelected
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.click
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeDown
import androidx.compose.ui.test.swipeUp
import androidx.compose.ui.unit.dp
import androidx.test.core.app.ApplicationProvider
import org.aerobag.app.domain.UiDataStatusPageRow
import org.aerobag.app.domain.UiDataStatusPageState
import org.aerobag.app.domain.UiThemeLoader
import org.aerobag.app.generated.UiServiceNotice
import org.aerobag.app.generated.UiServiceNoticeAction
import org.aerobag.app.generated.UiServiceNoticeTone
import org.aerobag.app.generated.UiServiceNotificationsState
import org.aerobag.app.generated.UiStatusSeverity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w1000dp-h800dp")
class ServiceNotificationsSectionTest {
    @get:Rule val compose = createComposeRule()

    @Test
    fun physicalTapsDispatchOpaqueActionsAndOnlyCoreUpdatesExpandContent() {
        val item = notice()
        val state = mutableStateOf(section().copy(expanded = false))
        val actions = mutableListOf<String>()
        mountStatus(state, onAction = actions::add)

        compose.onNodeWithTag("parity:service:section").assertIsDisplayed()
        compose.onNodeWithTag("parity:service:toggle").assertIsNotSelected()
        compose.onNodeWithText("Show or hide notices").assertDoesNotExist()
        compose.onNodeWithTag("parity:service:notice:notice").assertDoesNotExist()
        tap("parity:service:toggle")
        // Even an unread notice must stay hidden until the supplied snapshot expands the section.
        compose.onNodeWithTag("parity:service:notice:notice").assertDoesNotExist()
        compose.runOnIdle {
            assertEquals(listOf("opaque-entry-v1", "opaque-toggle-v1"), actions)
            state.value = state.value.copy(expanded = true)
        }
        compose.onNodeWithTag("parity:service:toggle").assertIsSelected()
        tap("parity:service:notice:notice")
        compose.onNodeWithTag("parity:service:body:notice").assertDoesNotExist()
        compose.runOnIdle {
            assertEquals("opaque-read-exact-revision", actions.last())
            state.value = state.value.copy(items = listOf(item.copy(
                expanded = true, unread = false, stateLabel = "Read", tone = UiServiceNoticeTone.History,
            )))
        }
        compose.onNodeWithTag("parity:service:body:notice").assertIsDisplayed()
        if (BuildConfig.AEROBAG_E2E_ENABLED) {
            compose.runOnIdle {
                assertTrue(E2eProjectionRegistry.read("parity:service:body:notice")!!.state
                    .contains("text:Use%20caution."))
            }
        }
        compose.onNodeWithText("Use caution.").assertIsDisplayed()
        compose.onNodeWithText("Read").assertIsDisplayed()
        tap("parity:service:mark-all-read")
        compose.runOnIdle {
            assertEquals(listOf(
                "opaque-entry-v1", "opaque-toggle-v1", "opaque-read-exact-revision", "opaque-mark-all-v1",
            ), actions)
            state.value = state.value.copy(
                markAllRead = null,
                toggleAction = action("opaque-toggle-v2", "Hide notices"),
            )
        }
        compose.onNodeWithTag("parity:service:mark-all-read").assertDoesNotExist()
        tap("parity:service:toggle")
        compose.runOnIdle {
            assertEquals("opaque-toggle-v2", actions.last())
            state.value = state.value.copy(expanded = false)
        }
        compose.onNodeWithTag("parity:service:body:notice").assertDoesNotExist()
    }

    @Test
    fun titleAndSummaryAreBothPhysicalTargetsOnTheSameFoldHeader() {
        val state = mutableStateOf(section().copy(expanded = false))
        val actions = mutableListOf<String>()
        mountStatus(state, onAction = actions::add)
        compose.onNodeWithText("Service Notifications", useUnmergedTree = true).performTouchInput { click() }
        compose.runOnIdle {
            assertEquals(listOf("opaque-entry-v1", "opaque-toggle-v1"), actions)
            state.value = state.value.copy(expanded = true)
        }
        compose.onNodeWithText("One unread", useUnmergedTree = true).performTouchInput { click() }
        compose.runOnIdle {
            assertEquals(listOf("opaque-entry-v1", "opaque-toggle-v1", "opaque-toggle-v1"), actions)
        }
        compose.onNodeWithTag("parity:service:toggle").assertIsSelected()
    }

    @Test
    fun backReentryDispatchesCurrentEntryOnceAfterActualUnmount() {
        val currentPage = mutableStateOf(AppPage.DataStatus)
        val state = mutableStateOf(section())
        val actions = mutableListOf<String>()
        mountStatus(state, page = currentPage, onSelectPage = { currentPage.value = it }) {
            actions.add(it)
        }
        compose.runOnIdle {
            assertEquals(listOf("opaque-entry-v1"), actions)
            state.value = state.value.copy(
                summary = "Updated while mounted",
                expanded = false,
                enterAction = action("opaque-entry-v2", "Enter Status"),
            )
        }
        compose.runOnIdle { assertEquals(listOf("opaque-entry-v1"), actions) }
        tap("parity:button:HOME")
        compose.onNodeWithTag("parity:page:data_status").assertDoesNotExist()
        tap("test:back-to-status")
        compose.onNodeWithTag("parity:page:data_status").assertIsDisplayed()
        compose.runOnIdle {
            assertEquals(listOf("opaque-entry-v1", "opaque-entry-v2"), actions)
            // Simulate core's entry result. Android must neither calculate nor override this policy.
            state.value = state.value.copy(expanded = true)
        }
        compose.onNodeWithTag("parity:service:notice:notice").assertIsDisplayed()
        tap("parity:button:HOME")
        compose.onNodeWithTag("parity:page:data_status").assertDoesNotExist()
        tap("test:back-to-status")
        compose.runOnIdle {
            assertEquals(listOf("opaque-entry-v1", "opaque-entry-v2", "opaque-entry-v2"), actions)
        }
    }

    @Test
    fun readOnlyStateCanRemainExpandedUntilCoreChangesIt() {
        val state = mutableStateOf(section().copy(
            items = listOf(notice().copy(unread = false, archived = true, tone = UiServiceNoticeTone.History)),
            markAllRead = null,
        ))
        mountStatus(state)
        compose.onNodeWithTag("parity:service:notice:notice").assertIsDisplayed()
        compose.onNodeWithTag("parity:service:mark-all-read").assertDoesNotExist()
    }

    @Test
    fun sectionSpansTheGridBeforeVariableWidthStatusTiles() {
        val state = mutableStateOf(section().copy(expanded = false))
        mountStatus(state, width = 960, rows = listOf(row(0), row(1)))

        val content = compose.onNodeWithTag("parity:data-status:content").fetchSemanticsNode().boundsInRoot
        val section = compose.onNodeWithTag("parity:service:section").fetchSemanticsNode().boundsInRoot
        val first = compose.onNodeWithTag(rowTag(0)).fetchSemanticsNode().boundsInRoot
        val second = compose.onNodeWithTag(rowTag(1)).fetchSemanticsNode().boundsInRoot
        assertEquals(content.left, section.left, 0.5f)
        assertEquals(content.right, section.right, 0.5f)
        assertTrue(first.top >= section.bottom)
        assertEquals(first.top, second.top, 0.5f)
        assertTrue(first.right < second.left)
    }

    @Test
    fun markAllReadIsCompactAndRightAlignedOnWideScreens() = assertCompactMarkAllRead(960)

    @Test
    fun markAllReadIsCompactAndRightAlignedOnNarrowScreens() = assertCompactMarkAllRead(320)

    private fun assertCompactMarkAllRead(width: Int) {
        val actions = mutableListOf<String>()
        mountStatus(mutableStateOf(section()), width = width, onAction = actions::add)
        val section = compose.onNodeWithTag("parity:service:section").fetchSemanticsNode().boundsInRoot
        val button = compose.onNodeWithTag("parity:service:mark-all-read").fetchSemanticsNode().boundsInRoot
        val thumbPx = with(compose.density) { ThumbSize.toPx() }
        assertEquals(section.right, button.right, 0.5f)
        assertEquals(thumbPx * 3.5f, button.width, 0.5f)
        assertEquals(thumbPx, button.height, 0.5f)
        assertTrue(button.left > section.left)
        tap("parity:service:mark-all-read")
        compose.runOnIdle { assertEquals("opaque-mark-all-v1", actions.last()) }
    }

    @Test
    fun physicalScrollMovesNoticesAndTilesTogetherWithoutReenteringStatus() {
        val state = mutableStateOf(section().copy(items = listOf(notice().copy(
            expanded = true,
            body = List(25) { "Notice detail line $it" }.joinToString("\n"),
        ))))
        val actions = mutableListOf<String>()
        mountStatus(state, rows = List(24) { row(it) }, onAction = actions::add)
        compose.onNodeWithTag("parity:service:toggle").assertIsDisplayed()
        repeat(8) {
            compose.onNodeWithTag("parity:data-status:content").performTouchInput { swipeUp() }
        }
        compose.onNodeWithTag(rowTag(23)).assertIsDisplayed()
        compose.onNodeWithTag("parity:service:toggle").assertIsNotDisplayed()
        repeat(8) {
            compose.onNodeWithTag("parity:data-status:content").performTouchInput { swipeDown() }
        }
        compose.onNodeWithTag("parity:service:toggle").assertIsDisplayed()
        compose.runOnIdle { assertEquals(listOf("opaque-entry-v1"), actions) }
        tap("parity:service:toggle")
        compose.runOnIdle { assertEquals("opaque-toggle-v1", actions.last()) }
    }

    private fun mountStatus(
        state: State<UiServiceNotificationsState>,
        width: Int = 400,
        rows: List<UiDataStatusPageRow> = emptyList(),
        page: State<AppPage> = mutableStateOf(AppPage.DataStatus),
        onSelectPage: (AppPage) -> Unit = {},
        onAction: (String) -> Unit = {},
    ) {
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        compose.setContent {
            CompositionLocalProvider(
                LocalAerobagUiTheme provides theme,
                LocalNavigationPageOptions provides NavigationPagePolicy(emptyList(), 2, AppPage.Map),
            ) {
                Box(Modifier.size(width.dp, 740.dp)) {
                    if (page.value == AppPage.DataStatus) {
                        DataStatusPage(
                            page = AppPage.DataStatus,
                            state = UiDataStatusPageState("Status", "Current data", rows),
                            serviceNotifications = state.value,
                            navElement = null,
                            mostRecentChartOrPlatePage = AppPage.Map,
                            onOpenPlan = {},
                            onOpenRecentChartOrPlate = {},
                            onSelectPage = onSelectPage,
                            onTimeDisplayAction = {},
                            onServiceNoticeAction = onAction,
                        )
                    } else {
                        CompactSquareButton(
                            label = "Back to Status",
                            modifier = Modifier.size(160.dp, 48.dp),
                            testTag = "test:back-to-status",
                            onClick = { onSelectPage(AppPage.DataStatus) },
                        )
                    }
                }
            }
        }
    }

    private fun tap(tag: String) = compose.onNodeWithTag(tag).performTouchInput { click() }

    private fun action(id: String, label: String) = UiServiceNoticeAction(actionId = id, label = label)

    private fun section() = UiServiceNotificationsState(
        title = "Service Notifications",
        summary = "One unread",
        expanded = true,
        enterAction = action("opaque-entry-v1", "Enter Status"),
        toggleAction = action("opaque-toggle-v1", "Show or hide notices"),
        markAllRead = action("opaque-mark-all-v1", "Mark all read"),
        sourceStatus = emptyList(),
        items = listOf(notice()),
    )

    private fun notice() = UiServiceNotice(
        id = "notice", title = "Distribution quality", body = "Use caution.",
        timing = "Published today", stateLabel = "Unread", severity = UiStatusSeverity.Caution,
        tone = UiServiceNoticeTone.Caution, unread = true, archived = false, expanded = false,
        openAction = action("opaque-read-exact-revision", "Distribution quality"),
    )

    private fun row(index: Int) = UiDataStatusPageRow(
        id = "tile-$index", label = "Data tile $index", value = "Current",
        severity = org.aerobag.app.domain.UiStatusSeverity.Info,
        detail = "Status tile detail", facts = emptyList(),
    )

    private fun rowTag(index: Int) = "parity:data-status-row:tile-$index:severity:info"
}
