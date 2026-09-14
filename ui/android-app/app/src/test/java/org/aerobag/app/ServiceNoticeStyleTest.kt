// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.ui.graphics.Color
import androidx.test.core.app.ApplicationProvider
import org.aerobag.app.domain.UiThemeLoader
import org.aerobag.app.generated.UiServiceNoticeTone
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
class ServiceNoticeStyleTest {
    @Test
    fun historyUsesSharedQuietGrayAndNeutralTextWithoutAHighlight() {
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext()).controls
        val style = serviceNoticeStyle(UiServiceNoticeTone.History, theme)
        assertEquals(Color(0xFFD9DEE1), style.background)
        assertEquals(theme.dataStatusQuietBg, style.background)
        assertEquals(theme.panelFg, style.foreground)
        assertNull(style.highlight)
        // The presentation helper takes only tone, never logical severity/unread/archive flags.
        val alternate = theme.copy(dataStatusQuietBg = Color.LightGray, panelFg = Color.DarkGray)
        assertEquals(
            ServiceNoticeStyle(Color.LightGray, Color.DarkGray, null),
            serviceNoticeStyle(UiServiceNoticeTone.History, alternate),
        )
    }

    @Test
    fun activeTonesUseTheirEstablishedSharedStatusPalette() {
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext()).controls
        for ((tone, background, highlight) in listOf(
            Triple(UiServiceNoticeTone.Info, theme.dataStatusInfoBg, theme.dataStatusInfoStroke),
            Triple(UiServiceNoticeTone.Caution, theme.dataStatusCautionBg, theme.dataStatusCautionStroke),
            Triple(UiServiceNoticeTone.Warning, theme.dataStatusWarningBg, theme.dataStatusWarningStroke),
        )) {
            assertEquals(ServiceNoticeStyle(background, theme.panelFg, highlight), serviceNoticeStyle(tone, theme))
        }
    }
}
