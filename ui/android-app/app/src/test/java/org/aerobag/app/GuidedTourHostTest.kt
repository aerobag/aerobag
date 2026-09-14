// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later
package org.aerobag.app

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.*
import androidx.compose.ui.platform.LocalDensity
import androidx.test.core.app.ApplicationProvider
import org.aerobag.app.domain.MapDisplayFrame
import org.aerobag.app.domain.MapViewportState
import org.aerobag.app.domain.MapSelectionHighlight
import org.aerobag.app.domain.UiThemeLoader
import org.aerobag.app.domain.latLonToWorld
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.click
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.performKeyInput
import androidx.compose.ui.test.pressKey
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.material3.Text
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import org.aerobag.app.generated.*
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk=[34])
class GuidedTourHostTest {
    @get:Rule val compose = createComposeRule()
    private fun step() = UiGuidedTour(backEnabled=true,body="Tour example.",chapter="Charts",closeLabel="Close tour",generation=2,
        placement=UiTourPlacement.Auto,presentation=UiTourPresentation.Callout,restartLabel="Start over",nextLabel="Next",page=UiTourPage.Map,position=2,stepId="chart",subject="",rowUid=null,optionUid=null,surface=UiTourSurface.None,
        targets=emptyList(),title="The chart",total=5,mapPoint=null,
        shortcuts=listOf(UiTourShortcut(key="enter",action=UiTourAction.Next),UiTourShortcut(key="n",action=UiTourAction.Next),UiTourShortcut(key="b",action=UiTourAction.Back)),
        viewport=UiTourViewport(centered=false,lat=47.0,lon=-122.0,trackUp=false,zoom=7.0))

    @Config(qualifiers="w400dp-h700dp-port")
    @Test fun portraitScrimBlocksPhysicalTouchesAndButtonsReceiveThem() = verifyInput(400,700)
    @Config(qualifiers="w700dp-h400dp-land")
    @Test fun landscapeScrimBlocksPhysicalTouchesAndButtonsReceiveThem() = verifyInput(700,400)

    @OptIn(ExperimentalTestApi::class)
    @Test fun keyboardShortcutsUseTourActionsAndRespectReadiness() {
        val actions=mutableListOf<UiTourAction>()
        var busy by mutableStateOf(false)
        var tour by mutableStateOf(step().copy(backEnabled=false))
        var opened by mutableStateOf(false)
        compose.setContent {
            GuidedTourHost(if(opened) tour else null,busy,null,{actions.add(it)},Modifier.size(700.dp,700.dp)) {
                Box(Modifier.fillMaxSize().testTag("open-tour").clickable { opened=true })
            }
        }
        compose.onNodeWithTag("open-tour").performTouchInput { click() }
        compose.onNodeWithTag("guided-tour-scrim").assertIsFocused()
        // Enter works immediately after touch entry, and with Close focused.
        compose.onRoot().performKeyInput { pressKey(Key.Tab) }
        compose.onRoot().performKeyInput { pressKey(Key.B); pressKey(Key.Enter); pressKey(Key.NumPadEnter) }
        compose.runOnIdle { assertEquals(listOf(UiTourAction.Next,UiTourAction.Next),actions); tour=tour.copy(backEnabled=true) }
        compose.onRoot().performKeyInput {
            pressKey(Key.N)
            keyDown(Key.ShiftLeft); pressKey(Key.N); keyUp(Key.ShiftLeft)
            pressKey(Key.B)
            keyDown(Key.CtrlLeft); pressKey(Key.Enter); keyUp(Key.CtrlLeft)
        }
        compose.runOnIdle {
            assertEquals(listOf(UiTourAction.Next,UiTourAction.Next,UiTourAction.Next,UiTourAction.Next,UiTourAction.Back),actions)
            actions.clear(); busy=true
        }
        compose.onRoot().performKeyInput { pressKey(Key.Enter); pressKey(Key.N); pressKey(Key.B) }
        compose.runOnIdle { assertTrue(actions.isEmpty()); busy=false; tour=tour.copy(targets=listOf("missing-scene-target")) }
        compose.onRoot().performKeyInput { pressKey(Key.Enter); pressKey(Key.B) }
        compose.runOnIdle { assertEquals(listOf(UiTourAction.Back),actions) }
    }
    private fun verifyInput(width:Int,height:Int) {
        var appClicks=0
        val actions=mutableListOf<UiTourAction>()
        compose.setContent {
            GuidedTourHost(step(),false,null,{actions.add(it)},Modifier.size(width.dp,height.dp)) {
                Box(Modifier.fillMaxSize().zIndex(Float.MAX_VALUE).testTag("app").clickable { appClicks++ })
                TourAwarePopup(Offset.Zero,IntOffset.Zero,{}) {
                    Box(Modifier.fillMaxSize().testTag("menu").clickable { appClicks++ })
                }
            }
        }
        // Physical hit testing catches both a transparent scrim that leaks input
        // and a high-z child escaping the app's structural plane.
        compose.onNodeWithTag("guided-tour-scrim").performTouchInput { click(bottomRight) }
        for(tag in listOf("parity:guided-tour-next","parity:guided-tour-back","parity:guided-tour-restart","parity:guided-tour-close")) {
            compose.onNodeWithTag(tag).performTouchInput { click() }
        }
        compose.runOnIdle { assertEquals(0,appClicks); assertEquals(listOf(UiTourAction.Next,UiTourAction.Back,UiTourAction.Restart,UiTourAction.Close),actions) }
    }

