// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.domain

import kotlinx.serialization.decodeFromString
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Test

class NavDbAdvanceWireTest {
    @Test
    fun decodesCoreArtifactIdentityWithoutRelaxingUnknownKeyChecks() {
        val result = Json.decodeFromString<WireNavDbAdvanceResult>(
            """{
                "disposition":"adopted",
                "active_artifact_filename":"nav_db_NAV16_2608.zip",
                "retained_artifact_filenames":["nav_db_NAV16_2608.zip"]
            }""".trimIndent(),
        )

        assertEquals("nav_db_NAV16_2608.zip", result.active_artifact_filename)
        assertEquals(listOf("nav_db_NAV16_2608.zip"), result.retained_artifact_filenames)
    }
}
