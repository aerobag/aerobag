package net.jonh.aerobag.prototype.domain

enum class ContentPolicy {
    OfflineRequired,
    PreferLocal,
    StreamAllowed,
}

enum class ContentAvailability {
    LocalOnly,
    RemoteOnly,
    LocalAndRemote,
    Unavailable,
}

data class PackageId(
    val region: String,
    val family: String,
    val cycle: String,
) {
    fun packageName(): String {
        val familyCode = when (family) {
            "sectional" -> "SEC"
            "ifr_low" -> "ENR_L"
            "ifr_high" -> "ENR_H"
            "ifr_area" -> "ENR_A"
            else -> family.uppercase()
        }
        return "${region.uppercase()}_$familyCode"
    }
}

data class FlightPlanLeg(
    val fromAirport: String,
    val toAirport: String,
    val airway: String? = null,
)

data class FlightPlan(
    val id: String,
    val name: String,
    val legs: List<FlightPlanLeg>,
    val departure: String?,
    val destination: String?,
    val alternate: String?,
    val cruiseAltitudeFt: Int?,
    val notes: String?,
    val updatedAtEpochMs: Long,
    val version: Long,
)

data class PlateRecord(
    val airportId: String,
    val regionId: String,
)

data class CatalogPackage(
    val id: PackageId,
    val packageName: String,
    val regionId: String,
)

data class Catalog(
    val cycle: String,
    val packages: List<CatalogPackage>,
    val plates: List<PlateRecord>,
)

enum class MapChartFamily {
    Sectional,
    Tac,
}

data class MapProbe(
    val family: MapChartFamily,
    val lat: Double,
    val lon: Double,
)

data class MapViewportSeed(
    val lat: Double,
    val lon: Double,
    val zoom: Double,
)

data class TileLevelAvailability(
    val zoom: Int,
    val xMin: Int,
    val xMax: Int,
    val yTmsMin: Int,
    val yTmsMax: Int,
)

data class MapView(
    val chartFamily: MapChartFamily,
    val chartName: String,
    val chartIndex: Int,
    val tileRoot: String,
    val tileSize: Int,
    val minZoom: Double,
    val maxZoom: Double,
    val initialViewport: MapViewportSeed,
    val levels: List<TileLevelAvailability>,
)

data class MapTileView(
    val chartFamily: MapChartFamily,
    val chartName: String,
    val chartIndex: Int,
    val tileRoot: String,
    val zoom: Int,
    val tileSize: Int,
    val radius: Int,
    val centerX: Int,
    val centerYTms: Int,
    val probeOffsetX: Double,
    val probeOffsetY: Double,
)

data class ChartLookupResult(
    val family: MapChartFamily,
    val name: String,
    val displayName: String,
)

data class InstalledPackage(
    val packageId: PackageId,
    val integrityOk: Boolean,
)

data class ContentInventory(
    val installedPackages: List<InstalledPackage>,
)

data class ContentRequirement(
    val packageIds: List<PackageId>,
)

data class AvailabilityDetail(
    val availability: ContentAvailability,
    val cycleCurrent: Boolean,
    val integrityOk: Boolean,
    val cached: Boolean,
    val offlineUsable: Boolean,
)

data class ContentReportItem(
    val label: String,
    val availability: AvailabilityDetail,
)

data class ContentReport(
    val fullySatisfied: Boolean,
    val items: List<ContentReportItem>,
)

data class AppState(
    val activePlan: FlightPlan? = null,
    val contentPolicy: ContentPolicy = ContentPolicy.PreferLocal,
    val lastContentRequirements: List<ContentRequirement> = emptyList(),
    val lastContentReport: ContentReport? = null,
)
