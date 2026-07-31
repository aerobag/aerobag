// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.job.JobInfo
import android.app.job.JobParameters
import android.app.job.JobScheduler
import android.app.job.JobService
import android.content.BroadcastReceiver
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.PersistableBundle
import android.util.Log
import androidx.annotation.RequiresApi
import androidx.core.app.NotificationCompat
import androidx.work.CoroutineWorker
import androidx.work.Constraints
import androidx.work.ExistingWorkPolicy
import androidx.work.ForegroundInfo
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import androidx.work.workDataOf
import java.io.File
import java.util.UUID
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString

@Serializable
internal enum class DurableOfflinePackageSyncPhase {
    @SerialName("queued")
    Queued,

    @SerialName("running")
    Running,

    @SerialName("awaiting_adoption")
    AwaitingAdoption,

    @SerialName("complete")
    Complete,
}

@Serializable
internal data class DurableOfflinePackageSyncRecord(
    val id: String,
    val command: OfflinePackagesControllerCommandWire.Sync,
    val phase: DurableOfflinePackageSyncPhase,
    val progress: OfflinePackagesSyncProgressWire,
    val message: String,
    @SerialName("created_at_epoch_ms")
    val createdAtEpochMs: Long,
    @SerialName("updated_at_epoch_ms")
    val updatedAtEpochMs: Long,
    @SerialName("cancel_requested")
    val cancelRequested: Boolean = false,
    val summary: OfflinePackagesSyncSummary? = null,
)

internal object OfflinePackageSyncStore {
    private const val DirectoryName = "offline-package-sync"
    private const val StateFilename = "state.json"
    private var initialized = false
    private lateinit var applicationContext: Context
    private val mutableState = MutableStateFlow<DurableOfflinePackageSyncRecord?>(null)

    @Synchronized
    fun observe(context: Context): StateFlow<DurableOfflinePackageSyncRecord?> {
        initialize(context)
        return mutableState
    }

    @Synchronized
    fun read(context: Context): DurableOfflinePackageSyncRecord? {
        initialize(context)
        return mutableState.value
    }

    @Synchronized
    fun begin(
        context: Context,
        command: OfflinePackagesControllerCommandWire.Sync,
    ): DurableOfflinePackageSyncRecord {
        initialize(context)
        val now = System.currentTimeMillis()
        return write(
            DurableOfflinePackageSyncRecord(
                id = UUID.randomUUID().toString(),
                command = command,
                phase = DurableOfflinePackageSyncPhase.Queued,
                progress = OfflinePackagesSyncProgressWire(
                    plannedFetchArtifactIds = command.plan.fetch.toSet(),
                ),
                message = "Waiting to download offline packages",
                createdAtEpochMs = now,
                updatedAtEpochMs = now,
            ),
        )
    }

    @Synchronized
    fun markRunning(
        context: Context,
        syncId: String,
    ): DurableOfflinePackageSyncRecord? =
        update(context, syncId) {
            it.copy(
                phase = DurableOfflinePackageSyncPhase.Running,
                message = it.message.takeIf(String::isNotBlank) ?: "Downloading offline packages",
                updatedAtEpochMs = System.currentTimeMillis(),
            )
        }

    @Synchronized
    fun updateProgress(
        context: Context,
        syncId: String,
        message: String,
        progress: OfflinePackagesSyncProgressWire,
    ): DurableOfflinePackageSyncRecord? =
        update(context, syncId) {
            it.copy(
                phase = DurableOfflinePackageSyncPhase.Running,
                progress = progress.copy(
                    plannedFetchArtifactIds = progress.plannedFetchArtifactIds
                        .ifEmpty { it.command.plan.fetch.toSet() },
                ),
                message = message,
                updatedAtEpochMs = System.currentTimeMillis(),
            )
        }

    @Synchronized
    fun markAwaitingAdoption(
        context: Context,
        syncId: String,
        summary: OfflinePackagesSyncSummary,
    ): DurableOfflinePackageSyncRecord? =
        update(context, syncId) {
            it.copy(
                phase = DurableOfflinePackageSyncPhase.AwaitingAdoption,
                progress = it.progress.copy(activeFetchBytesByArtifactId = emptyMap()),
                message = if (summary.warnings.isEmpty()) {
                    "Downloads complete"
                } else {
                    "Downloads complete with ${summary.warnings.size} warning(s)"
                },
                summary = summary,
                updatedAtEpochMs = System.currentTimeMillis(),
            )
        }

