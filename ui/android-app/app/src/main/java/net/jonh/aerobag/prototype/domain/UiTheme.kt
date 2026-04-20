package net.jonh.aerobag.prototype.domain

import android.content.Context
import androidx.compose.ui.graphics.Color
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

data class UiTheme(
    val controls: ControlsTheme,
    val plateFolder: PlateFolderTheme,
)

data class ControlsTheme(
    val buttonBg: Color,
    val headerButton: Color,
    val buttonFg: Color,
    val panelBg: Color,
    val panelBorder: Color,
    val panelFg: Color,
    val panelMuted: Color,
    val chartSurfaceBg: Color,
    val cdiPointer: Color,
)

data class PlateFolderTheme(
    val thumbnailBg: Color,
    val labelColors: Map<String, Color>,
)

@Serializable
private data class WireUiTheme(
    val controls: WireControlsTheme,
    val plate_folder: WirePlateFolderTheme,
)

@Serializable
private data class WireControlsTheme(
    val button_bg: String,
    val header_button: String,
    val button_fg: String,
    val panel_bg: String,
    val panel_border: String,
    val panel_fg: String,
    val panel_muted: String,
    val chart_surface_bg: String,
    val cdi_pointer: String,
)

@Serializable
private data class WirePlateFolderTheme(
    val thumbnail_bg: String,
    val label_colors: Map<String, String>,
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
                headerButton = wire.controls.header_button.toColor(),
                buttonFg = wire.controls.button_fg.toColor(),
                panelBg = wire.controls.panel_bg.toColor(),
                panelBorder = wire.controls.panel_border.toColor(),
                panelFg = wire.controls.panel_fg.toColor(),
                panelMuted = wire.controls.panel_muted.toColor(),
                chartSurfaceBg = wire.controls.chart_surface_bg.toColor(),
                cdiPointer = wire.controls.cdi_pointer.toColor(),
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
