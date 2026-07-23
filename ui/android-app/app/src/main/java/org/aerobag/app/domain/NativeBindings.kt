package org.aerobag.app.domain

interface CoreSettingsStore {
    fun readSettings(): ByteArray?
    fun writeSettings(bytes: ByteArray)
}

interface NativeBridge {
    fun createOfflinePackagesController(packagesStateJson: String, libraryCacheJson: String): Long

    fun dispatchOfflinePackagesControllerJson(handle: Long, inputJson: String): String

    fun destroyOfflinePackagesController(handle: Long)

    fun createUiSessionWorkScheduler(): Long

    fun uiSessionWorkSchedulerRequestJson(handle: Long, requestJson: String): String

    fun uiSessionWorkSchedulerCompleteJson(handle: Long, requestId: Long): String

    fun destroyUiSessionWorkScheduler(handle: Long)

    fun createLiveFeedCache(sourceRootUrl: String, installedStatesJson: String): Long

    fun liveFeedEventsUrl(sourceRootUrl: String): String

    fun liveFeedStatusUrl(sourceRootUrl: String): String

    fun normalizeLiveFeedSourceRootUrl(sourceRootUrl: String): String

    fun liveFeedCacheMissingRequestsJson(handle: Long): String

    fun liveFeedCacheMissingRequestsAtEpochMsJson(handle: Long, epochMs: Long): String

    fun liveFeedCacheCurrentRefreshRequestsAtEpochMsJson(handle: Long, epochMs: Long): String

    fun liveFeedCacheRuntimeDecisionJson(handle: Long, inputJson: String): String

    fun liveFeedCacheRecordRequestFailure(handle: Long, requestId: String, epochMs: Long)

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

    fun liveFeedCacheResourceManifestJson(handle: Long, product: String): String

    fun liveFeedCacheResourceBytes(handle: Long, product: String, blobSha256: String): ByteArray

    fun liveFeedCacheBeginRestoringResources(handle: Long, manifestJson: String)

    fun liveFeedCacheRestoreResourceBytes(
        handle: Long,
        product: String,
        blobSha256: String,
        resourceBytes: ByteArray,
    )

    fun liveFeedCacheFinishRestoringResources(handle: Long, product: String)

    fun liveFeedCacheInstallProductInSessionJson(handle: Long, sessionHandle: Long, product: String): String

    fun liveFeedCacheSyncCatalogInSessionJson(handle: Long, sessionHandle: Long): String

    fun destroyLiveFeedCache(handle: Long)

    fun initializeOfflinePackagesJson(inputJson: String): String

    fun reduceOfflinePackagesJson(inputJson: String): String

    fun planOfflinePackagesFromBundleJson(inputJson: String): String

    fun planCurrentArtifactsDiscoveryJson(inputJson: String): String

    fun navDbOpenControllerCreate(candidatesJson: String): Long

    fun navDbOpenControllerCreateFromInstalledArtifacts(
        installedArtifactsJson: String,
        libraryCacheJson: String,
    ): Long

    fun navDbOpenControllerStep(handle: Long): String

    fun navDbOpenControllerIngestResource(handle: Long, resourceId: String, resourceBytes: ByteArray)

    fun navDbOpenControllerFinish(handle: Long): String

    fun navDbOpenControllerDestroy(handle: Long)

    fun navKvInsertResource(handle: Long, resourceId: String, resourceBytes: ByteArray)

    fun debugDropNavKvPagesForAttachedSessions(handle: Long)

    fun navKvDestroy(handle: Long)

    fun attachNavKvStoreToSession(navKvHandle: Long, sessionHandle: Long)

    fun coreHadOperation(handle: Long, operationJson: String): String

    fun drainSessionResourceEffectsJson(handle: Long): String

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

