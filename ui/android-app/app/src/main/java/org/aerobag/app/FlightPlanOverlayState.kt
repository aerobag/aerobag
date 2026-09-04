// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import org.aerobag.app.domain.AirportInfoUiView
import org.aerobag.app.domain.WeatherDetailUiView

internal sealed interface FlightPlanOverlayState {
    data object None : FlightPlanOverlayState

    data class RowTray(
        val rowUid: String,
    ) : FlightPlanOverlayState

    data class Weather(
        val detail: WeatherDetailUiView,
    ) : FlightPlanOverlayState

    data class AirportInfo(
        val airportId: String,
        val detail: AirportInfoUiView? = null,
        val error: String? = null,
    ) : FlightPlanOverlayState
}

internal sealed interface FlightPlanOverlayAction {
    data class SelectRow(
        val rowUid: String,
    ) : FlightPlanOverlayAction

    data class ShowWeather(
        val detail: WeatherDetailUiView,
    ) : FlightPlanOverlayAction

    data class ShowAirportInfo(
        val airportId: String,
    ) : FlightPlanOverlayAction

    data class ResolveAirportInfo(
        val airportId: String,
        val detail: AirportInfoUiView,
    ) : FlightPlanOverlayAction

    data class FailAirportInfo(
        val airportId: String,
        val error: String,
    ) : FlightPlanOverlayAction

    data object DismissRowTray : FlightPlanOverlayAction

    data object Dismiss : FlightPlanOverlayAction
}

internal fun FlightPlanOverlayState.transition(action: FlightPlanOverlayAction): FlightPlanOverlayState =
    when (action) {
        is FlightPlanOverlayAction.SelectRow -> FlightPlanOverlayState.RowTray(action.rowUid)
        is FlightPlanOverlayAction.ShowWeather -> FlightPlanOverlayState.Weather(action.detail)
        is FlightPlanOverlayAction.ShowAirportInfo ->
            FlightPlanOverlayState.AirportInfo(action.airportId)
        is FlightPlanOverlayAction.ResolveAirportInfo ->
            if (this is FlightPlanOverlayState.AirportInfo && airportId == action.airportId) {
                copy(detail = action.detail, error = null)
            } else {
                this
            }
        is FlightPlanOverlayAction.FailAirportInfo ->
            if (this is FlightPlanOverlayState.AirportInfo && airportId == action.airportId) {
                copy(detail = null, error = action.error)
            } else {
                this
            }
        FlightPlanOverlayAction.DismissRowTray ->
            if (this is FlightPlanOverlayState.RowTray) FlightPlanOverlayState.None else this
        FlightPlanOverlayAction.Dismiss -> FlightPlanOverlayState.None
    }

internal data class FlightPlanOverlayPresentation(
    val selectedRowUid: String?,
    val weatherDetail: WeatherDetailUiView?,
    val airportInfo: FlightPlanOverlayState.AirportInfo?,
)

internal fun FlightPlanOverlayState.present(): FlightPlanOverlayPresentation =
    when (this) {
        FlightPlanOverlayState.None -> FlightPlanOverlayPresentation(null, null, null)
        is FlightPlanOverlayState.RowTray -> FlightPlanOverlayPresentation(rowUid, null, null)
        is FlightPlanOverlayState.Weather -> FlightPlanOverlayPresentation(null, detail, null)
        is FlightPlanOverlayState.AirportInfo -> FlightPlanOverlayPresentation(null, null, this)
    }

internal fun FlightPlanOverlayState.e2eProjectionState(): String {
    val rowTray = if (this is FlightPlanOverlayState.RowTray) "open" else "closed"
    val detailId =
        when (this) {
            is FlightPlanOverlayState.Weather -> "weather-detail-modal"
            is FlightPlanOverlayState.AirportInfo -> "airport-info-modal:$airportId"
            else -> "none"
        }
    return "row_tray:$rowTray:detail:${e2eProjectionToken(detailId)}"
}

private fun e2eProjectionToken(value: String): String =
    value.replace("%", "%25").replace(":", "%3A")

@Stable
internal class FlightPlanOverlayController {
    var state: FlightPlanOverlayState by mutableStateOf(FlightPlanOverlayState.None)
        private set

    fun dispatch(action: FlightPlanOverlayAction) {
        state = state.transition(action)
    }
}
