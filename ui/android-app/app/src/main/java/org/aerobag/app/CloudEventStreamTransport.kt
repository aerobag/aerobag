// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import java.net.SocketTimeoutException
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import okhttp3.Call
import okhttp3.OkHttpClient
import okhttp3.Request
import org.aerobag.app.generated.CloudEventStreamEvent
import org.aerobag.app.generated.CloudEventStreamEventKind
import org.aerobag.app.generated.CloudEventStreamPlan

internal class AndroidCloudEventStreamTransport(
    private val httpClient: OkHttpClient = OkHttpClient(),
) : AutoCloseable {
    private var active: ActiveStream? = null

    fun reconcile(
        plan: CloudEventStreamPlan?,
        scope: CoroutineScope,
        report: (CloudEventStreamEvent) -> Unit,
    ) {
        if (active?.streamId == plan?.streamId) return
        closeActive()
        if (plan == null) return

        val job = scope.launch(Dispatchers.IO) {
            execute(plan, report)
        }
        active = ActiveStream(plan.streamId, job)
    }

    override fun close() {
        closeActive()
    }

    private fun closeActive() {
        active?.job?.cancel()
        active = null
    }

    private suspend fun execute(
        plan: CloudEventStreamPlan,
        report: (CloudEventStreamEvent) -> Unit,
    ) {
        report(plan.event(CloudEventStreamEventKind.Connecting))
        val client = httpClient.newBuilder()
            .connectTimeout(plan.connectTimeoutMs, TimeUnit.MILLISECONDS)
            .readTimeout(plan.idleTimeoutMs, TimeUnit.MILLISECONDS)
            .build()
        val call = client.newCall(
            Request.Builder()
                .url(plan.url)
                .header("Accept", "text/event-stream")
                .build(),
        )
        cancelCallWithCoroutine(call)

        try {
            call.execute().use { response ->
                if (!response.isSuccessful) {
                    reportIfActive(
                        plan.event(
                            CloudEventStreamEventKind.Error,
                            detail = "Aerobag Cloud event stream HTTP ${response.code}: ${response.message}",
                        ),
                        report,
                    )
                    return
                }
                val body = response.body
                if (body == null) {
                    reportIfActive(
                        plan.event(
                            CloudEventStreamEventKind.Error,
                            detail = "Aerobag Cloud event stream response has no body.",
                        ),
                        report,
                    )
                    return
                }

                reportIfActive(plan.event(CloudEventStreamEventKind.Connected), report)
                val accumulator = SseDataAccumulator()
                while (currentCoroutineContext().isActive) {
                    val line = body.source().readUtf8Line()
                    val message = accumulator.accept(line)
                    if (message != null) {
                        reportIfActive(
                            plan.event(CloudEventStreamEventKind.Message, data = message),
                            report,
                        )
                    }
                    if (line == null) {
                        reportIfActive(plan.event(CloudEventStreamEventKind.Closed), report)
                        return
                    }
                }
            }
        } catch (error: SocketTimeoutException) {
            reportIfActive(
                plan.event(
                    CloudEventStreamEventKind.IdleTimeout,
                    detail = error.message ?: "Aerobag Cloud event stream idle timeout.",
                ),
                report,
            )
        } catch (error: Throwable) {
            reportIfActive(
                plan.event(
                    CloudEventStreamEventKind.Error,
                    detail = error.message ?: error::class.simpleName ?: "Event stream failed.",
                ),
                report,
            )
        }
    }

    private suspend fun reportIfActive(
        event: CloudEventStreamEvent,
        report: (CloudEventStreamEvent) -> Unit,
    ) {
        if (currentCoroutineContext().isActive) report(event)
    }

    private suspend fun cancelCallWithCoroutine(call: Call) {
        currentCoroutineContext()[Job]?.invokeOnCompletion { call.cancel() }
    }

    private data class ActiveStream(
        val streamId: Long,
        val job: Job,
    )
}

internal class SseDataAccumulator {
    private val dataLines = mutableListOf<String>()

    fun accept(line: String?): String? {
        if (line == null || line.isEmpty()) {
            if (dataLines.isEmpty()) return null
            return dataLines.joinToString("\n").also { dataLines.clear() }
        }
        if (line.startsWith("data:")) {
            dataLines += line.removePrefix("data:").removePrefix(" ")
        }
        return null
    }
}

private fun CloudEventStreamPlan.event(
    kind: CloudEventStreamEventKind,
    data: String? = null,
    detail: String? = null,
): CloudEventStreamEvent = CloudEventStreamEvent(
    streamId = streamId,
    kind = kind,
    data = data,
    detail = detail,
)
