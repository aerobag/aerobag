// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import org.aerobag.app.domain.WeatherDetailUiView

internal sealed interface FlightPlanOverlayState {
    data object None : FlightPlanOverlayState

    data class RowTray(
        val rowUid: String,
    ) : FlightPlanOverlayState

    data class Weather(
        val detail: WeatherDetailUiView,
    ) : FlightPlanOverlayState
}

internal sealed interface FlightPlanOverlayAction {
    data class SelectRow(
        val rowUid: String,
    ) : FlightPlanOverlayAction

    data class ShowWeather(
        val detail: WeatherDetailUiView,
    ) : FlightPlanOverlayAction

    data object Dismiss : FlightPlanOverlayAction
}

internal fun FlightPlanOverlayState.transition(action: FlightPlanOverlayAction): FlightPlanOverlayState =
    when (action) {
        is FlightPlanOverlayAction.SelectRow -> FlightPlanOverlayState.RowTray(action.rowUid)
        is FlightPlanOverlayAction.ShowWeather -> FlightPlanOverlayState.Weather(action.detail)
        FlightPlanOverlayAction.Dismiss -> FlightPlanOverlayState.None
    }

internal data class FlightPlanOverlayPresentation(
    val selectedRowUid: String?,
    val weatherDetail: WeatherDetailUiView?,
)

internal fun FlightPlanOverlayState.present(): FlightPlanOverlayPresentation =
    when (this) {
        FlightPlanOverlayState.None -> FlightPlanOverlayPresentation(null, null)
        is FlightPlanOverlayState.RowTray -> FlightPlanOverlayPresentation(rowUid, null)
        is FlightPlanOverlayState.Weather -> FlightPlanOverlayPresentation(null, detail)
    }

@Stable
internal class FlightPlanOverlayController {
    var state: FlightPlanOverlayState by mutableStateOf(FlightPlanOverlayState.None)
        private set

    fun dispatch(action: FlightPlanOverlayAction) {
        state = state.transition(action)
    }
}
