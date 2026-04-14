package net.jonh.aerobag.prototype.domain

interface NativeBridge {
    fun suggestAirwaysNearJson(
        dbPath: String,
        anchorJson: String,
        limit: Int,
    ): String

    fun resolveNavRefPositionJson(
        dbPath: String,
        navRefJson: String,
    ): String

    fun resolveNavRefPositionWithAirportJson(
        dbPath: String,
        navRefJson: String,
        airportIdJson: String,
    ): String

    fun projectFlightPlanRouteJson(
        dbPath: String,
        planJson: String,
    ): String

    fun loadAirwayBranchesJson(
        dbPath: String,
        airwayName: String,
    ): String

    fun listAirwayEntryCandidatesJson(
        dbPath: String,
        airwayName: String,
        originAnchorJson: String,
    ): String

    fun listAirwayExitCandidatesJson(
        dbPath: String,
        airwayName: String,
        entryJson: String,
        destinationAnchorJson: String,
    ): String

    fun listProceduresJson(
        dbPath: String,
        airportId: String,
        kindJson: String,
    ): String

    fun describeProcedureOptionsJson(
        dbPath: String,
        airportId: String,
        procedureId: String,
        kindJson: String,
    ): String

    fun materializeProcedureSelectionJson(
        dbPath: String,
        airportId: String,
        procedureId: String,
        kindJson: String,
        runwayTransitionJson: String,
        enrouteTransitionJson: String,
        componentIndex: Int,
    ): String

    fun buildFlightPlanUiJson(planJson: String): String

    fun activateLegUiJson(planJson: String, legIndex: Int): String

    fun activateNextLegUiJson(planJson: String): String

    fun deleteComponentUiJson(planJson: String, componentIndex: Int): String

    fun moveComponentUiJson(planJson: String, componentIndex: Int, delta: Int): String

    fun suspendSequencingUiJson(planJson: String): String

    fun unsuspendSequencingUiJson(planJson: String): String

    fun sequenceActiveLegUiJson(planJson: String): String

    fun prepareAirwayPresentationJson(
        airwayName: String,
        branchesJson: String,
        originPositionJson: String,
        destinationPositionJson: String,
    ): String

    fun insertAirwayFromSelectionUiJson(
        dbPath: String,
        planJson: String,
        startComponentIndex: Int,
        endComponentIndex: Int,
        entryJson: String,
        exitJson: String,
    ): String

    fun replaceAirwayFromSelectionUiJson(
        dbPath: String,
        planJson: String,
        componentIndex: Int,
        entryJson: String,
        exitJson: String,
    ): String

    fun sortAirwaySuggestionsForUiJson(suggestionsJson: String): String

    fun insertAirwayMaterializedUiJson(
        planJson: String,
        startComponentIndex: Int,
        endComponentIndexJson: String,
        selectionJson: String,
        airwayJson: String,
        resolvedLegsJson: String,
    ): String

    fun replaceAirwayMaterializedUiJson(
        planJson: String,
        componentIndex: Int,
        selectionJson: String,
        airwayJson: String,
        resolvedLegsJson: String,
    ): String

    fun insertProcedureMaterializedUiJson(
        planJson: String,
        startComponentIndex: Int,
        endComponentIndex: Int,
        builtJson: String,
    ): String

    fun replaceProcedureMaterializedUiJson(
        planJson: String,
        componentIndex: Int,
        builtJson: String,
    ): String

    fun describeProcedureOptionsFromRowsJson(
        airportId: String,
        procedureId: String,
        kindJson: String,
        rowsJson: String,
    ): String

    fun materializeProcedureFromRecordsJson(
        airportId: String,
        procedureId: String,
        kindJson: String,
        runwayTransitionJson: String,
        enrouteTransitionJson: String,
        componentIndex: Int,
        rowsJson: String,
        legsJson: String,
    ): String

