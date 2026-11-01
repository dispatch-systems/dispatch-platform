package com.dspdispatch.relay.connection

import com.dspdispatch.relay.dispatch.Dispatch
import com.dspdispatch.relay.dispatch.Outbox
import com.dspdispatch.relay.dispatch.Refusal
import com.dspdispatch.relay.pairing.Paired
import java.io.IOException
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.SerializationException

/** How the phone's line to its DSP stands, and since when. */
sealed interface Link {
    data object Connecting : Link

    data class Connected(val since: Long) : Link

    /** Calls are going unanswered, or refused for a while ([code]), since [since]. */
    data class Reconnecting(val since: Long, val code: String? = null) : Link
}

/**
 * The phone's line to its DSP: waiting on the outbox again as each wait ends, which keeps the
 * phone online at the DSP and hands [received] what Dispatch has for it to send.
 */
class Connection(private val dispatch: Dispatch, private val now: () -> Long = System::currentTimeMillis) {
    private val state = MutableStateFlow<Link>(Link.Connecting)
    val link: StateFlow<Link> = state.asStateFlow()

    /** The phone just paired: its DSP answered it, so the line is up before the first wait ends. */
    fun paired() {
        state.value = Link.Connected(now())
    }

    /**
     * Runs until the DSP no longer knows this phone, then returns: removed, or its key unknown.
     * Cancelling the coroutine stops it.
     */
    suspend fun run(paired: Paired, key: String, received: suspend (Outbox) -> Unit) {
        var failures = 0
        while (true) {
            try {
                val outbox = dispatch.outbox(paired.site, key)
                failures = 0
                if (state.value !is Link.Connected) state.value = Link.Connected(now())
                received(outbox)
            } catch (refusal: Refusal) {
                if (refusal.unpaired) return
                failures = unanswered(failures, refusal.code.takeUnless { refusal.passing })
            } catch (_: IOException) {
                failures = unanswered(failures, null)
            } catch (_: SerializationException) {
                // An answer that isn't Dispatch's, as from something between the phone and it.
                failures = unanswered(failures, UNREADABLE)
            }
        }
    }

    private companion object {
        const val UNREADABLE = "unreadable"
    }

    private suspend fun unanswered(failures: Int, code: String?): Int {
        val since = (state.value as? Link.Reconnecting)?.since ?: now()
        state.value = Link.Reconnecting(since, code)
        delay(backoff(failures + 1))
        return failures + 1
    }
}
