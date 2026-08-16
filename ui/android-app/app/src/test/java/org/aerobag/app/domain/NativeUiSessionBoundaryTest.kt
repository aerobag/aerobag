// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.domain

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class NativeUiSessionBoundaryTest {
    @Test
    fun everyMutableSessionSnapshotFieldHasAnUpdateLander() {
        val source = sourceFile("src/main/java/org/aerobag/app/domain/NativeAppCoreAdapter.kt").readText()
        val wireFields = source
            .substringAfter("private data class WireUiSessionSnapshot(")
            .substringBefore("\n)")
            .lineSequence()
            .mapNotNull { line -> Regex("""^\s*val ([a-z0-9_]+):""").find(line)?.groupValues?.get(1) }
            .toSet() - setOf("ui_contract_version", "session_revision")
        val landerBody = balancedBlockAfterMarker(source, "private fun landSessionUpdate(")
        val landedFields = Regex("""^\s*"([a-z0-9_]+)"\s*->""", RegexOption.MULTILINE)
            .findAll(landerBody)
            .map { match -> match.groupValues[1] }
            .toSet()

        assertEquals(
            "Every mutable top-level session field needs an explicit Android model lander.",
            wireFields,
            landedFields,
        )
    }

    @Test
    fun sessionSnapshotMutationsUseRecoverableNativeCommandBoundary() {
        val source = sourceFile("src/main/java/org/aerobag/app/domain/NativeAppCoreAdapter.kt").readText()
        val sessionBody = balancedBlockAfterMarker(source, "class NativeUiSession")

        assertTrue(
            "NativeUiSession needs one central helper for every snapshot mutation.",
            sessionBody.contains("private fun runPagedSnapshot(commandName: String"),
        )
        assertFalse(
            "Plain snapshot mutation paths erase HAD page faults and must not exist.",
            sessionBody.contains("runPlainSnapshot"),
        )
        assertTrue(
            "Committed mutations must resume snapshot projection instead of running twice.",
            sessionBody.contains("resumeSnapshot = { bridge.getSessionSnapshotPagedJson(handle) }"),
        )
        assertTrue(
            "NativeUiSession must expose core invalidations from paged mutations.",
            sessionBody.contains("fun subscribeInvalidations(listener: (List<String>) -> Unit)"),
        )
        val packagePreferenceMutation = balancedBlockAfterMarker(
            sessionBody,
            "fun recordOfflinePackagePreferences(",
        )
        assertTrue(
            "Package preferences must return and apply the same revisioned update as every model mutation.",
            packagePreferenceMutation.contains("executePagedSnapshot"),
        )
        assertFalse(
            "Package preferences must not retain the former invalidation-only exception.",
            packagePreferenceMutation.contains("executePagedInvalidationCommand"),
        )
        assertTrue(
            "Paged session mutations must publish core invalidations instead of dropping them.",
            sessionBody.contains(
                "publishPagedInvalidations(commandName, outcome, snapshotAlreadyReturned = true)",
            ),
        )
        assertTrue(
            "Paged operations must publish direct invalidations and then launch background resource effects.",
            sessionBody.contains("val invalidations = outcome.invalidations.distinct()") &&
                sessionBody.contains("sessionResourceEffectPump?.request()"),
        )
        assertTrue(
            "NEXRAD queries must publish core's frame-change invalidation.",
            Regex("""fun queryNexradOverlay\([\s\S]*?publishPagedInvalidations\("queryNexradOverlay", result\)""")
                .containsMatchIn(sessionBody),
        )
        assertTrue(
            "The native command helper must not absorb Kotlin-side programming/configuration failures.",
            sessionBody.contains("error.isNativeSessionCommandFailure()"),
        )
        assertTrue(
            "Rejected native session commands should refresh the snapshot but still report failure to the caller.",
            Regex("""val refreshedSnapshot = refreshSnapshotAfterRejectedCommand\(commandName, error\)\s*throw NativeSessionCommandRejectedException\(commandName, refreshedSnapshot, error\)""")
                .containsMatchIn(sessionBody),
        )
        assertTrue(
            "Recoverable native session command failures should carry the refreshed snapshot to UI callers.",
            source.contains("class NativeSessionCommandRejectedException") &&
                source.contains("val refreshedSnapshot: UiSessionSnapshot"),
        )
        assertFalse(
            "Paged snapshot mutations must pass through the named guarded helper.",
            Regex("""runPagedSnapshot\s*\{""").containsMatchIn(sessionBody),
        )
        assertFalse(
            "Snapshot-producing JNI calls must not be decoded outside the paged runner.",
            Regex("""decodeSnapshot\(\s*bridge\.""").containsMatchIn(sessionBody),
        )
        assertTrue(
            "NativeUiSession must reserve full snapshot decoding for startup and explicit recovery.",
            Regex("""decodeFromJsonElement<WireUiSessionSnapshot>""")
                .findAll(sessionBody)
                .count() == 1 &&
                sessionBody.contains("landSessionUpdate(") &&
                !balancedBlockAfterMarker(sessionBody, "private fun executePagedSnapshot(")
                    .contains("decodeAccumulatedSnapshot()"),
        )
        assertTrue(
            "Ordinary mutations must apply core's generated update and full refreshes must explicitly reset it.",
            sessionBody.contains("snapshotAccumulator.applyOrResyncDetailed(update)") &&
                sessionBody.contains("snapshotAccumulator.replaceFullSnapshot"),
        )
        assertTrue(
            "A revision gap must recover through core's explicit paged full-snapshot API.",
            sessionBody.contains("snapshotAccumulator.applyOrResyncDetailed(update)") &&
                sessionBody.contains("bridge.getSessionSnapshotPagedJson(handle)"),
        )
        assertFalse(
            "Mutation payloads must not retain the transitional full-snapshot merge path.",
            sessionBody.contains("applyTransitionalMutationSnapshot"),
        )
        assertFalse(
            "NativeUiSession must not retain a direct snapshot JSON decoder.",
            sessionBody.contains("decodeFromString<WireUiSessionSnapshot>"),
        )
        assertTrue(
            "Durable live-feed product installation must use the same paged snapshot runner.",
            sessionBody.contains("return runPagedSnapshot(\"installLiveFeedCacheProduct\")"),
        )
        assertTrue(
            "Durable live-feed catalog synchronization must use the same paged snapshot runner.",
            sessionBody.contains("return runPagedSnapshot(\"syncLiveFeedCacheCatalog\")"),
        )
        assertTrue(
            "Commands that return their new snapshot must not also request a redundant snapshot refresh.",
            sessionBody.contains(
                "publishPagedInvalidations(commandName, outcome, snapshotAlreadyReturned = true)",
            ) && sessionBody.contains("invalidations - \"session_snapshot\""),
        )
        assertTrue(
            "Command snapshots must support generated group-scoped retained-session delivery.",
            sessionBody.contains("fun subscribeSnapshots(") &&
                sessionBody.contains("internal fun subscribeSnapshotGroups(") &&
                sessionBody.contains("it.groups.any(changedGroups::contains)"),
        )
    }

    @Test
    fun mapSelectionQueriesServiceCoreResourceRequests() {
        val sessionSource =
            sourceFile("src/main/java/org/aerobag/app/domain/NativeAppCoreAdapter.kt").readText()
        val runnerSource =
            sourceFile("src/main/java/org/aerobag/app/UiSessionWorkRunner.kt").readText()
        val mapSource =
            sourceFile("src/main/java/org/aerobag/app/MapExplorerPage.kt").readText()

        for (marker in listOf("fun queryMapSelection(", "fun queryMapSelectionForNavRef(")) {
            val body = balancedBlockAfterMarker(sessionSource, marker)
            assertTrue(
                "$marker must fetch non-NAVKV resources requested by core.",
                body.contains("fetchSessionResource = fetchResource"),
            )
            assertTrue(
                "$marker must ingest fetched resources into the active core session.",
                body.contains("bridge.ingestResourceInSession(handle, resource.id, bytes)"),
            )
        }
        assertTrue(
            "The background selection runner must carry the resource fetcher into both payload types.",
            runnerSource.split("fetchResource = fetchResource").size - 1 >= 4,
        )
        assertTrue(
            "Raw clicks, time-display refresh, search inspection, and the selection perf path must use the normalized fetcher.",
            mapSource.split("fetchMapOverlayCoreResource(context, resource, devServerBaseUrl)").size - 1 == 5,
        )
    }

    @Test
    fun expensiveMapResourceWorkIsOnlyCallableThroughTheScheduledRunner() {
        val sessionSource =
            sourceFile("src/main/java/org/aerobag/app/domain/NativeAppCoreAdapter.kt").readText()
        val runnerSource =
            sourceFile("src/main/java/org/aerobag/app/UiSessionWorkRunner.kt").readText()
        val mapSource =
            sourceFile("src/main/java/org/aerobag/app/MapExplorerPage.kt").readText()
        val scheduledMethods = listOf(
            "queryMapOverlay",
            "queryMapSelection",
            "queryMapSelectionForNavRef",
            "queryNexradOverlay",
            "nexradTileBytes",
            "queryTerrainOverlay",
            "renderTerrainOverlayTile",
        )

        for (method in scheduledMethods) {
            assertTrue(
                "$method must remain an opt-in raw operation at the native-session boundary.",
                Regex("""@RawUiSessionWorkApi\s+fun $method\(""").containsMatchIn(sessionSource),
            )
            assertTrue(
                "$method must have exactly one sanctioned caller in UiSessionWorkRunner.",
                runnerSource.split("uiSession.$method(").size - 1 == 1,
            )
            assertFalse(
                "MapExplorerPage must not bypass UiSessionWorkRunner for $method.",
                mapSource.contains("uiSession.$method("),
            )
        }
        assertTrue(
            "Distinct terrain tiles must not coalesce into one lossy background slot.",
            runnerSource.contains("\"terrain_tile:\${request.cacheKey}\""),
        )
        assertTrue(
            "Distinct NEXRAD resources must not coalesce into one lossy background slot.",
            runnerSource.contains("\"nexrad_tile:\$src\""),
        )
    }

    @Test
    fun mapPageUsesCoreInvalidationAndProjectionRevisions() {
        val mainActivity = sourceFile("src/main/java/org/aerobag/app/MainActivity.kt").readText()
        val mapPage = sourceFile("src/main/java/org/aerobag/app/MapExplorerPage.kt").readText()
        val retainedSession = sourceFile("src/main/java/org/aerobag/app/RetainedSession.kt").readText()
        val retainedLiveFeeds =
            sourceFile("src/main/java/org/aerobag/app/RetainedLiveFeedRuntime.kt").readText()

        assertTrue(
            "Android app shell should subscribe to NativeUiSession invalidations.",
            mainActivity.contains("uiSession.subscribeInvalidations(::enqueueUiInvalidations)"),
        )
        assertTrue(
            "Android app shell should preserve core invalidation names from the shared contract.",
            mainActivity.contains("\"map_overlay\"") && mainActivity.contains("\"flight_plan_route\""),
        )
        assertTrue(
            "Android app shell should route snapshot invalidations through core's shared refresh scheduler.",
            mainActivity.contains("retainedCoreSession.sessionSnapshotRefreshRunner") &&
                mainActivity.contains("sessionSnapshotRefreshRunner.request(") &&
                mainActivity.contains("SessionSnapshotRefreshPriority.LowPriority"),
        )
        assertFalse(
            "Snapshot invalidations must not cancel and restart refresh work until it starves.",
            mainActivity.contains("LaunchedEffect(uiSession, uiInvalidationRevisions.sessionSnapshot)"),
        )
        assertTrue(
            "Activity snapshot delivery must collapse queued snapshots while preserving every changed group.",
            mainActivity.contains("val snapshotDelivery = LatestValueExecutor(") &&
                mainActivity.contains("previous.changedGroups + next.changedGroups") &&
                mainActivity.contains("uiSession.subscribeSnapshotPublications"),
        )
        assertTrue(
            "Snapshot scheduling and the complete live-feed runtime must survive activity recreation.",
            retainedSession.contains("val sessionSnapshotRefreshRunner:") &&
                retainedSession.contains("val liveFeedRuntime: RetainedLiveFeedRuntime") &&
                retainedSession.contains("it.liveFeedRuntime.start()") &&
                mainActivity.contains("retainedCoreSession.liveFeedRuntime"),
        )
        assertTrue(
            "The retained live-feed runtime must own one idempotent restore and connection pipeline.",
            retainedLiveFeeds.contains("if (started) return") &&
                retainedLiveFeeds.contains("LiveFeedCacheStore.restore(appContext, cache)") &&
                retainedLiveFeeds.contains("client.bootstrapAndRun("),
        )
        assertFalse(
            "Compose activity lifecycle must not restart live-feed restore or connection work.",
            mainActivity.contains("LiveFeedCacheStore.restore") ||
                mainActivity.contains("AndroidLiveFeedClient("),
        )
        assertFalse(
            "Map overlay must not own a second one-off session snapshot refresh path.",
            mapPage.contains("""outcome.invalidations.contains("session_snapshot")"""),
        )
        assertTrue(
            "Map overlay query should rerun when core emits map_overlay.",
            mapPage.contains("uiInvalidationRevisions.mapOverlay"),
        )
        assertTrue(
            "Flight-plan route projection should rerun from the core-owned route revision.",
            mapPage.contains("sessionSnapshot.flightPlanRouteRevision"),
        )
        assertTrue(
            "A route from another core flight-plan revision must not be rendered.",
            mapPage.contains(
                "flightPlanRouteProjection.flightPlanRouteRevision == sessionSnapshot.flightPlanRouteRevision",
            ),
        )
    }

    @Test
    fun androidDrainsCoreSessionResourceEffectsLikeWeb() {
        val nativeBridge = sourceFile("src/main/java/org/aerobag/app/domain/NativeBindings.kt").readText()
        val nativeSession = sourceFile("src/main/java/org/aerobag/app/domain/NativeAppCoreAdapter.kt").readText()
        val navKvStore = sourceFile("src/main/java/org/aerobag/app/domain/NavKvStore.kt").readText()
        val pagedOperationRunner =
            balancedBlockAfterMarker(navKvStore, "private fun runPagedSessionOperation(")

        assertTrue(
            "Android JNI bridge must expose core's pending session resource effects, matching web's drain_session_resource_effects.",
            nativeBridge.contains("fun drainSessionResourceEffectsJson(handle: Long): String") &&
                nativeBridge.contains("external override fun drainSessionResourceEffectsJson(handle: Long): String"),
        )
        assertTrue(
            "Android NavKvStore must expose an explicit session-effect pump and preserve after-success invalidations.",
            navKvStore.contains("pumpSessionResourceEffects(") &&
                navKvStore.contains("completion_invalidations") &&
                navKvStore.contains("completionInvalidations"),
        )
        assertTrue(
            "NativeUiSession must run effects on its asynchronous pump and publish their invalidations.",
            nativeSession.contains("bridge.drainSessionResourceEffectsJson(handle)") &&
                nativeSession.contains("AsyncSessionResourceEffectPump(") &&
                nativeSession.contains("sessionResourceEffectPump?.request()"),
        )
        assertFalse(
            "Normal paged operations must never synchronously drain background session effects.",
            navKvStore.contains("effectInvalidations") ||
                pagedOperationRunner.contains("pumpSessionResourceEffects"),
        )
    }

    @Test
    fun androidUiRoutesSessionCommandFailuresThroughRecoverableBoundary() {
        val mainActivity = sourceFile("src/main/java/org/aerobag/app/MainActivity.kt").readText()
        val flightPlanPage = sourceFile("src/main/java/org/aerobag/app/FlightPlanPage.kt").readText()
        val mapPage = sourceFile("src/main/java/org/aerobag/app/MapExplorerPage.kt").readText()
        val chartsPage = sourceFile("src/main/java/org/aerobag/app/ChartsPage.kt").readText()
        val playbackWidget = sourceFile("src/main/java/org/aerobag/app/PlaybackWidget.kt").readText()
        val homePage = sourceFile("src/main/java/org/aerobag/app/HomePage.kt").readText()

        assertTrue(
            "MainActivity should own the central recoverable session-command UI boundary.",
            mainActivity.contains("fun recoverSessionCommandFailure(error: Throwable") &&
                mainActivity.contains("fun applySessionCommand(") &&
                mainActivity.contains("Action failed; app state was refreshed."),
        )
        val packagePreferenceHandoff = balancedBlockAfterMarker(
            mainActivity,
            "suspend fun recordOfflinePackagePreferencesForCloud(",
        )
        assertTrue(
            "Package preference persistence must not block Android's main thread.",
            packagePreferenceHandoff.contains("withContext(Dispatchers.Default)") &&
                packagePreferenceHandoff.contains("uiSession.recordOfflinePackagePreferences"),
        )
        assertTrue(
            "Background package preference failures must return to the central recovery boundary.",
            packagePreferenceHandoff.contains("recoverSessionCommandFailure"),
        )
        val localPackagePreferencePublish = balancedBlockAfterMarker(
            homePage,
            "result.preferencesForCloudJson?.let",
        )
        assertTrue(
            "Locally published package preferences must be marked observed before cloud handoff to prevent an echo.",
            localPackagePreferencePublish.indexOf("appliedSynchronizedOfflinePackagePreferencesJson") in
                0 until localPackagePreferencePublish.indexOf("onOfflinePackagePreferencesForCloud"),
        )
        for ((name, source) in listOf(
            "FlightPlanPage.kt" to flightPlanPage,
            "MapExplorerPage.kt" to mapPage,
            "ChartsPage.kt" to chartsPage,
            "PlaybackWidget.kt" to playbackWidget,
        )) {
            assertTrue(
                "$name should route recoverable session command failures to the app shell.",
                source.contains("onSessionCommandFailure: (Throwable) -> Unit"),
            )
        }

        val directSnapshotPatterns = listOf(
            Regex("""applySessionSnapshot\(\s*uiSession\."""),
            Regex("""onApplySessionSnapshot\(\s*uiSession\."""),
            Regex("""onSessionSnapshotChange\(\s*uiSession\."""),
            Regex("""onSnapshotChange\(\s*uiSession\."""),
            Regex("""onSessionSnapshotChange\(\s*\n\s*uiSession\."""),
        )
        for ((name, source) in listOf(
            "MainActivity.kt" to mainActivity,
            "FlightPlanPage.kt" to flightPlanPage,
            "MapExplorerPage.kt" to mapPage,
            "ChartsPage.kt" to chartsPage,
            "PlaybackWidget.kt" to playbackWidget,
        )) {
            for (pattern in directSnapshotPatterns) {
                assertFalse(
                    "$name should not evaluate uiSession mutations directly inside snapshot application calls.",
                    pattern.containsMatchIn(source),
                )
            }
        }
        assertFalse(
            "MainActivity should not hide session command failures behind generic runCatching blocks.",
            Regex("""runCatching\s*\{\s*uiSession\.""").containsMatchIn(mainActivity),
        )
        assertTrue(
            "ChartsPage's remaining plate-load runCatching must route typed command rejections to the app shell.",
            chartsPage.contains("error is org.aerobag.app.domain.NativeSessionCommandRejectedException") &&
                chartsPage.contains("onSessionCommandFailure(error)"),
        )
    }

    private fun sourceFile(path: String): File {
        val start = File(".").canonicalFile
        return generateSequence(start) { it.parentFile }
            .map { File(it, path) }
            .firstOrNull { it.isFile }
            ?: error("could not locate source file $path from $start")
    }

    private fun balancedBlockAfterMarker(source: String, marker: String): String {
        val start = source.indexOf(marker)
        require(start >= 0) { "missing marker $marker" }
        val bodyStart = source.indexOf('{', start)
        require(bodyStart >= 0) { "missing block start after $marker" }
        var depth = 0
        for (index in bodyStart until source.length) {
            when (source[index]) {
                '{' -> depth += 1
                '}' -> {
                    depth -= 1
                    if (depth == 0) {
                        return source.substring(start, index + 1)
                    }
                }
            }
        }
        error("unterminated block after $marker")
    }
}
