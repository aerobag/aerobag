// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later
package org.aerobag.app

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.SideEffect
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.unit.dp
import androidx.test.core.app.ApplicationProvider
import org.aerobag.app.domain.MapDisplayFrame
import org.aerobag.app.domain.MapViewportState
import org.aerobag.app.domain.UiThemeLoader
import org.aerobag.app.domain.latLonToWorld
import org.aerobag.app.generated.GeographicAnnotationPoint
import org.aerobag.app.generated.GeographicLineAnnotation
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class GeographicLineOverlayTest {
    @get:Rule val compose = createComposeRule()

    @Test fun annotationRepaintsWithTheLiveMapFrameWithoutANewCoreModel() {
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        val center = latLonToWorld(0.0, 0.0)
        val viewport = MapViewportState(centerWorldX = center.x, centerWorldY = center.y, zoom = 6.0, rotationDeg = 0.0)
        val frame = mutableStateOf(MapDisplayFrame(viewport, 320f, 320f))
        lateinit var view: android.view.View
        val annotation = GeographicLineAnnotation(
            points = listOf(GeographicAnnotationPoint(lat = 0.0, lon = -0.5), GeographicAnnotationPoint(lat = 0.0, lon = 0.5)),
            labelPosition = GeographicAnnotationPoint(lat = 0.0, lon = 0.0), label = "1500 GPS", labelBearingDeg = 90.0,
        )
        compose.setContent {
            val side = with(LocalDensity.current) { 320.dp.toPx() }
            view = LocalView.current
            SideEffect { frame.value = frame.value.copy(widthPx = side, heightPx = side) }
            Box(Modifier.size(320.dp).background(Color.White)) {
                GeographicLineOverlay(annotation, frame, theme)
            }
        }
        fun paintedAtCenter(): Int {
            val point = frame.value.latLonToScreen(0.0, 0.0)
            val bounds = compose.onNodeWithTag("altitude-intercept-arc").fetchSemanticsNode().boundsInRoot
            // Draw the real Compose view to a software bitmap. PixelCopy waits
            // for a hardware window refresh, which Robolectric does not drive.
            val pixels = compose.runOnIdle {
                android.graphics.Bitmap.createBitmap(view.width, view.height, android.graphics.Bitmap.Config.ARGB_8888).also {
                    view.draw(android.graphics.Canvas(it))
                }
            }
            var count = 0
            for (y in (point.y + bounds.top).toInt() - 3..(point.y + bounds.top).toInt() + 3) {
                for (x in (point.x + bounds.left).toInt() - 3..(point.x + bounds.left).toInt() + 3) {
                    if (pixels.getPixel(x, y) == theme.aviation.altitudeIntercept.toArgb()) count++
                }
            }
            pixels.recycle()
            return count
        }
        assertTrue("initial arc must paint", paintedAtCenter() > 0)
        compose.runOnIdle {
            frame.value = frame.value.copy(viewport = viewport.copy(centerWorldX = center.x - 0.5, zoom = 6.5, rotationDeg = 45.0))
        }
        assertTrue("arc must follow pan/zoom/rotation without replacement geometry", paintedAtCenter() > 0)
    }

    @Test fun eastboundLabelFacesEastInNorthUpAndScreenUpInTrackUp() {
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        val center = latLonToWorld(0.0, 0.0)
        val frame = mutableStateOf(MapDisplayFrame(MapViewportState(
            centerWorldX = center.x, centerWorldY = center.y, zoom = 6.0, rotationDeg = 0.0,
        ), 320f, 320f))
        lateinit var view: android.view.View
        val annotation = GeographicLineAnnotation(points = emptyList(),
            labelPosition = GeographicAnnotationPoint(lat = 0.0, lon = 0.0), label = "2500 GPS", labelBearingDeg = 90.0)
        compose.setContent {
            val side = with(LocalDensity.current) { 320.dp.toPx() }
            view = LocalView.current
            SideEffect { frame.value = frame.value.copy(widthPx = side, heightPx = side) }
            Box(Modifier.size(320.dp).background(Color.White)) { GeographicLineOverlay(annotation, frame, theme) }
        }
        fun labelBounds(): android.graphics.Rect {
            val bounds = compose.onNodeWithTag("altitude-intercept-arc").fetchSemanticsNode().boundsInRoot
            return compose.runOnIdle {
                val pixels = android.graphics.Bitmap.createBitmap(view.width, view.height, android.graphics.Bitmap.Config.ARGB_8888)
                view.draw(android.graphics.Canvas(pixels))
                val painted = android.graphics.Rect()
                for (y in bounds.top.toInt() until bounds.bottom.toInt()) {
                    for (x in bounds.left.toInt() until bounds.right.toInt()) {
                        if (pixels.getPixel(x, y) == theme.aviation.altitudeIntercept.toArgb()) painted.union(x, y, x + 1, y + 1)
                    }
                }
                pixels.recycle()
                painted
            }
        }
        val screen = compose.onNodeWithTag("altitude-intercept-arc").fetchSemanticsNode().boundsInRoot
        val northUp = labelBounds()
        assertTrue("eastbound text must be vertical in north-up", northUp.height() > northUp.width() * 2)
        assertTrue("text top must face east", northUp.left > screen.center.x)
        compose.runOnIdle { frame.value = frame.value.copy(viewport = frame.value.viewport.copy(rotationDeg = 90.0)) }
        val trackUp = labelBounds()
        assertTrue("eastbound text must be horizontal in track-up", trackUp.width() > trackUp.height() * 2)
        assertTrue("text must sit ahead of the arc", trackUp.bottom < screen.center.y)
    }
}
