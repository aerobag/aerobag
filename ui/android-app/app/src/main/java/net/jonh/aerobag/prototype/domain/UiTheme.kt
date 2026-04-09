package net.jonh.aerobag.prototype.domain

import android.content.Context
import androidx.compose.ui.graphics.Color
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

data class UiTheme(
    val plateFolder: PlateFolderTheme,
)

data class PlateFolderTheme(
    val thumbnailBg: Color,
    val labelColors: Map<String, Color>,
)

@Serializable
private data class WireUiTheme(
    val plate_folder: WirePlateFolderTheme,
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