    fun loadOfflinePackageLibraryCacheInSessionJson(
        handle: Long,
        libraryCacheJson: String,
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

    fun registerOwnshipSourceInSessionPagedJson(
        handle: Long,
        registrationJson: String,
    ): String

    fun updateOwnshipSourceStatusInSessionPagedJson(
        handle: Long,
        updateJson: String,
    ): String

    fun pushSituationSampleInSessionPagedJson(
        handle: Long,
        sampleJson: String,
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

    fun loadPlaybackTraceInSessionPagedJson(
        handle: Long,
        sourcePathJson: String,
        traceJson: String,
    ): String

    fun playPlaybackInSessionPagedJson(
        handle: Long,
        nowEpochMs: Double,
    ): String

    fun pausePlaybackInSessionPagedJson(
        handle: Long,
        nowEpochMs: Double,
    ): String

    fun seekPlaybackInSessionPagedJson(
        handle: Long,
        cursorSeconds: Double,
        nowEpochMs: Double,
    ): String

    fun setPlaybackRateInSessionPagedJson(
        handle: Long,
        rate: Double,
        nowEpochMs: Double,
    ): String

    fun tickPlaybackInSessionPagedJson(
        handle: Long,
        nowEpochMs: Double,
    ): String

    fun tickBadAutopilotInSessionPagedJson(
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

    fun selectChartReferenceInSessionJson(
        handle: Long,
        familyIdJson: String,
        suggestedChartIdsJson: String,
    ): String

    fun setMapLayerVisibilityInSessionPagedJson(
        handle: Long,
        layerIdJson: String,
        visible: Boolean,
    ): String

    fun setMapLayerEnabledInSessionPagedJson(
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

    fun getSessionSnapshotPagedJson(handle: Long): String

    fun getSessionSnapshotAtEpochMsPagedJson(handle: Long, epochMs: Long): String

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

    fun acceptDisclaimerInSessionJson(
        handle: Long,
        agreementId: String,
    ): String

    fun loadPlateProcedureInSessionJson(
        handle: Long,
        loadId: String,
    ): String

    fun restoreDirectToInSessionJson(handle: Long): String

    fun activateNextLegInSessionJson(handle: Long): String

    fun stopNavigationInSessionJson(handle: Long): String

    fun suspendSequencingInSessionJson(handle: Long): String

    fun unsuspendSequencingInSessionJson(handle: Long): String

    fun sequenceActiveLegInSessionJson(handle: Long): String

    fun syncGuidanceGeometryInSessionJson(handle: Long): String

    fun projectFlightPlanRouteInSessionJson(handle: Long): String

    fun restoreChartPageStateInSessionJson(
        handle: Long,
        recentAirportIdsJson: String,
        plateTargetAirportIdJson: String,
        selectedAirportIdJson: String,
        selectedReferenceFamilyIdJson: String,
        selectedChartIdJson: String,
        suggestedChartIdsJson: String,
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

    fun refreshLiveFeedCurrentInSessionJson(handle: Long): String

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

    fun getRasterTilePlanInSessionWithDisplayScaleJson(
        handle: Long,
        viewportJson: String,
        widthPx: Double,
        heightPx: Double,
        devicePixelRatio: Double,
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

    external override fun createUiSessionWorkScheduler(): Long

    external override fun uiSessionWorkSchedulerRequestJson(handle: Long, requestJson: String): String

    external override fun uiSessionWorkSchedulerCompleteJson(handle: Long, requestId: Long): String

    external override fun destroyUiSessionWorkScheduler(handle: Long)

    external override fun createLiveFeedCache(sourceRootUrl: String, installedStatesJson: String): Long

    external override fun liveFeedEventsUrl(sourceRootUrl: String): String

    external override fun liveFeedStatusUrl(sourceRootUrl: String): String

    external override fun normalizeLiveFeedSourceRootUrl(sourceRootUrl: String): String

    external override fun liveFeedCacheMissingRequestsJson(handle: Long): String

    external override fun liveFeedCacheMissingRequestsAtEpochMsJson(handle: Long, epochMs: Long): String

    external override fun liveFeedCacheCurrentRefreshRequestsAtEpochMsJson(handle: Long, epochMs: Long): String

    external override fun liveFeedCacheRuntimeDecisionJson(handle: Long, inputJson: String): String

    external override fun liveFeedCacheRecordRequestFailure(handle: Long, requestId: String, epochMs: Long)

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

    external override fun liveFeedCacheResourceManifestJson(handle: Long, product: String): String

    external override fun liveFeedCacheResourceBytes(
        handle: Long,
        product: String,
        blobSha256: String,
    ): ByteArray

    external override fun liveFeedCacheBeginRestoringResources(handle: Long, manifestJson: String)

    external override fun liveFeedCacheRestoreResourceBytes(
        handle: Long,
        product: String,
        blobSha256: String,
        resourceBytes: ByteArray,
    )

    external override fun liveFeedCacheFinishRestoringResources(handle: Long, product: String)

    external override fun liveFeedCacheInstallProductInSessionJson(handle: Long, sessionHandle: Long, product: String): String

    external override fun liveFeedCacheSyncCatalogInSessionJson(handle: Long, sessionHandle: Long): String

    external override fun destroyLiveFeedCache(handle: Long)

    external override fun initializeOfflinePackagesJson(inputJson: String): String

    external override fun reduceOfflinePackagesJson(inputJson: String): String

    external override fun planOfflinePackagesFromBundleJson(inputJson: String): String

    external override fun planCurrentArtifactsDiscoveryJson(inputJson: String): String

    external override fun navDbOpenControllerCreate(candidatesJson: String): Long

    external override fun navDbOpenControllerCreateFromInstalledArtifacts(
        installedArtifactsJson: String,
        libraryCacheJson: String,
    ): Long

    external override fun navDbOpenControllerStep(handle: Long): String

    external override fun navDbOpenControllerIngestResource(handle: Long, resourceId: String, resourceBytes: ByteArray)

    external override fun navDbOpenControllerFinish(handle: Long): String

    external override fun navDbOpenControllerDestroy(handle: Long)

    external override fun navKvInsertResource(handle: Long, resourceId: String, resourceBytes: ByteArray)

    external override fun debugDropNavKvPagesForAttachedSessions(handle: Long)

    external override fun navKvDestroy(handle: Long)

    external override fun attachNavKvStoreToSession(navKvHandle: Long, sessionHandle: Long)

    external override fun coreHadOperation(handle: Long, operationJson: String): String

    external override fun drainSessionResourceEffectsJson(handle: Long): String

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

    external override fun loadOfflinePackageLibraryCacheInSessionJson(
        handle: Long,
        libraryCacheJson: String,
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

    external override fun registerOwnshipSourceInSessionPagedJson(
        handle: Long,
        registrationJson: String,
    ): String

    external override fun updateOwnshipSourceStatusInSessionPagedJson(
        handle: Long,
        updateJson: String,
    ): String

    external override fun pushSituationSampleInSessionPagedJson(
        handle: Long,
        sampleJson: String,
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

    external override fun loadPlaybackTraceInSessionPagedJson(
        handle: Long,
        sourcePathJson: String,
        traceJson: String,
    ): String

    external override fun playPlaybackInSessionPagedJson(
        handle: Long,
        nowEpochMs: Double,
    ): String

    external override fun pausePlaybackInSessionPagedJson(
        handle: Long,
        nowEpochMs: Double,
    ): String

    external override fun seekPlaybackInSessionPagedJson(
        handle: Long,
        cursorSeconds: Double,
        nowEpochMs: Double,
    ): String

    external override fun setPlaybackRateInSessionPagedJson(
        handle: Long,
        rate: Double,
        nowEpochMs: Double,
    ): String

    external override fun tickPlaybackInSessionPagedJson(
        handle: Long,
        nowEpochMs: Double,
    ): String

    external override fun tickBadAutopilotInSessionPagedJson(
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

    external override fun selectChartReferenceInSessionJson(
        handle: Long,
        familyIdJson: String,
        suggestedChartIdsJson: String,
    ): String

    external override fun setMapLayerVisibilityInSessionPagedJson(
        handle: Long,
        layerIdJson: String,
        visible: Boolean,
    ): String

    external override fun setMapLayerEnabledInSessionPagedJson(
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

    external override fun getSessionSnapshotPagedJson(handle: Long): String

    external override fun getSessionSnapshotAtEpochMsPagedJson(handle: Long, epochMs: Long): String

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

    external override fun acceptDisclaimerInSessionJson(
        handle: Long,
        agreementId: String,
    ): String

    external override fun loadPlateProcedureInSessionJson(
        handle: Long,
        loadId: String,
    ): String

    external override fun restoreDirectToInSessionJson(handle: Long): String

    external override fun activateNextLegInSessionJson(handle: Long): String

    external override fun stopNavigationInSessionJson(handle: Long): String

    external override fun suspendSequencingInSessionJson(handle: Long): String

    external override fun unsuspendSequencingInSessionJson(handle: Long): String

    external override fun sequenceActiveLegInSessionJson(handle: Long): String

    external override fun syncGuidanceGeometryInSessionJson(handle: Long): String

    external override fun projectFlightPlanRouteInSessionJson(handle: Long): String

    external override fun restoreChartPageStateInSessionJson(
        handle: Long,
        recentAirportIdsJson: String,
        plateTargetAirportIdJson: String,
        selectedAirportIdJson: String,
        selectedReferenceFamilyIdJson: String,
        selectedChartIdJson: String,
        suggestedChartIdsJson: String,
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

    external override fun refreshLiveFeedCurrentInSessionJson(handle: Long): String

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

    external override fun getRasterTilePlanInSessionWithDisplayScaleJson(
        handle: Long,
        viewportJson: String,
        widthPx: Double,
        heightPx: Double,
        devicePixelRatio: Double,
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
