package org.aerobag.app

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class MapRouteProjectionPolicyTest {
    @Test
    fun mapPageDoesNotShortCircuitCoreRouteProjectionFromPlanShape() {
        val source = sourceFile("src/main/java/org/aerobag/app/MapExplorerPage.kt").readText()

        assertTrue(
            "MapExplorerPage must ask core to project the flight-plan route.",
            source.contains("uiSession.projectFlightPlanRoute()"),
        )
        assertFalse(
            "MapExplorerPage must not infer route emptiness from resolved legs; direct-to can project a route for an otherwise empty plan.",
            source.contains("resolvedLegs.isEmpty()"),
        )
        assertFalse(
            "MapExplorerPage must not infer route availability from resolved legs; core owns route projection policy.",
            source.contains("resolvedLegs.isNotEmpty()"),
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
