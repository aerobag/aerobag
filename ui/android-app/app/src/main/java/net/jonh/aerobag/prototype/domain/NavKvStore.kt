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
import java.util.Locale

private const val NAV_KV_ROOT_ASSET_PATH = "nav-kv/root"
private const val NAV_KV_VALUE_ASSET_ROOT = "nav-kv/values"

class NavKvStore private constructor(
    private val context: Context,
    private val bridge: NativeBridge,
    private val json: Json,
    private val handle: Long,
) : AutoCloseable {
    private val loadedPages = mutableSetOf<Int>()

    companion object {
        fun open(
            context: Context,
            bridge: NativeBridge = NativeBindings,
            json: Json = Json {
                encodeDefaults = true
                ignoreUnknownKeys = true
            },
        ): NavKvStore {
            val appContext = context.applicationContext
            val rootBytes = appContext.assets.open(NAV_KV_ROOT_ASSET_PATH).use { it.readBytes() }
            val handle = bridge.navKvOpen(rootBytes)
            return NavKvStore(appContext, bridge, json, handle)
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
        val pageBytes = context.assets.open("$NAV_KV_VALUE_ASSET_ROOT/$pageName").use { it.readBytes() }
        bridge.navKvInsertPage(handle, pageIndex, pageBytes)
    }

    override fun close() {
        bridge.navKvDestroy(handle)
    }
}
