// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Test

class CoreResourceSourceTest {
    @Test
    fun decodesLiveFeedPackageMemberWithoutUrlRouting() {
        val source = parseCoreResourceSource(
            Json.parseToJsonElement(
                """
                {
                  "kind": "live_feed_package_member",
                  "product": "nexrad",
                  "version": "nexrad-v7",
                  "blob_sha256": "abc123",
                  "member_path": "tiles/res4/12/34.png"
                }
                """.trimIndent(),
            ).jsonObject,
        )

        assertEquals(
            CoreResourceSource.LiveFeedPackageMember(
                product = "nexrad",
                version = "nexrad-v7",
                blobSha256 = "abc123",
                memberPath = "tiles/res4/12/34.png",
            ),
            source,
        )
    }
}
