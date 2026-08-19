// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.domain

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.KSerializer
import kotlinx.serialization.descriptors.PrimitiveKind
import kotlinx.serialization.descriptors.PrimitiveSerialDescriptor
import kotlinx.serialization.descriptors.SerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import kotlinx.serialization.json.JsonDecoder
import kotlinx.serialization.json.JsonEncoder
import kotlinx.serialization.json.JsonContentPolymorphicSerializer
import kotlinx.serialization.json.JsonNames
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.encodeToJsonElement
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

@Serializable
data class WireAppUiState(
    val active_plan: WireFlightPlanUiState? = null,
    val aircraft_plan_view_path: String = "",
    val ownship: WireOwnshipUiState = WireOwnshipUiState(),
    val flight_data_banner: WireFlightDataBannerModel = WireFlightDataBannerModel(),
)

@Serializable
data class WireOwnshipState(
    val policy: WireOwnshipPolicy = WireOwnshipPolicy(),
    val resolved: WireResolvedOwnshipState = WireResolvedOwnshipState(),
    val render: WireOwnshipRenderState = WireOwnshipRenderState(),
    val controls: WireOwnshipControlModel = WireOwnshipControlModel(),
    val sources: List<WireOwnshipSourceStatus> = emptyList(),
)

@Serializable
data class WireOwnshipPolicy(
    val selection: WireOwnshipSelection = WireOwnshipSelection.Auto,
    val source_priority: List<String> = emptyList(),
    val allow_auto_replay: Boolean = false,
    val allow_auto_simulated: Boolean = false,
)

@Serializable(with = WireOwnshipSelectionSerializer::class)
sealed interface WireOwnshipSelection {
    data object Auto : WireOwnshipSelection
    data class Source(val source_id: String) : WireOwnshipSelection
}

@Serializable
data class WireResolvedOwnshipState(
    val mode: WireOwnshipMode = WireOwnshipMode.None,
    val active_source_id: String? = null,
    val active_source_kind: WireOwnshipSourceKind? = null,
    val banner_text: String = "NO GPS POSITION",
    val banner_severity: WireOwnshipBannerSeverity = WireOwnshipBannerSeverity.Warning,
    val guidance_enabled: Boolean = false,
    val sequencing_enabled: Boolean = false,
)

@Serializable
data class WireOwnshipRenderState(
    val mode: WireOwnshipMode = WireOwnshipMode.None,
    val banner_text: String = "NO GPS POSITION",
    val banner_severity: WireOwnshipBannerSeverity = WireOwnshipBannerSeverity.Warning,
    val draw_aircraft: Boolean = false,
    val draw_predictor: Boolean = false,
    val draw_cdi: Boolean = false,
    val position: WireLatLon? = null,
    val track_deg_true: Double? = null,
    val orientation_deg: Double? = null,
    val magnetic_variation_deg: Double? = null,
    val speed_kt: Double? = null,
    val terrain_altitude_bucket_ft: Double? = null,
)

@Serializable
data class WireOwnshipControlModel(
    val mode: WireOwnshipMode = WireOwnshipMode.None,
    val selection: WireOwnshipSelection = WireOwnshipSelection.Auto,
    val launcher_label: String = "No GPS",
    val launcher_tone: WireOwnshipControlTone = WireOwnshipControlTone.Unavailable,
    val launcher_text_tone: WireOwnshipLauncherTextTone = WireOwnshipLauncherTextTone.Unavailable,
    val sources: List<WireOwnshipSourceMenuItem> = emptyList(),
    val situation_controls: List<WireSituationControlMenuItem> = emptyList(),
    val text_action: WireOwnshipTextAction? = null,
    val next_refresh_epoch_ms: Long? = null,
)

@Serializable
data class WireOwnshipTextAction(
    val action_id: String,
    val label: String,
    val value: String,
    val placeholder: String,
    val submit_label: String,
    val enabled: Boolean,
    val disabled_reason: String? = null,
)

@Serializable
data class WireOwnshipUiState(
    val render: WireOwnshipRenderState = WireOwnshipRenderState(),
    val controls: WireOwnshipControlModel = WireOwnshipControlModel(),
)

@Serializable
data class WireSituationRingCandidate(
    val radius_nm: Double,
    val label: String,
)

@Serializable
data class WireMapFollowUiState(
    val can_center_here: Boolean = false,
    val following: Boolean = false,
    val disabled_reason: String? = null,
)

@Serializable
data class WirePlaybackUiState(
    val status: WirePlaybackStatus = WirePlaybackStatus.Empty,
    val source_path: String? = null,
    val title_label: String = "Playback",
    val registration: String? = null,
    val icao: String? = null,
    val aircraft_type: String? = null,
    val point_count: Int = 0,
    val duration_seconds: Double = 0.0,
    val cursor_seconds: Double = 0.0,
    val cursor_label: String = "0:00",
    val duration_label: String = "0:00",
    val rate: Double = 1.0,
    val tick_interval_ms: Int = 100,
    val speed_profile_norm: List<Double?> = emptyList(),
    val altitude_profile_norm: List<Double?> = emptyList(),
    val gap_spans: List<WirePlaybackGapSpan> = emptyList(),
)

@Serializable
data class WirePlaybackGapSpan(
    val start_seconds: Double,
    val end_seconds: Double,
)

