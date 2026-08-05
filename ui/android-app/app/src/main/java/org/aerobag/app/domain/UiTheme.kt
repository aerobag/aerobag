// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.domain

import android.content.Context
import androidx.compose.ui.graphics.Color
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

data class UiTheme(
    val controls: ControlsTheme,
    val aviation: AviationTheme,
    val flightPlanRoute: FlightPlanRouteTheme,
    val plateFolder: PlateFolderTheme,
)

data class ControlsTheme(
    val buttonChecked: Color,
    val buttonUnchecked: Color,
    val headerButton: Color,
    val buttonDisabled: Color,
    val buttonDisabledIconSaturation: Float,
    val buttonDisabledIconOpacity: Float,
    val buttonFg: Color,
    val panelBg: Color,
    val panelBorder: Color,
    val panelFg: Color,
    val panelMuted: Color,
    val mapSelectionDisplayBg: Color,
    val mapSelectionDisplayFg: Color,
    val situationStatusBg: Color,
    val situationStatusFg: Color,
    val situationStatusUnavailableFg: Color,
    val dataStatusWarningBg: Color,
    val dataStatusWarningStroke: Color,
    val dataStatusQuietBg: Color,
    val dataStatusQuietStroke: Color,
    val chartSurfaceBg: Color,
    val flightDataBg: Color,
    val flightDataBorder: Color,
    val flightDataLabel: Color,
    val flightDataValue: Color,
    val flightDataMissingValue: Color,
    val flightDataPassedValue: Color,
    val flightDataActiveValue: Color,
    val flightDataModeledValue: Color,
    val cdiPointer: Color,
    val compassNorth: Color,
    val compassSouth: Color,
)

data class PlateFolderTheme(
    val thumbnailBg: Color,
    val labelColors: Map<String, Color>,
)

data class AviationTheme(
    val classBDBlue: Color,
    val classCMagenta: Color,
    val tfrActive: Color,
    val tfrUpcoming: Color,
    val intersectionCyan: Color,
    val darkGray: Color,
    val obstacleDanger: Color,
    val obstacleCaution: Color,
    val obstacleMuted: Color,
    val obstacleUnder: Color,
    val airportRunwayPaved: Color,
    val airportRunwayTurf: Color,
    val airportRunwayUnpaved: Color,
    val airportRunwayWater: Color,
    val airportRunwayInactive: Color,
)

data class FlightPlanRouteTheme(
    val contrast: Color,
    val completed: Color,
    val active: Color,
    val activeLegRemaining: Color,
    val remaining: Color,
    val distancePillBg: Color,
    val distancePillFg: Color,
)

@Serializable
private data class WireUiTheme(
    val controls: WireControlsTheme,
    val aviation: WireAviationTheme,
    val flight_plan_route: WireFlightPlanRouteTheme,
    val plate_folder: WirePlateFolderTheme,
)

@Serializable
private data class WireControlsTheme(
    val button_checked: String,
    val button_unchecked: String,
    val header_button: String,
    val button_disabled: String,
    val button_disabled_icon_saturation: Float,
    val button_disabled_icon_opacity: Float,
    val button_fg: String,
    val panel_bg: String,
    val panel_border: String,
    val panel_fg: String,
    val panel_muted: String,
    val map_selection_display_bg: String,
    val map_selection_display_fg: String,
    val situation_status_bg: String,
    val situation_status_fg: String,
    val situation_status_unavailable_fg: String,
    val data_status_warning_bg: String,
    val data_status_warning_stroke: String,
    val data_status_quiet_bg: String,
    val data_status_quiet_stroke: String,
    val chart_surface_bg: String,
    val flight_data_bg: String,
    val flight_data_border: String,
    val flight_data_label: String,
    val flight_data_value: String,
    val flight_data_missing_value: String,
    val flight_data_passed_value: String,
    val flight_data_active_value: String,
    val flight_data_modeled_value: String,
    val cdi_pointer: String,
    val compass_north: String,
    val compass_south: String,
)

@Serializable
private data class WirePlateFolderTheme(
    val thumbnail_bg: String,
    val label_colors: Map<String, String>,
)

@Serializable
private data class WireAviationTheme(
    val class_b_d_blue: String,
    val class_c_magenta: String,
    val tfr_active: String,
    val tfr_upcoming: String,
    val intersection_cyan: String,
    val dark_gray: String,
    val obstacle_danger: String,
    val obstacle_caution: String,
    val obstacle_muted: String,
    val obstacle_under: String,
    val airport_runway_paved: String,
    val airport_runway_turf: String,
    val airport_runway_unpaved: String,
    val airport_runway_water: String,
    val airport_runway_inactive: String,
)

@Serializable
private data class WireFlightPlanRouteTheme(
    val contrast: String,
    val completed: String,
    val active: String,
    val active_leg_remaining: String,
    val remaining: String,
    val distance_pill_bg: String,
    val distance_pill_fg: String,
)

object UiThemeLoader {
    private const val ASSET_PATH = "fixtures/ui-theme.json"