    @Config(qualifiers="w1000dp-h700dp-land-mdpi")
    @Test fun spotAnchorFollowsDisplayedMapAndTourBlocksTouchesAtTheMarker() {
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        val spot = MapSelectionHighlight.Spot(47.205, -121.99)
        val center = latLonToWorld(spot.lat, spot.lon)
        var viewport by mutableStateOf(MapViewportState(center.x,center.y,9.0))
        var visible by mutableStateOf(true)
        var registry: TourAnchors? = null
        var frame: MapDisplayFrame? = null
        var appClicks = 0
        compose.setContent {
            val density = LocalDensity.current
            frame = MapDisplayFrame(viewport, with(density) { 1000.dp.toPx() }, with(density) { 700.dp.toPx() })
            GuidedTourHost(if (visible) step().copy(targets=listOf("map-spot-marker"),placement=UiTourPlacement.TopRight) else null,
                false,null,{if(it == UiTourAction.Close) visible=false},Modifier.size(1000.dp,700.dp)) {
                LocalGuidedTourAnchors.current?.let { registry=it }
                Box(Modifier.fillMaxSize().clickable { appClicks++ })
                MapSelectionSpotMarker(spot,frame!!,theme)
            }
        }
        val before = compose.runOnIdle { registry!!.bounds.getValue("tour:map-spot-marker") }
        compose.runOnIdle { viewport=viewport.copy(centerWorldX=viewport.centerWorldX+0.06,zoom=10.0,rotationDeg=65.0) }
        val after = compose.runOnIdle { registry!!.bounds.getValue("tour:map-spot-marker") }
        val point = frame!!.latLonToScreen(spot.lat,spot.lon)
        assertTrue(before != after)
        assertEquals(point.x,after.center.x,1f)
        assertEquals(point.y,after.bottom-4f,1f)
        assertEquals(32f,after.width,1f)
        assertEquals(43f,after.height,1f)
        compose.onNodeWithTag("guided-tour-scrim").performTouchInput { click(after.center) }
        compose.runOnIdle { assertEquals(0,appClicks) }
        compose.onNodeWithTag("parity:guided-tour-next").performTouchInput { click() }
        compose.onNodeWithTag("parity:guided-tour-close").performTouchInput { click() }
        compose.runOnIdle { assertTrue(registry!!.bounds.isEmpty()) }
    }

