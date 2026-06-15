package org.aerobag.app

import org.aerobag.app.domain.OwnshipControlTone
import org.aerobag.app.domain.OwnshipSourceKind
import org.aerobag.app.domain.OwnshipSourceMenuItem
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidGpsPowerTest {
    @Test
    fun highPrecisionGpsRunsOnlyForAndroidGpsSource() {
        assertTrue(AndroidGpsPower.shouldRunHighPrecisionGpsForSource(AndroidGpsSource.SourceId))
        assertFalse(AndroidGpsPower.shouldRunHighPrecisionGpsForSource(PlanPreviewOwnshipSourceId))
        assertFalse(AndroidGpsPower.shouldRunHighPrecisionGpsForSource("external-gps"))
    }

    @Test
    fun batterySavingFallbackUsesPlanPreview() {
        assertEquals(PlanPreviewOwnshipSourceId, AndroidGpsPower.batterySavingFallbackSourceId())
    }

    @Test
    fun pausedAndroidGpsSourceKeepsModeLabelInTray() {
        val source = OwnshipSourceMenuItem(
            sourceId = AndroidGpsSource.SourceId,
            sourceKind = OwnshipSourceKind.DeviceGps,
            label = "GPS",
            launcherLabel = "No GPS",
            tone = OwnshipControlTone.Unavailable,
            enabled = true,
            active = false,
            statusLabel = "Paused",
        )

        assertEquals("GPS", situationSourceButtonLabel(source))
    }
}