@Serializable
data class WireOwnshipSourceMenuItem(
    val source_id: String,
    val source_kind: WireOwnshipSourceKind,
    val label: String,
    val launcher_label: String,
    val tone: WireOwnshipControlTone = WireOwnshipControlTone.Neutral,
    val enabled: Boolean,
    val disabled_reason: String? = null,
    val active: Boolean,
    val status_label: String,
    val power_state: WireOwnshipSourcePowerState? = null,
    val keep_tray_open_on_select: Boolean = false,
)

@Serializable
data class WireSituationControlMenuItem(
    val input: WireSituationControlInput,
    val label: String,
    val enabled: Boolean,
    val disabled_reason: String? = null,
)

@Serializable
enum class WireSituationControlInput {
    @SerialName("skip_backward")
    SkipBackward,

    @SerialName("fast_rewind")
    FastRewind,

    @SerialName("fast_forward")
    FastForward,

    @SerialName("skip_forward")
    SkipForward,

    @SerialName("pause")
    Pause,

    @SerialName("resume")
    Resume,
}

@Serializable
enum class WireOwnshipSourcePowerState {
    @SerialName("running")
    Running,

    @SerialName("paused")
    Paused,

    @SerialName("sleeping")
    Sleeping,
}

@Serializable
data class WireOwnshipSourceStatus(
    val source_id: String,
    val source_kind: WireOwnshipSourceKind,
    val display_name: String,
    val connection_state: WireSourceConnectionState,
    val last_event_time_epoch_ms: Long? = null,
    val last_received_time_epoch_ms: Long? = null,
    val stale_after_ms: Long = 0,
    val selectable: Boolean = true,
    val enabled: Boolean = true,
    val auto_eligible: Boolean = true,
    val active: Boolean = false,
    val status_label: String = "",
)

@Serializable
data class WirePlanLeg(
    val from: WireNavRef,
    val to: WireNavRef,
    val airway: String? = null,
)

@Serializable(with = WireNavRefSerializer::class)
sealed interface WireNavRef {
    data class Airport(val code: String) : WireNavRef

    data class Navaid(val code: String) : WireNavRef

    @Serializable
    data class ArincNavaid(
        val identifier: String,
        val icao_code: String,
        val section_code: String,
        val subsection_code: String,
    ) : WireNavRef

    @Serializable
    data class TerminalNavaid(
        val airport_id: String,
        val identifier: String,
        val icao_code: String,
        val section_code: String,
        val subsection_code: String,
    ) : WireNavRef

    data class Fix(val code: String) : WireNavRef

    data class LatLon(val value: WireLatLon) : WireNavRef

    data class Spot(val value: WireLatLon) : WireNavRef
}

@Serializable
data class WireLatLon(
    val lat: Double,
    val lon: Double,
)

object WireNavRefSerializer : KSerializer<WireNavRef> {
    override val descriptor: SerialDescriptor =
        PrimitiveSerialDescriptor("WireNavRef", PrimitiveKind.STRING)

    override fun serialize(encoder: Encoder, value: WireNavRef) {
        require(encoder is JsonEncoder) { "WireNavRef is JSON-only" }
        val element = when (value) {
            is WireNavRef.Airport -> JsonObject(mapOf("Airport" to JsonPrimitive(value.code)))
            is WireNavRef.Navaid -> JsonObject(mapOf("Navaid" to JsonPrimitive(value.code)))
            is WireNavRef.ArincNavaid -> JsonObject(
                mapOf(
                    "ArincNavaid" to encoder.json.encodeToJsonElement(WireNavRef.ArincNavaid.serializer(), value),
                ),
            )
            is WireNavRef.TerminalNavaid -> JsonObject(
                mapOf(
                    "TerminalNavaid" to encoder.json.encodeToJsonElement(WireNavRef.TerminalNavaid.serializer(), value),
                ),
            )
            is WireNavRef.Fix -> JsonObject(mapOf("Fix" to JsonPrimitive(value.code)))
            is WireNavRef.LatLon -> JsonObject(
                mapOf(
                    "LatLon" to encoder.json.encodeToJsonElement(WireLatLon.serializer(), value.value),
                ),
            )
            is WireNavRef.Spot -> JsonObject(
                mapOf(
                    "Spot" to encoder.json.encodeToJsonElement(WireLatLon.serializer(), value.value),
                ),
            )
        }
        encoder.encodeJsonElement(element)
    }

    override fun deserialize(decoder: Decoder): WireNavRef {
        require(decoder is JsonDecoder) { "WireNavRef is JSON-only" }
        val element = decoder.decodeJsonElement()
        require(element is JsonObject && element.size == 1) {
            "Expected single-key NavRef object, got $element"
        }

        val (kind, value) = element.entries.single()
        return when (kind) {
            "Airport" -> WireNavRef.Airport(value.jsonPrimitive.content)
            "Navaid" -> WireNavRef.Navaid(value.jsonPrimitive.content)
            "ArincNavaid" -> decoder.json.decodeFromJsonElement(WireNavRef.ArincNavaid.serializer(), value)
            "TerminalNavaid" -> decoder.json.decodeFromJsonElement(WireNavRef.TerminalNavaid.serializer(), value)
            "Fix" -> WireNavRef.Fix(value.jsonPrimitive.content)
            "LatLon" -> WireNavRef.LatLon(decoder.json.decodeFromJsonElement(WireLatLon.serializer(), value))
            "Spot" -> WireNavRef.Spot(decoder.json.decodeFromJsonElement(WireLatLon.serializer(), value))
            else -> error("Unsupported NavRef variant: $kind")
        }
    }
}