    @Synchronized
    fun markComplete(
        context: Context,
        syncId: String,
        summary: OfflinePackagesSyncSummary,
    ): DurableOfflinePackageSyncRecord? =
        update(context, syncId) {
            it.copy(
                phase = DurableOfflinePackageSyncPhase.Complete,
                progress = it.progress.copy(activeFetchBytesByArtifactId = emptyMap()),
                message = if (summary.warnings.isEmpty()) {
                    "Offline packages are up to date"
                } else {
                    "Offline package sync completed with ${summary.warnings.size} warning(s)"
                },
                summary = summary,
                updatedAtEpochMs = System.currentTimeMillis(),
            )
        }

    @Synchronized
    fun markQueuedAfterSystemStop(
        context: Context,
        syncId: String,
    ): DurableOfflinePackageSyncRecord? =
        update(context, syncId) {
            if (it.cancelRequested || it.phase == DurableOfflinePackageSyncPhase.Complete) {
                it
            } else {
                it.copy(
                    phase = DurableOfflinePackageSyncPhase.Queued,
                    progress = it.progress.copy(activeFetchBytesByArtifactId = emptyMap()),
                    message = "Download interrupted; waiting to resume",
                    updatedAtEpochMs = System.currentTimeMillis(),
                )
            }
        }

    @Synchronized
    fun requestCancel(context: Context): DurableOfflinePackageSyncRecord? {
        initialize(context)
        val current = mutableState.value ?: return null
        if (current.phase == DurableOfflinePackageSyncPhase.Complete) {
            return current
        }
        return write(
            current.copy(
                cancelRequested = true,
                message = "Canceling offline package sync",
                updatedAtEpochMs = System.currentTimeMillis(),
            ),
        )
    }

    @Synchronized
    fun completeCancellation(context: Context): DurableOfflinePackageSyncRecord? {
        initialize(context)
        val current = mutableState.value ?: return null
        return markComplete(
            context,
            current.id,
            OfflinePackagesSyncSummary(
                fetchedCount = current.progress.completedFetchArtifactIds.size,
                gcCount = 0,
                warnings = listOf(
                    OfflinePackagesWarning(
                        artifactId = "sync",
                        familyId = null,
                        regionId = null,
                        message = "sync canceled",
                    ),
                ),
            ),
        )
    }

    @Synchronized
    private fun initialize(context: Context) {
        if (initialized) return
        applicationContext = context.applicationContext
        mutableState.value = runCatching {
            stateFile().takeIf(File::isFile)?.let {
                PackageManagementJson.decodeFromString<DurableOfflinePackageSyncRecord>(it.readText())
            }
        }.onFailure { error ->
            Log.e(LogTag, "failed to restore durable package sync state", error)
        }.getOrNull()
        initialized = true
    }

    @Synchronized
    private fun update(
        context: Context,
        syncId: String,
        transform: (DurableOfflinePackageSyncRecord) -> DurableOfflinePackageSyncRecord,
    ): DurableOfflinePackageSyncRecord? {
        initialize(context)
        val current = mutableState.value?.takeIf { it.id == syncId } ?: return null
        return write(transform(current))
    }

    @Synchronized
    private fun write(record: DurableOfflinePackageSyncRecord): DurableOfflinePackageSyncRecord {
        val target = stateFile()
        target.parentFile?.mkdirs()
        val temp = File(target.parentFile, "${target.name}.tmp")
        temp.writeText(PackageManagementJson.encodeToString(record))
        if (!temp.renameTo(target)) {
            temp.copyTo(target, overwrite = true)
            check(temp.delete()) { "failed to delete ${temp.absolutePath}" }
        }
        mutableState.value = record
        return record
    }

    private fun stateFile(): File =
        File(File(applicationContext.filesDir, DirectoryName), StateFilename)

    private const val LogTag = "OfflinePackages"
}

