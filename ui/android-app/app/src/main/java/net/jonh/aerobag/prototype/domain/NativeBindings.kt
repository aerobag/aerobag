package net.jonh.aerobag.prototype.domain

interface NativeBridge {
    fun createUiSessionJson(
        catalogJson: String,
        resourceIndexJson: String,
        planJson: String,
        recentAirportIdsJson: String,
        selectedAirportIdJson: String,
        selectedChartIdJson: String,
    ): String

    fun removeLegInSessionJson(
        handle: Long,
        index: Int,
    ): String

    fun moveWaypointInSessionJson(
        handle: Long,
        waypointIndex: Int,
        delta: Int,
    ): String

    fun selectAirportInSessionJson(
        handle: Long,
        airportIdJson: String,
    ): String

    fun selectChartInSessionJson(
        handle: Long,
        chartIdJson: String,
    ): String

    fun getSessionSnapshotJson(handle: Long): String

    fun restoreChartPageStateInSessionJson(
        handle: Long,
        recentAirportIdsJson: String,
        selectedAirportIdJson: String,
        selectedChartIdJson: String,
    ): String

    fun destroySession(handle: Long)

    fun removeFlightPlanLegJson(
        planJson: String,
        index: Int,
    ): String

    fun replaceFlightPlanStateJson(
        stateJson: String,
        catalogJson: String,
        planJson: String,
    ): String

    fun setContentPolicyStateJson(
        stateJson: String,
        catalogJson: String,
        policyJson: String,
    ): String

    fun refreshContentStateJson(
        stateJson: String,
        catalogJson: String,
        inventoryJson: String,
    ): String

    fun chartForPositionJson(
        catalogJson: String,
        geometryJson: String,
        familyJson: String,
        lat: Double,
        lon: Double,
    ): String

    fun deriveChartPageJson(
        resourceIndexJson: String,
        planJson: String,
    ): String

    fun deriveChartPageStateJson(
        resourceIndexJson: String,
        planJson: String,
        recentAirportIdsJson: String,
        selectedAirportIdJson: String,
        selectedChartIdJson: String,
    ): String
}

object NativeBindings : NativeBridge {
    init {
        System.loadLibrary("app_ffi")
    }

    external override fun createUiSessionJson(
        catalogJson: String,
        resourceIndexJson: String,
        planJson: String,
        recentAirportIdsJson: String,
        selectedAirportIdJson: String,
        selectedChartIdJson: String,
    ): String

    external override fun removeLegInSessionJson(
        handle: Long,
        index: Int,
    ): String

    external override fun moveWaypointInSessionJson(
        handle: Long,
        waypointIndex: Int,
        delta: Int,
    ): String

    external override fun selectAirportInSessionJson(
        handle: Long,
        airportIdJson: String,
    ): String

    external override fun selectChartInSessionJson(
        handle: Long,
        chartIdJson: String,
    ): String

    external override fun getSessionSnapshotJson(handle: Long): String

    external override fun restoreChartPageStateInSessionJson(
        handle: Long,
        recentAirportIdsJson: String,
        selectedAirportIdJson: String,
        selectedChartIdJson: String,
    ): String

    external override fun destroySession(handle: Long)

    external override fun removeFlightPlanLegJson(
        planJson: String,
        index: Int,
    ): String

    external override fun replaceFlightPlanStateJson(
        stateJson: String,
        catalogJson: String,
        planJson: String,
    ): String

    external override fun setContentPolicyStateJson(
        stateJson: String,
        catalogJson: String,
        policyJson: String,
    ): String

    external override fun refreshContentStateJson(
        stateJson: String,
        catalogJson: String,
        inventoryJson: String,
    ): String

    external override fun chartForPositionJson(
        catalogJson: String,
        geometryJson: String,
        familyJson: String,
        lat: Double,
        lon: Double,
    ): String

    external override fun deriveChartPageJson(
        resourceIndexJson: String,
        planJson: String,
    ): String

    external override fun deriveChartPageStateJson(
        resourceIndexJson: String,
        planJson: String,
        recentAirportIdsJson: String,
        selectedAirportIdJson: String,
        selectedChartIdJson: String,
    ): String
}
