package com.dspdispatch.relay.connection

import android.util.Log
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.delay

/** Where the relay tells of an error it didn't expect, as a full disk or a bug, as it carries on past it. */
fun interface Unexpected {
    /** [error] happened as the relay was [doing] something. */
    fun happened(doing: String, error: Throwable)
}

/** [Unexpected] errors, told to the phone's own log, read with `adb logcat`. */
object Logcat : Unexpected {
    private const val TAG = "Relay"

    override fun happened(doing: String, error: Throwable) {
        Log.e(TAG, doing, error)
    }
}

/**
 * Runs [work] until it returns, and again after a growing pause each time it fails with an error
 * it didn't expect, told to [unexpected] as [doing]: no one error ends the relay, nor ends the app
 * to fail the same way as it starts again. Cancelling the coroutine stops it.
 */
suspend fun <T> keepRunning(doing: String, unexpected: Unexpected, work: suspend () -> T): T {
    var failures = 0
    while (true) {
        try {
            return work()
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (error: Exception) {
            unexpected.happened(doing, error)
            delay(backoff(++failures))
        }
    }
}
