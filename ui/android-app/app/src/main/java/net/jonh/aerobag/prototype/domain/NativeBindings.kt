package net.jonh.aerobag.prototype.domain

interface NativeBridge {
    fun createOfflinePackagesController(packagesStateJson: String): Long

    fun dispatchOfflinePackagesControllerJson(handle: Long, inputJson: String): String

    fun destroyOfflinePackagesController(handle: Long)

    fun initializeOfflinePackagesJson(inputJson: String): String

    fun reduceOfflinePackagesJson(inputJson: String): String

    fun planOfflinePackagesFromBundleJson(inputJson: String): String

    fun navKvOpen(rootBytes: ByteArray): Long

    fun navKvInsertResource(handle: Long, resourceId: String, resourceBytes: ByteArray)

    fun navKvDestroy(handle: Long)

    fun attachNavKvStoreToSession(navKvHandle: Long, sessionHandle: Long)

    fun coreHadOperation(handle: Long, operationJson: String): String

    fun situationRingCandidatesJson(): String

    fun emptyFlightPlanJson(): String

    fun activateLegUiJson(planJson: String, legIndex: Int): String

    fun prepareAirwayPresentationJson(
        airwayName: String,
        branchesJson: String,
        originPositionJson: String,
        destinationPositionJson: String,
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

    fun createUiSessionJson(
        vectorManifestJson: String,
        planJson: String,
        recentAirportIdsJson: String,
        selectedAirportIdJson: String,
        selectedChartIdJson: String,
    ): String

    fun setRasterResourceModeInSessionJson(
        handle: Long,
        modeJson: String,
    ): String

    fun performMapSelectionActionInSessionJson(
        handle: Long,
        actionJson: String,
    ): String

    fun insertWaypointAtFlightPlanRowInSessionJson(
        handle: Long,
        rowUid: String,
        before: Boolean,
        waypointJson: String,
    ): String

    fun suggestWaypointIdentifiersAtFlightPlanRowInSessionJson(
        handle: Long,
        rowUid: String,
        before: Boolean,
        prefix: String,
        limit: Int,
    ): String

    fun insertAirwayAtFlightPlanRowInSessionJson(
        handle: Long,
        rowUid: String,
        presentationJson: String,
        entryIndex: Int,
        exitIndex: Int,
    ): String

    fun selectProcedureAtFlightPlanRowInSessionJson(
        handle: Long,
        rowUid: String,
        airportId: String,
        procedureId: String,
        kindJson: String,
        runwayTransitionJson: String,
        enrouteTransitionJson: String,
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

    fun selectOwnshipSourceInSessionJson(
        handle: Long,
        selectionJson: String,
    ): String

    fun applySituationControlInputInSessionJson(
        handle: Long,
        inputJson: String,
        nowEpochMs: Double,
    ): String

    fun engageMapFollowInSessionJson(
        handle: Long,
        viewportJson: String,
    ): String

    fun disengageMapFollowInSessionJson(
        handle: Long,
        viewportJson: String,
    ): String

    fun setMapFollowOffsetInSessionJson(
        handle: Long,
        viewportJson: String,
        offsetXPx: Double,
        offsetYPx: Double,
    ): String

    fun loadPlaybackTraceInSessionJson(
        handle: Long,
        sourcePathJson: String,
        traceJson: String,
    ): String

    fun playPlaybackInSessionJson(
        handle: Long,
        nowEpochMs: Double,
    ): String

    fun pausePlaybackInSessionJson(
        handle: Long,
        nowEpochMs: Double,
    ): String

    fun seekPlaybackInSessionJson(
        handle: Long,
        cursorSeconds: Double,
        nowEpochMs: Double,
    ): String

    fun setPlaybackRateInSessionJson(
        handle: Long,
        rate: Double,
        nowEpochMs: Double,
    ): String

    fun tickPlaybackInSessionJson(
        handle: Long,
        nowEpochMs: Double,
    ): String

    fun selectAirportInSessionJson(
        handle: Long,
        airportIdJson: String,
    ): String

    fun selectChartInSessionJson(
        handle: Long,
        chartIdJson: String,
    ): String

    fun setMapLayerVisibilityInSessionJson(
        handle: Long,
        layerIdJson: String,
        visible: Boolean,
    ): String

    fun setMapLayerEnabledInSessionJson(
        handle: Long,
        layerIdJson: String,
        enabled: Boolean,
    ): String

    fun setDebugFlagInSessionJson(
        handle: Long,
        flagIdJson: String,
        enabled: Boolean,
    ): String

    fun loadRasterMapCatalogInSessionJson(handle: Long): String

    fun selectMapFamilyInSessionJson(
        handle: Long,
        familyIdJson: String,
    ): String

    fun selectRasterMapInSessionJson(
        handle: Long,
        selectedMapIdJson: String,
    ): String

    fun getSessionSnapshotJson(handle: Long): String

    fun replaceFlightPlanInSessionJson(
        handle: Long,
        planJson: String,
    ): String

    fun performFlightPlanRowActionInSessionJson(
        handle: Long,
        rowUid: String,
        actionUid: String,
    ): String

    fun loadPlateProcedureInSessionJson(
        handle: Long,
        loadId: String,
    ): String

    fun activateNextLegInSessionJson(handle: Long): String

    fun suspendSequencingInSessionJson(handle: Long): String

    fun unsuspendSequencingInSessionJson(handle: Long): String

    fun sequenceActiveLegInSessionJson(handle: Long): String

    fun setGuidanceLegGeometryInSessionJson(
        handle: Long,
        geometriesJson: String,
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

    fun ingestAirspaceRefTilesInSessionJson(
        handle: Long,
        tilesJson: String,
    ): String

    fun ingestAirspaceFeaturesInSessionJson(
        handle: Long,
        featuresJson: String,
    ): String

    fun ingestAirspaceLabelTilesInSessionJson(
        handle: Long,
        tilesJson: String,
    ): String

    fun ingestResourceInSession(
        handle: Long,
        resourceId: String,
        resourceBytes: ByteArray,
    ): String

    fun getMapOverlayInSessionJson(
        handle: Long,
        viewportJson: String,
        widthPx: Double,
        heightPx: Double,
    ): String

    fun getMapSelectionInSessionJson(
        handle: Long,
        viewportJson: String,
        widthPx: Double,
        heightPx: Double,
        clickJson: String,
        hitRadiusPx: Double,
    ): String

    fun getTerrainOverlayInSessionJson(
        handle: Long,
        viewportJson: String,
        widthPx: Double,
        heightPx: Double,
    ): String

    fun getRasterTilePlanInSessionJson(
        handle: Long,
        viewportJson: String,
        widthPx: Double,
        heightPx: Double,
    ): String

    fun renderTerrainOverlayTileInSession(
        handle: Long,
        tileBytes: ByteArray,
        aircraftAltitudeFt: Double,
    ): ByteArray

    fun renderTerrainOverlayTilesInSession(
        handle: Long,
        packedTileBytes: ByteArray,
        aircraftAltitudeFt: Double,
    ): ByteArray

    fun syncMapFollowInSessionJson(
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
        planJson: String,
    ): String

    fun setContentPolicyStateJson(
        stateJson: String,
        policyJson: String,
    ): String

    fun refreshContentStateJson(
        stateJson: String,
        inventoryJson: String,
    ): String

}

object NativeBindings : NativeBridge {
    init {
        System.loadLibrary("app_ffi")
    }

    external override fun createOfflinePackagesController(packagesStateJson: String): Long

    external override fun dispatchOfflinePackagesControllerJson(handle: Long, inputJson: String): String

    external override fun destroyOfflinePackagesController(handle: Long)

    external override fun initializeOfflinePackagesJson(inputJson: String): String

    external override fun reduceOfflinePackagesJson(inputJson: String): String

    external override fun planOfflinePackagesFromBundleJson(inputJson: String): String

    external override fun navKvOpen(rootBytes: ByteArray): Long

    external override fun navKvInsertResource(handle: Long, resourceId: String, resourceBytes: ByteArray)

    external override fun navKvDestroy(handle: Long)

    external override fun attachNavKvStoreToSession(navKvHandle: Long, sessionHandle: Long)

    external override fun coreHadOperation(handle: Long, operationJson: String): String

    external override fun situationRingCandidatesJson(): String

    external override fun activateLegUiJson(planJson: String, legIndex: Int): String

    external override fun prepareAirwayPresentationJson(
        airwayName: String,
        branchesJson: String,
        originPositionJson: String,
        destinationPositionJson: String,
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

    external override fun emptyFlightPlanJson(): String

    external override fun createUiSessionJson(
        vectorManifestJson: String,
        planJson: String,
        recentAirportIdsJson: String,
        selectedAirportIdJson: String,
        selectedChartIdJson: String,
    ): String

    external override fun setRasterResourceModeInSessionJson(
        handle: Long,
        modeJson: String,
    ): String

    external override fun performMapSelectionActionInSessionJson(
        handle: Long,
        actionJson: String,
    ): String

    external override fun insertWaypointAtFlightPlanRowInSessionJson(
        handle: Long,
        rowUid: String,
        before: Boolean,
        waypointJson: String,
    ): String

    external override fun suggestWaypointIdentifiersAtFlightPlanRowInSessionJson(
        handle: Long,
        rowUid: String,
        before: Boolean,
        prefix: String,
        limit: Int,
    ): String

    external override fun insertAirwayAtFlightPlanRowInSessionJson(
        handle: Long,
        rowUid: String,
        presentationJson: String,
        entryIndex: Int,
        exitIndex: Int,
    ): String

    external override fun selectProcedureAtFlightPlanRowInSessionJson(
        handle: Long,
        rowUid: String,
        airportId: String,
        procedureId: String,
        kindJson: String,
        runwayTransitionJson: String,
        enrouteTransitionJson: String,
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

    external override fun selectOwnshipSourceInSessionJson(
        handle: Long,
        selectionJson: String,
    ): String

    external override fun applySituationControlInputInSessionJson(
        handle: Long,
        inputJson: String,
        nowEpochMs: Double,
    ): String

    external override fun engageMapFollowInSessionJson(
        handle: Long,
        viewportJson: String,
    ): String

    external override fun disengageMapFollowInSessionJson(
        handle: Long,
        viewportJson: String,
    ): String

    external override fun setMapFollowOffsetInSessionJson(
        handle: Long,
        viewportJson: String,
        offsetXPx: Double,
        offsetYPx: Double,
    ): String

    external override fun loadPlaybackTraceInSessionJson(
        handle: Long,
        sourcePathJson: String,
        traceJson: String,
    ): String

    external override fun playPlaybackInSessionJson(
        handle: Long,
        nowEpochMs: Double,
    ): String

    external override fun pausePlaybackInSessionJson(
        handle: Long,
        nowEpochMs: Double,
    ): String

    external override fun seekPlaybackInSessionJson(
        handle: Long,
        cursorSeconds: Double,
        nowEpochMs: Double,
    ): String

    external override fun setPlaybackRateInSessionJson(
        handle: Long,
        rate: Double,
        nowEpochMs: Double,
    ): String

    external override fun tickPlaybackInSessionJson(
        handle: Long,
        nowEpochMs: Double,
    ): String

    external override fun selectAirportInSessionJson(
        handle: Long,
        airportIdJson: String,
    ): String

    external override fun selectChartInSessionJson(
        handle: Long,
        chartIdJson: String,
    ): String

    external override fun setMapLayerVisibilityInSessionJson(
        handle: Long,
        layerIdJson: String,
        visible: Boolean,
    ): String

    external override fun setMapLayerEnabledInSessionJson(
        handle: Long,
        layerIdJson: String,
        enabled: Boolean,
    ): String

    external override fun setDebugFlagInSessionJson(
        handle: Long,
        flagIdJson: String,
        enabled: Boolean,
    ): String

    external override fun loadRasterMapCatalogInSessionJson(handle: Long): String

    external override fun selectMapFamilyInSessionJson(
        handle: Long,
        familyIdJson: String,
    ): String

    external override fun selectRasterMapInSessionJson(
        handle: Long,
        selectedMapIdJson: String,
    ): String

    external override fun getSessionSnapshotJson(handle: Long): String

    external override fun replaceFlightPlanInSessionJson(
        handle: Long,
        planJson: String,
    ): String

    external override fun performFlightPlanRowActionInSessionJson(
        handle: Long,
        rowUid: String,
        actionUid: String,
    ): String

    external override fun loadPlateProcedureInSessionJson(
        handle: Long,
        loadId: String,
    ): String

    external override fun activateNextLegInSessionJson(handle: Long): String

    external override fun suspendSequencingInSessionJson(handle: Long): String

    external override fun unsuspendSequencingInSessionJson(handle: Long): String

    external override fun sequenceActiveLegInSessionJson(handle: Long): String

    external override fun setGuidanceLegGeometryInSessionJson(
        handle: Long,
        geometriesJson: String,
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

    external override fun ingestAirspaceRefTilesInSessionJson(
        handle: Long,
        tilesJson: String,
    ): String

    external override fun ingestAirspaceFeaturesInSessionJson(
        handle: Long,
        featuresJson: String,
    ): String

    external override fun ingestAirspaceLabelTilesInSessionJson(
        handle: Long,
        tilesJson: String,
    ): String

    external override fun ingestResourceInSession(
        handle: Long,
        resourceId: String,
        resourceBytes: ByteArray,
    ): String

    external override fun getMapOverlayInSessionJson(
        handle: Long,
        viewportJson: String,
        widthPx: Double,
        heightPx: Double,
    ): String

    external override fun getMapSelectionInSessionJson(
        handle: Long,
        viewportJson: String,
        widthPx: Double,
        heightPx: Double,
        clickJson: String,
        hitRadiusPx: Double,
    ): String

    external override fun getTerrainOverlayInSessionJson(
        handle: Long,
        viewportJson: String,
        widthPx: Double,
        heightPx: Double,
    ): String

    external override fun getRasterTilePlanInSessionJson(
        handle: Long,
        viewportJson: String,
        widthPx: Double,
        heightPx: Double,
    ): String

    external override fun renderTerrainOverlayTileInSession(
        handle: Long,
        tileBytes: ByteArray,
        aircraftAltitudeFt: Double,
    ): ByteArray

    external override fun renderTerrainOverlayTilesInSession(
        handle: Long,
        packedTileBytes: ByteArray,
        aircraftAltitudeFt: Double,
    ): ByteArray

    external override fun syncMapFollowInSessionJson(
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
        planJson: String,
    ): String

    external override fun setContentPolicyStateJson(
        stateJson: String,
        policyJson: String,
    ): String

    external override fun refreshContentStateJson(
        stateJson: String,
        inventoryJson: String,
    ): String

}
