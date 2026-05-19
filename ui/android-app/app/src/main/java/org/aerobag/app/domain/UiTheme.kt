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
    val buttonBg: Color,
    val buttonSelectedBg: Color,
    val headerButton: Color,
    val disabledButton: Color,
    val buttonFg: Color,
    val panelBg: Color,
    val panelBorder: Color,
    val panelFg: Color,
    val panelMuted: Color,
    val mapSelectionDisplayBg: Color,
    val mapSelectionDisplayFg: Color,
    val situationStatusBg: Color,
    val situationStatusFg: Color,
    val chartSurfaceBg: Color,
    val cdiPointer: Color,
)

data class PlateFolderTheme(
    val thumbnailBg: Color,
    val labelColors: Map<String, Color>,
)

data class AviationTheme(
    val classBDBlue: Color,
    val classCMagenta: Color,
    val tfrRed: Color,
    val intersectionCyan: Color,
    val darkGray: Color,
)

data class FlightPlanRouteTheme(
    val completed: Color,
    val active: Color,
    val activeLegRemaining: Color,
    val remaining: Color,
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
    val button_bg: String,
    val button_selected_bg: String,
    val header_button: String,
    val disabled_button: String,
    val button_fg: String,
    val panel_bg: String,
    val panel_border: String,
    val panel_fg: String,
    val panel_muted: String,
    val map_selection_display_bg: String,
    val map_selection_display_fg: String,
    val situation_status_bg: String,
    val situation_status_fg: String,
    val chart_surface_bg: String,
    val cdi_pointer: String,
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
    val tfr_red: String,
    val intersection_cyan: String,
    val dark_gray: String,
)

@Serializable
private data class WireFlightPlanRouteTheme(
    val completed: String,
    val active: String,
    val active_leg_remaining: String,
    val remaining: String,
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
                buttonBg = wire.controls.button_bg.toColor(),
                buttonSelectedBg = wire.controls.button_selected_bg.toColor(),
                headerButton = wire.controls.header_button.toColor(),
                disabledButton = wire.controls.disabled_button.toColor(),
                buttonFg = wire.controls.button_fg.toColor(),
                panelBg = wire.controls.panel_bg.toColor(),
                panelBorder = wire.controls.panel_border.toColor(),
                panelFg = wire.controls.panel_fg.toColor(),
                panelMuted = wire.controls.panel_muted.toColor(),
                mapSelectionDisplayBg = wire.controls.map_selection_display_bg.toColor(),
                mapSelectionDisplayFg = wire.controls.map_selection_display_fg.toColor(),
                situationStatusBg = wire.controls.situation_status_bg.toColor(),
                situationStatusFg = wire.controls.situation_status_fg.toColor(),
                chartSurfaceBg = wire.controls.chart_surface_bg.toColor(),
                cdiPointer = wire.controls.cdi_pointer.toColor(),
            ),
            aviation = AviationTheme(
                classBDBlue = wire.aviation.class_b_d_blue.toColor(),
                classCMagenta = wire.aviation.class_c_magenta.toColor(),
                tfrRed = wire.aviation.tfr_red.toColor(),
                intersectionCyan = wire.aviation.intersection_cyan.toColor(),
                darkGray = wire.aviation.dark_gray.toColor(),
            ),
            flightPlanRoute = FlightPlanRouteTheme(
                completed = wire.flight_plan_route.completed.toColor(),
                active = wire.flight_plan_route.active.toColor(),
                activeLegRemaining = wire.flight_plan_route.active_leg_remaining.toColor(),
                remaining = wire.flight_plan_route.remaining.toColor(),
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
