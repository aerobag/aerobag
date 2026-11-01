// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.test.click
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.unit.dp
import androidx.test.core.app.ApplicationProvider
import org.aerobag.app.domain.UiThemeLoader
import org.aerobag.app.domain.OwnshipControlModel
import org.aerobag.app.domain.OwnshipLauncherTextTone
import org.junit.Assert.assertEquals
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
class OutlinedButtonLabelTest {
    @get:Rule val compose = createComposeRule()

    @Test fun singleLineHasOneAccessibleLabelAndReceivesPhysicalInput() = label("HOME", 1)
    @Test fun multilineHasOneAccessibleLabelAndReceivesPhysicalInput() = label("DATA\nSTATUS", 2)

    @Test
    fun ownshipSourceIconsDoNotAddBlackOutlineToNormalOrUnavailableLabels() {
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        val controls = mutableStateOf(OwnshipControlModel(
            launcherLabel = "GPS", launcherTextTone = OwnshipLauncherTextTone.Normal,
        ))
        lateinit var view: android.view.View
        var clicks = 0
        compose.setContent {
            view = LocalView.current
            CompositionLocalProvider(LocalAerobagUiTheme provides theme) {
                Box(Modifier.size(240.dp).background(Color.White)) {
                    SituationStatusBadge(controls.value, open = false, onToggle = { clicks++ })
                }
            }
        }
        var expectedClicks = 0
        for (symbol in listOf(null, "source_device", "source_receiver", "source_internet")) {
            for (tone in OwnshipLauncherTextTone.entries) {
                compose.runOnIdle {
                    controls.value = controls.value.copy(
                        launcherSymbolId = symbol, launcherTextTone = tone,
                        launcherLabel = if (tone == OwnshipLauncherTextTone.Normal) "GPS" else "No GPS",
                    )
                }
                val button = compose.onNodeWithTag("parity:ownship-launcher")
                val fill = if (tone == OwnshipLauncherTextTone.Normal) theme.controls.situationStatusFg
                    else theme.controls.situationStatusUnavailableFg
                val pixels = paintedColors(view, "parity:ownship-launcher")
                assertTrue("label must paint its theme fill ($symbol, $tone)", pixels.getOrDefault(fill.toArgb(), 0) > 0)
                assertEquals("ownship label must not acquire a black stroke ($symbol, $tone)",
                    0, pixels.getOrDefault(Color.Black.toArgb(), 0))
                button.performTouchInput { click() }
                expectedClicks++
                compose.runOnIdle { assertEquals(expectedClicks, clicks) }
            }
        }
    }

    private fun label(text: String, maxLines: Int) {
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        lateinit var view: android.view.View
        var clicks = 0
        compose.setContent {
            view = LocalView.current
            CompositionLocalProvider(LocalAerobagUiTheme provides theme) {
                Box(Modifier.size(200.dp).background(Color.White)) {
                    CompactSquareButton(label = text, maxLines = maxLines,
                        actionSymbolId = "source_device",
                        modifier = Modifier.size(100.dp), testTag = "button", onClick = { clicks++ })
                }
            }
        }
        // Outline copies are paint, not sixteen extra accessible text controls.
        // Test the exported merged label, not the debug unmerged tree, which
        // intentionally includes descendants of clearAndSetSemantics nodes.
        assertEquals(listOf(AnnotatedString(text)),
            compose.onNodeWithTag("button").fetchSemanticsNode().config[SemanticsProperties.Text])
        val pixels = paintedColors(view, "button")
        assertTrue("ordinary icon labels must retain their black outline",
            pixels.getOrDefault(Color.Black.toArgb(), 0) > 0)
        assertTrue("ordinary icon labels must retain their foreground fill",
            pixels.getOrDefault(theme.controls.buttonFg.toArgb(), 0) > 0)
        compose.onNodeWithTag("button").performTouchInput { click() }
        compose.runOnIdle { assertEquals(1, clicks) }
    }

    private fun paintedColors(view: android.view.View, tag: String): Map<Int, Int> {
        val bounds = compose.onNodeWithTag(tag).fetchSemanticsNode().boundsInRoot
        return compose.runOnIdle {
            // PixelCopy waits for a hardware window refresh; draw the real
            // Compose view to a software bitmap in Robolectric instead.
            val bitmap = android.graphics.Bitmap.createBitmap(view.width, view.height,
                android.graphics.Bitmap.Config.ARGB_8888)
            try {
                view.draw(android.graphics.Canvas(bitmap))
                val counts = mutableMapOf<Int, Int>()
                for (y in bounds.top.toInt() until bounds.bottom.toInt()) {
                    for (x in bounds.left.toInt() until bounds.right.toInt()) {
                        val color = bitmap.getPixel(x, y)
                        counts[color] = counts.getOrDefault(color, 0) + 1
                    }
                }
                counts
            } finally { bitmap.recycle() }
        }
    }
}
