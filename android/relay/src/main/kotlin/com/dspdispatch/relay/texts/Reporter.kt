package com.dspdispatch.relay.texts

import com.dspdispatch.relay.connection.backoff
import com.dspdispatch.relay.dispatch.Dispatch
import com.dspdispatch.relay.dispatch.Refusal
import java.io.IOException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.ReceiveChannel
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerializationException

/**
 * Reports what the [queue] holds to the phone's DSP: its texts in batches, each until Dispatch
 * answers it, then each file's bytes, streamed from the [provider].
 */
class Reporter(private val dispatch: Dispatch, private val queue: Queue, private val provider: Provider) {
    // A report Dispatch refuses as it is goes again halved, until the text it refuses is alone.
    private var most = MOST_TEXTS

    /**
     * Reports to [site] with [key] whatever waits, then waits on [wake] for more, until the
     * coroutine is cancelled. A call that fails is made again after a growing pause.
     */
    suspend fun run(site: String, key: String, wake: ReceiveChannel<Unit>) {
        var failures = 0
        while (true) {
            try {
                if (!report(site, key) && !upload(site, key)) wake.receive()
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

    /** Reports the oldest texts waiting, if any. */
    internal suspend fun report(site: String, key: String): Boolean {
        val report = batch(queue.waiting(most))
        if (report.texts.isEmpty()) return false
        val answer = try {
            dispatch.texts(site, key, report)
        } catch (refusal: Refusal) {
            if (refusal.code != Refusal.INVALID_INPUT && refusal.code != Refusal.REQUEST_TOO_LARGE) throw refusal
            if (report.texts.size == 1) queue.refused(report.texts.single().clientId) else most = report.texts.size / 2
            return true
        }
        most = MOST_TEXTS
        queue.answered(answer.texts.associate { it.clientId to it.files })
        return true
    }

    /** Sends the bytes of the oldest file Dispatch waits for, if any. */
    internal suspend fun upload(site: String, key: String): Boolean {
        val file = queue.upload() ?: return false
        val id = checkNotNull(file.dispatchId)
        // Measured as it's sent: the part may have changed since it was read, and Dispatch fails
        // an upload holding fewer bytes than it said.
        val size = withContext(Dispatchers.IO) { provider.size(file.uri) }
        if (size != null) {
            try {
                dispatch.file(site, key, id, size) { provider.open(file.uri) }
            } catch (refusal: Refusal) {
                if (refusal.code != Refusal.FILE_NOT_FOUND && refusal.code != Refusal.UPLOAD_TOO_LARGE) throw refusal
            }
        }
        queue.uploaded(file.id)
        return true
    }
}
