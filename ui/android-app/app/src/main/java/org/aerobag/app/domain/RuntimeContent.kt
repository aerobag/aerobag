package org.aerobag.app.domain

import android.content.Context
import android.os.SystemClock
import java.time.Instant
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import org.aerobag.app.diagnosticLogInfo

data class RuntimeBootstrap(
    val packageManagementNowEpochMsOverride: Long?,
)

data class RuntimeContent(
    val bootstrap: RuntimeBootstrap,
    val navKvStore: NavKvStore,
    val installedPackageIds: List<String>,
)

data class NavDbArtifactStatus(
    val packageId: String,
    val filename: String,
    val readable: Boolean,
    val message: String? = null,
)

data class NavDbStatus(
    val installed: List<NavDbArtifactStatus>,
)

@Serializable
private data class WireDevBootstrap(
    val package_management_now_utc: String? = null,
)

object AndroidRuntimeContent {
    private const val BOOTSTRAP_ASSET_PATH = "fixtures/dev-bootstrap.json"
    private const val TAG = "AndroidRuntimeContent"
    private val json = Json {
        encodeDefaults = true
    }

    fun loadBootstrap(context: Context): RuntimeBootstrap {
        val bootstrapPayload = context.assets.open(BOOTSTRAP_ASSET_PATH).bufferedReader().use { it.readText() }
        val bootstrap = json.decodeFromString<WireDevBootstrap>(bootstrapPayload)
        return RuntimeBootstrap(
            packageManagementNowEpochMsOverride = bootstrap.package_management_now_utc?.let {
                Instant.parse(it).toEpochMilli()
            },
        )
    }

    fun loadInstalledRuntime(
        context: Context,
        bootstrap: RuntimeBootstrap,
        libraryCacheJson: String,
    ): RuntimeContent {
        val navKvOpenStartMs = SystemClock.elapsedRealtime()
        val installedArtifacts = InstalledPackages.listInstalledArtifacts(context.applicationContext)
        val navKvStore = NavKvStore.openInstalledArtifacts(
            installedArtifacts,
            libraryCacheJson = libraryCacheJson,
        ).first
        val navKvOpenMs = SystemClock.elapsedRealtime() - navKvOpenStartMs
        val installedPackageIds = installedArtifacts
            .map { it.artifactId }
            .distinct()
            .sorted()
        return loadInstalledRuntime(
            bootstrap = bootstrap,
            navKvStore = navKvStore,
            navKvOpenMs = navKvOpenMs,
            installedPackageIds = installedPackageIds,
        )
    }

    fun loadInstalledRuntime(
        bootstrap: RuntimeBootstrap,
        navKvStore: NavKvStore,
        navKvOpenMs: Long,
        installedPackageIds: List<String> = emptyList(),
    ): RuntimeContent {
        val startMs = SystemClock.elapsedRealtime()
        return RuntimeContent(
            bootstrap = bootstrap,
            navKvStore = navKvStore,
            installedPackageIds = installedPackageIds,
        ).also {
            diagnosticLogInfo(TAG) {
                "loadInstalledRuntime completed in ${SystemClock.elapsedRealtime() - startMs}ms " +
                    "(navKvOpen=${navKvOpenMs}ms)"
            }
        }
    }

    fun inspectNavDbStatus(
        context: Context,
        libraryCacheJson: String,
        bridge: NativeBridge = NativeBindings,
    ): NavDbStatus {
        val appContext = context.applicationContext
        val installedArtifacts = InstalledPackages.listInstalledArtifacts(appContext)
        val installed = runCatching {
            NavKvStore.inspectInstalledArtifacts(
                installedArtifacts,
                libraryCacheJson = libraryCacheJson,
                bridge = bridge,
            ).statuses
        }.getOrElse {
            installedArtifacts.map { artifact ->
                NavDbArtifactStatus(
                    packageId = artifact.artifactId,
                    filename = artifact.filename,
                    readable = false,
                    message = it.message ?: it::class.simpleName,
                )
            }
        }
        return NavDbStatus(installed = installed)
    }
}
