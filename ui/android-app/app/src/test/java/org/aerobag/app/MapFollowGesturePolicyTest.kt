package org.aerobag.app

import java.io.File
import org.junit.Assert.assertTrue
import org.junit.Test

class MapFollowGesturePolicyTest {
    @Test
    fun mapDragFollowSyncUsesFinalGestureViewport() {
        val source = sourceFile("src/main/java/org/aerobag/app/MapExplorerPage.kt").readText()

        assertTrue(
            "CTR drag finalization must sync follow state from the gesture's final viewport, " +
                "not from global viewportState, because a stale follow target can overwrite " +
                "that global state and snap ownship back to screen center.",
            Regex(
                """if\s*\(\s*movedViewportDuringGesture\s*\)\s*\{\s*syncFollowStateForViewport\(\s*gestureViewport\s*\)""",
            ).containsMatchIn(source),
        )
    }

    private fun sourceFile(path: String): File {
        val start = File(".").canonicalFile
        return generateSequence(start) { it.parentFile }
            .map { File(it, path) }
            .firstOrNull { it.isFile }
            ?: error("could not locate source file $path from $start")
    }
}
