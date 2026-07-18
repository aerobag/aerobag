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
            sessionBody.contains("fun setInvalidationListener(listener: ((List<String>) -> Unit)?)"),
        )
        assertTrue(
            "Paged session mutations must publish core invalidations instead of dropping them.",
            sessionBody.contains("publishInvalidations(commandName, outcome.invalidations)"),
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
    }

    @Test
    fun mapPageSubscribesToCoreInvalidationsForOverlayAndRouteRefresh() {
        val mainActivity = sourceFile("src/main/java/org/aerobag/app/MainActivity.kt").readText()
        val mapPage = sourceFile("src/main/java/org/aerobag/app/MapExplorerPage.kt").readText()

        assertTrue(
            "Android app shell should subscribe to NativeUiSession invalidations.",
            mainActivity.contains("uiSession.setInvalidationListener(::publishUiInvalidations)"),
        )
        assertTrue(
            "Android app shell should preserve core invalidation names from the shared contract.",
            mainActivity.contains("\"map_overlay\"") && mainActivity.contains("\"flight_plan_route\""),
        )
        assertTrue(
            "Map overlay query should rerun when core emits map_overlay.",
            mapPage.contains("uiInvalidationRevisions.mapOverlay"),
        )
        assertTrue(
            "Flight-plan route projection should rerun when core emits flight_plan_route.",
            mapPage.contains("uiInvalidationRevisions.flightPlanRoute"),
        )
    }

    @Test
    fun androidDrainsCoreSessionResourceEffectsLikeWeb() {
        val nativeBridge = sourceFile("src/main/java/org/aerobag/app/domain/NativeBindings.kt").readText()
        val nativeSession = sourceFile("src/main/java/org/aerobag/app/domain/NativeAppCoreAdapter.kt").readText()
        val navKvStore = sourceFile("src/main/java/org/aerobag/app/domain/NavKvStore.kt").readText()

        assertTrue(
            "Android JNI bridge must expose core's pending session resource effects, matching web's drain_session_resource_effects.",
            nativeBridge.contains("fun drainSessionResourceEffectsJson(handle: Long): String") &&
                nativeBridge.contains("external override fun drainSessionResourceEffectsJson(handle: Long): String"),
        )
        assertTrue(
            "Android NavKvStore must pump pending session effects and publish their after-success invalidations.",
            navKvStore.contains("pumpSessionResourceEffects(") &&
                navKvStore.contains("after_success_invalidations") &&
                navKvStore.contains("afterSuccessInvalidations"),
        )
        assertTrue(
            "NativeUiSession must wire the session-effect pump to core invalidation listeners.",
            nativeSession.contains("bridge.drainSessionResourceEffectsJson(handle)") &&
                nativeSession.contains("publishInvalidations(\"session_effect\", effectInvalidations)"),
        )
    }

    @Test
    fun androidUiRoutesSessionCommandFailuresThroughRecoverableBoundary() {
        val mainActivity = sourceFile("src/main/java/org/aerobag/app/MainActivity.kt").readText()
        val flightPlanPage = sourceFile("src/main/java/org/aerobag/app/FlightPlanPage.kt").readText()
        val mapPage = sourceFile("src/main/java/org/aerobag/app/MapExplorerPage.kt").readText()
        val chartsPage = sourceFile("src/main/java/org/aerobag/app/ChartsPage.kt").readText()
        val playbackWidget = sourceFile("src/main/java/org/aerobag/app/PlaybackWidget.kt").readText()

        assertTrue(
            "MainActivity should own the central recoverable session-command UI boundary.",
            mainActivity.contains("fun recoverSessionCommandFailure(error: Throwable") &&
                mainActivity.contains("fun applySessionCommand(") &&
                mainActivity.contains("Action failed; app state was refreshed."),
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