internal object OfflinePackageSyncScheduler {
    fun schedule(
        context: Context,
        command: OfflinePackagesControllerCommandWire.Sync,
    ): Result<DurableOfflinePackageSyncRecord> = runCatching {
        val appContext = context.applicationContext
        val record = OfflinePackageSyncStore.begin(appContext, command)
        ensureOfflinePackageSyncNotificationChannel(appContext)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            scheduleUserInitiatedTransfer(appContext, record)
        } else {
            scheduleForegroundWorker(appContext, record)
        }
        record
    }.onFailure { error ->
        val record = OfflinePackageSyncStore.read(context)
        if (record != null) {
            OfflinePackageSyncStore.markComplete(
                context,
                record.id,
                failureSummary("failed to schedule offline package sync", error),
            )
        }
    }

    fun cancel(context: Context) {
        val appContext = context.applicationContext
        val record = OfflinePackageSyncStore.requestCancel(appContext) ?: return
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            appContext.getSystemService(JobScheduler::class.java).cancel(OfflinePackageSyncJobId)
        } else {
            WorkManager.getInstance(appContext).cancelUniqueWork(OfflinePackageSyncWorkName)
        }
        OfflinePackageSyncStore.completeCancellation(appContext)
        diagnosticLogInfo("OfflinePackages") { "durable sync cancel requested id=${record.id}" }
    }

    @RequiresApi(Build.VERSION_CODES.UPSIDE_DOWN_CAKE)
    private fun scheduleUserInitiatedTransfer(
        context: Context,
        record: DurableOfflinePackageSyncRecord,
    ) {
        val extras = PersistableBundle().apply {
            putString(OfflinePackageSyncIdExtra, record.id)
        }
        val totalBytes = offlinePackageSyncTotalBytes(record.command)
        val job = JobInfo.Builder(
            OfflinePackageSyncJobId,
            ComponentName(context, OfflinePackageSyncJobService::class.java),
        )
            .setExtras(extras)
            .setRequiredNetworkType(JobInfo.NETWORK_TYPE_ANY)
            .setUserInitiated(true)
            .setPersisted(true)
            .apply {
                if (totalBytes > 0L) {
                    setEstimatedNetworkBytes(totalBytes, 0L)
                }
            }
            .build()
        check(context.getSystemService(JobScheduler::class.java).schedule(job) == JobScheduler.RESULT_SUCCESS) {
            "Android rejected the offline package transfer job"
        }
    }

    private fun scheduleForegroundWorker(
        context: Context,
        record: DurableOfflinePackageSyncRecord,
    ) {
        val request = OneTimeWorkRequestBuilder<OfflinePackageSyncWorker>()
            .setInputData(workDataOf(OfflinePackageSyncIdExtra to record.id))
            .setConstraints(
                Constraints.Builder()
                    .setRequiredNetworkType(NetworkType.CONNECTED)
                    .build(),
            )
            .build()
        WorkManager.getInstance(context).enqueueUniqueWork(
            OfflinePackageSyncWorkName,
            ExistingWorkPolicy.REPLACE,
            request,
        )
    }
}

@RequiresApi(Build.VERSION_CODES.UPSIDE_DOWN_CAKE)
class OfflinePackageSyncJobService : JobService() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private var activeJob: Job? = null
    private var activeSyncId: String? = null

    override fun onCreate() {
        super.onCreate()
        ensureOfflinePackageSyncNotificationChannel(this)
    }

    override fun onStartJob(params: JobParameters): Boolean {
        val syncId = params.extras.getString(OfflinePackageSyncIdExtra) ?: return false
        val record = OfflinePackageSyncStore.read(this)?.takeIf { it.id == syncId } ?: return false
        activeSyncId = syncId
        setNotification(
            params,
            OfflinePackageSyncNotificationId,
            buildOfflinePackageSyncNotification(this, record),
            JOB_END_NOTIFICATION_POLICY_REMOVE,
        )
        activeJob = scope.launch {
            try {
                executeDurableOfflinePackageSync(applicationContext, syncId) { next ->
                    setNotification(
                        params,
                        OfflinePackageSyncNotificationId,
                        buildOfflinePackageSyncNotification(applicationContext, next),
                        JOB_END_NOTIFICATION_POLICY_REMOVE,
                    )
                }
                jobFinished(params, false)
            } catch (error: CancellationException) {
                if (OfflinePackageSyncStore.read(applicationContext)?.cancelRequested == true) {
                    OfflinePackageSyncStore.completeCancellation(applicationContext)
                    jobFinished(params, false)
                }
            } catch (error: Throwable) {
                Log.e(LogTag, "durable package sync failed", error)
                OfflinePackageSyncStore.markAwaitingAdoption(
                    applicationContext,
                    syncId,
                    failureSummary("offline package sync failed", error),
                )
                jobFinished(params, false)
            }
        }
        return true
    }

    override fun onStopJob(params: JobParameters): Boolean {
        activeJob?.cancel(CancellationException("offline package transfer stopped by Android"))
        activeJob = null
        val record = activeSyncId?.let { id ->
            OfflinePackageSyncStore.markQueuedAfterSystemStop(applicationContext, id)
        }
        return record?.cancelRequested != true
    }

    override fun onDestroy() {
        scope.cancel()
        super.onDestroy()
    }

    private companion object {
        const val LogTag = "OfflinePackages"
    }
}

