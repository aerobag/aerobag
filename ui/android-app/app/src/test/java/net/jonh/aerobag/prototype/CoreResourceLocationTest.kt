package net.jonh.aerobag.prototype

import org.junit.Assert.assertEquals
import org.junit.Test

class CoreResourceLocationTest {
    @Test
    fun resolvesInstalledPackageMemberAddress() {
        val location = resolveCoreResourceLocation(
            "package://METARS_2604/points/metars/7/20/42.json",
            "http://10.0.2.2:8083/packages",
        )

        assertEquals(
            CoreResourceLocation.InstalledPackage(
                packageId = "METARS_2604",
                memberPath = "points/metars/7/20/42.json",
            ),
            location,
        )
    }

    @Test
    fun resolvesPublicationCurrentArtifactsAddress() {
        val location = resolveCoreResourceLocation(
            "/packages/current_artifacts.json",
            "http://10.0.2.2:8083/packages",
        )

        assertEquals(
            CoreResourceLocation.Url("http://10.0.2.2:8083/packages/current_artifacts.json"),
            location,
        )
    }

    @Test
    fun resolvesPublicationBundleAddress() {
        val location = resolveCoreResourceLocation(
            "/packages/published_packaged/bundles/bundle_cycle.json",
            "http://10.0.2.2:8083/packages/",
        )

        assertEquals(
            CoreResourceLocation.Url("http://10.0.2.2:8083/packages/published_packaged/bundles/bundle_cycle.json"),
            location,
        )
    }

    @Test
    fun rejectsUnsupportedAddress() {
        val error = kotlin.runCatching {
            resolveCoreResourceLocation(
                "/not-a-publication-route/example.bin",
                "http://10.0.2.2:8083/packages",
            )
        }.exceptionOrNull()

        assertEquals(
            "unsupported core resource address: /not-a-publication-route/example.bin",
            error?.message,
        )
    }
}
