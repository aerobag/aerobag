package net.jonh.aerobag.prototype.domain

interface AppCoreAdapter {
    fun replaceFlightPlan(state: AppState, plan: FlightPlan): AppState
    fun setContentPolicy(state: AppState, policy: ContentPolicy): AppState
    fun refreshContent(state: AppState, inventory: ContentInventory): AppState
}

class MockAppCoreAdapter : AppCoreAdapter {
    override fun replaceFlightPlan(state: AppState, plan: FlightPlan): AppState {
        require(plan.legs.isNotEmpty()) { "Flight plan must contain at least one leg" }

        return state.copy(
            activePlan = plan,
            lastContentReport = null,
        )
    }

    override fun setContentPolicy(state: AppState, policy: ContentPolicy): AppState {
        return state.copy(contentPolicy = policy)
    }

    override fun refreshContent(state: AppState, inventory: ContentInventory): AppState {
        @Suppress("UNUSED_PARAMETER")
        val ignoredInventory = inventory
        @Suppress("UNUSED_VARIABLE")
        ignoredInventory
        return state.copy(lastContentReport = null)
    }
}
