package net.jonh.aerobag.prototype.domain

import android.content.Context
import kotlinx.serialization.KSerializer
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import java.io.File
import java.util.Locale
import java.util.zip.ZipFile

class NavKvStore private constructor(
    private val context: Context,
    private val bridge: NativeBridge,
    private val json: Json,
    private val handle: Long,
    private val navDbPackageId: String,
    private val valueEntryPrefix: String,
) : AutoCloseable {
    private val loadedPages = mutableSetOf<Int>()

    companion object {
        fun open(
            context: Context,
            navDbPackageId: String,
            bridge: NativeBridge = NativeBindings,
            json: Json = Json {
                encodeDefaults = true
                ignoreUnknownKeys = true
            },
        ): NavKvStore {
            val appContext = context.applicationContext
            val navDbZip = InstalledPackages.installedFile(appContext, InstalledPackageKind.Data, navDbPackageId)
            val layout = detectNavKvLayout(navDbZip)
            val rootBytes = InstalledPackages.readZipEntryBytes(navDbZip, layout.rootEntryName)
            val handle = bridge.navKvOpen(rootBytes)
            return NavKvStore(appContext, bridge, json, handle, navDbPackageId, layout.valueEntryPrefix)
        }

        private fun detectNavKvLayout(zipFile: File): NavKvLayout {
            ZipFile(zipFile).use { zip ->
                val rootEntryName = zip.entries().asSequence()
                    .map { it.name }
                    .firstOrNull { it.endsWith(".root") }
                    ?: error("missing *.root entry in ${zipFile.absolutePath}")
                val valueEntryPrefix = rootEntryName.removeSuffix(".root") + ".values_"
                return NavKvLayout(rootEntryName = rootEntryName, valueEntryPrefix = valueEntryPrefix)
            }
        }
    }

    fun runCoreOperationElement(operation: JsonObject): JsonElement {
        while (true) {
            val outcome = json.parseToJsonElement(bridge.coreHadOperation(handle, operation.toString())).jsonObject
            return when (val state = outcome.getValue("state").jsonPrimitive.content) {
                "complete" -> outcome["result"] ?: JsonNull
                "need_pages" -> {
                    for (page in outcome.getValue("pages").jsonArray) {
                        ensurePage(page.jsonPrimitive.content.toInt())
                    }
                    continue
                }
                else -> error("unknown HAD operation state: $state")
            }
        }
    }

    fun <T> runCoreOperation(operation: JsonObject, serializer: KSerializer<T>): T =
        json.decodeFromJsonElement(serializer, runCoreOperationElement(operation))

    @Synchronized
    private fun ensurePage(pageIndex: Int) {
        if (!loadedPages.add(pageIndex)) {
            return
        }
        val pageName = String.format(Locale.US, "%04d", pageIndex)
        val pageBytes = InstalledPackages.readZipEntryBytes(
            context,
            InstalledPackageKind.Data,
            navDbPackageId,
            "$valueEntryPrefix$pageName",
        )
        bridge.navKvInsertPage(handle, pageIndex, pageBytes)
    }

    override fun close() {
        bridge.navKvDestroy(handle)
    }
}

private data class NavKvLayout(
    val rootEntryName: String,
    val valueEntryPrefix: String,
)
