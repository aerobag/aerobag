// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import org.aerobag.app.generated.UiSessionUpdateGroup
import org.junit.Assert.assertEquals
import org.junit.Test

class SessionRenderModelTest {
    @Test
    fun highRateUpdatesDoNotInvalidateTheApplicationShell() {
        assertEquals(
            setOf(SessionRenderScope.HighRate),
            sessionRenderScopes(
                setOf(UiSessionUpdateGroup.Ownship, UiSessionUpdateGroup.Situation),
                fullSnapshot = false,
            ),
        )
    }

    @Test
    fun mixedUpdatesInvalidateBothRenderOwners() {
        assertEquals(
            setOf(SessionRenderScope.Shell, SessionRenderScope.HighRate),
            sessionRenderScopes(
                setOf(UiSessionUpdateGroup.Ownship, UiSessionUpdateGroup.Status),
                fullSnapshot = false,
            ),
        )
    }

    @Test
    fun revisionOnlyUpdatesInvalidateNoRenderedModel() {
        assertEquals(emptySet<SessionRenderScope>(), sessionRenderScopes(emptySet(), fullSnapshot = false))
    }

    @Test
    fun explicitRecoveryInvalidatesBothRenderOwners() {
        assertEquals(
            SessionRenderScope.entries.toSet(),
            sessionRenderScopes(emptySet(), fullSnapshot = true),
        )
    }
}
