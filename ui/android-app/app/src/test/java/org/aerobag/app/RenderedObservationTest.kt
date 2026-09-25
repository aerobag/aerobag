// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.view.View
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.Text
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.test.click
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeUp
import androidx.compose.ui.unit.dp
import androidx.test.core.app.ApplicationProvider
import org.aerobag.app.domain.UiThemeLoader
import org.aerobag.app.generated.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w400dp-h800dp")
class RenderedObservationTest {
    @get:Rule val compose = createComposeRule()

    @Test fun replacementIsPublishedAtLayoutBoundaryAndUnmountCannotLeaveGhosts() {
        assertTrue("JVM tests must exercise the journey publisher", BuildConfig.AEROBAG_E2E_ENABLED)
        val name = mutableStateOf("first")
        val mounted = mutableStateOf(true)
        lateinit var view: View
        var clicks = 0
        compose.setContent {
            view = LocalView.current
            if (mounted.value) Box(Modifier.size(80.dp)
                .e2eIndexedLabel("parity:lifecycle:${name.value}", name.value)
                .clickable { clicks++ }) { Text(name.value) }
        }
        fun frame() = compose.runOnIdle { view.viewTreeObserver.dispatchOnPreDraw() }
        frame()
        val first = E2eProjectionRegistry.read("parity:lifecycle:first")!!
        assertNotNull(first.bounds)
        compose.onNodeWithTag("parity:lifecycle:first").performTouchInput { click() }
        compose.runOnIdle { assertEquals(1, clicks); name.value = "second" }
        frame()
        assertNull(E2eProjectionRegistry.read("parity:lifecycle:first"))
        assertTrue(E2eProjectionRegistry.read("parity:lifecycle:second")!!.revision > first.revision)
        compose.runOnIdle { mounted.value = false }
        frame()
        assertTrue(E2eProjectionRegistry.readPrefix("parity:lifecycle:").isEmpty())
        compose.runOnIdle { mounted.value = true }
        frame()
        assertTrue(E2eProjectionRegistry.read("parity:lifecycle:second")!!.revision > first.revision)
        compose.onNodeWithTag("parity:lifecycle:second").performTouchInput { click() }
        compose.runOnIdle { assertEquals(2, clicks) }
    }

    @Test fun actualScrollWidgetPublishesPositionBoundaryAndClipping() {
        assertTrue("JVM tests must exercise the journey publisher", BuildConfig.AEROBAG_E2E_ENABLED)
        lateinit var view: View
        compose.setContent {
            view = LocalView.current
            Column(Modifier.size(200.dp).e2eIndexedElement("parity:scroller")
                .observedVerticalScroll(rememberScrollState())) {
                repeat(8) { index -> Text("Row $index", Modifier.height(100.dp)
                    .e2eIndexedLabel("parity:scroll-row:$index", "Row $index")) }
            }
        }
        compose.runOnIdle { view.viewTreeObserver.dispatchOnPreDraw() }
        val before = E2eProjectionRegistry.readPrefix("parity:scroll:").single().second
        assertTrue(before.state.contains("position:0:backward:false:forward:true:moving:false"))
        val clipped = E2eProjectionRegistry.read("parity:scroll-row:7")!!.bounds
        assertEquals("[0,0][0,0]", clipped)
        compose.onNodeWithTag("parity:scroller").performTouchInput { swipeUp() }
        compose.runOnIdle { view.viewTreeObserver.dispatchOnPreDraw() }
        val after = E2eProjectionRegistry.readPrefix("parity:scroll:").single().second
        assertNotEquals(before.state, after.state)
        assertTrue(after.state.contains("backward:true"))
        assertTrue(after.state.contains("moving:false"))
    }