    @Test fun chapterTitleIsCenteredAndRestartReceivesPhysicalTouches() {
        val actions=mutableListOf<UiTourAction>()
        compose.setContent {
            GuidedTourHost(step().copy(title="Flight Planning",body="",presentation=UiTourPresentation.TitleCard,
                placement=UiTourPlacement.Center),false,null,{actions.add(it)},Modifier.size(700.dp,400.dp)) {}
        }
        val title=compose.onNodeWithTag("guided-tour-title").fetchSemanticsNode().boundsInRoot
        val screen=compose.onNodeWithTag("guided-tour-scrim").fetchSemanticsNode().boundsInRoot
        val panel=compose.onNodeWithTag("parity:guided-tour-panel").fetchSemanticsNode().boundsInRoot
        val content=compose.onNodeWithTag("guided-tour-title-content").fetchSemanticsNode().boundsInRoot
        assertEquals(screen.center.x,panel.center.x,1f)
        assertEquals(screen.center.y,panel.center.y,1f)
        assertTrue(panel.width < screen.width)
        assertTrue(panel.height < screen.height)
        assertEquals(content.center.x,title.center.x,1f)
        assertEquals(content.center.y,title.center.y,1f)
        val restart=compose.onNodeWithTag("parity:guided-tour-restart").fetchSemanticsNode().boundsInRoot
        val close=compose.onNodeWithTag("parity:guided-tour-close").fetchSemanticsNode().boundsInRoot
        val next=compose.onNodeWithTag("parity:guided-tour-next").fetchSemanticsNode().boundsInRoot
        assertTrue(restart.bottom <= close.top)
        assertTrue(restart.center.x < next.left)
        compose.onNodeWithTag("parity:guided-tour-restart").performTouchInput { click() }
        compose.runOnIdle { assertEquals(listOf(UiTourAction.Restart),actions) }
    }

    @Config(qualifiers="w1000dp-h700dp-land")
    @Test fun requestedCornerKeepsTheWaypointsUncovered() {
        compose.setContent {
            GuidedTourHost(step().copy(placement=UiTourPlacement.TopRight),false,null,{},Modifier.size(1000.dp,700.dp)) {}
        }
        val panel=compose.onNodeWithTag("parity:guided-tour-panel").fetchSemanticsNode().boundsInRoot
        val screen=compose.onNodeWithTag("guided-tour-scrim").fetchSemanticsNode().boundsInRoot
        assertTrue("panel=$panel screen=$screen", panel.left > screen.center.x)
        assertTrue(panel.top < screen.center.y)
        assertTrue(panel.right <= screen.right)
    }

    @Test fun cardFitsANarrowScreenIncludingItsMargins() {
        compose.setContent { GuidedTourHost(step().copy(placement=UiTourPlacement.TopRight),false,null,{}) {} }
        val panel=compose.onNodeWithTag("parity:guided-tour-panel").fetchSemanticsNode().boundsInRoot
        val screen=compose.onNodeWithTag("guided-tour-scrim").fetchSemanticsNode().boundsInRoot
        assertTrue("panel=$panel screen=$screen", panel.left >= screen.left && panel.right <= screen.right)
    }

    @Test fun remountWithExistingStepRegistersNewControlGeometry() {
        var mounted by mutableStateOf(true)
        var seen: TourAnchors?=null
        compose.setContent {
            if(mounted) GuidedTourHost(step(),false,null,{}) {
                seen=LocalGuidedTourAnchors.current
                Box(Modifier.size(48.dp).e2eIndexedControl("target",enabled=true))
            }
        }
        compose.runOnIdle { assertNotNull(seen?.bounds?.get("target")); mounted=false }
        compose.runOnIdle { assertEquals(emptyMap<String,Any>(),seen?.bounds?.toMap()); mounted=true }
        compose.runOnIdle { assertNotNull(seen?.bounds?.get("target")) }
    }

    @Test fun popupUpdatesWhenAsyncContentArrives() {
        var loaded by mutableStateOf(false)
        compose.setContent {
            GuidedTourHost(step(),false,null,{}) {
                val label = if (loaded) "Airport information" else "Loading airport"
                TourAwarePopup(onDismiss={}) {
                    Text(label)
                }
            }
        }
        compose.onNodeWithText("Loading airport").assertExists()
        compose.runOnIdle { loaded=true }
        compose.onNodeWithText("Airport information").assertExists()
    }

    @Test fun popupStaysInsideTheAppAtWindowEdges() {
        compose.setContent {
            GuidedTourHost(step(),false,null,{},Modifier.size(400.dp,700.dp)) {
                TourAwarePopup(position=Offset(10000f,10000f),onDismiss={}) {
                    Box(Modifier.size(200.dp,100.dp).testTag("edge-menu"))
                }
            }
        }
        val menu=compose.onNodeWithTag("edge-menu").fetchSemanticsNode().boundsInRoot
        val app=compose.onNodeWithTag("guided-tour-scrim").fetchSemanticsNode().boundsInRoot
        assertTrue(menu.left >= app.left && menu.top >= app.top && menu.right <= app.right && menu.bottom <= app.bottom)
    }
}