object WireOwnshipSelectionSerializer : KSerializer<WireOwnshipSelection> {
    override val descriptor: SerialDescriptor =
        PrimitiveSerialDescriptor("WireOwnshipSelection", PrimitiveKind.STRING)

    override fun serialize(encoder: Encoder, value: WireOwnshipSelection) {
        require(encoder is JsonEncoder) { "WireOwnshipSelection is JSON-only" }
        val element = when (value) {
            WireOwnshipSelection.Auto -> JsonPrimitive("auto")
            is WireOwnshipSelection.Source -> JsonObject(
                mapOf(
                    "source" to JsonObject(
                        mapOf("source_id" to JsonPrimitive(value.source_id)),
                    ),
                ),
            )
        }
        encoder.encodeJsonElement(element)
    }

    override fun deserialize(decoder: Decoder): WireOwnshipSelection {
        require(decoder is JsonDecoder) { "WireOwnshipSelection is JSON-only" }
        val element = decoder.decodeJsonElement()
        if (element is JsonPrimitive) {
            return when (element.content) {
                "auto" -> WireOwnshipSelection.Auto
                else -> error("Unsupported WireOwnshipSelection")
            }
        }
        val obj = element as? JsonObject ?: error("WireOwnshipSelection must be an object")
        val externallyTagged = obj.entries.singleOrNull()
        return when (externallyTagged?.key) {
            "source" -> WireOwnshipSelection.Source(source_id = decodeOwnshipSourceId(externallyTagged.value))
            else -> error("Unsupported WireOwnshipSelection")
        }
    }
}

private fun decodeOwnshipSourceId(element: kotlinx.serialization.json.JsonElement): String {
    val obj = element as? JsonObject ?: error("source_id required")
    val sourceId = obj["source_id"] ?: error("source_id required")
    return sourceId.jsonPrimitive.content
}

@Serializable
enum class WireOwnshipMode {
    @SerialName("none")
    None,

    @SerialName("live")
    Live,

    @SerialName("replay")
    Replay,

    @SerialName("simulated")
    Simulated,
}

@Serializable
enum class WirePlaybackStatus {
    @SerialName("empty")
    Empty,

    @SerialName("paused")
    Paused,

    @SerialName("playing")
    Playing,
}

@Serializable
enum class WireOwnshipBannerSeverity {
    @SerialName("info")
    Info,

    @SerialName("caution")
    Caution,

    @SerialName("warning")
    Warning,
}

@Serializable
enum class WireOwnshipControlTone {
    @SerialName("ready")
    Ready,

    @SerialName("unavailable")
    Unavailable,

    @SerialName("neutral")
    Neutral,
}

@Serializable
enum class WireOwnshipLauncherTextTone {
    @SerialName("normal")
    Normal,

    @SerialName("unavailable")
    Unavailable,
}

@Serializable
enum class WireOwnshipSourceKind {
    @SerialName("device_gps")
    DeviceGps,

    @SerialName("external_gps")
    ExternalGps,

    @SerialName("external_ahrs")
    ExternalAhrs,

    @SerialName("gpx_playback")
    GpxPlayback,

    @SerialName("adsb_track_playback")
    AdsbTrackPlayback,

    @SerialName("live_network_track")
    LiveNetworkTrack,

    @SerialName("flight_plan_simulator")
    FlightPlanSimulator,

    @SerialName("bad_autopilot")
    BadAutopilot,
}

@Serializable
enum class WireSourceConnectionState {
    @SerialName("unavailable")
    Unavailable,

    @SerialName("searching")
    Searching,

    @SerialName("connected")
    Connected,

    @SerialName("stale")
    Stale,

    @SerialName("failed")
    Failed,
}

@Serializable
data class WireVectorTileRequest(
    val layer: String,
    val z: Int,
    val x: Int,
    val y: Int,
)

@Serializable
data class WireAirspaceFeatureRequest(
    val id: String,
    val path: String,
)

@Serializable
data class WirePointVectorRecord(
    val id: String,
    val kind: String,
    val lat: Double,
    val lon: Double,
    val label: String,
    val style_class: String,
    val towered: Boolean? = null,
    val fuel_available: Boolean? = null,
    val has_paved_runway: Boolean? = null,
    val heliport: Boolean? = null,
    val has_water_runway: Boolean? = null,
    val longest_runway_length_ft: Double? = null,
    val longest_runway_heading_true_deg: Double? = null,
    val elevation_msl_ft: Double? = null,
)

@Serializable
data class WirePointTilePayload(
    val schema_version: Int,
    val layer: String,
    val z: Int,
    val x: Int,
    val y: Int,
    val records: List<WirePointVectorRecord>,
)