    @Test fun cloudPanelPublishesTheStateThatItActuallyRenders() {
        val panel = mutableStateOf(UiCloudPanel(
            id = "receive_setup", title = "Set up from another device",
            state = UiCloudPanelState.Active, actions = emptyList(), timeFacts = emptyList(),
        ))
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        lateinit var view: View
        compose.setContent {
            view = LocalView.current
            CompositionLocalProvider(LocalAerobagUiTheme provides theme) {
                CloudPanelView(panel.value, mutableMapOf(), "", {}, Modifier)
            }
        }
        compose.runOnIdle { view.viewTreeObserver.dispatchOnPreDraw() }
        assertTrue(E2eProjectionRegistry.read("parity:cloud-panel:receive_setup")!!.state.contains(":state:active:"))
        compose.runOnIdle { panel.value = panel.value.copy(state = UiCloudPanelState.Complete) }
        compose.runOnIdle { view.viewTreeObserver.dispatchOnPreDraw() }
        assertTrue(E2eProjectionRegistry.read("parity:cloud-panel:receive_setup")!!.state.contains(":state:complete:"))
    }

    @Test fun successfulEmptySnapshotsAndInvalidRequestsAreDifferentOutcomes() {
        E2eProjectionRegistry.query("parity:never-mounted", null).use { cursor ->
            assertEquals(0, cursor.count)
            assertEquals(1, cursor.extras.getInt("schema"))
            assertEquals(E2eProjectionRegistry.incarnation, cursor.extras.getString("incarnation"))
        }
        assertThrows(IllegalArgumentException::class.java) { E2eProjectionRegistry.query(null, null) }
        assertThrows(IllegalArgumentException::class.java) { E2eProjectionRegistry.query("typo", null) }
    }

    @Test fun mapSelectionOwnerRemainsReadableAcrossOverlayMountAndPhysicalDismissal() {
        val open = mutableStateOf(true)
        lateinit var view: View
        val id = "org.aerobag.app:id/e2e_map_selection_projection"
        compose.setContent {
            view = LocalView.current
            Box(Modifier.fillMaxSize()) {
                E2eProjectionView(R.id.e2e_map_selection_projection, "open:${open.value}")
                if (open.value) MapInspectionOverlay(false, { open.value = false }) {
                    Box(Modifier.size(80.dp).e2eIndexedLabel("parity:overlay-close", "Close")
                        .clickable { open.value = false }) { Text("Close") }
                }
            }
        }
        fun frame() = compose.runOnIdle { view.viewTreeObserver.dispatchOnPreDraw() }
        repeat(2) {
            frame()
            assertEquals("open:true", E2eProjectionRegistry.read(id)!!.state)
            compose.onNodeWithTag("parity:overlay-close").performTouchInput { click() }
            frame()
            assertEquals("open:false", E2eProjectionRegistry.read(id)!!.state)
            assertNull(E2eProjectionRegistry.read("parity:overlay-close"))
            compose.runOnIdle { open.value = true }
        }
    }

    @Test fun readerSeesWholeFrameEvenWhenTheNextFrameReplacesEveryRow() {
        val a = Any(); val b = Any()
        fun frame(value: String) = mapOf(
            a to ("parity:batch:a" to E2eProjectionSnapshot(value, "[0,0][10,10]", 0)),
            b to ("parity:batch:b" to E2eProjectionSnapshot(value, "[0,10][10,20]", 0)))
        val owners = mapOf(a to "parity:batch:a", b to "parity:batch:b")
        try {
            E2eProjectionRegistry.replaceFrame(emptyMap(), frame("old"))
            E2eProjectionRegistry.query(null, "parity:batch:").use { old ->
                E2eProjectionRegistry.replaceFrame(owners, frame("new"))
                while (old.moveToNext()) assertEquals("old", old.getString(old.getColumnIndexOrThrow("state")))
            }
            assertEquals(setOf("new"), E2eProjectionRegistry.readPrefix("parity:batch:").map { it.second.state }.toSet())
        } finally { E2eProjectionRegistry.replaceFrame(owners, emptyMap()) }
    }
}