    fun createUiSessionJson(
        catalogJson: String,
        chartCatalogJson: String,
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

    fun registerOwnshipSourceInSessionJson(
        handle: Long,
        registrationJson: String,
    ): String

    fun updateOwnshipSourceStatusInSessionJson(
        handle: Long,
        updateJson: String,
    ): String

    fun pushSituationSampleInSessionJson(
        handle: Long,
        sampleJson: String,
    ): String

    fun setOwnshipPolicyInSessionJson(
        handle: Long,
        policyJson: String,
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

    fun replaceFlightPlanInSessionJson(
        handle: Long,
        planJson: String,
    ): String

    fun restoreChartPageStateInSessionJson(
        handle: Long,
        recentAirportIdsJson: String,
        selectedAirportIdJson: String,
        selectedChartIdJson: String,
    ): String

    fun ingestPointTilesInSessionJson(
        handle: Long,
        tilesJson: String,
    ): String

    fun getMapOverlayInSessionJson(
        handle: Long,
        viewportJson: String,
        widthPx: Double,
        heightPx: Double,
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

    external override fun suggestAirwaysNearJson(
        dbPath: String,
        anchorJson: String,
        limit: Int,
    ): String

    external override fun resolveNavRefPositionJson(
        dbPath: String,
        navRefJson: String,
    ): String

    external override fun resolveNavRefPositionWithAirportJson(
        dbPath: String,
        navRefJson: String,
        airportIdJson: String,
    ): String

    external override fun projectFlightPlanRouteJson(
        dbPath: String,
        planJson: String,
    ): String

    external override fun loadAirwayBranchesJson(
        dbPath: String,
        airwayName: String,
    ): String

    external override fun listAirwayEntryCandidatesJson(
        dbPath: String,
        airwayName: String,
        originAnchorJson: String,
    ): String

    external override fun listAirwayExitCandidatesJson(
        dbPath: String,
        airwayName: String,
        entryJson: String,
        destinationAnchorJson: String,
    ): String

    external override fun listProceduresJson(
        dbPath: String,
        airportId: String,
        kindJson: String,
    ): String

    external override fun describeProcedureOptionsJson(
        dbPath: String,
        airportId: String,
        procedureId: String,
        kindJson: String,
    ): String

    external override fun materializeProcedureSelectionJson(
        dbPath: String,
        airportId: String,
        procedureId: String,
        kindJson: String,
        runwayTransitionJson: String,
        enrouteTransitionJson: String,
        componentIndex: Int,
    ): String

    external override fun buildFlightPlanUiJson(planJson: String): String

    external override fun activateLegUiJson(planJson: String, legIndex: Int): String

    external override fun activateNextLegUiJson(planJson: String): String

    external override fun deleteComponentUiJson(planJson: String, componentIndex: Int): String

    external override fun moveComponentUiJson(planJson: String, componentIndex: Int, delta: Int): String

    external override fun suspendSequencingUiJson(planJson: String): String

    external override fun unsuspendSequencingUiJson(planJson: String): String

    external override fun sequenceActiveLegUiJson(planJson: String): String

    external override fun prepareAirwayPresentationJson(
        airwayName: String,
        branchesJson: String,
        originPositionJson: String,
        destinationPositionJson: String,
    ): String

    external override fun insertAirwayFromSelectionUiJson(
        dbPath: String,
        planJson: String,
        startComponentIndex: Int,
        endComponentIndex: Int,
        entryJson: String,
        exitJson: String,
    ): String

    external override fun replaceAirwayFromSelectionUiJson(
        dbPath: String,
        planJson: String,
        componentIndex: Int,
        entryJson: String,
        exitJson: String,
    ): String

    external override fun sortAirwaySuggestionsForUiJson(suggestionsJson: String): String

    external override fun insertAirwayMaterializedUiJson(
        planJson: String,
        startComponentIndex: Int,
        endComponentIndexJson: String,
        selectionJson: String,
        airwayJson: String,
        resolvedLegsJson: String,
    ): String

    external override fun replaceAirwayMaterializedUiJson(
        planJson: String,
        componentIndex: Int,
        selectionJson: String,
        airwayJson: String,
        resolvedLegsJson: String,
    ): String

    external override fun insertProcedureMaterializedUiJson(
        planJson: String,
        startComponentIndex: Int,
        endComponentIndex: Int,
        builtJson: String,
    ): String

    external override fun replaceProcedureMaterializedUiJson(
        planJson: String,
        componentIndex: Int,
        builtJson: String,
    ): String

    external override fun describeProcedureOptionsFromRowsJson(
        airportId: String,
        procedureId: String,
        kindJson: String,
        rowsJson: String,
    ): String

    external override fun materializeProcedureFromRecordsJson(
        airportId: String,
        procedureId: String,
        kindJson: String,
        runwayTransitionJson: String,
        enrouteTransitionJson: String,
        componentIndex: Int,
        rowsJson: String,
        legsJson: String,
    ): String

    external override fun createUiSessionJson(
        catalogJson: String,
        chartCatalogJson: String,
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

    external override fun registerOwnshipSourceInSessionJson(
        handle: Long,
        registrationJson: String,
    ): String

    external override fun updateOwnshipSourceStatusInSessionJson(
        handle: Long,
        updateJson: String,
    ): String

    external override fun pushSituationSampleInSessionJson(
        handle: Long,
        sampleJson: String,
    ): String

    external override fun setOwnshipPolicyInSessionJson(
        handle: Long,
        policyJson: String,
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

    external override fun replaceFlightPlanInSessionJson(
        handle: Long,
        planJson: String,
    ): String

    external override fun restoreChartPageStateInSessionJson(
        handle: Long,
        recentAirportIdsJson: String,
        selectedAirportIdJson: String,
        selectedChartIdJson: String,
    ): String

    external override fun ingestPointTilesInSessionJson(
        handle: Long,
        tilesJson: String,
    ): String

    external override fun getMapOverlayInSessionJson(
        handle: Long,
        viewportJson: String,
        widthPx: Double,
        heightPx: Double,
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