@Serializable
data class WireVisibleMapFeature(
    val id: String,
    val kind: String,
    val label: String,
    val symbol_kind: String = "fix",
    val style_class: String,
    val obstacle_variant: String? = null,
    val obstacle_tone: String? = null,
    val screen_x: Double,
    val screen_y: Double,
    val towered: Boolean,
    val fuel_available: Boolean,
    val has_paved_runway: Boolean? = null,
    val heliport: Boolean? = null,
    val has_water_runway: Boolean? = null,
    val runway_length_ratio: Double,
    val longest_runway_heading_true_deg: Double? = null,
    val label_style: String = "default",
)

typealias WireNavSymbolFeature = org.aerobag.app.generated.NavSymbolFeature

@Serializable
data class WireMapOverlayQueryResult(
    val visible_features: List<WireVisibleMapFeature>,
    val flight_plan_features: List<WireVisibleMapFeature> = emptyList(),
    val visible_metars: List<WireVisibleMetarFeature> = emptyList(),
    val visible_pireps: List<WireVisiblePirepFeature> = emptyList(),
    val visible_traffic: List<WireVisibleAdsbTraffic> = emptyList(),
    val traffic_next_refresh_epoch_ms: Long? = null,
    val airspace_paths: List<WireAirspaceDisplayPath> = emptyList(),
    val tfr_paths: List<WireAirspaceDisplayPath> = emptyList(),
    val airspace_labels: List<WireAirspaceDisplayLabel> = emptyList(),
    val offline_regions: List<WireOfflineRegionDisplay> = emptyList(),
)

@Serializable
data class WireVisibleAdsbTraffic(
    val id: String,
    val screen_x: Double,
    val screen_y: Double,
    val track_deg_true: Double? = null,
    val label: String,
    val detail_label: String,
)

@Serializable
data class WireVisibleMetarFeature(
    val station_id: String,
    val screen_x: Double,
    val screen_y: Double,
    val flight_category: String,
    val ceiling_amount: String,
)

@Serializable
data class WireVisiblePirepFeature(
    val id: String,
    val screen_x: Double,
    val screen_y: Double,
    val symbol: String,
    val icing: String,
    val turbulence: String,
)

@Serializable
data class WireAirspaceDisplayStroke(
    val color_key: String,
    val width_px: Double,
    val dash_px: List<Double> = emptyList(),
    val line_cap: String = "round",
)

@Serializable
data class WireAirspaceDisplayStyle(
    val fill_color_key: String,
    val fill_opacity: Double,
    val strokes: List<WireAirspaceDisplayStroke> = emptyList(),
)

@Serializable
data class WireAirspaceScreenPoint(
    val x: Double,
    val y: Double,
)

@Serializable
data class WireAirspaceDisplaySubpath(
    val closed: Boolean,
    val points: List<WireAirspaceScreenPoint> = emptyList(),
)

@Serializable
data class WireAirspaceDisplayDecoration(
    val color_key: String,
    val width_px: Double,
    val line_cap: String = "round",
    val paths: List<WireAirspaceDisplaySubpath> = emptyList(),
    val segments: List<List<Double>> = emptyList(),
)

@Serializable
data class WireAirspaceDisplayPath(
    val id: String,
    val name: String,
    val style_key: String,
    val style: WireAirspaceDisplayStyle,
    val paths: List<WireAirspaceDisplaySubpath> = emptyList(),
    val decorations: List<WireAirspaceDisplayDecoration> = emptyList(),
)

@Serializable
data class WireAirspaceDisplayLabel(
    val feature_id: String,
    val glyph: WireAirspaceLimitGlyph,
    val screen_x: Double,
    val screen_y: Double,
)

@Serializable
data class WireAirspaceLimitGlyph(
    val upper: String,
    val lower: String,
    val style_key: String,
    val color_key: String,
)

@Serializable
data class WireOfflineRegionDisplay(
    val id: String,
    val kind: String,
    val region_id: String,
    val label: String,
    val color_key: String,
    val points: List<WireAirspaceScreenPoint> = emptyList(),
    val label_x: Double,
    val label_y: Double,
)

@Serializable
data class WireMapSelectionQueryResult(
    val click_lat: Double,
    val click_lon: Double,
    val initial_selected_item_id: String? = null,
    val categories: List<WireMapSelectionCategory> = emptyList(),
)

@Serializable
data class WireMapSelectionForNavRefResult(
    val position: WireLatLon,
    val target_zoom: Double,
    val selection: WireMapSelectionQueryResult,
    val selected_item_id: String? = null,
)

@Serializable
data class WireMapSelectionCategory(
    val id: String,
    val label: String,
    val items: List<WireMapSelectionItem> = emptyList(),
)

@Serializable
data class WireMapSelectionItem(
    val id: String,
    val label: String,
    val sublabel: String,
    val description: String? = null,
    val distance: String? = null,
    val distance_target: WireLatLon? = null,
    val secondary_description: String? = null,
    val detail_text: String? = null,
    val highlight: @Serializable(with = WireMapSelectionHighlightSerializer::class) WireMapSelectionHighlight,
    val nav_ref: WireNavRef? = null,
    val symbol_feature: WireNavSymbolFeature? = null,
    val metar_feature: WireVisibleMetarFeature? = null,
    val weather_detail: WireWeatherDetailUiView? = null,
    val automatic_action_uid: String? = null,
    val pirep_feature: WireVisiblePirepFeature? = null,
    val airspace_icon: WireAirspaceDisplayPath? = null,
    val actions: List<WireMapSelectionAction> = emptyList(),
)

