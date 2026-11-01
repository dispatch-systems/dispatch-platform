package com.dspdispatch.relay.setup

import android.content.Context
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import com.dspdispatch.relay.RelayService
import com.dspdispatch.relay.pairing.Standing
import com.dspdispatch.relay.relay
import java.util.concurrent.TimeUnit

/**
 * Every 15 minutes while paired, the shortest Android allows: starts the relay again if it isn't
 * running, and checks Set up's steps, notifying a step undone.
 */
class Watchdog(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result {
        val relay = applicationContext.relay
        if (relay.pairings.standing.value !is Standing.Pairing) {
            WorkManager.getInstance(applicationContext).cancelUniqueWork(NAME)
            return Result.success()
        }
        if (!RelayService.running) RelayService.start(applicationContext)
        relay.setup.check(notify = true)
        return Result.success()
    }

    companion object {
        private const val NAME = "watchdog"
        private const val EVERY_MINUTES = 15L

        /** Keeps the watchdog scheduled; a no-op while it is. */
        fun schedule(context: Context) {
            WorkManager.getInstance(context).enqueueUniquePeriodicWork(
                NAME,
                ExistingPeriodicWorkPolicy.KEEP,
                PeriodicWorkRequestBuilder<Watchdog>(EVERY_MINUTES, TimeUnit.MINUTES).build(),
            )
        }
    }
}
