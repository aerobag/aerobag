package net.jonh.aerobag.prototype.domain

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
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.encodeToJsonElement
import kotlinx.serialization.json.jsonPrimitive

@Serializable
data class WireAppState(
    val active_plan: WireFlightPlan? = null,
    val content_policy: WireContentPolicy = WireContentPolicy.PreferLocal,
    val last_content_requirements: List<WireContentRequirement> = emptyList(),
    val last_content_report: WireContentReport? = null,
)

@Serializable
data class WireFlightPlan(
    val id: String,
    val name: String,
    val legs: List<WirePlanLeg>,
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

    data class Fix(val code: String) : WireNavRef

    data class LatLon(val value: WireLatLon) : WireNavRef
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
            is WireNavRef.Fix -> JsonObject(mapOf("Fix" to JsonPrimitive(value.code)))
            is WireNavRef.LatLon -> JsonObject(
                mapOf(
                    "LatLon" to encoder.json.encodeToJsonElement(WireLatLon.serializer(), value.value),
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
            "Fix" -> WireNavRef.Fix(value.jsonPrimitive.content)
            "LatLon" -> WireNavRef.LatLon(decoder.json.decodeFromJsonElement(WireLatLon.serializer(), value))
            else -> error("Unsupported NavRef variant: $kind")
        }
    }
}

@Serializable
enum class WireContentPolicy {
    OfflineRequired,

    PreferLocal,

    StreamAllowed,
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
    @SerialName("sectional")
    Sectional,
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
    val id: String,
    val family_id: WireChartFamilyId,
    val name: String,
    val display_name: String,
    val cycle: String,
    val region_ids: List<WireRegionId>,
    val max_zoom: Int,
    val tile_path_template: String,
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
    val asset_base_path: String,
)

@Serializable
data class WireSupplementRecord(
    val airport_id: String,
    val region_id: WireRegionId,
    val cycle: String,
    val page_count: Int,
    val asset_base_path: String,
)
