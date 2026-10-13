// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.test.click
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.unit.dp
import androidx.test.core.app.ApplicationProvider
import org.aerobag.app.domain.UiThemeLoader
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
class OutlinedButtonLabelTest {
    @get:Rule val compose = createComposeRule()

    @Test fun singleLineHasOneAccessibleLabelAndReceivesPhysicalInput() = label("HOME", 1)
    @Test fun multilineHasOneAccessibleLabelAndReceivesPhysicalInput() = label("DATA\nSTATUS", 2)

    private fun label(text: String, maxLines: Int) {
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        var clicks = 0
        compose.setContent {
            CompositionLocalProvider(LocalAerobagUiTheme provides theme) {
                Box(Modifier.size(200.dp)) {
                    CompactSquareButton(label = text, maxLines = maxLines,
                        iconResId = android.R.drawable.ic_menu_compass,
                        modifier = Modifier.size(100.dp), testTag = "button", onClick = { clicks++ })
                }
            }
        }
        // Outline copies are paint, not sixteen extra accessible text controls.
        // Test the exported merged label, not the debug unmerged tree, which
        // intentionally includes descendants of clearAndSetSemantics nodes.
        assertEquals(listOf(AnnotatedString(text)),
            compose.onNodeWithTag("button").fetchSemanticsNode().config[SemanticsProperties.Text])
        compose.onNodeWithTag("button").performTouchInput { click() }
        compose.runOnIdle { assertEquals(1, clicks) }
    }
}
