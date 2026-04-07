package net.jonh.aerobag.prototype.domain

import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

interface MapLookupAdapter {
    fun chartForPosition(
        catalogJson: String,
        geometryJson: String,
        family: MapChartFamily,
        lat: Double,
        lon: Double,
    ): ChartLookupResult?
}

class NativeMapLookupAdapter(
    private val bridge: NativeBridge = NativeBindings,
    private val json: Json = Json {
        encodeDefaults = true
        ignoreUnknownKeys = true
    },
) : MapLookupAdapter {
    override fun chartForPosition(
        catalogJson: String,
        geometryJson: String,
        family: MapChartFamily,
        lat: Double,
        lon: Double,
    ): ChartLookupResult? {
        val resultJson = bridge.chartForPositionJson(
            catalogJson = catalogJson,
            geometryJson = geometryJson,
            familyJson = json.encodeToString(family.toWire()),
            lat = lat,
            lon = lon,
        )
        return json.decodeFromString<WireChartRecord?>(resultJson)?.toUi()
    }
}

class MockMapLookupAdapter(
    private val json: Json = Json {
        ignoreUnknownKeys = true
    },
) : MapLookupAdapter {
    override fun chartForPosition(
        catalogJson: String,
        geometryJson: String,
        family: MapChartFamily,
        lat: Double,
        lon: Double,
    ): ChartLookupResult? {
        val catalog = json.decodeFromString<WireCatalog>(catalogJson)
        val geometry = json.decodeFromString<WireGeometryBundle>(geometryJson)
        val points = geometry.polygons.firstOrNull()?.points ?: return null
        if (!pointInPolygon(lat, lon, points)) {
            return null
        }

        return catalog.charts.firstOrNull { it.family_id == family.toWire() }?.toUi()
    }
}

private fun MapChartFamily.toWire() = when (this) {
    MapChartFamily.Sectional -> WireChartFamilyId.Sectional
    MapChartFamily.Tac -> WireChartFamilyId.Tac
    MapChartFamily.IfrLow -> WireChartFamilyId.IfrLow
    MapChartFamily.IfrHigh -> WireChartFamilyId.IfrHigh
}

private fun WireChartRecord.toUi() = ChartLookupResult(
    family = when (family_id) {
        WireChartFamilyId.Sectional -> MapChartFamily.Sectional
        WireChartFamilyId.Tac -> MapChartFamily.Tac
        WireChartFamilyId.IfrLow -> MapChartFamily.IfrLow
        WireChartFamilyId.IfrHigh -> MapChartFamily.IfrHigh
    },
    name = name,
    displayName = display_name,
)

private fun pointInPolygon(lat: Double, lon: Double, points: List<List<Double>>): Boolean {
    var inside = false
    var previousIndex = points.lastIndex

    for (currentIndex in points.indices) {
        val current = points[currentIndex]
        val previous = points[previousIndex]
        val currentLon = current[0]
        val currentLat = current[1]
        val previousLon = previous[0]
        val previousLat = previous[1]
        val crossesLatitude = (currentLat > lat) != (previousLat > lat)

        if (crossesLatitude) {
            val interpolatedLon =
                previousLon + (currentLon - previousLon) * (lat - previousLat) / (currentLat - previousLat)
            if (lon < interpolatedLon) {
                inside = !inside
            }
        }

        previousIndex = currentIndex
    }

    return inside
}
