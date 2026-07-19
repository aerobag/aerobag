package org.aerobag.app

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class HomeNavigationPolicyTest {
    @Test
    fun homePageConsumesCoreButtonsAndUsesTheCommonDisabledActionToast() {
        val mainSource = sourceFile("src/main/java/org/aerobag/app/MainActivity.kt").readText()
        val homeSource = sourceFile("src/main/java/org/aerobag/app/HomePage.kt").readText()

        assertFalse(
            "Android must not restore a platform-owned Home button inventory.",
            mainSource.contains("HomeGridButtons"),
        )
        assertTrue(
            "HomePage must render the ordered core Home button model.",
            homeSource.contains(".buttons\n            .map { it.toHomeGridButton() }"),
        )
        assertTrue(
            "Disabled Home buttons must use the shared Android explanation toast.",
            homeSource.contains("showDisabledActionToast(context, reason)"),
        )
    }

    @Test
    fun androidAdvertisesOfflinePackageManagement() {
        val adapterSource = sourceFile(
            "src/main/java/org/aerobag/app/domain/NativeAppCoreAdapter.kt",
        ).readText()

        assertTrue(
            "Android must advertise its Offline Packages capability to core.",
            adapterSource.contains("put(\"offline_packages\", buildJsonObject {})"),
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
