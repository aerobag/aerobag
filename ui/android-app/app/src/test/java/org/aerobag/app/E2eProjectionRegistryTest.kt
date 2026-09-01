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

    @Test
    fun disposingReplacementRestoresStillRenderedProjection() {
        val resourceId = "parity:altitude-planner-control:aircraft"
        val survivingOwner = Any()
        val transientOwner = Any()

        E2eProjectionRegistry.publish(resourceId, "surviving", survivingOwner, "[1,2][3,4]")
        val survivingRevision = E2eProjectionRegistry.read(resourceId)?.revision ?: 0L
        E2eProjectionRegistry.publish(resourceId, "transient", transientOwner, "[5,6][7,8]")
        E2eProjectionRegistry.remove(resourceId, transientOwner)

        val restored = E2eProjectionRegistry.read(resourceId)
        assertEquals("surviving", restored?.state)
        assertEquals("[1,2][3,4]", restored?.bounds)
        assertEquals(survivingRevision, restored?.revision)

        E2eProjectionRegistry.remove(resourceId, survivingOwner)
        assertNull(E2eProjectionRegistry.read(resourceId))
    }
}