@Serializable(with = WireMapSelectionHighlightSerializer::class)
sealed interface WireMapSelectionHighlight {
    data class FeatureRef(val id: String) : WireMapSelectionHighlight
    data class Metar(val station_id: String) : WireMapSelectionHighlight
    data class Pirep(val id: String) : WireMapSelectionHighlight
    data class AdsbTraffic(val id: String) : WireMapSelectionHighlight
    data class OfflineRegion(val id: String) : WireMapSelectionHighlight
    data class Spot(val lat: Double, val lon: Double) : WireMapSelectionHighlight
}

object WireMapSelectionHighlightSerializer : JsonContentPolymorphicSerializer<WireMapSelectionHighlight>(WireMapSelectionHighlight::class) {
    override fun selectDeserializer(element: kotlinx.serialization.json.JsonElement): kotlinx.serialization.DeserializationStrategy<WireMapSelectionHighlight> {
        return when (element.jsonObject["kind"]?.jsonPrimitive?.content) {
            "feature_ref" -> WireMapSelectionHighlightFeatureRef.serializer()
            "metar" -> WireMapSelectionHighlightMetar.serializer()
            "pirep" -> WireMapSelectionHighlightPirep.serializer()
            "adsb_traffic" -> WireMapSelectionHighlightAdsbTraffic.serializer()
            "offline_region" -> WireMapSelectionHighlightOfflineRegion.serializer()
            "spot" -> WireMapSelectionHighlightSpot.serializer()
            else -> WireMapSelectionHighlightSpot.serializer()
        }
    }
}

@Serializable
@SerialName("feature_ref")
data class WireMapSelectionHighlightFeatureRef(
    val kind: String = "feature_ref",
    val id: String,
) : WireMapSelectionHighlight

@Serializable
@SerialName("metar")
data class WireMapSelectionHighlightMetar(
    val kind: String = "metar",
    val station_id: String,
) : WireMapSelectionHighlight

@Serializable
@SerialName("pirep")
data class WireMapSelectionHighlightPirep(
    val kind: String = "pirep",
    val id: String,
) : WireMapSelectionHighlight

@Serializable
@SerialName("adsb_traffic")
data class WireMapSelectionHighlightAdsbTraffic(
    val kind: String = "adsb_traffic",
    val id: String,
) : WireMapSelectionHighlight

@Serializable
@SerialName("offline_region")
data class WireMapSelectionHighlightOfflineRegion(
    val kind: String = "offline_region",
    val id: String,
) : WireMapSelectionHighlight

@Serializable
@SerialName("spot")
data class WireMapSelectionHighlightSpot(
    val kind: String = "spot",
    val lat: Double,
    val lon: Double,
) : WireMapSelectionHighlight

@Serializable
data class WireMapSelectionAction(
    val id: String,
    val label: String,
    val enabled: Boolean,
    val display_only: Boolean,
    val action_uid: String? = null,
    val placeholder: Boolean = false,
    val disabled_reason: String? = null,
    val airspace_limit: WireAirspaceLimitGlyph? = null,
)

@Serializable
data class WireMapSelectionActionDecision(
    val perform_session_mutation: Boolean,
    val dismiss_selection: Boolean,
    val effect: WireMapSelectionActionEffect? = null,
)

@Serializable
data class WireMapSelectionActionEffect(
    val kind: String,
    val detail: WireWeatherDetailUiView? = null,
    val airport_id: String? = null,
    val loading_text: String? = null,
    val failure_prefix: String? = null,
    val title: String? = null,
    val text: String? = null,
    val status: WireMapSelectionDetailStatus? = null,
    val target: String? = null,
    val chart_id: String? = null,
)

@Serializable
data class WireMapSelectionDetailStatus(
    val text: String,
    val color_key: String,
    val action_id: String? = null,
)

@Serializable
data class WireWeatherDetailUiView(
    val station_id: String,
    val advisory_text: String,
    val metar_text: String? = null,
    val metar_age_label: String? = null,
    val metar_age_warning: Boolean = false,
    val taf_text: String? = null,
    val taf_age_label: String? = null,
    val taf_age_warning: Boolean = false,
    val notams: List<WireAirportNotamUiView> = emptyList(),
)

@Serializable
data class WireAirportNotamUiView(
    val id: String,
    val label: String,
    val text: String,
)

@Serializable
sealed interface WireTerrainOverlayStatus {
    val state: String
}

@Serializable
@SerialName("hidden")
data class WireTerrainOverlayStatusHidden(
    override val state: String = "hidden",
) : WireTerrainOverlayStatus

@Serializable
@SerialName("no_position")
data class WireTerrainOverlayStatusNoPosition(
    override val state: String = "no_position",
) : WireTerrainOverlayStatus

@Serializable
@SerialName("no_altitude")
data class WireTerrainOverlayStatusNoAltitude(
    override val state: String = "no_altitude",
) : WireTerrainOverlayStatus

@Serializable
@SerialName("too_many_tiles")
data class WireTerrainOverlayStatusTooManyTiles(
    override val state: String = "too_many_tiles",
    val count: Int,
) : WireTerrainOverlayStatus

@Serializable
@SerialName("unavailable")
data class WireTerrainOverlayStatusUnavailable(
    override val state: String = "unavailable",
    val reason: String,
) : WireTerrainOverlayStatus

