// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class E2eProjectionRegistryTest {
    @Test
    fun staleCompositionCannotRemoveReplacementProjection() {
        val resourceId = "org.aerobag.app:id/e2e_map_follow_projection"
        val firstOwner = Any()
        val replacementOwner = Any()

        E2eProjectionRegistry.publish(resourceId, "first", firstOwner)
        val firstRevision = E2eProjectionRegistry.read(resourceId)?.revision ?: 0L
        E2eProjectionRegistry.publish(resourceId, "replacement", replacementOwner)
        E2eProjectionRegistry.remove(resourceId, firstOwner)

        val replacement = E2eProjectionRegistry.read(resourceId)
        assertEquals("replacement", replacement?.state)
        assertTrue((replacement?.revision ?: 0L) > firstRevision)

        E2eProjectionRegistry.remove(resourceId, replacementOwner)
        assertNull(E2eProjectionRegistry.read(resourceId))
    }
}
