package org.aerobag.app.domain

import android.content.Context
import android.os.SystemClock
import android.util.Log
import java.time.Instant
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

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

    fun loadInstalledRuntime(context: Context, bootstrap: RuntimeBootstrap): RuntimeContent {
        val navKvOpenStartMs = SystemClock.elapsedRealtime()
        val navKvStore = openInstalledNavDb(context).first
        val navKvOpenMs = SystemClock.elapsedRealtime() - navKvOpenStartMs
        val installedPackageIds = InstalledPackages.listInstalledArtifacts(context.applicationContext)
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
            Log.i(
                TAG,
                "loadInstalledRuntime completed in ${SystemClock.elapsedRealtime() - startMs}ms " +
                    "(navKvOpen=${navKvOpenMs}ms)",
            )
        }
    }

    fun inspectNavDbStatus(
        context: Context,
        bridge: NativeBridge = NativeBindings,
    ): NavDbStatus {
        val appContext = context.applicationContext
        val installed = runCatching {
            NavKvStore.inspectCandidates(installedNavDbArtifacts(appContext), bridge = bridge).statuses
        }.getOrElse {
            installedNavDbArtifacts(appContext).map { artifact ->
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

    private fun openInstalledNavDb(
        context: Context,
        bridge: NativeBridge = NativeBindings,
    ): Pair<NavKvStore, NavDbOpenReport> =
        NavKvStore.openCandidates(installedNavDbArtifacts(context.applicationContext), bridge = bridge)

    private fun installedNavDbArtifacts(context: Context): List<InstalledPackageArtifact> =
        InstalledPackages.listInstalledArtifacts(context)
            .filter { it.artifactId.startsWith("NAV_DB_") }
            .sortedWith(
                compareByDescending<InstalledPackageArtifact> { it.file.lastModified() }
                    .thenByDescending { it.filename },
            )
}
