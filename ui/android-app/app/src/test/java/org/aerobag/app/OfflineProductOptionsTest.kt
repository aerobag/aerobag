package org.aerobag.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class OfflineProductOptionsTest {
    @Test
    fun productOptionsIncludeTerrainWarningPackages() {
        val productIds = OfflineProductOptions.map { it.id }

        assertTrue(productIds.contains("terrain"))
    }

    @Test
    fun productOptionIdsAreUnique() {
        val productIds = OfflineProductOptions.map { it.id }

        assertEquals(productIds.toSet().size, productIds.size)
    }
}