@Serializable
@SerialName("ready")
data class WireTerrainOverlayStatusReady(
    override val state: String = "ready",
    val count: Int,
) : WireTerrainOverlayStatus

@Serializable
data class WireTerrainOverlayTileRequest(
    val key: String,
    val cache_key: String,
    val product_id: String,
    val path: String,
    val source_tiles: List<WireTerrainOverlaySourceTile>,
    val z: Int,
    val x: Int,
    val y_tms: Int,
    val left: Double,
    val top: Double,
    val size: Double,
)

@Serializable
data class WireTerrainOverlaySourceTile(
    val product_id: String,
    val path: String,
    val resource: WireCoreResourceRequest? = null,
)

@Serializable
data class WireCoreResourceRequest(
    val id: String,
    val source: JsonObject,
    val optional: Boolean = false,
    val max_response_bytes: Long? = null,
)

@Serializable
data class WireTerrainOverlayQueryResult(
    val status: @Serializable(with = WireTerrainOverlayStatusSerializer::class) WireTerrainOverlayStatus,
    val tile_requests: List<WireTerrainOverlayTileRequest>,
    val altitude_bucket_ft: Double? = null,
    val frame_key: String? = null,
    val schedule: WireTerrainOverlayScheduleDecision,
)

@Serializable
data class WireTerrainOverlayScheduleDecision(
    val cached_count: Int,
    val in_flight_count: Int,
    val missing_count: Int,
    val frame_complete: Boolean,
    val work_batch: List<WireTerrainOverlayTileRequest> = emptyList(),
)

object WireTerrainOverlayStatusSerializer : JsonContentPolymorphicSerializer<WireTerrainOverlayStatus>(WireTerrainOverlayStatus::class) {
    override fun selectDeserializer(element: kotlinx.serialization.json.JsonElement): kotlinx.serialization.DeserializationStrategy<WireTerrainOverlayStatus> {
        val state = element.jsonObject["state"]?.jsonPrimitive?.content
        return when (state) {
            "hidden" -> WireTerrainOverlayStatusHidden.serializer()
            "no_position" -> WireTerrainOverlayStatusNoPosition.serializer()
            "no_altitude" -> WireTerrainOverlayStatusNoAltitude.serializer()
            "too_many_tiles" -> WireTerrainOverlayStatusTooManyTiles.serializer()
            "unavailable" -> WireTerrainOverlayStatusUnavailable.serializer()
            "ready" -> WireTerrainOverlayStatusReady.serializer()
            else -> WireTerrainOverlayStatusHidden.serializer()
        }
    }
}

@Serializable
data class WireAirwaySuggestion(
    val airway_name: String,
    val nearest_branch_key: String? = null,
    val nearest_nav_ref: WireNavRef,
    val nearest_sequence: Int,
    val distance_from_anchor_nm: Double,
)

typealias WireWaypointIdentifierSuggestion =
    org.aerobag.app.generated.WaypointIdentifierSuggestion

@Serializable
data class WireAirwayPresentationPoint(
    val uid: String,
    val sequence: Int,
    val nav_ref: WireNavRef,
)

@Serializable
data class WireAirwayPresentationPlan(
    val airway_name: String,
    val branch_key: String,
    val points: List<WireAirwayPresentationPoint>,
    val suggested_entry_uid: String,
    val suggested_exit_uid: String? = null,
)

@Serializable
enum class WireProcedureKind {
    @SerialName("sid")
    Sid,

    @SerialName("star")
    Star,

    @SerialName("approach")
    Approach,
}

@Serializable
data class WireProcedureSummary(
    val airport_id: String,
    val procedure_id: String,
    val display_label: String,
    val kind: WireProcedureKind,
    val accent_category: String,
    val enabled: Boolean,
    val disabled_reason: String? = null,
)

@Serializable
data class WireProcedureDistinctRow(
    val route_type: String,
    val transition_id: String,
)

@Serializable
data class WireProcedureSpecChoice(
    val runway_transition: String? = null,
    val enroute_transition: String? = null,
)

@Serializable
data class WireProcedureOptions(
    val airport_id: String,
    val procedure_id: String,
    val kind: WireProcedureKind,
    val runway_transitions: List<String>,
    val enroute_transitions: List<String>,
    val has_common_segment: Boolean,
    val valid_choices: List<WireProcedureSpecChoice>,
)

@Serializable
data class WireProcedureLegMaterializationKey(
    val airport_id: String,
    val procedure_id: String,
    val route_type: String,
    val transition_id: String,
)

@Serializable
data class WireProcedureLegMaterializationRecord(
    val key: WireProcedureLegMaterializationKey,
    val sequence: Int,
    val nav_ref: WireNavRef? = null,
    val path_termination: String,
)

@Serializable
enum class WireRouteSegmentStatus {
    @SerialName("completed")
    Completed,

    @SerialName("active")
    Active,

    @SerialName("active_leg_remaining")
    ActiveLegRemaining,

    @SerialName("remaining")
    Remaining,
}

@Serializable
data class WireFlightPlanRouteSegment(
    val id: String,
    @SerialName("leg_id")
    val legId: String,
    val from: WireLatLon,
    val to: WireLatLon,
    val path: List<WireLatLon> = emptyList(),
    val style: String = "solid",
    val distance_nm: Double,
    val course_deg: Double,
    val status: WireRouteSegmentStatus,
)