class OfflinePackageSyncWorker(
    appContext: Context,
    workerParams: WorkerParameters,
) : CoroutineWorker(appContext, workerParams) {
    override suspend fun doWork(): Result {
        val syncId = inputData.getString(OfflinePackageSyncIdExtra) ?: return Result.failure()
        val record = OfflinePackageSyncStore.read(applicationContext)
            ?.takeIf { it.id == syncId }
            ?: return Result.failure()
        setForeground(offlinePackageSyncForegroundInfo(applicationContext, record))
        return try {
            executeDurableOfflinePackageSync(applicationContext, syncId) { next ->
                setForeground(offlinePackageSyncForegroundInfo(applicationContext, next))
            }
            Result.success()
        } catch (error: CancellationException) {
            if (OfflinePackageSyncStore.read(applicationContext)?.cancelRequested == true) {
                withContext(NonCancellable) {
                    OfflinePackageSyncStore.completeCancellation(applicationContext)
                }
                Result.failure()
            } else {
                OfflinePackageSyncStore.markQueuedAfterSystemStop(applicationContext, syncId)
                throw error
            }
        } catch (error: Throwable) {
            Log.e("OfflinePackages", "durable package sync failed", error)
            OfflinePackageSyncStore.markAwaitingAdoption(
                applicationContext,
                syncId,
                failureSummary("offline package sync failed", error),
            )
            Result.failure()
        }
    }
}

class OfflinePackageSyncCancelReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent?) {
        if (intent?.action == OfflinePackageSyncCancelAction) {
            OfflinePackageSyncScheduler.cancel(context)
        }
    }
}

private suspend fun executeDurableOfflinePackageSync(
    context: Context,
    syncId: String,
    onStateChanged: suspend (DurableOfflinePackageSyncRecord) -> Unit,
) {
    val record = OfflinePackageSyncStore.markRunning(context, syncId) ?: return
    onStateChanged(record)
    val activeConnections = ActivePackageConnections()
    try {
        val summary = syncOfflinePackages(
            context = context,
            plan = record.command.plan,
            bundle = record.command.bundle,
            packageSourceBaseUrl = record.command.packageSourceBaseUrl,
            packagedArtifactRoot = record.command.packagedArtifactRoot,
            maxParallelFetches = record.command.maxParallelFetches,
            activeConnections = activeConnections,
            onProgress = { message, progress ->
                currentCoroutineContext().ensureActive()
                val next = OfflinePackageSyncStore.updateProgress(
                    context,
                    syncId,
                    message,
                    requireNotNull(progress),
                )
                if (next != null) {
                    onStateChanged(next)
                }
            },
            // Runtime adoption and deletion happen when the foreground session
            // observes AwaitingAdoption. Until then every old package is retained.
            beforeGc = { record.command.plan.gc.toSet() },
        )
        OfflinePackageSyncStore.markAwaitingAdoption(context, syncId, summary)?.let {
            onStateChanged(it)
        }
    } finally {
        activeConnections.disconnectAll()
    }
}

internal fun offlinePackageSyncForegroundInfo(
    context: Context,
    record: DurableOfflinePackageSyncRecord,
): ForegroundInfo =
    ForegroundInfo(
        OfflinePackageSyncNotificationId,
        buildOfflinePackageSyncNotification(context, record),
        ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC,
    )

