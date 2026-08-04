// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.domain

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class NativeUiSessionBoundaryTest {
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
            "Package preferences must use the invalidation-only session boundary.",
            packagePreferenceMutation.contains("executePagedInvalidationCommand"),
        )
        assertFalse(
            "Package preferences must not synchronously project a complete UI snapshot.",
            packagePreferenceMutation.contains("runPagedSnapshot"),
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
            "NativeUiSession must have exactly one snapshot-wire decoder, inside the paged runner.",
            Regex("""decodeFromJsonElement<WireUiSessionSnapshot>""")
                .findAll(sessionBody)
                .count() == 1,
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
            "Command snapshots must be delivered through the retained session boundary.",
            sessionBody.contains("fun subscribeSnapshots(") &&
                sessionBody.contains("snapshotListener?.invoke(nextSnapshot)"),
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
            "Raw clicks, search inspection, and the selection perf path must use the normalized fetcher.",
            mapSource.split("fetchMapOverlayCoreResource(context, resource, devServerBaseUrl)").size - 1 == 4,
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
            "Activity snapshot delivery must collapse stale queued snapshots to the latest value.",
            mainActivity.contains("LatestValueExecutor(mainExecutor, ::applySessionSnapshot)") &&
                mainActivity.contains("uiSession.subscribeSnapshots(snapshotDelivery::submit)"),
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
                navKvStore.contains("after_success_invalidations") &&
                navKvStore.contains("afterSuccessInvalidations"),
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