@Serializable
data class WireFlightPlanRouteDistanceAnnotation(
    val id: String,
    val segment_indexes: List<Int>,
    val text: String,
    val distance_nm: Double,
    val status: WireRouteSegmentStatus,
    val required_feature_ids: List<String>,
    val minimum_path_to_pill_width_ratio: Double,
)

@Serializable
data class WireFlightPlanRouteProjection(
    val flight_plan_route_revision: Long,
    val segments: List<WireFlightPlanRouteSegment>,
    val distance_annotations: List<WireFlightPlanRouteDistanceAnnotation> = emptyList(),
)

@Serializable
enum class WireSequencingMode {
    @SerialName("follow_plan")
    FollowPlan,

    @SerialName("suspended")
    Suspended,

    @SerialName("direct_to")
    DirectTo,
}

@Serializable
enum class WireRouteComponentViewKind {
    @SerialName("waypoint")
    Waypoint,

    @SerialName("airway")
    Airway,

    @SerialName("procedure")
    Procedure,
}

@Serializable
data class WireDirectToUiView(
    val start: WireNavRef,
    val target: WireNavRef,
    val target_row_id: String,
    val on_plan_target: Boolean,
)

@Serializable
data class WireGuidanceUiView(
    val sequencing_mode: WireSequencingMode,
    val active_from_row_uid: String? = null,
    val active_to_row_uid: String? = null,
    val active_leg: WirePlanLeg? = null,
    val nav_element: WireNavElementUiView = WireNavElementUiView(),
    val direct_to: WireDirectToUiView? = null,
    val suspend_boundary_after_active_leg: Boolean = false,
)

@Serializable
enum class WireFlightPlanControlId {
    @SerialName("activate_next_leg")
    ActivateNextLeg,

    @SerialName("redo")
    Redo,

    @SerialName("restore_direct_to")
    RestoreDirectTo,

    @SerialName("sequence_active_leg")
    SequenceActiveLeg,

    @SerialName("stop_navigation")
    StopNavigation,

    @SerialName("suspend_sequencing")
    SuspendSequencing,

    @SerialName("undo")
    Undo,

    @SerialName("unsuspend_sequencing")
    UnsuspendSequencing,
}

@Serializable
data class WireFlightPlanControlUiView(
    val id: WireFlightPlanControlId,
    val label: String,
    val enabled: Boolean,
    val disabled_reason: String? = null,
)

@Serializable
data class WireNavElementUiView(
    val active_leg_summary: String = "",
    val cdi_indicator_dots: Float? = null,
    val cdi_offscale_readout: String? = null,
)

@Serializable
data class WireFlightPlanUiState(
    val plan_id: String,
    val plan_version: Long,
    val display_rows: List<WireFlightPlanDisplayRowUiView>,
    val data_columns: List<WireFlightDataColumn>,
    val controls: List<WireFlightPlanControlUiView> = emptyList(),
    val altitude_planner: WireAltitudePlannerUiView,
    val guidance: WireGuidanceUiView? = null,
)

@Serializable
data class WireAltitudePlannerControlUiView(
    val id: String,
    val label: String,
    val enabled: Boolean,
    val action_uid: String? = null,
    val disabled_reason: String? = null,
    val options: List<WireAltitudePlannerControlOptionUiView> = emptyList(),
)

@Serializable
data class WireAltitudePlannerControlOptionUiView(
    val label: String,
    val action_uid: String,
    val selected: Boolean,
)

@Serializable
data class WireAltitudePlannerUnavailableReason(
    val code: String,
    val message: String,
)

@Serializable
data class WireAltitudeComparisonUiView(
    val action_uid: String? = null,
    val selected: Boolean,
    val enabled: Boolean,
    val disabled_reason: String? = null,
    val advisory: String? = null,
    val cells: List<WireFlightDataCell>,
)

@Serializable
data class WireAltitudeComparisonPanelUiView(
    val columns: List<WireFlightDataColumn>,
    val rows: List<WireAltitudeComparisonUiView>,
    val advisories: List<String> = emptyList(),
)

@Serializable
data class WireAltitudePlannerForecastUiView(
    val summary: String,
)

@Serializable
data class WireAltitudePlannerDepartureEditorUiView(
    val title: String,
    val time_label: String,
    val time_value: String,
    val basis_label: String,
    val time_display_action_id: String,
    val when_label: String,
    val when_value: String,
    val when_suffix: String,
    val when_is_past: Boolean,
    val enabled: Boolean,
    val disabled_reason: String? = null,
)

@Serializable
data class WireAltitudePlannerUiView(
    val title: String,
    val estimate_kind: String,
    val estimate_summary: WireFlightPlanEstimateModeUiView,
    val controls: List<WireAltitudePlannerControlUiView>,
    val departure: WireAltitudePlannerDepartureEditorUiView,
    val forecast: WireAltitudePlannerForecastUiView? = null,
    val advisories: List<String> = emptyList(),
    val unavailable_reasons: List<WireAltitudePlannerUnavailableReason> = emptyList(),
)

@Serializable
data class WireFlightPlanEstimateModeUiView(
    val label: String,
    val estimate_kind: String,
)