    private val json = Json {
        ignoreUnknownKeys = true
    }

    fun load(context: Context): UiTheme {
        val payload = context.assets.open(ASSET_PATH).bufferedReader().use { it.readText() }
        val wire = json.decodeFromString<WireUiTheme>(payload)
        return UiTheme(
            controls = ControlsTheme(
                buttonChecked = wire.controls.button_checked.toColor(),
                buttonUnchecked = wire.controls.button_unchecked.toColor(),
                headerButton = wire.controls.header_button.toColor(),
                buttonDisabled = wire.controls.button_disabled.toColor(),
                buttonDisabledIconSaturation = wire.controls.button_disabled_icon_saturation,
                buttonDisabledIconOpacity = wire.controls.button_disabled_icon_opacity,
                buttonFg = wire.controls.button_fg.toColor(),
                panelBg = wire.controls.panel_bg.toColor(),
                panelBorder = wire.controls.panel_border.toColor(),
                panelFg = wire.controls.panel_fg.toColor(),
                panelMuted = wire.controls.panel_muted.toColor(),
                mapSelectionDisplayBg = wire.controls.map_selection_display_bg.toColor(),
                mapSelectionDisplayFg = wire.controls.map_selection_display_fg.toColor(),
                situationStatusBg = wire.controls.situation_status_bg.toColor(),
                situationStatusFg = wire.controls.situation_status_fg.toColor(),
                situationStatusUnavailableFg = wire.controls.situation_status_unavailable_fg.toColor(),
                dataStatusWarningBg = wire.controls.data_status_warning_bg.toColor(),
                dataStatusWarningStroke = wire.controls.data_status_warning_stroke.toColor(),
                dataStatusQuietBg = wire.controls.data_status_quiet_bg.toColor(),
                dataStatusQuietStroke = wire.controls.data_status_quiet_stroke.toColor(),
                chartSurfaceBg = wire.controls.chart_surface_bg.toColor(),
                flightDataBg = wire.controls.flight_data_bg.toColor(),
                flightDataBorder = wire.controls.flight_data_border.toColor(),
                flightDataLabel = wire.controls.flight_data_label.toColor(),
                flightDataValue = wire.controls.flight_data_value.toColor(),
                flightDataMissingValue = wire.controls.flight_data_missing_value.toColor(),
                flightDataPassedValue = wire.controls.flight_data_passed_value.toColor(),
                flightDataActiveValue = wire.controls.flight_data_active_value.toColor(),
                flightDataModeledValue = wire.controls.flight_data_modeled_value.toColor(),
                cdiPointer = wire.controls.cdi_pointer.toColor(),
                compassNorth = wire.controls.compass_north.toColor(),
                compassSouth = wire.controls.compass_south.toColor(),
            ),
            aviation = AviationTheme(
                classBDBlue = wire.aviation.class_b_d_blue.toColor(),
                classCMagenta = wire.aviation.class_c_magenta.toColor(),
                tfrActive = wire.aviation.tfr_active.toColor(),
                tfrUpcoming = wire.aviation.tfr_upcoming.toColor(),
                intersectionCyan = wire.aviation.intersection_cyan.toColor(),
                darkGray = wire.aviation.dark_gray.toColor(),
                obstacleDanger = wire.aviation.obstacle_danger.toColor(),
                obstacleCaution = wire.aviation.obstacle_caution.toColor(),
                obstacleMuted = wire.aviation.obstacle_muted.toColor(),
                obstacleUnder = wire.aviation.obstacle_under.toColor(),
                airportRunwayPaved = wire.aviation.airport_runway_paved.toColor(),
                airportRunwayTurf = wire.aviation.airport_runway_turf.toColor(),
                airportRunwayUnpaved = wire.aviation.airport_runway_unpaved.toColor(),
                airportRunwayWater = wire.aviation.airport_runway_water.toColor(),
                airportRunwayInactive = wire.aviation.airport_runway_inactive.toColor(),
            ),
            flightPlanRoute = FlightPlanRouteTheme(
                contrast = wire.flight_plan_route.contrast.toColor(),
                completed = wire.flight_plan_route.completed.toColor(),
                active = wire.flight_plan_route.active.toColor(),
                activeLegRemaining = wire.flight_plan_route.active_leg_remaining.toColor(),
                remaining = wire.flight_plan_route.remaining.toColor(),
                distancePillBg = wire.flight_plan_route.distance_pill_bg.toColor(),
                distancePillFg = wire.flight_plan_route.distance_pill_fg.toColor(),
            ),
            plateFolder = PlateFolderTheme(
                thumbnailBg = wire.plate_folder.thumbnail_bg.toColor(),
                labelColors = wire.plate_folder.label_colors.mapValues { (_, value) -> value.toColor() },
            ),
        )
    }
}

private fun String.toColor(): Color {
    val hex = removePrefix("#")
    val argb = when (hex.length) {
        6 -> "FF$hex"
        8 -> "${hex.takeLast(2)}${hex.dropLast(2)}"
        else -> error("Unsupported color format: $this")
    }
    return Color(argb.toLong(16))
}
