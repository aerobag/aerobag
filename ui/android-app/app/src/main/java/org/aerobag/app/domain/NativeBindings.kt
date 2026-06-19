package org.aerobag.app.domain

interface CoreSettingsStore {
    fun readSettings(): ByteArray?
    fun writeSettings(bytes: ByteArray)
}

interface NativeBridge {
    fun createOfflinePackagesController(packagesStateJson: String, libraryCacheJson: String): Long

    fun dispatchOfflinePackagesControllerJson(handle: Long, inputJson: String): String

    fun destroyOfflinePackagesController(handle: Long)

    fun createLiveFeedCache(installedStatesJson: String): Long

    fun liveFeedCacheMissingRequestsJson(handle: Long): String

    fun liveFeedCacheInstallFetchedBytesJson(
        handle: Long,
        requestJson: String,
        payloadBytes: ByteArray,
    ): String

    fun liveFeedCacheIngestSseEventJson(handle: Long, eventJson: String): String

    fun liveFeedCacheInstalledSummaryJson(handle: Long, product: String): String

    fun liveFeedCacheIngestInstalledPayloadBytes(
        handle: Long,
        summaryJson: String,
        payloadBytes: ByteArray,
    )

    fun liveFeedCacheInstalledPayloadBytes(handle: Long, product: String): ByteArray

    fun liveFeedCacheInstallProductInSessionJson(handle: Long, sessionHandle: Long, product: String): String

    fun liveFeedCacheSyncCatalogInSessionJson(handle: Long, sessionHandle: Long): String

    fun destroyLiveFeedCache(handle: Long)

    fun initializeOfflinePackagesJson(inputJson: String): String

    fun reduceOfflinePackagesJson(inputJson: String): String

    fun planOfflinePackagesFromBundleJson(inputJson: String): String

    fun planCurrentArtifactsDiscoveryJson(inputJson: String): String

    fun navDbOpenControllerCreate(candidatesJson: String): Long

    fun navDbOpenControllerStep(handle: Long): String

    fun navDbOpenControllerIngestResource(handle: Long, resourceId: String, resourceBytes: ByteArray)

    fun navDbOpenControllerFinish(handle: Long): String

    fun navDbOpenControllerStatuses(handle: Long): String

    fun navDbOpenControllerDestroy(handle: Long)

    fun navKvInsertResource(handle: Long, resourceId: String, resourceBytes: ByteArray)

    fun navKvDestroy(handle: Long)

    fun attachNavKvStoreToSession(navKvHandle: Long, sessionHandle: Long)

    fun coreHadOperation(handle: Long, operationJson: String): String

    fun situationRingCandidatesJson(): String

    fun emptyFlightPlanJson(): String

    fun prepareAirwayPresentationJson(
        airwayName: String,
        branchesJson: String,
        originPositionJson: String,
        destinationPositionJson: String,
    ): String

    fun createUiSessionJson(
        planJson: String,
        recentAirportIdsJson: String,
        selectedAirportIdJson: String,
        selectedChartIdJson: String,
    ): String

    fun configurePlatformCapabilitiesInSessionJson(
        handle: Long,
        capabilitiesJson: String,
        settingsStore: CoreSettingsStore,
    ): String