@Serializable
enum class WireFlightPlanDisplayRowKind {
    @SerialName("waypoint")
    Waypoint,

    @SerialName("group")
    Group,

    @SerialName("discontinuity")
    Discontinuity,

    @SerialName("summary")
    Summary,
}

@Serializable
data class WireFlightPlanRowActionUiView(
    val id: String,
    val uid: String = "",
    val menu_column: Int = 0,
    val label: String,
    val enabled: Boolean,
    val disabled_reason: String? = null,
)

@Serializable
data class WireFlightPlanRowActionDecision(
    val perform_session_mutation: Boolean,
    val dismiss_tray: Boolean,
    val effect: WireFlightPlanRowActionEffect? = null,
)

@Serializable
data class WireFlightPlanRowActionEffect(
    val kind: String,
    val detail: WireWeatherDetailUiView? = null,
    val airport_id: String? = null,
    val target: String? = null,
    val row_uid: String? = null,
    val before: Boolean? = null,
    val origin_anchor: WireNavRef? = null,
    val destination_anchor: WireNavRef? = null,
    val procedure_kind: WireProcedureKind? = null,
)

@Serializable
data class WireAirportInfoUiView(
    val airport_id: String,
    val name: String,
    val location_label: String? = null,
    val elevation_label: String,
    val traffic_pattern_altitude_label: String,
    val traffic_pattern_altitude_source: String,
    val time_label: String,
    val time_display_action_id: String,
    val time_zone_label: String,
    val sunrise: WireAirportSolarEventUiView? = null,
    val sunset: WireAirportSolarEventUiView? = null,
    val communications: List<WireAirportCommunicationUiView> = emptyList(),
    val runway_diagram_complex: Boolean = false,
    val runways: List<WireAirportRunwayUiView> = emptyList(),
)

@Serializable
data class WireAirportSolarEventUiView(
    val time_label: String,
    val time_display_action_id: String,
    val next_in_label: String? = null,
)

@Serializable
data class WireAirportCommunicationUiView(
    val label: String,
    val value: String,
    val kind: String,
)

@Serializable
data class WireAirportRunwayUiView(
    val end_a_label: String,
    val end_b_label: String,
    val dimensions_label: String,
    val surface_label: String,
    val surface_color_key: String,
    val diagram_end_a_x: Double,
    val diagram_end_a_y: Double,
    val diagram_end_b_x: Double,
    val diagram_end_b_y: Double,
    val diagram_width_ratio: Double,
    val diagram_end_a_pattern: WireAirportRunwayPatternUiView? = null,
    val diagram_end_b_pattern: WireAirportRunwayPatternUiView? = null,
)

@Serializable
data class WireAirportRunwayPatternUiView(
    val base_x: Double,
    val base_y: Double,
    val corner_x: Double,
    val corner_y: Double,
    val final_x: Double,
    val final_y: Double,
)

typealias WireFlightDataCell = org.aerobag.app.generated.FlightDataCell
typealias WireFlightDataColumn = org.aerobag.app.generated.FlightDataColumn

@Serializable
data class WireFlightDataBannerModel(
    val cells: List<WireFlightDataCell> = emptyList(),
)

@Serializable
data class WireFlightPlanWeatherBadgeUiView(
    val flight_category: String,
    val ceiling_amount: String,
)

@Serializable
data class WireFlightPlanDisplayRowUiView(
    val uid: String = "",
    val label: String,
    val row_kind: WireFlightPlanDisplayRowKind,
    val component_kind: WireRouteComponentViewKind? = null,
    val component_uid: String? = null,
    val procedure_id: String? = null,
    val procedure_kind: WireProcedureKind? = null,
    val data_cells: List<WireFlightDataCell>,
    val show_plate_target_id: String? = null,
    val chart_airport_id: String? = null,
    val nav_ref: WireNavRef? = null,
    val symbol_feature: WireNavSymbolFeature? = null,
    val weather_badge: WireFlightPlanWeatherBadgeUiView? = null,
    val depth: Int,
    val active: Boolean,
    val enabled: Boolean = true,
    val disabled_reason: String? = null,
    val synthetic_direct_to: Boolean = false,
    val can_add_airway_after: Boolean,
    val can_add_procedure_before: Boolean,
    val can_remove_component: Boolean,
    val can_reorder_component: Boolean,
    val can_reorder_up: Boolean,
    val can_reorder_down: Boolean,
    val origin_anchor: WireNavRef? = null,
    val destination_anchor: WireNavRef? = null,
    val preceding_waypoint: WireNavRef? = null,
    val following_waypoint: WireNavRef? = null,
    val action_matrix: List<List<WireFlightPlanRowActionUiView>> = emptyList(),
)

@Serializable
data class WireFlightPlanEntryToken(
    val start: Int,
    val end: Int,
    val state: String,
)

@Serializable
data class WireFlightPlanEntryIssue(
    val start: Int,
    val end: Int,
    val message: String,
)

@Serializable
data class WireFlightPlanEntryPreview(
    val can_commit: Boolean,
    val tokens: List<WireFlightPlanEntryToken>,
    val issues: List<WireFlightPlanEntryIssue>,
)

@Serializable
data class WireMapViewportSeed(
    val lat: Double,
    val lon: Double,
    val zoom: Double,
)