internal fun buildOfflinePackageSyncNotification(
    context: Context,
    record: DurableOfflinePackageSyncRecord,
): Notification {
    ensureOfflinePackageSyncNotificationChannel(context)
    val totalBytes = offlinePackageSyncTotalBytes(record.command)
    val downloadedBytes = offlinePackageSyncDownloadedBytes(record)
    val completedCount = record.progress.completedFetchArtifactIds.size
    val totalCount = record.command.plan.fetch.size
    val text = when (record.phase) {
        DurableOfflinePackageSyncPhase.Queued -> record.message
        DurableOfflinePackageSyncPhase.Running ->
            "${formatProgressMegabytes(downloadedBytes)} / ${formatProgressMegabytes(totalBytes)} · $completedCount/$totalCount"
        DurableOfflinePackageSyncPhase.AwaitingAdoption -> record.message
        DurableOfflinePackageSyncPhase.Complete -> record.message
    }
    val cancelIntent = PendingIntent.getBroadcast(
        context,
        0,
        Intent(context, OfflinePackageSyncCancelReceiver::class.java)
            .setAction(OfflinePackageSyncCancelAction),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )
    val contentIntent = PendingIntent.getActivity(
        context,
        0,
        Intent(context, MainActivity::class.java)
            .putExtra(OpenOfflinePackagesExtra, true)
            .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )
    return NotificationCompat.Builder(context, OfflinePackageSyncNotificationChannelId)
        .setSmallIcon(R.drawable.plan_view_icon)
        .setContentTitle("Aerobag offline packages")
        .setContentText(text)
        .setStyle(NotificationCompat.BigTextStyle().bigText(text))
        .setOnlyAlertOnce(true)
        .setOngoing(record.phase != DurableOfflinePackageSyncPhase.Complete)
        .setContentIntent(contentIntent)
        .apply {
            if (record.phase == DurableOfflinePackageSyncPhase.Running && totalBytes > 0L) {
                setProgress(
                    Int.MAX_VALUE,
                    ((downloadedBytes.coerceIn(0L, totalBytes).toDouble() / totalBytes) * Int.MAX_VALUE)
                        .toInt(),
                    false,
                )
            } else if (record.phase == DurableOfflinePackageSyncPhase.Queued) {
                setProgress(0, 0, true)
            }
            if (record.phase == DurableOfflinePackageSyncPhase.Queued ||
                record.phase == DurableOfflinePackageSyncPhase.Running
            ) {
                addAction(R.drawable.plan_view_icon, "Cancel", cancelIntent)
            }
        }
        .build()
}

internal fun ensureOfflinePackageSyncNotificationChannel(context: Context) {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
    context.getSystemService(NotificationManager::class.java).createNotificationChannel(
        NotificationChannel(
            OfflinePackageSyncNotificationChannelId,
            "Offline package sync",
            NotificationManager.IMPORTANCE_LOW,
        ).apply {
            description = "Progress while Aerobag downloads offline charts and data"
        },
    )
}

internal fun offlinePackageSyncTotalBytes(command: OfflinePackagesControllerCommandWire.Sync): Long {
    val packagesById = command.bundle.packages.associateBy { it.id }
    return command.plan.fetch.sumOf { packagesById[it]?.sizeBytes ?: 0L }
}

internal fun offlinePackageSyncDownloadedBytes(record: DurableOfflinePackageSyncRecord): Long {
    val packagesById = record.command.bundle.packages.associateBy { it.id }
    val completedBytes = record.progress.completedFetchArtifactIds.sumOf {
        packagesById[it]?.sizeBytes ?: 0L
    }
    return completedBytes + record.progress.activeFetchBytesByArtifactId.values.sum()
}

private fun failureSummary(
    prefix: String,
    error: Throwable,
): OfflinePackagesSyncSummary =
    OfflinePackagesSyncSummary(
        fetchedCount = 0,
        gcCount = 0,
        warnings = listOf(
            OfflinePackagesWarning(
                artifactId = "sync",
                familyId = null,
                regionId = null,
                message = "$prefix: ${error.message ?: error::class.simpleName ?: "unknown error"}",
            ),
        ),
    )

internal const val OpenOfflinePackagesExtra = "org.aerobag.app.extra.OPEN_OFFLINE_PACKAGES"
private const val OfflinePackageSyncIdExtra = "offline_package_sync_id"
private const val OfflinePackageSyncWorkName = "offline-package-sync"
private const val OfflinePackageSyncJobId = 31_407
private const val OfflinePackageSyncNotificationChannelId = "aerobag_offline_packages"
private const val OfflinePackageSyncNotificationId = 1_201
private const val OfflinePackageSyncCancelAction = "org.aerobag.app.action.CANCEL_OFFLINE_PACKAGE_SYNC"
