// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import java.io.File
import org.junit.Assert.assertTrue
import org.junit.Test

class ImeActionFreshnessTest {
    @Test
    fun imeActionsCallTheLatestRecomposedCallback() {
        val common = sourceFile("src/main/java/org/aerobag/app/DebugAndCommonWidgets.kt").readText()
        assertTrue(
            "IME callbacks must dereference the latest action after async validation recomposes a field.",
            common.contains("val currentAction = rememberUpdatedState(action)") &&
                common.contains("return remember { { currentAction.value() } }"),
        )

        val expectedWiring =
            mapOf(
                "FlightPlanPage.kt" to
                    listOf(
                        "val submitAction = rememberCurrentAction(onSubmit)",
                        "KeyboardActions(onDone = { submitAction() })",
                    ),
                "ChartsPage.kt" to
                    listOf(
                        "val submitAction = rememberCurrentAction {",
                        "submitAction()",
                    ),
                "MapExplorerPage.kt" to
                    listOf(
                        "val submitAction = rememberCurrentAction(onSubmit)",
                        "KeyboardActions(onDone = { submitAction() })",
                    ),
                "AltitudePlannerPage.kt" to
                    listOf(
                        "val doneAction = rememberCurrentAction(onDone)",
                        "KeyboardActions(onDone = { doneAction() })",
                    ),
                "MainActivity.kt" to
                    listOf(
                        "val submitAction = rememberCurrentAction {",
                        "onGo = { submitAction() }",
                    ),
            )

        expectedWiring.forEach { (fileName, snippets) ->
            val source = sourceFile("src/main/java/org/aerobag/app/$fileName").readText()
            snippets.forEach { snippet ->
                assertTrue("$fileName must use the current IME action: $snippet", source.contains(snippet))
            }
        }
    }

    private fun sourceFile(path: String): File {
        val start = File(".").canonicalFile
        return generateSequence(start) { it.parentFile }
            .map { File(it, path) }
            .firstOrNull { it.isFile }
            ?: error("could not locate source file $path from $start")
    }
}
