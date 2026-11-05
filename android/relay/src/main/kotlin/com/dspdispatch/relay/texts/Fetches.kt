package com.dspdispatch.relay.texts

import android.database.SQLException
import com.dspdispatch.relay.dispatch.Dispatch
import com.dspdispatch.relay.dispatch.Refusal
import java.io.IOException
import kotlin.coroutines.CoroutineContext
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerializationException

/**
 * Sends the bytes of the videos the phone keeps that Dispatch asks for, as someone plays one, from
 * the [provider] while [texting] allows it, read on [io]: each once each time it's asked for. One
 * the provider holds no more is said to be gone, and one the [queue] doesn't know too, once no
 * text waits to be reported whose answer may yet name it. One that can't be sent now is left:
 * whoever plays it asks again.
 */
class Fetches(
    private val dispatch: Dispatch,
    private val queue: Queue,
    private val provider: PartBytes,
    private val texting: Texting,
    private val io: CoroutineContext = Dispatchers.IO,
) {
    private val asked = Channel<List<String>>(Channel.UNLIMITED)

    /** Dispatch asks for the videos [ids], as its outbox's answer says. */
    fun ask(ids: List<String>) {
        if (ids.isNotEmpty()) asked.trySend(ids)
    }

    /** Sends to [site] with [key] each video asked for, in turn, until the coroutine is cancelled. */
    suspend fun run(site: String, key: String) {
        for (ids in asked) ids.forEach { send(site, key, it) }
    }

    /** Sends the video Dispatch knows as [id], or says it's gone. */
    internal suspend fun send(site: String, key: String, id: String) {
        try {
            // A report Dispatch kept, its answer lost, is answered again: its videos known then, as
            // its text leaves the queue, so looked for again once none waits.
            val video = queue.onPhone(id) ?: if (queue.waiting(1).isEmpty()) queue.onPhone(id) else return
            texting.await()
            val size = video?.let { withContext(io) { provider.size(it.uri) } }
            if (video == null || size == null) {
                dispatch.gone(site, key, id)
                queue.goneFromPhone(id)
            } else {
                dispatch.file(site, key, id, size) { opened(video.uri) }
            }
        } catch (_: IOException) {
            // The network, or the provider, away a moment.
        } catch (_: Refusal) {
        } catch (_: SerializationException) {
        } catch (_: SecurityException) {
            // Texting was turned off as it was read.
        } catch (_: SQLException) {
        }
    }

    // Opened on the network's own thread, where anything but an IOException ends the app.
    private fun opened(uri: String) = try {
        provider.open(uri)
    } catch (failed: RuntimeException) {
        throw IOException("The video couldn't be read", failed)
    }
}
