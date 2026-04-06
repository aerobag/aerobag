package net.jonh.aerobag.prototype.domain

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ContentLogicTest {
    @Test
    fun streamAllowedTreatsRemoteOnlyContentAsSatisfied() {
        var state = ContentLogic.replaceFlightPlan(AppState(), SampleData.catalog, SampleData.samplePlan)
        state = ContentLogic.setContentPolicy(state, ContentPolicy.StreamAllowed)
        state = ContentLogic.refreshContent(state, SampleData.remoteOnlyInventory)

        assertTrue(state.lastContentReport!!.fullySatisfied)
        assertEquals(
            ContentAvailability.RemoteOnly,
            state.lastContentReport!!.items.first().availability.availability,
        )
    }

    @Test
    fun offlineRequiredNeedsInstalledContent() {
        var state = ContentLogic.replaceFlightPlan(AppState(), SampleData.catalog, SampleData.samplePlan)
        state = ContentLogic.setContentPolicy(state, ContentPolicy.OfflineRequired)
        state = ContentLogic.refreshContent(state, SampleData.remoteOnlyInventory)

        assertEquals(
            ContentAvailability.Unavailable,
            state.lastContentReport!!.items.first().availability.availability,
        )
    }

    @Test
    fun installedContentIsOfflineUsable() {
        var state = ContentLogic.replaceFlightPlan(AppState(), SampleData.catalog, SampleData.samplePlan)
        state = ContentLogic.setContentPolicy(state, ContentPolicy.OfflineRequired)
        state = ContentLogic.refreshContent(state, SampleData.installedInventory)

        assertTrue(state.lastContentReport!!.fullySatisfied)
        assertTrue(state.lastContentReport!!.items.first().availability.offlineUsable)
    }

    @Test(expected = IllegalArgumentException::class)
    fun emptyPlansAreRejected() {
        ContentLogic.replaceFlightPlan(
            AppState(),
            SampleData.catalog,
            SampleData.samplePlan.copy(legs = emptyList()),
        )
    }
}
