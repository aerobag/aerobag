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
import androidx.compose.ui.geometry.Offset
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

    @Test fun nonvisualReadinessChangesSchedulePublicationWithoutElementRecomposition() {
        val drawingWindow = object : View(ApplicationProvider.getApplicationContext()) {
            var invalidations = 0
            override fun invalidate() { invalidations++; super.invalidate() }
            override fun post(action: Runnable): Boolean = android.os.Handler(android.os.Looper.getMainLooper()).post(action)
        }
        val moving = mutableStateOf(true)
        compose.setContent {
            CompositionLocalProvider(LocalView provides drawingWindow) {
                Box(Modifier.size(80.dp).e2eIndexedGeometry("parity:nonvisual-readiness") {
                    "moving:${moving.value}"
                })
            }
        }
        compose.runOnIdle {
            drawingWindow.viewTreeObserver.dispatchOnPreDraw()
            assertTrue(E2eProjectionRegistry.read("parity:nonvisual-readiness")!!.state.contains("moving:true"))
        }
        // Drain the publication's after-draw check before changing readiness.
        org.robolectric.Shadows.shadowOf(android.os.Looper.getMainLooper()).idle()
        var before = 0
        compose.runOnIdle {
            before = drawingWindow.invalidations
            moving.value = false
            androidx.compose.runtime.snapshots.Snapshot.sendApplyNotifications()
        }
        org.robolectric.Shadows.shadowOf(android.os.Looper.getMainLooper()).idle()
        compose.runOnIdle {
            assertTrue("state-only transition must schedule its own frame", drawingWindow.invalidations > before)
            drawingWindow.viewTreeObserver.dispatchOnPreDraw()
            assertTrue(E2eProjectionRegistry.read("parity:nonvisual-readiness")!!.state.contains("moving:false"))
        }
    }

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

    @Test fun traversalDragAndHoldDoesNotFlingPastUnobservedRows() {
        lateinit var view: View
        compose.setContent {
            view = LocalView.current
            ObservedLazyColumn(Modifier.size(200.dp, 400.dp).e2eIndexedElement("parity:traversal")) {
                items(120) { index ->
                    Text("Row $index", Modifier.height(50.dp).e2eIndexedLabel("parity:traversal-row:$index", "Row $index"))
                }
            }
        }
        var previousIndex = 0
        repeat(4) {
            compose.onNodeWithTag("parity:traversal").performTouchInput {
                down(Offset(center.x, height * 0.85f))
                repeat(10) { step -> moveTo(Offset(center.x, height * (0.85f - 0.07f * (step + 1))), delayMillis = 25) }
                advanceEventTime(150)
                up()
            }
            compose.runOnIdle { view.viewTreeObserver.dispatchOnPreDraw() }
            val state = E2eProjectionRegistry.readPrefix("parity:scroll:").single().second.state
            val index = state.substringAfter(":position:").substringBefore(',').toInt()
            assertTrue("each gesture advances", index > previousIndex)
            assertTrue("adjacent observed viewports must overlap: $previousIndex -> $index", index - previousIndex < 8)
            assertTrue(state.contains(":moving:false"))
            previousIndex = index
        }
    }

    @Test fun scrollReadinessIncludesEdgeEffectEvenWhenPositionHasStopped() {
        // Control the effect's lifetime independently of ScrollState and Compose
        // recomposition: Android's stretch animation has exactly this lifetime.
        val effect = object : androidx.compose.foundation.OverscrollEffect {
            override var isInProgress = false
            override val node = object : Modifier.Node() {}
            override fun applyToScroll(delta: Offset, source: androidx.compose.ui.input.nestedscroll.NestedScrollSource,
                performScroll: (Offset) -> Offset) = performScroll(delta)
            override suspend fun applyToFling(velocity: androidx.compose.ui.unit.Velocity,
                performFling: suspend (androidx.compose.ui.unit.Velocity) -> androidx.compose.ui.unit.Velocity) { performFling(velocity) }
        }
        val factory = object : androidx.compose.foundation.OverscrollFactory {
            override fun createOverscrollEffect() = effect
            override fun equals(other: Any?) = this === other
            override fun hashCode() = System.identityHashCode(this)
        }
        lateinit var view: View
        compose.setContent {
            view = LocalView.current
            CompositionLocalProvider(androidx.compose.foundation.LocalOverscrollFactory provides factory) {
                ObservedLazyColumn(Modifier.size(200.dp)) {
                    items(8) { Text("Row $it", Modifier.height(80.dp)) }
                }
            }
        }
        for (moving in listOf(false, true, false)) {
            compose.runOnIdle {
                effect.isInProgress = moving
                view.viewTreeObserver.dispatchOnPreDraw()
                val state = E2eProjectionRegistry.readPrefix("parity:scroll:").single().second.state
                assertTrue(state.contains("position:0,0"))
                assertTrue("must sample live edge effect: $state", state.contains(":moving:$moving"))
            }
        }
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

    @Test fun procedureChoicesWithTheSameEnrouteTransitionRemainIndividuallyClickable() {
        val choices = listOf("RW16R", "RW34L").map { runway ->
            org.aerobag.app.domain.ProcedureSpecChoice(runway, "ARRIE", "ARRIE from $runway")
        }
        val selected = mutableListOf<String?>()
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        lateinit var view: View
        compose.setContent {
            view = LocalView.current
            CompositionLocalProvider(LocalAerobagUiTheme provides theme) {
                Column {
                    choices.forEach { choice ->
                        ProcedureTransitionRow(choice) { selected.add(choice.runwayTransition) }
                    }
                }
            }
        }
        compose.runOnIdle { view.viewTreeObserver.dispatchOnPreDraw() }
        assertEquals(2, E2eProjectionRegistry.readPrefix("parity:plan-procedure-transition:ARRIE:").size)
        for (runway in listOf("RW16R", "RW34L")) {
            compose.onNodeWithTag("parity:plan-procedure-transition:ARRIE:$runway").performTouchInput { click() }
        }
        compose.runOnIdle { assertEquals(listOf("RW16R", "RW34L"), selected) }
    }

    @Test fun airportFactsPublishRenderedTextNotJustTheirIdentity() {
        val value = mutableStateOf("1000 MSL (published)")
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        lateinit var view: View
        compose.setContent {
            view = LocalView.current
            CompositionLocalProvider(LocalAerobagUiTheme provides theme) {
                AirportInfoFact("Traffic pattern altitude", value.value)
            }
        }
        for (text in listOf("1000 MSL (published)", "1032 MSL (derived)")) {
            compose.runOnIdle { value.value = text }
            compose.runOnIdle { view.viewTreeObserver.dispatchOnPreDraw() }
            val (id, fact) = E2eProjectionRegistry.readPrefix("parity:airport-info-fact:").single()
            assertEquals("parity:airport-info-fact:Traffic pattern altitude:$text", id)
            val displayedText = android.net.Uri.decode(fact.state.substringAfter("text:").substringBefore(":"))
            assertEquals("Traffic pattern altitude $text", displayedText)
        }
    }

    @Test fun detailsAndStartupErrorsPublishTheirRenderedBody() {
        val panel = mutableStateOf(0)
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        lateinit var view: View
        compose.setContent {
            view = LocalView.current
            CompositionLocalProvider(LocalAerobagUiTheme provides theme) {
                when (panel.value) {
                    0 -> NotamModal(org.aerobag.app.domain.NotamDetailUiView(
                        "Procedure NOTAMs", "Check briefing", "None", listOf(
                            org.aerobag.app.domain.AirportNotamUiView("one", "IAP", "Approach unavailable"))))
                    1 -> MapSelectionDetailModal("TFR", "Temporary flight restrictions")
                    2 -> OfflinePackagesErrorPanel("Unsupported publication", false, {})
                    else -> OfflinePackagesLibraryPanel(
                        message = "No manifest supported by this app",
                        storageCapacityLabel = null, packageSourceBaseUrl = "http://fixture/",
                        onPackageSourceBaseUrlChange = {}, refreshInFlight = false, sourceEditable = false,
                        refreshEnabled = true, refreshCancelEnabled = false, cancelRequested = false,
                        onRefresh = {}, onCancelRefresh = {}, closeEnabled = false, onClose = {},
                    )
                }
            }
        }
        val cases = listOf(
            "parity:procedure-notam-modal" to "Approach unavailable",
            "parity:map-selection-detail-modal:TFR" to "Temporary flight restrictions",
            "parity:offline-library-panel" to "Unsupported publication",
            "parity:offline-library-panel" to "No manifest supported by this app",
        )
        cases.forEachIndexed { index, (id, body) ->
            compose.runOnIdle { panel.value = index }
            compose.runOnIdle { view.viewTreeObserver.dispatchOnPreDraw() }
            val snapshot = requireNotNull(E2eProjectionRegistry.read(id))
            val text = android.net.Uri.decode(snapshot.state.substringAfter("text:").substringBefore(":"))
            assertTrue("published text must include rendered body", text.contains(body))
            if (index > 0 && cases[index - 1].first != id) assertNull(E2eProjectionRegistry.read(cases[index - 1].first))
        }
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
