// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import java.io.File
import org.aerobag.app.domain.AirportInfoUiView
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
            advisoryText = "NOTAMs and weather may be incomplete; check official sources.",
            metarText = "KBOI METAR",
            metarAgeLabel = null,
            tafText = null,
            tafAgeLabel = null,
        )
    private val airportInfo =
        AirportInfoUiView(
            airportId = "KBOI",
            name = "Boise Air Terminal",
            elevationLabel = "2,871 ft",
            trafficPatternAltitudeLabel = "3,871 ft",
            trafficPatternAltitudeSource = "derived",
            timeLabel = "12:00 MDT",
            timeDisplayActionId = "toggle_time_display_mode",
            timeZoneLabel = "MDT",
            sunrise = null,
            sunset = null,
            communications = emptyList(),
            runwayDiagramComplex = false,
            runways = emptyList(),
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
    fun airportInfoReplacesRowTrayAtomicallyAndPublishesLoadingImmediately() {
        var state: FlightPlanOverlayState = FlightPlanOverlayState.RowTray("row-KBOI")

        state = state.transition(FlightPlanOverlayAction.ShowAirportInfo("KBOI"))

        assertNull(state.present().selectedRowUid)
        assertEquals("KBOI", state.present().airportInfo?.airportId)
        assertNull(state.present().airportInfo?.detail)
    }

    @Test
    fun airportInfoResolutionUpdatesOnlyTheMatchingOpenModal() {
        val loading =
            FlightPlanOverlayState.RowTray("row-KBOI")
                .transition(FlightPlanOverlayAction.ShowAirportInfo("KBOI"))

        val resolved =
            loading.transition(
                FlightPlanOverlayAction.ResolveAirportInfo("KBOI", airportInfo),
            )
        val stale =
            resolved
                .transition(FlightPlanOverlayAction.ShowAirportInfo("KMAN"))
                .transition(FlightPlanOverlayAction.ResolveAirportInfo("KBOI", airportInfo))

        assertEquals(airportInfo, resolved.present().airportInfo?.detail)
        assertEquals("KMAN", stale.present().airportInfo?.airportId)
        assertNull(stale.present().airportInfo?.detail)
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
    fun flightPlanModalsAreOwnedByTheSingleAppOverlayHost() {
        val appSource = sourceFile("src/main/java/org/aerobag/app/MainActivity.kt").readText()
        val flightPlanSource = sourceFile("src/main/java/org/aerobag/app/FlightPlanPage.kt").readText()

        assertTrue(
            "AerobagApp must render flight-plan modals on the app-wide modal plane.",
            appSource.contains("FlightPlanOverlayHost(") &&
                appSource.contains("flightPlanOverlayController,"),
        )
        assertTrue(
            "The overlay host must directly observe controller state so every modal invalidates its own composition.",
            appSource.contains("val presentation = controller.state.present()"),
        )
        assertTrue(
            "The app-wide flight-plan weather host must render the common weather modal.",
            appSource.contains("WeatherDetailModal("),
        )
        assertTrue(
            "The app-wide flight-plan host must render airport information.",
            appSource.contains("AirportInfoModal("),
        )
        assertFalse(
            "FlightPlanPage must request modals rather than rendering them below row selection.",
            flightPlanSource.contains("WeatherDetailModal(") ||
                flightPlanSource.contains("AirportInfoModal("),
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
