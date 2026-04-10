package net.jonh.aerobag.prototype.domain

interface AppCoreAdapter {
    fun replaceFlightPlan(state: AppState, catalog: Catalog, plan: FlightPlan): AppState
    fun setContentPolicy(state: AppState, policy: ContentPolicy): AppState
    fun refreshContent(state: AppState, inventory: ContentInventory): AppState
}

class MockAppCoreAdapter : AppCoreAdapter {
    override fun replaceFlightPlan(state: AppState, catalog: Catalog, plan: FlightPlan): AppState {
        require(plan.legs.isNotEmpty()) { "Flight plan must contain at least one leg" }

        val packageIds = buildList {
            for (leg in plan.legs) {
                for (airport in listOfNotNull(airportCode(leg.from), airportCode(leg.to))) {
                    for (plate in catalog.plates) {
                        if (!plate.airportId.equals(airport, ignoreCase = true)) continue
                        val pkg = catalog.packages.firstOrNull { it.regionId == plate.regionId } ?: continue
                        add(pkg.id)
                    }
                }
            }
        }.distinct()

        return state.copy(
            activePlan = plan,
            lastContentRequirements = listOf(ContentRequirement(packageIds)),
            lastContentReport = null,
        )
    }

    override fun setContentPolicy(state: AppState, policy: ContentPolicy): AppState {
        return state.copy(contentPolicy = policy)
    }

    override fun refreshContent(state: AppState, inventory: ContentInventory): AppState {
        val items = state.lastContentRequirements.flatMap { requirement ->
            requirement.packageIds.map { pkg ->
                val installed = inventory.installedPackages.any {
                    it.integrityOk && it.packageId == pkg
                }
                val availability = when {
                    installed && state.contentPolicy == ContentPolicy.StreamAllowed -> ContentAvailability.LocalAndRemote
                    installed -> ContentAvailability.LocalOnly
                    state.contentPolicy == ContentPolicy.StreamAllowed -> ContentAvailability.RemoteOnly
                    else -> ContentAvailability.Unavailable
                }

                ContentReportItem(
                    label = pkg.packageName(),
                    availability = AvailabilityDetail(
                        availability = availability,
                        cycleCurrent = true,
                        integrityOk = installed,
                        cached = installed,
                        offlineUsable = installed,
                    ),
                )
            }
        }

        val fullySatisfied = items.all { item ->
            when (state.contentPolicy) {
                ContentPolicy.StreamAllowed -> item.availability.availability != ContentAvailability.Unavailable
                ContentPolicy.OfflineRequired, ContentPolicy.PreferLocal ->
                    item.availability.availability == ContentAvailability.LocalOnly ||
                        item.availability.availability == ContentAvailability.LocalAndRemote
            }
        }

        return state.copy(
            lastContentReport = ContentReport(
                fullySatisfied = fullySatisfied,
                items = items,
            ),
        )
    }
}

private fun airportCode(ref: NavRef): String? = when (ref) {
    is NavRef.Airport -> ref.code
    else -> null
}
