// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later
package org.aerobag.app

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.click
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.unit.dp
import androidx.test.core.app.ApplicationProvider
import org.aerobag.app.domain.UiThemeLoader
import org.aerobag.app.generated.*
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
class ServiceNotificationsPageTest {
    @get:Rule val compose = createComposeRule()

    @Test fun physicalTapDispatchesDisplayedActionAndCoreUpdateRevealsBody() {
        val item = UiServiceNotice(id="notice", title="Distribution quality", body="Use caution.",
            timing="Published today", stateLabel="Unread", severity=UiStatusSeverity.Caution,
            unread=true, archived=false, expanded=false,
            openAction=UiServiceNoticeAction(actionId="service:read:exact-revision",label="Distribution quality"))
        val page = mutableStateOf(UiServiceNotificationsState(title="Service Notifications",summary="One unread",
            sourceStatus=emptyList(),items=listOf(item)))
        val clicks = mutableListOf<String>()
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        compose.setContent {
            CompositionLocalProvider(LocalAerobagUiTheme provides theme,
                LocalNavigationPageOptions provides NavigationPagePolicy(emptyList(),2,AppPage.Map)) {
                Box(Modifier.size(360.dp, 720.dp)) {
                    ServiceNotificationsPage(page.value,null,AppPage.Map,{}, {}, {}, { clicks.add(it) })
                }
            }
        }
        compose.onNodeWithText("Use caution.").assertDoesNotExist()
        compose.onNodeWithText("DISTRIBUTION QUALITY").performTouchInput { click() }
        compose.runOnIdle {
            assertEquals(listOf("service:read:exact-revision"), clicks)
            page.value = page.value.copy(items=listOf(item.copy(expanded=true,unread=false,stateLabel="Read")))
        }
        compose.onNodeWithText("Use caution.").assertIsDisplayed()
        compose.onNodeWithText("Read").assertIsDisplayed()
    }
}
