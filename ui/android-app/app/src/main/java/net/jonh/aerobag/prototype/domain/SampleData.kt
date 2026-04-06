package net.jonh.aerobag.prototype.domain

object SampleData {
    val catalog = Catalog(
        cycle = "2026-04-16",
        packages = listOf(
            CatalogPackage(
                id = PackageId(
                    region = "ne",
                    family = "sectional",
                    cycle = "2026-04-16",
                ),
                packageName = "NE_SEC",
                regionId = "ne",
            ),
        ),
        plates = listOf(
            PlateRecord(
                airportId = "KBOS",
                regionId = "ne",
            ),
        ),
    )

    val samplePlan = FlightPlan(
        id = "plan-1",
        name = "KBOS local",
        legs = listOf(
            FlightPlanLeg(
                fromAirport = "KBOS",
                toAirport = "KBOS",
            ),
        ),
        departure = "KBOS",
        destination = "KBOS",
        alternate = null,
        cruiseAltitudeFt = 3000,
        notes = "Prototype content sync scenario",
        updatedAtEpochMs = 0,
        version = 1,
    )

    val remoteOnlyInventory = ContentInventory(
        installedPackages = emptyList(),
    )

    val installedInventory = ContentInventory(
        installedPackages = listOf(
            InstalledPackage(
                packageId = PackageId(
                    region = "ne",
                    family = "sectional",
                    cycle = "2026-04-16",
                ),
                integrityOk = true,
            ),
        ),
    )
}
