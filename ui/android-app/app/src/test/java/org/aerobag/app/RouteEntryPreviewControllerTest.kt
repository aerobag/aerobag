package org.aerobag.app

import org.aerobag.app.domain.FlightPlanEntryPreview
import org.aerobag.app.domain.FlightPlanEntryToken
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class RouteEntryPreviewControllerTest {
    @Test
    fun stalePreviewCompletionCannotPublishOrClearNewerLoadingState() {
        val controller = RouteEntryPreviewController()
        val firstPreview = previewFor("KPAE")
        val secondPreview = previewFor("KPAE KAPA")
        var state = initialState()

        val first = controller.begin("KPAE", state)
        state = first.state
        assertTrue(state.loading)

        val second = controller.begin("KPAE KAPA", state)
        state = second.state
        assertTrue(state.loading)

        state = controller.complete(first.id, firstPreview, state)
        assertTrue(
            "A stale preview completion must not clear the current request's loading state.",
            state.loading,
        )
        assertEquals(
            "A stale preview completion must not publish stale token coloring.",
            emptyFlightPlanEntryPreview(),
            state.preview,
        )
        assertNull(state.error)

        state = controller.finish(first.id, state)
        assertTrue(
            "A stale cancelled request finally block must not clear the current request's loading state.",
            state.loading,
        )

        state = controller.complete(second.id, secondPreview, state)
        assertFalse(state.loading)
        assertEquals(secondPreview, state.preview)
        assertNull(state.error)
    }

    private fun initialState(): RouteEntryPreviewUiState =
        RouteEntryPreviewUiState(
            preview = emptyFlightPlanEntryPreview(),
            loading = false,
            error = null,
        )

    private fun previewFor(input: String): FlightPlanEntryPreview =
        FlightPlanEntryPreview(
            canCommit = true,
            tokens = listOf(FlightPlanEntryToken(start = 0, end = input.length, state = "recognized")),
            issues = emptyList(),
        )
}
