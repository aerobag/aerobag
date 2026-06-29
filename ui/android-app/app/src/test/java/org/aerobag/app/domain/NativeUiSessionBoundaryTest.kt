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
            "NativeUiSession needs one central helper for direct snapshot mutations.",
            sessionBody.contains("private fun runPlainSnapshot(commandName: String"),
        )
        assertTrue(
            "NativeUiSession needs one central helper for paged snapshot mutations.",
            sessionBody.contains("private fun runPagedSnapshot(commandName: String"),
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
        assertFalse(
            "Paged snapshot mutations must pass through the named guarded helper.",
            Regex("""runPagedSnapshot\s*\{""").containsMatchIn(sessionBody),
        )
        assertFalse(
            "Direct native snapshot mutations must pass through runPlainSnapshot; only recovery may read the current snapshot directly.",
            Regex("""decodeSnapshot\(\s*bridge\.(?!getSessionSnapshotAtEpochMsJson)""").containsMatchIn(sessionBody),
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
