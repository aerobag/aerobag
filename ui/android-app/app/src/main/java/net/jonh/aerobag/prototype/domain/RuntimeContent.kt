package net.jonh.aerobag.prototype.domain

import android.content.Context
import android.os.SystemClock
import android.util.Log
import java.time.Instant
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

data class RuntimeBootstrap(
    val packageManagementNowEpochMsOverride: Long?,
)

data class RuntimeContent(
    val bootstrap: RuntimeBootstrap,
    val vectorManifestJson: String,
    val navKvStore: NavKvStore,
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
    val content_policy: String,
    val recent_airport_ids: List<String> = emptyList(),
    val selected_airport_id: String? = null,
    val selected_chart_id: String? = null,
    val package_management_now_utc: String? = null,
)

object AndroidRuntimeContent {
    private const val BOOTSTRAP_ASSET_PATH = "fixtures/dev-bootstrap.json"
    private const val TAG = "AndroidRuntimeContent"
    private val json = Json {
        encodeDefaults = true
        ignoreUnknownKeys = true
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
        return loadInstalledRuntime(
            bootstrap = bootstrap,
            navKvStore = navKvStore,
            navKvOpenMs = navKvOpenMs,
        )
    }

    fun loadInstalledRuntime(
        bootstrap: RuntimeBootstrap,
        navKvStore: NavKvStore,
        navKvOpenMs: Long,
    ): RuntimeContent {
        val startMs = SystemClock.elapsedRealtime()
        val vectorManifestStartMs = SystemClock.elapsedRealtime()
        val vectorManifestJson = navKvStore.runCoreOperationElement(
            buildJsonObject {
                put("kind", "vector_manifest")
            },
        ).toString()
        val vectorManifestMs = SystemClock.elapsedRealtime() - vectorManifestStartMs
        return RuntimeContent(
            bootstrap = bootstrap,
            vectorManifestJson = vectorManifestJson,
            navKvStore = navKvStore,
        ).also {
            Log.i(
                TAG,
                "loadInstalledRuntime completed in ${SystemClock.elapsedRealtime() - startMs}ms " +
                    "(navKvOpen=${navKvOpenMs}ms vectorManifest=${vectorManifestMs}ms)",
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
