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
data class WireAppState(
    val active_plan: WireFlightPlan? = null,
    val ownship: WireOwnshipState = WireOwnshipState(),
    val content_policy: WireContentPolicy = WireContentPolicy.PreferLocal,
    val last_content_report: WireContentReport? = null,
)

@Serializable
data class WireAppUiState(
    val active_plan: WireFlightPlanUiState? = null,
    val ownship: WireOwnshipUiState = WireOwnshipUiState(),
    val flight_data_banner: WireFlightDataBannerModel = WireFlightDataBannerModel(),
    val content_policy: WireContentPolicy = WireContentPolicy.PreferLocal,
    val last_content_report: WireContentReport? = null,
)

@Serializable
data class WireUiSnapshotAppState(
    val active_plan: WireFlightPlan? = null,
    val content_policy: WireContentPolicy = WireContentPolicy.PreferLocal,
    val last_content_report: WireContentReport? = null,
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
    val active: Boolean,
    val status_label: String,
)

@Serializable
data class WireSituationControlMenuItem(
    val input: WireSituationControlInput,
    val label: String,
    val enabled: Boolean,
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
data class WireFlightPlan(
    val id: String,
    val name: String,
    val legs: List<WirePlanLeg>,
    val route_components: List<WireRouteComponent> = emptyList(),
    val route_component_uids: List<String> = emptyList(),
    val route_component_uid_counter: Long = 0,
    val resolved_legs: List<WireResolvedLeg> = emptyList(),
    val guidance: WireGuidanceState? = null,
    val departure: String? = null,
    val destination: String? = null,
    val alternate: String? = null,
    val cruise_altitude_ft: Int? = null,
    val notes: String? = null,
    val updated_at_epoch_ms: Long,
    val version: Long,
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
enum class WireContentPolicy {
    OfflineRequired,

    PreferLocal,

    StreamAllowed,
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
enum class WireContentAvailability {
    LocalOnly,

    RemoteOnly,

    LocalAndRemote,

    Unavailable,
}

@Serializable
enum class WireChartFamilyId {
    @SerialName("sec")
    Sec,

    @SerialName("tac")
    Tac,

    @SerialName("enr-l")
    EnrL,

    @SerialName("enr-h")
    EnrH,

    @SerialName("shaded-relief")
    ShadedRelief,

    @SerialName("world-basemap")
    WorldBasemap,
}

@Serializable
enum class WireTileStorageKind {
    @SerialName("asset_tree")
    AssetTree,

    @SerialName("sectional_package")
    SectionalPackage,

    @SerialName("static_product")
    StaticProduct,
}

@Serializable
enum class WireRegionId {
    @SerialName("ne")
    Ne,

    @SerialName("nc")
    Nc,

    @SerialName("nw")
    Nw,

    @SerialName("se")
    Se,

    @SerialName("sc")
    Sc,

    @SerialName("sw")
    Sw,

    @SerialName("ec")
    Ec,

    @SerialName("ak")
    Ak,

    @SerialName("pac")
    Pac,

    @SerialName("world")
    World,
}

@Serializable
data class WirePackageId(
    val region: WireRegionId,
    val family: WireChartFamilyId,
    val cycle: String,
)

@Serializable
data class WireInstalledPackage(
    val package_id: WirePackageId,
    val integrity_ok: Boolean,
)

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
    val style_class: String,
    val obstacle_variant: String? = null,
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

@Serializable
data class WireNavSymbolFeature(
    val kind: String,
    val label: String,
    val style_class: String,
    val obstacle_variant: String? = null,
    val towered: Boolean,
    val fuel_available: Boolean,
    val has_paved_runway: Boolean? = null,
    val heliport: Boolean? = null,
    val has_water_runway: Boolean? = null,
    val runway_length_ratio: Double,
    val longest_runway_heading_true_deg: Double? = null,
)

@Serializable
data class WireMapOverlayQueryResult(
    val visible_features: List<WireVisibleMapFeature>,
    val flight_plan_features: List<WireVisibleMapFeature> = emptyList(),
    val visible_metars: List<WireVisibleMetarFeature> = emptyList(),
    val visible_pireps: List<WireVisiblePirepFeature> = emptyList(),
    val airspace_paths: List<WireAirspaceDisplayPath> = emptyList(),
    val tfr_paths: List<WireAirspaceDisplayPath> = emptyList(),
    val airspace_labels: List<WireAirspaceDisplayLabel> = emptyList(),
    val offline_regions: List<WireOfflineRegionDisplay> = emptyList(),
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
    val detail_text: String? = null,
    val highlight: @Serializable(with = WireMapSelectionHighlightSerializer::class) WireMapSelectionHighlight,
    val nav_ref: WireNavRef? = null,
    val symbol_feature: WireNavSymbolFeature? = null,
    val metar_feature: WireVisibleMetarFeature? = null,
    val pirep_feature: WireVisiblePirepFeature? = null,
    val airspace_icon: WireAirspaceDisplayPath? = null,
    val actions: List<WireMapSelectionAction> = emptyList(),
)

@Serializable(with = WireMapSelectionHighlightSerializer::class)
sealed interface WireMapSelectionHighlight {
    data class FeatureRef(val id: String) : WireMapSelectionHighlight
    data class Metar(val station_id: String) : WireMapSelectionHighlight
    data class Pirep(val id: String) : WireMapSelectionHighlight
    data class OfflineRegion(val id: String) : WireMapSelectionHighlight
    data class Spot(val lat: Double, val lon: Double) : WireMapSelectionHighlight
}

object WireMapSelectionHighlightSerializer : JsonContentPolymorphicSerializer<WireMapSelectionHighlight>(WireMapSelectionHighlight::class) {
    override fun selectDeserializer(element: kotlinx.serialization.json.JsonElement): kotlinx.serialization.DeserializationStrategy<WireMapSelectionHighlight> {
        return when (element.jsonObject["kind"]?.jsonPrimitive?.content) {
            "feature_ref" -> WireMapSelectionHighlightFeatureRef.serializer()
            "metar" -> WireMapSelectionHighlightMetar.serializer()
            "pirep" -> WireMapSelectionHighlightPirep.serializer()
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
    val detail_text: String? = null,
    val airspace_limit: WireAirspaceLimitGlyph? = null,
    val session_action: String? = null,
    val flight_plan_row_action: WireMapSelectionFlightPlanRowAction? = null,
)

@Serializable
data class WireMapSelectionFlightPlanRowAction(
    val row_uid: String,
    val action_uid: String,
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

@Serializable
data class WireWaypointIdentifierSuggestion(
    val identifier: String,
    val nav_ref: WireNavRef,
    val kind: String,
    val display_name: String,
    val distance_from_anchor_nm: Double,
)

@Serializable
data class WireAirwayEntryCandidate(
    val airway_name: String,
    val branch_key: String,
    val branch_point_index: Int,
    val sequence: Int,
    val nav_ref: WireNavRef,
    val distance_from_anchor_nm: Double,
    val previous_nav_ref: WireNavRef? = null,
    val next_nav_ref: WireNavRef? = null,
)

@Serializable
data class WireAirwayExitCandidate(
    val airway_name: String,
    val branch_key: String,
    val branch_point_index: Int,
    val sequence: Int,
    val nav_ref: WireNavRef,
    val leg_offset_from_entry: Int,
    val is_entry: Boolean,
    val distance_from_target_nm: Double? = null,
)

@Serializable
data class WireAirwayAutoSelection(
    val airway_name: String,
    val branch_key: String,
    val entry: WireAirwayEntryCandidate,
    val exit: WireAirwayExitCandidate,
    val origin_distance_nm: Double,
    val destination_distance_nm: Double,
    val total_anchor_distance_nm: Double,
)

@Serializable
data class WireAirwaySegment(
    val name: String,
    val branch_key: String? = null,
    val entry: WireNavRef,
    val exit: WireNavRef,
)

@Serializable
data class WireAirwayFixPoint(
    val airway_name: String,
    val sequence: Int,
    val position: WireLatLon,
    val nav_ref: WireNavRef,
)

@Serializable
data class WireAirwayBranch(
    val display_name: String,
    val branch_key: String,
    val points: List<WireAirwayFixPoint>,
)

@Serializable
data class WireAirwayPresentationPoint(
    val branch_point_index: Int,
    val sequence: Int,
    val nav_ref: WireNavRef,
)

@Serializable
data class WireAirwayPresentationPlan(
    val airway_name: String,
    val branch_key: String,
    val points: List<WireAirwayPresentationPoint>,
    val suggested_entry_index: Int,
    val suggested_exit_index: Int? = null,
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

@Serializable(with = WireProcedureDiscontinuitySerializer::class)
sealed interface WireProcedureDiscontinuity {
    data object Vectors : WireProcedureDiscontinuity
    data object Hold : WireProcedureDiscontinuity
    data class Other(val value: String) : WireProcedureDiscontinuity
}

object WireProcedureDiscontinuitySerializer : KSerializer<WireProcedureDiscontinuity> {
    override val descriptor: SerialDescriptor =
        PrimitiveSerialDescriptor("WireProcedureDiscontinuity", PrimitiveKind.STRING)

    override fun serialize(encoder: Encoder, value: WireProcedureDiscontinuity) {
        val text = when (value) {
            WireProcedureDiscontinuity.Vectors -> "vectors"
            WireProcedureDiscontinuity.Hold -> "hold"
            is WireProcedureDiscontinuity.Other -> value.value
        }
        encoder.encodeString(text)
    }

    override fun deserialize(decoder: Decoder): WireProcedureDiscontinuity =
        when (val text = decoder.decodeString()) {
            "vectors" -> WireProcedureDiscontinuity.Vectors
            "hold" -> WireProcedureDiscontinuity.Hold
            else -> WireProcedureDiscontinuity.Other(text)
        }
}

@Serializable
data class WireProcedureSegment(
    val airport_id: String,
    val procedure_id: String,
    val kind: WireProcedureKind,
    val runway_transition: String? = null,
    val enroute_transition: String? = null,
    val terminal_discontinuity: WireProcedureDiscontinuity? = null,
)

@Serializable
data class WireProcedureSummary(
    val airport_id: String,
    val procedure_id: String,
    val kind: WireProcedureKind,
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

@Serializable(with = WireResolvedLegSourceSerializer::class)
sealed interface WireResolvedLegSource {
    data class RouteComponent(val component_index: Int) : WireResolvedLegSource
    data class SyntheticBridge(val from_component_index: Int, val to_component_index: Int) : WireResolvedLegSource
}

object WireResolvedLegSourceSerializer : KSerializer<WireResolvedLegSource> {
    override val descriptor: SerialDescriptor =
        PrimitiveSerialDescriptor("WireResolvedLegSource", PrimitiveKind.STRING)

    override fun serialize(encoder: Encoder, value: WireResolvedLegSource) {
        require(encoder is JsonEncoder) { "WireResolvedLegSource is JSON-only" }
        val element = when (value) {
            is WireResolvedLegSource.RouteComponent -> JsonObject(
                mapOf("kind" to JsonPrimitive("route_component"), "component_index" to JsonPrimitive(value.component_index)),
            )
            is WireResolvedLegSource.SyntheticBridge -> JsonObject(
                mapOf(
                    "kind" to JsonPrimitive("synthetic_bridge"),
                    "from_component_index" to JsonPrimitive(value.from_component_index),
                    "to_component_index" to JsonPrimitive(value.to_component_index),
                ),
            )
        }
        encoder.encodeJsonElement(element)
    }

    override fun deserialize(decoder: Decoder): WireResolvedLegSource {
        require(decoder is JsonDecoder) { "WireResolvedLegSource is JSON-only" }
        val obj = decoder.decodeJsonElement() as? JsonObject ?: error("WireResolvedLegSource must be an object")
        return when (obj["kind"]?.jsonPrimitive?.content) {
            "route_component" -> WireResolvedLegSource.RouteComponent(
                component_index = obj["component_index"]?.jsonPrimitive?.content?.toInt() ?: error("component_index required"),
            )
            "synthetic_bridge" -> WireResolvedLegSource.SyntheticBridge(
                from_component_index = obj["from_component_index"]?.jsonPrimitive?.content?.toInt() ?: error("from_component_index required"),
                to_component_index = obj["to_component_index"]?.jsonPrimitive?.content?.toInt() ?: error("to_component_index required"),
            )
            else -> error("Unsupported WireResolvedLegSource")
        }
    }
}

@Serializable
data class WireResolvedLeg(
    val id: String,
    val from: WireNavRef,
    val to: WireNavRef,
    val procedure_airport_id: String? = null,
    val source: WireResolvedLegSource,
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
enum class WireSequencingMode {
    @SerialName("follow_plan")
    FollowPlan,

    @SerialName("suspended")
    Suspended,

    @SerialName("direct_to")
    DirectTo,
}

@Serializable
data class WireDirectToState(
    val start: WireNavRef,
    val target: WireNavRef,
    val target_component_uid: String? = null,
    val target_leg_id: String? = null,
    val resume_leg_id: String? = null,
)

@Serializable
data class WireGuidanceState(
    val active_leg_index: Int,
    val active_detail_index: Int? = null,
    val display_split_leg_id: String? = null,
    val sequencing_mode: WireSequencingMode,
    val direct_to: WireDirectToState? = null,
    val suspend_reason: WireSuspendReason? = null,
)

@Serializable
enum class WireSuspendReason {
    @SerialName("manual")
    Manual,

    @SerialName("boundary")
    Boundary,

    @SerialName("route_end")
    RouteEnd,

    @SerialName("direct_to_complete")
    DirectToComplete,
}

@Serializable(with = WireRouteComponentSerializer::class)
sealed interface WireRouteComponent {
    data class Waypoint(val waypoint: WireNavRef) : WireRouteComponent
    data class Airway(val airway: WireAirwaySegment) : WireRouteComponent
    data class Procedure(val procedure: WireProcedureSegment) : WireRouteComponent
}

object WireRouteComponentSerializer : KSerializer<WireRouteComponent> {
    override val descriptor: SerialDescriptor =
        PrimitiveSerialDescriptor("WireRouteComponent", PrimitiveKind.STRING)

    override fun serialize(encoder: Encoder, value: WireRouteComponent) {
        require(encoder is JsonEncoder) { "WireRouteComponent is JSON-only" }
        val element = when (value) {
            is WireRouteComponent.Waypoint -> JsonObject(
                mapOf(
                    "kind" to JsonPrimitive("waypoint"),
                    "waypoint" to encoder.json.encodeToJsonElement(WireNavRefSerializer, value.waypoint),
                ),
            )
            is WireRouteComponent.Airway -> JsonObject(
                mapOf(
                    "kind" to JsonPrimitive("airway"),
                    "airway" to encoder.json.encodeToJsonElement(WireAirwaySegment.serializer(), value.airway),
                ),
            )
            is WireRouteComponent.Procedure -> JsonObject(
                mapOf(
                    "kind" to JsonPrimitive("procedure"),
                    "procedure" to encoder.json.encodeToJsonElement(WireProcedureSegment.serializer(), value.procedure),
                ),
            )
        }
        encoder.encodeJsonElement(element)
    }

    override fun deserialize(decoder: Decoder): WireRouteComponent {
        require(decoder is JsonDecoder) { "WireRouteComponent is JSON-only" }
        val obj = decoder.decodeJsonElement() as? JsonObject ?: error("WireRouteComponent must be an object")
        return when (obj["kind"]?.jsonPrimitive?.content) {
            "waypoint" -> WireRouteComponent.Waypoint(
                waypoint = decoder.json.decodeFromJsonElement(WireNavRefSerializer, obj["waypoint"] ?: error("waypoint required")),
            )
            "airway" -> WireRouteComponent.Airway(
                airway = decoder.json.decodeFromJsonElement(WireAirwaySegment.serializer(), obj["airway"] ?: error("airway required")),
            )
            "procedure" -> WireRouteComponent.Procedure(
                procedure = decoder.json.decodeFromJsonElement(WireProcedureSegment.serializer(), obj["procedure"] ?: error("procedure required")),
            )
            else -> error("Unsupported WireRouteComponent")
        }
    }
}

@Serializable(with = WireConcretizedNavItemSerializer::class)
sealed interface WireConcretizedNavItem {
    data class Waypoint(val nav_ref: WireNavRef) : WireConcretizedNavItem
    data class Discontinuity(val discontinuity: WireProcedureDiscontinuity, val label: String) : WireConcretizedNavItem
}

object WireConcretizedNavItemSerializer : KSerializer<WireConcretizedNavItem> {
    override val descriptor: SerialDescriptor =
        PrimitiveSerialDescriptor("WireConcretizedNavItem", PrimitiveKind.STRING)

    override fun serialize(encoder: Encoder, value: WireConcretizedNavItem) {
        require(encoder is JsonEncoder) { "WireConcretizedNavItem is JSON-only" }
        val element = when (value) {
            is WireConcretizedNavItem.Waypoint -> JsonObject(
                mapOf(
                    "kind" to JsonPrimitive("waypoint"),
                    "nav_ref" to encoder.json.encodeToJsonElement(WireNavRefSerializer, value.nav_ref),
                ),
            )
            is WireConcretizedNavItem.Discontinuity -> JsonObject(
                mapOf(
                    "kind" to JsonPrimitive("discontinuity"),
                    "discontinuity" to encoder.json.encodeToJsonElement(WireProcedureDiscontinuitySerializer, value.discontinuity),
                    "label" to JsonPrimitive(value.label),
                ),
            )
        }
        encoder.encodeJsonElement(element)
    }

    override fun deserialize(decoder: Decoder): WireConcretizedNavItem {
        require(decoder is JsonDecoder) { "WireConcretizedNavItem is JSON-only" }
        val obj = decoder.decodeJsonElement() as? JsonObject ?: error("WireConcretizedNavItem must be an object")
        return when (obj["kind"]?.jsonPrimitive?.content) {
            "waypoint" -> WireConcretizedNavItem.Waypoint(
                nav_ref = decoder.json.decodeFromJsonElement(WireNavRefSerializer, obj["nav_ref"] ?: error("nav_ref required")),
            )
            "discontinuity" -> WireConcretizedNavItem.Discontinuity(
                discontinuity = decoder.json.decodeFromJsonElement(WireProcedureDiscontinuitySerializer, obj["discontinuity"] ?: error("discontinuity required")),
                label = obj["label"]?.jsonPrimitive?.content ?: error("label required"),
            )
            else -> error("Unsupported WireConcretizedNavItem")
        }
    }
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
data class WireRouteComponentUiView(
    val uid: String,
    val component_index: Int,
    val kind: WireRouteComponentViewKind,
    val summary: String,
    val procedure_id: String? = null,
    val procedure_kind: WireProcedureKind? = null,
    val chart_airport_id: String? = null,
    val items: List<WireConcretizedNavItem>,
    val active: Boolean,
    val can_add_airway_after: Boolean,
    val can_add_procedure_before: Boolean,
    val can_remove: Boolean,
    val can_reorder: Boolean,
    val can_reorder_up: Boolean,
    val can_reorder_down: Boolean,
    val preceding_waypoint: WireNavRef? = null,
    val following_waypoint: WireNavRef? = null,
)

@Serializable
data class WireResolvedLegUiView(
    val leg_index: Int,
    val leg_id: String,
    val component_index: Int? = null,
    val from: WireNavRef,
    val to: WireNavRef,
    val active: Boolean,
    val suspend_boundary_after: Boolean,
)

@Serializable
data class WireDirectToUiView(
    val start: WireNavRef,
    val target: WireNavRef,
    val target_component_uid: String? = null,
    val target_leg_id: String? = null,
    val resume_leg_id: String? = null,
    val on_plan_target: Boolean,
)

@Serializable
data class WireGuidanceUiView(
    val sequencing_mode: WireSequencingMode,
    val active_leg_index: Int? = null,
    val display_split_leg_index: Int? = null,
    val active_from_row_uid: String? = null,
    val active_to_row_uid: String? = null,
    val active_component_index: Int? = null,
    val active_leg: WirePlanLeg? = null,
    val nav_element: WireNavElementUiView = WireNavElementUiView(),
    val direct_to: WireDirectToUiView? = null,
    val can_sequence_active_leg: Boolean = false,
    val can_activate_next_leg: Boolean = false,
    val can_suspend: Boolean = false,
    val can_unsuspend: Boolean = false,
    val suspend_boundary_after_active_leg: Boolean = false,
)

@Serializable
data class WireNavElementUiView(
    val active_leg_summary: String = "",
    val cdi_indicator_dots: Float? = null,
    val cdi_offscale_readout: String? = null,
)

@Serializable
data class WireFlightPlanUiState(
    val components: List<WireRouteComponentUiView>,
    val resolved_legs: List<WireResolvedLegUiView>,
    val display_rows: List<WireFlightPlanDisplayRowUiView>,
    val data_columns: List<WireFlightDataColumn>,
    val guidance: WireGuidanceUiView? = null,
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
    val label: String,
    val enabled: Boolean,
    val execution: String = "ui_controller",
    val dismiss_tray_on_success: Boolean = true,
)

@Serializable
data class WireFlightDataCell(
    val id: String,
    val label: String,
    val value: String? = null,
    val tone: String = "normal",
)

@Serializable
data class WireFlightDataColumn(
    val id: String,
    val label: String,
)

@Serializable
data class WireFlightDataBannerModel(
    val cells: List<WireFlightDataCell> = emptyList(),
)

@Serializable
data class WireFlightPlanDisplayRowUiView(
    val uid: String = "",
    val label: String,
    val row_kind: WireFlightPlanDisplayRowKind,
    val component_kind: WireRouteComponentViewKind? = null,
    val component_uid: String? = null,
    val component_index: Int? = null,
    val procedure_id: String? = null,
    val procedure_kind: WireProcedureKind? = null,
    val leg_index: Int? = null,
    val data_cells: List<WireFlightDataCell>,
    val show_plate_target_id: String? = null,
    val chart_airport_id: String? = null,
    val nav_ref: WireNavRef? = null,
    val symbol_feature: WireNavSymbolFeature? = null,
    val depth: Int,
    val active: Boolean,
    val enabled: Boolean = true,
    val synthetic_direct_to: Boolean = false,
    val can_add_airway_after: Boolean,
    val can_add_procedure_before: Boolean,
    val can_remove_component: Boolean,
    val can_reorder_component: Boolean,
    val can_reorder_up: Boolean,
    val can_reorder_down: Boolean,
    val replace_procedure_component_index: Int? = null,
    val start_component_index: Int? = null,
    val end_component_index: Int? = null,
    val origin_anchor: WireNavRef? = null,
    val destination_anchor: WireNavRef? = null,
    val preceding_waypoint: WireNavRef? = null,
    val following_waypoint: WireNavRef? = null,
    val action_matrix: List<List<WireFlightPlanRowActionUiView>> = emptyList(),
)

@Serializable
data class WireFlightPlanUiMutation(
    val plan: WireFlightPlan,
    val ui_state: WireFlightPlanUiState,
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
data class WireAirwayPlanMutation(
    val plan: WireFlightPlan,
    val selection: WireAirwayAutoSelection,
    val airway: WireAirwaySegment,
    val resolved_legs: List<WireResolvedLeg>,
)

@Serializable
data class WireMaterializedAirway(
    val selection: WireAirwayAutoSelection,
    val airway: WireAirwaySegment,
    val resolvedLegs: List<WireResolvedLeg>,
)

@Serializable
data class WireAirwayPlanUiMutation(
    val mutation: WireAirwayPlanMutation,
    val ui_state: WireFlightPlanUiState,
)

@Serializable
data class WireProcedurePlanMutation(
    val plan: WireFlightPlan,
    val component_index: Int,
    val procedure: WireProcedureSegment,
    val concretized_items: List<WireConcretizedNavItem>,
    val resolved_legs: List<WireResolvedLeg>,
)

@Serializable
data class WireProcedurePlanUiMutation(
    val mutation: WireProcedurePlanMutation,
    val ui_state: WireFlightPlanUiState,
)

@Serializable
data class WireMaterializedProcedure(
    val procedure: WireProcedureSegment,
    val concretized_items: List<WireConcretizedNavItem>,
    val resolved_legs: List<WireResolvedLeg>,
)

@Serializable
data class WireContentInventory(
    val installed_packages: List<WireInstalledPackage>,
    val cached_tilesets: List<WireCachedTileset> = emptyList(),
    val cached_plates: List<WireCachedPlate> = emptyList(),
)

@Serializable
data class WireCachedTileset(
    val chart_id: String,
    val fully_cached: Boolean,
)

@Serializable
data class WireCachedPlate(
    val plate_id: String,
    val cached_pages: List<Int>,
)

@Serializable
data class WireContentRequirement(
    val package_ids: List<WirePackageId>,
    val chart_ids: List<String> = emptyList(),
    val plate_ids: List<String> = emptyList(),
)

@Serializable
data class WireAvailabilityDetail(
    val availability: WireContentAvailability,
    val cycle_current: Boolean,
    val integrity_ok: Boolean,
    val cached: Boolean,
    val offline_usable: Boolean,
)

@Serializable
data class WireContentReportItem(
    val label: String,
    val availability: WireAvailabilityDetail,
)

@Serializable
data class WireContentReport(
    val fully_satisfied: Boolean,
    val items: List<WireContentReportItem>,
)

@Serializable
data class WireCatalog(
    val schema_version: Int,
    val cycle: String,
    val catalog_revision: String,
    val families: List<WireCatalogFamily>,
    val regions: List<WireCatalogRegion>,
    val packages: List<WireCatalogPackage>,
    val charts: List<WireChartRecord> = emptyList(),
    val plates: List<WirePlateRecord> = emptyList(),
    val supplements: List<WireSupplementRecord> = emptyList(),
)

@Serializable
data class WireCatalogFamily(
    val id: WireChartFamilyId,
    val display_name: String,
    val kind: String,
    val max_zoom: Int? = null,
    val tile_size: Int? = null,
)

@Serializable
data class WireCatalogRegion(
    val id: WireRegionId,
    val display_name: String,
    val sort_order: Int,
)

@Serializable
data class WireCatalogPackage(
    val id: WirePackageId,
    val package_name: String,
    val family_id: WireChartFamilyId,
    val region_id: WireRegionId,
    val cycle: String,
    val artifact_kind: String,
    val relative_url: String,
    val manifest_name: String,
    val size_bytes: Long? = null,
    val checksum_sha256: String? = null,
)

@Serializable
data class WireChartRecord(
    val id: WireChartId,
    val family_id: WireChartFamilyId,
    val name: String,
    val display_name: String,
    val cycle: String,
    val region_ids: List<WireRegionId>,
    val max_zoom: Int,
    val tile_path_template: String,
)

@Serializable
data class WireChartId(
    val family: WireChartFamilyId,
    val name: String,
    val cycle: String,
)


@Serializable
data class WireGeometryBundle(
    val schema_version: Int,
    val polygons: List<WirePolygonRecord>,
)

@Serializable
data class WirePolygonRecord(
    val id: String,
    val points: List<List<Double>>,
)

@Serializable
data class WireInitialProbe(
    val family: WireChartFamilyId,
    val lat: Double,
    val lon: Double,
)

@Serializable
data class WireMapViewportSeed(
    val lat: Double,
    val lon: Double,
    val zoom: Double,
)

@Serializable
data class WirePlateId(
    val airport_id: String,
    val procedure_code: String,
    val page: Int,
    val cycle: String,
)

@Serializable
data class WirePlateRecord(
    val id: WirePlateId,
    val airport_id: String,
    val region_id: WireRegionId,
    val cycle: String,
    val procedure_code: String,
    val display_name: String,
    val kind: String,
    val georeferenced: Boolean,
    val page_count: Int,
)

@Serializable
data class WireSupplementRecord(
    val airport_id: String,
    val region_id: WireRegionId,
    val cycle: String,
    val page_count: Int,
)