    fun setInstalledPackageIdsInSessionJson(
        handle: Long,
        packageIdsJson: String,
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

    fun previewFlightPlanEntryInSessionJson(
        handle: Long,
        input: String,
    ): String

    fun appendFlightPlanEntryInSessionJson(
        handle: Long,
        input: String,
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

    fun registerOwnshipSourceInSessionPagedJson(
        handle: Long,
        registrationJson: String,
    ): String

    fun updateOwnshipSourceStatusInSessionJson(
        handle: Long,
        updateJson: String,
    ): String

    fun updateOwnshipSourceStatusInSessionPagedJson(
        handle: Long,
        updateJson: String,
    ): String

    fun pushSituationSampleInSessionJson(
        handle: Long,
        sampleJson: String,
    ): String

    fun pushSituationSampleInSessionPagedJson(
        handle: Long,
        sampleJson: String,
    ): String

    fun selectOwnshipSourceInSessionJson(
        handle: Long,
        selectionJson: String,
    ): String

    fun selectOwnshipSourceInSessionPagedJson(
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

    fun loadPlaybackTraceInSessionJson(
        handle: Long,
        sourcePathJson: String,
        traceJson: String,
    ): String

    fun loadPlaybackTraceInSessionPagedJson(
        handle: Long,
        sourcePathJson: String,
        traceJson: String,
    ): String

    fun playPlaybackInSessionJson(
        handle: Long,
        nowEpochMs: Double,
    ): String

    fun playPlaybackInSessionPagedJson(
        handle: Long,
        nowEpochMs: Double,
    ): String

    fun pausePlaybackInSessionJson(
        handle: Long,
        nowEpochMs: Double,
    ): String

    fun pausePlaybackInSessionPagedJson(
        handle: Long,
        nowEpochMs: Double,
    ): String

    fun seekPlaybackInSessionJson(
        handle: Long,
        cursorSeconds: Double,
        nowEpochMs: Double,
    ): String

    fun seekPlaybackInSessionPagedJson(
        handle: Long,
        cursorSeconds: Double,
        nowEpochMs: Double,
    ): String

    fun setPlaybackRateInSessionJson(
        handle: Long,
        rate: Double,
        nowEpochMs: Double,
    ): String

    fun setPlaybackRateInSessionPagedJson(
        handle: Long,
        rate: Double,
        nowEpochMs: Double,
    ): String

    fun tickPlaybackInSessionJson(
        handle: Long,
        nowEpochMs: Double,
    ): String

    fun tickPlaybackInSessionPagedJson(
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

    fun getSessionSnapshotAtEpochMsJson(handle: Long, epochMs: Long): String

    fun performFlightPlanRowActionInSessionJson(
        handle: Long,
        rowUid: String,
        actionUid: String,
    ): String

    fun performStatusActionInSessionJson(
        handle: Long,
        actionId: String,
    ): String

    fun performSettingsActionInSessionJson(
        handle: Long,
        actionJson: String,
    ): String

    fun loadPlateProcedureInSessionJson(
        handle: Long,
        loadId: String,
    ): String

    fun activateNextLegInSessionJson(handle: Long): String

    fun suspendSequencingInSessionJson(handle: Long): String

    fun unsuspendSequencingInSessionJson(handle: Long): String

    fun sequenceActiveLegInSessionJson(handle: Long): String

    fun syncGuidanceGeometryInSessionJson(handle: Long): String

    fun projectFlightPlanRouteInSessionJson(handle: Long): String

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

    fun syncLiveFeedsInSessionJson(handle: Long): String

    fun ingestLiveFeedSseEventsInSessionJson(handle: Long, eventsJson: String): String

    fun reportLiveFeedConnectionEventInSessionJson(handle: Long, eventJson: String): String

    fun getMapOverlayInSessionJson(
        handle: Long,
        viewportJson: String,
        widthPx: Double,
        heightPx: Double,
    ): String

    fun getMapOverlayInSessionWithPointDisplayScaleJson(
        handle: Long,
        viewportJson: String,
        widthPx: Double,
        heightPx: Double,
        pointDisplayScale: Double,
    ): String

    fun getMapSelectionInSessionJson(
        handle: Long,
        viewportJson: String,
        widthPx: Double,
        heightPx: Double,
        clickJson: String,
    ): String

    fun getMapSelectionInSessionWithPointDisplayScaleJson(
        handle: Long,
        viewportJson: String,
        widthPx: Double,
        heightPx: Double,
        clickJson: String,
        pointDisplayScale: Double,
    ): String

    fun getMapSelectionForNavRefInSessionWithPointDisplayScaleJson(
        handle: Long,
        viewportJson: String,
        widthPx: Double,
        heightPx: Double,
        navRefJson: String,
        pointDisplayScale: Double,
    ): String

    fun getTerrainOverlayInSessionJson(
        handle: Long,
        viewportJson: String,
        widthPx: Double,
        heightPx: Double,
    ): String

    fun getScheduledTerrainOverlayInSessionJson(
        handle: Long,
        viewportJson: String,
        widthPx: Double,
        heightPx: Double,
        decodedCacheKeysJson: String,
        inFlightCacheKeysJson: String,
    ): String

    fun getNexradOverlayInSessionJson(
        handle: Long,
        viewportJson: String,
        widthPx: Double,
        heightPx: Double,
    ): String

    fun resolveChartAssetResourceInSessionJson(
        handle: Long,
        chartId: String,
        assetKind: String,
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

    fun renderTerrainOverlayTileByKeyInSession(
        handle: Long,
        tileKey: String,
        aircraftAltitudeFt: Double,
    ): ByteArray

    fun nexradTileBytesInSession(
        handle: Long,
        src: String,
    ): ByteArray

    fun prepareNexradTileInSessionJson(
        handle: Long,
        src: String,
    ): String

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
}

object NativeBindings : NativeBridge {
    init {
        System.loadLibrary("app_ffi")
        installCoreDebugLogger()
    }

    private external fun installCoreDebugLogger()

    external fun configureGpsCaptureLogPath(path: String)

    external override fun createOfflinePackagesController(
        packagesStateJson: String,
        libraryCacheJson: String,
    ): Long

    external override fun dispatchOfflinePackagesControllerJson(handle: Long, inputJson: String): String

    external override fun destroyOfflinePackagesController(handle: Long)

    external override fun createLiveFeedCache(installedStatesJson: String): Long

    external override fun liveFeedCacheMissingRequestsJson(handle: Long): String

    external override fun liveFeedCacheInstallFetchedBytesJson(
        handle: Long,
        requestJson: String,
        payloadBytes: ByteArray,
    ): String

    external override fun liveFeedCacheIngestSseEventJson(handle: Long, eventJson: String): String

    external override fun liveFeedCacheInstalledSummaryJson(handle: Long, product: String): String

    external override fun liveFeedCacheIngestInstalledPayloadBytes(
        handle: Long,
        summaryJson: String,
        payloadBytes: ByteArray,
    )

    external override fun liveFeedCacheInstalledPayloadBytes(handle: Long, product: String): ByteArray

    external override fun liveFeedCacheInstallProductInSessionJson(handle: Long, sessionHandle: Long, product: String): String

    external override fun liveFeedCacheSyncCatalogInSessionJson(handle: Long, sessionHandle: Long): String

    external override fun destroyLiveFeedCache(handle: Long)

    external override fun initializeOfflinePackagesJson(inputJson: String): String

    external override fun reduceOfflinePackagesJson(inputJson: String): String

    external override fun planOfflinePackagesFromBundleJson(inputJson: String): String

    external override fun planCurrentArtifactsDiscoveryJson(inputJson: String): String

    external override fun navDbOpenControllerCreate(candidatesJson: String): Long

    external override fun navDbOpenControllerStep(handle: Long): String

    external override fun navDbOpenControllerIngestResource(handle: Long, resourceId: String, resourceBytes: ByteArray)

    external override fun navDbOpenControllerFinish(handle: Long): String

    external override fun navDbOpenControllerStatuses(handle: Long): String

    external override fun navDbOpenControllerDestroy(handle: Long)

    external override fun navKvInsertResource(handle: Long, resourceId: String, resourceBytes: ByteArray)

    external override fun navKvDestroy(handle: Long)

    external override fun attachNavKvStoreToSession(navKvHandle: Long, sessionHandle: Long)

    external override fun coreHadOperation(handle: Long, operationJson: String): String

    external override fun situationRingCandidatesJson(): String

    external override fun prepareAirwayPresentationJson(
        airwayName: String,
        branchesJson: String,
        originPositionJson: String,
        destinationPositionJson: String,
    ): String

    external override fun emptyFlightPlanJson(): String

    external override fun createUiSessionJson(
        planJson: String,
        recentAirportIdsJson: String,
        selectedAirportIdJson: String,
        selectedChartIdJson: String,
    ): String

    external override fun configurePlatformCapabilitiesInSessionJson(
        handle: Long,
        capabilitiesJson: String,
        settingsStore: CoreSettingsStore,
    ): String

    external override fun setInstalledPackageIdsInSessionJson(
        handle: Long,
        packageIdsJson: String,
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

    external override fun previewFlightPlanEntryInSessionJson(
        handle: Long,
        input: String,
    ): String

    external override fun appendFlightPlanEntryInSessionJson(
        handle: Long,
        input: String,
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

    external override fun registerOwnshipSourceInSessionPagedJson(
        handle: Long,
        registrationJson: String,
    ): String

    external override fun updateOwnshipSourceStatusInSessionJson(
        handle: Long,
        updateJson: String,
    ): String

    external override fun updateOwnshipSourceStatusInSessionPagedJson(
        handle: Long,
        updateJson: String,
    ): String

    external override fun pushSituationSampleInSessionJson(
        handle: Long,
        sampleJson: String,
    ): String

    external override fun pushSituationSampleInSessionPagedJson(
        handle: Long,
        sampleJson: String,
    ): String

    external override fun selectOwnshipSourceInSessionJson(
        handle: Long,
        selectionJson: String,
    ): String

    external override fun selectOwnshipSourceInSessionPagedJson(
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

    external override fun loadPlaybackTraceInSessionJson(
        handle: Long,
        sourcePathJson: String,
        traceJson: String,
    ): String

    external override fun loadPlaybackTraceInSessionPagedJson(
        handle: Long,
        sourcePathJson: String,
        traceJson: String,
    ): String

    external override fun playPlaybackInSessionJson(
        handle: Long,
        nowEpochMs: Double,
    ): String

    external override fun playPlaybackInSessionPagedJson(
        handle: Long,
        nowEpochMs: Double,
    ): String

    external override fun pausePlaybackInSessionJson(
        handle: Long,
        nowEpochMs: Double,
    ): String

    external override fun pausePlaybackInSessionPagedJson(
        handle: Long,
        nowEpochMs: Double,
    ): String

    external override fun seekPlaybackInSessionJson(
        handle: Long,
        cursorSeconds: Double,
        nowEpochMs: Double,
    ): String

    external override fun seekPlaybackInSessionPagedJson(
        handle: Long,
        cursorSeconds: Double,
        nowEpochMs: Double,
    ): String

    external override fun setPlaybackRateInSessionJson(
        handle: Long,
        rate: Double,
        nowEpochMs: Double,
    ): String

    external override fun setPlaybackRateInSessionPagedJson(
        handle: Long,
        rate: Double,
        nowEpochMs: Double,
    ): String

    external override fun tickPlaybackInSessionJson(
        handle: Long,
        nowEpochMs: Double,
    ): String

    external override fun tickPlaybackInSessionPagedJson(
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

    external override fun getSessionSnapshotAtEpochMsJson(handle: Long, epochMs: Long): String

    external override fun performFlightPlanRowActionInSessionJson(
        handle: Long,
        rowUid: String,
        actionUid: String,
    ): String

    external override fun performStatusActionInSessionJson(
        handle: Long,
        actionId: String,
    ): String

    external override fun performSettingsActionInSessionJson(
        handle: Long,
        actionJson: String,
    ): String

    external override fun loadPlateProcedureInSessionJson(
        handle: Long,
        loadId: String,
    ): String

    external override fun activateNextLegInSessionJson(handle: Long): String

    external override fun suspendSequencingInSessionJson(handle: Long): String

    external override fun unsuspendSequencingInSessionJson(handle: Long): String

    external override fun sequenceActiveLegInSessionJson(handle: Long): String

    external override fun syncGuidanceGeometryInSessionJson(handle: Long): String

    external override fun projectFlightPlanRouteInSessionJson(handle: Long): String

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

    external override fun syncLiveFeedsInSessionJson(handle: Long): String

    external override fun ingestLiveFeedSseEventsInSessionJson(handle: Long, eventsJson: String): String

    external override fun reportLiveFeedConnectionEventInSessionJson(handle: Long, eventJson: String): String

    external override fun getMapOverlayInSessionJson(
        handle: Long,
        viewportJson: String,
        widthPx: Double,
        heightPx: Double,
    ): String

    external override fun getMapOverlayInSessionWithPointDisplayScaleJson(
        handle: Long,
        viewportJson: String,
        widthPx: Double,
        heightPx: Double,
        pointDisplayScale: Double,
    ): String

    external override fun getMapSelectionInSessionJson(
        handle: Long,
        viewportJson: String,
        widthPx: Double,
        heightPx: Double,
        clickJson: String,
    ): String

    external override fun getMapSelectionInSessionWithPointDisplayScaleJson(
        handle: Long,
        viewportJson: String,
        widthPx: Double,
        heightPx: Double,
        clickJson: String,
        pointDisplayScale: Double,
    ): String

    external override fun getMapSelectionForNavRefInSessionWithPointDisplayScaleJson(
        handle: Long,
        viewportJson: String,
        widthPx: Double,
        heightPx: Double,
        navRefJson: String,
        pointDisplayScale: Double,
    ): String

    external override fun getTerrainOverlayInSessionJson(
        handle: Long,
        viewportJson: String,
        widthPx: Double,
        heightPx: Double,
    ): String

    external override fun getScheduledTerrainOverlayInSessionJson(
        handle: Long,
        viewportJson: String,
        widthPx: Double,
        heightPx: Double,
        decodedCacheKeysJson: String,
        inFlightCacheKeysJson: String,
    ): String

    external override fun getNexradOverlayInSessionJson(
        handle: Long,
        viewportJson: String,
        widthPx: Double,
        heightPx: Double,
    ): String

    external override fun resolveChartAssetResourceInSessionJson(
        handle: Long,
        chartId: String,
        assetKind: String,
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

    external override fun renderTerrainOverlayTileByKeyInSession(
        handle: Long,
        tileKey: String,
        aircraftAltitudeFt: Double,
    ): ByteArray

    external override fun nexradTileBytesInSession(
        handle: Long,
        src: String,
    ): ByteArray

    external override fun prepareNexradTileInSessionJson(
        handle: Long,
        src: String,
    ): String

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
}
