// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.click
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.unit.dp
import org.aerobag.app.domain.MapDisplayFrame
import org.aerobag.app.domain.MapViewportState
import org.aerobag.app.domain.latLonToWorld
import org.aerobag.app.generated.UiGlidePoint
import org.aerobag.app.generated.UiGlideRing
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
class GlideRingOverlayTest {
    @get:Rule val compose = createComposeRule()

    @Test fun glideGeometryDoesNotInterceptMapOrControlsAfterFrameChanges() {
        val center = latLonToWorld(47.0, -122.0)
        val frame = mutableStateOf(MapDisplayFrame(MapViewportState(center.x, center.y, 9.0), 300f, 500f))
        val ring = UiGlideRing(
            paths = listOf(listOf(UiGlidePoint(47.0, -122.0), UiGlidePoint(47.1, -122.1))),
            labelPosition = UiGlidePoint(47.1, -122.1), windLabel = "+12", windDirectionDegTrue = 90.0,
            speedLabel = "62 kt IAS", message = null, recheckAfterMs = 1000,
        )
        var mapTaps = 0
        var homeTaps = 0
        compose.setContent {
            MapSurfaceLayers(modifier = Modifier.size(300.dp, 500.dp), mapContent = {
                Box(Modifier.fillMaxSize().testTag("map-input").clickable { mapTaps++ })
                GlideRingGeometry(ring, frame)
            }, controls = {
                Box(Modifier.align(Alignment.BottomStart).size(48.dp).testTag("home")
                    .clickable { homeTaps++ })
            })
        }
        repeat(2) {
            compose.onNodeWithTag("glide-ring").performTouchInput { click() }
            compose.onNodeWithTag("home").performTouchInput { click() }
            compose.runOnIdle {
                frame.value = frame.value.copy(viewport = frame.value.viewport.copy(
                    centerWorldX = center.x + 0.01, rotationDeg = 135.0, zoom = 10.0))
            }
        }
        compose.runOnIdle { assertEquals(2, mapTaps); assertEquals(2, homeTaps) }
    }
}
