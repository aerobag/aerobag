// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.domain

data class PagedSessionOperationMetricsSnapshot(
    val coreCallCount: Int,
    val coreCallUs: Long,
    val resourceRoundCount: Int,
    val resourceRequestCount: Int,
    val resourceLoadCount: Int,
    val resourceCacheHitCount: Int,
    val resourceBytes: Long,
    val resourceFetchUs: Long,
    val resourceIngestUs: Long,
    val resourceRounds: List<PagedResourceRoundMetricsSnapshot>,
)

data class PagedResourceRoundMetricsSnapshot(
    val index: Int,
    val resourceIds: List<String>,
    val sourceKinds: Map<String, Int>,
    val fetchWallUs: Long,
    val fetchWorkUs: Long,
    val maxConcurrency: Int,
    val ingestUs: Long,
)

class PagedSessionOperationMetrics {
    private var coreCallCount = 0
    private var coreCallUs = 0L
    private var resourceRoundCount = 0
    private var resourceRequestCount = 0
    private var resourceLoadCount = 0
    private var resourceCacheHitCount = 0
    private var resourceBytes = 0L
    private var resourceFetchUs = 0L
    private var resourceIngestUs = 0L
    private val resourceRounds = mutableListOf<MutableResourceRoundMetrics>()
    private var activeResourceRoundIndex: Int? = null

    fun <T> measureCoreCall(operation: () -> T): T {
        val startedNs = System.nanoTime()
        return try {
            operation()
        } finally {
            coreCallCount += 1
            coreCallUs += elapsedUs(startedNs)
        }
    }

    fun beginResourceRound(resources: List<CoreResourceRequest>): Int {
        resourceRoundCount += 1
        return resourceRounds.size.also { index ->
            resourceRounds += MutableResourceRoundMetrics(
                index = index + 1,
                resourceIds = resources.map(CoreResourceRequest::id),
                sourceKinds = resources
                    .groupingBy { it.source.kindForLog() }
                    .eachCount()
                    .toSortedMap(),
            )
            activeResourceRoundIndex = index
        }
    }

    internal fun recordResourceBatch(roundIndex: Int, batch: ResourceFrontierLoadBatch) {
        val round = resourceRounds[roundIndex]
        round.fetchWallUs = batch.wallUs
        round.fetchWorkUs = batch.outcomes.sumOf(ResourceFrontierLoadOutcome::elapsedUs)
        round.maxConcurrency = batch.maxConcurrency
        resourceRequestCount += batch.outcomes.size
        resourceLoadCount += batch.outcomes.count { it.bytes != null }
        resourceBytes += batch.outcomes.sumOf { it.bytes?.size?.toLong() ?: 0L }
        resourceFetchUs += round.fetchWorkUs
    }

    fun finishResourceRound(roundIndex: Int) {
        check(activeResourceRoundIndex == roundIndex) {
            "resource metrics round finished out of order"
        }
        activeResourceRoundIndex = null
    }

    fun recordResourceCacheHit() {
        resourceRequestCount += 1
        resourceCacheHitCount += 1
    }

    fun <T> measureResourceLoad(byteCount: (T) -> Int, operation: () -> T): T {
        resourceRequestCount += 1
        val startedNs = System.nanoTime()
        return try {
            operation().also { value ->
                resourceLoadCount += 1
                resourceBytes += byteCount(value).toLong()
            }
        } finally {
            resourceFetchUs += elapsedUs(startedNs)
        }
    }

    fun <T> measureResourceIngest(operation: () -> T): T {
        val startedNs = System.nanoTime()
        return try {
            operation()
        } finally {
            val elapsedUs = elapsedUs(startedNs)
            resourceIngestUs += elapsedUs
            activeResourceRoundIndex?.let { resourceRounds[it].ingestUs += elapsedUs }
        }
    }

    fun snapshot(): PagedSessionOperationMetricsSnapshot = PagedSessionOperationMetricsSnapshot(
        coreCallCount = coreCallCount,
        coreCallUs = coreCallUs,
        resourceRoundCount = resourceRoundCount,
        resourceRequestCount = resourceRequestCount,
        resourceLoadCount = resourceLoadCount,
        resourceCacheHitCount = resourceCacheHitCount,
        resourceBytes = resourceBytes,
        resourceFetchUs = resourceFetchUs,
        resourceIngestUs = resourceIngestUs,
        resourceRounds = resourceRounds.map(MutableResourceRoundMetrics::snapshot),
    )

    private fun elapsedUs(startedNs: Long): Long =
        ((System.nanoTime() - startedNs).coerceAtLeast(0L) + 500L) / 1_000L
}

private data class MutableResourceRoundMetrics(
    val index: Int,
    val resourceIds: List<String>,
    val sourceKinds: Map<String, Int>,
    var fetchWallUs: Long = 0L,
    var fetchWorkUs: Long = 0L,
    var maxConcurrency: Int = 0,
    var ingestUs: Long = 0L,
) {
    fun snapshot(): PagedResourceRoundMetricsSnapshot = PagedResourceRoundMetricsSnapshot(
        index = index,
        resourceIds = resourceIds,
        sourceKinds = sourceKinds,
        fetchWallUs = fetchWallUs,
        fetchWorkUs = fetchWorkUs,
        maxConcurrency = maxConcurrency,
        ingestUs = ingestUs,
    )
}
