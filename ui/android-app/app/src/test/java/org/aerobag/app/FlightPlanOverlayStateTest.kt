// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import java.io.File
import org.aerobag.app.domain.WeatherDetailUiView
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class FlightPlanOverlayStateTest {
    private val weather =
        WeatherDetailUiView(
            stationId = "KBOI",
            metarText = "KBOI METAR",
            metarAgeLabel = null,
            tafText = null,
            tafAgeLabel = null,
        )

    @Test
    fun weatherReplacesRowTrayAtomically() {
        var state: FlightPlanOverlayState = FlightPlanOverlayState.None
        state = state.transition(FlightPlanOverlayAction.SelectRow("row-KBOI"))

        state = state.transition(FlightPlanOverlayAction.ShowWeather(weather))

        assertNull(state.present().selectedRowUid)
        assertEquals(weather, state.present().weatherDetail)
    }

    @Test
    fun selectingAnotherRowCannotRetainStaleWeather() {
        var state: FlightPlanOverlayState = FlightPlanOverlayState.None
        state = state.transition(FlightPlanOverlayAction.ShowWeather(weather))

        state = state.transition(FlightPlanOverlayAction.SelectRow("row-KMAN"))

        assertEquals("row-KMAN", state.present().selectedRowUid)
        assertNull(state.present().weatherDetail)
    }

    @Test
    fun dismissClearsEitherOverlayKind() {
        val state =
            FlightPlanOverlayState.RowTray("row-KBOI")
                .transition(FlightPlanOverlayAction.Dismiss)

        assertEquals(FlightPlanOverlayState.None, state)
        assertNull(state.present().selectedRowUid)
        assertNull(state.present().weatherDetail)
    }

    @Test
    fun controllerPublishesEachOverlayTransition() {
        val controller = FlightPlanOverlayController()

        controller.dispatch(FlightPlanOverlayAction.SelectRow("row-KBOI"))
        assertEquals("row-KBOI", controller.state.present().selectedRowUid)

        controller.dispatch(FlightPlanOverlayAction.ShowWeather(weather))
        assertEquals(weather, controller.state.present().weatherDetail)

        controller.dispatch(FlightPlanOverlayAction.Dismiss)
        assertEquals(FlightPlanOverlayState.None, controller.state)
    }

    @Test
    fun weatherModalIsOwnedByTheAppOverlayHost() {
        val appSource = sourceFile("src/main/java/org/aerobag/app/MainActivity.kt").readText()
        val flightPlanSource = sourceFile("src/main/java/org/aerobag/app/FlightPlanPage.kt").readText()

        assertTrue(
            "AerobagApp must render flight-plan weather on the app-wide modal plane.",
            appSource.contains("FlightPlanWeatherOverlayHost(flightPlanOverlayController)"),
        )
        assertTrue(
            "The overlay host must directly observe controller state so weather invalidates its own composition.",
            appSource.contains("controller.state.present().weatherDetail?.let"),
        )
        assertTrue(
            "The app-wide flight-plan weather host must render the common weather modal.",
            appSource.contains("WeatherDetailModal("),
        )
        assertFalse(
            "FlightPlanPage must request weather rather than owning a modal that can miss invalidation.",
            flightPlanSource.contains("WeatherDetailModal("),
        )
    }

    private fun sourceFile(path: String): File {
        val start = File(".").canonicalFile
        return generateSequence(start) { it.parentFile }
            .map { File(it, path) }
            .firstOrNull { it.isFile }
            ?: error("could not locate source file $path from $start")
    }
}
