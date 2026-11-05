package com.dspdispatch.relay.texts

import com.dspdispatch.relay.connection.backoff
import com.dspdispatch.relay.dispatch.Dispatch
import com.dspdispatch.relay.dispatch.Refusal
import com.dspdispatch.relay.dispatch.StatusesReport
import java.io.IOException
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.SerializationException

/**
 * Tells the phone's DSP how the attempts at its texts went, in batches, each kept on the phone
 * until Dispatch answers it.
 */
class Statuses(private val dispatch: Dispatch, private val sends: Sends) {
    private val wake = Channel<Unit>(Channel.CONFLATED)
    private val answered = MutableStateFlow(0)

    // A report Dispatch refuses as it is goes again halved, until the status it refuses is alone.
    private var most = MOST_TEXTS

    /** How many reports Dispatch answered: each moves a text on, so its outbox may have changed. */
    val told: StateFlow<Int> = answered.asStateFlow()

    /** There may be statuses to tell. */
    fun more() {
        wake.trySend(Unit)
    }

    /**
     * Tells [site] with [key] whatever waits, then waits for more, until the coroutine is
     * cancelled. A call that fails is made again after a growing pause.
     */
    suspend fun run(site: String, key: String) {
        var failures = 0
        while (true) {
            try {
                if (!tell(site, key)) wake.receive()
                failures = 0
            } catch (_: IOException) {
                delay(backoff(++failures))
            } catch (_: Refusal) {
                delay(backoff(++failures))
            } catch (_: SerializationException) {
                // An answer that isn't Dispatch's, as from something between the phone and it.
                delay(backoff(++failures))
            }
        }
    }

    /** Tells the oldest statuses waiting, if any; one Dispatch refuses alone is left, never told. */
    internal suspend fun tell(site: String, key: String): Boolean {
        // A status is at most a few hundred bytes, its error the app's own: a full batch is far
        // within the 64 KiB a call carries.
        val waiting = sends.statuses(most)
        if (waiting.isEmpty()) return false
        try {
            dispatch.statuses(site, key, StatusesReport(waiting.map(Told::reported)))
        } catch (refusal: Refusal) {
            if (refusal.code != Refusal.INVALID_INPUT && refusal.code != Refusal.REQUEST_TOO_LARGE) throw refusal
            if (waiting.size == 1) sends.told(waiting) else most = waiting.size / 2
            return true
        }
        most = MOST_TEXTS
        sends.told(waiting)
        answered.value++
        return true
    }
}
