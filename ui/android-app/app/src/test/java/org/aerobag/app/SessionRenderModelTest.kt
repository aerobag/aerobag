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

    @Test
    fun newerOtherOwnerDoesNotMakeLaggingOwnerPublicationStale() {
        val initial = TestSnapshot(revision = 0, shell = "shell-0", highRate = "high-0")
        val shellOwner = testOwner(initial) { it.shell }
        val highRateOwner = testOwner(initial) { it.highRate }

        assertEquals(
            RenderOwnerPublicationDisposition.Applied,
            highRateOwner.publish(TestSnapshot(revision = 2, shell = "shell-1", highRate = "high-2")),
        )
        assertEquals(
            RenderOwnerPublicationDisposition.Applied,
            shellOwner.publish(TestSnapshot(revision = 1, shell = "shell-1", highRate = "high-1")),
        )

        assertEquals(1L, shellOwner.publishedRevision)
        assertEquals("shell-1", shellOwner.state.value)
        assertEquals(2L, highRateOwner.publishedRevision)
        assertEquals("high-2", highRateOwner.state.value)
    }

    @Test
    fun olderPublicationCannotRegressTheSameOwner() {
        val initial = TestSnapshot(revision = 0, shell = "shell-0", highRate = "high-0")
        val shellOwner = testOwner(initial) { it.shell }

        shellOwner.publish(TestSnapshot(revision = 2, shell = "shell-2", highRate = "high-2"))

        assertEquals(
            RenderOwnerPublicationDisposition.Stale,
            shellOwner.publish(TestSnapshot(revision = 1, shell = "shell-1", highRate = "high-1")),
        )
        assertEquals(2L, shellOwner.publishedRevision)
        assertEquals("shell-2", shellOwner.state.value)
    }

    @Test
    fun equalRevisionRecoveryUpdatesOnlyTheOwnerThatMissedIt() {
        val initial = TestSnapshot(revision = 0, shell = "shell-0", highRate = "high-0")
        val shellOwner = testOwner(initial) { it.shell }
        val highRateOwner = testOwner(initial) { it.highRate }
        val revisionOne = TestSnapshot(revision = 1, shell = "shell-1", highRate = "high-1")

        highRateOwner.publish(revisionOne)

        assertEquals(RenderOwnerPublicationDisposition.Applied, shellOwner.publish(revisionOne))
        assertEquals(RenderOwnerPublicationDisposition.Current, highRateOwner.publish(revisionOne))
        assertEquals("shell-1", shellOwner.state.value)
        assertEquals("high-1", highRateOwner.state.value)
    }

    private data class TestSnapshot(
        val revision: Long,
        val shell: String,
        val highRate: String,
    )

    private fun <Projection> testOwner(
        initial: TestSnapshot,
        project: (TestSnapshot) -> Projection,
    ) = VersionedRenderOwner(
        initialSource = initial,
        revisionOf = TestSnapshot::revision,
        project = project,
    )
}
