package com.dspdispatch.relay.texts

import com.dspdispatch.relay.connection.backoff
import com.dspdispatch.relay.dispatch.Dispatch
import com.dspdispatch.relay.dispatch.Refusal
import java.io.IOException
import java.io.InputStream
import kotlin.coroutines.CoroutineContext
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.ReceiveChannel
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerializationException

/**
 * Reports what the [queue] holds to the phone's DSP: its texts in batches, each until Dispatch
 * answers it, then each file's bytes, streamed from the [provider] while [texting] allows it, or
 * from the [jpegs] made of its HEIF pictures, both read on [io].
 */
class Reporter(
    private val dispatch: Dispatch,
    private val queue: Queue,
    private val provider: Provider,
    private val texting: Texting,
    private val jpegs: Jpegs,
    private val io: CoroutineContext = Dispatchers.IO,
) {
    // A report Dispatch refuses as it is goes again halved, until the text it refuses is alone.
    private var most = MOST_TEXTS

    /**
     * Reports to [site] with [key] whatever waits, then waits on [wake] for more, until the
     * coroutine is cancelled. A call that fails is made again after a growing pause.
     */
    suspend fun run(site: String, key: String, wake: ReceiveChannel<Unit>) {
        // JPEGs of no file waiting: an earlier pairing's, or left as the app stopped.
        withContext(io) { jpegs.keepOnly(queue.uris()) }
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
            } catch (_: SecurityException) {
                // Texting was turned off as a file was read: it stays, sent once Texting is back.
                delay(backoff(++failures))
            }
        }
    }

    /** Reports the oldest texts waiting, if any. */
    internal suspend fun report(site: String, key: String): Boolean {
        val waiting = queue.waiting(most).map { it.copy(files = it.files.map { file -> shown(file) }) }
        val report = batch(waiting)
        if (report.texts.isEmpty()) return false
        val answer = try {
            dispatch.texts(site, key, report)
        } catch (refusal: Refusal) {
            if (refusal.code != Refusal.INVALID_INPUT && refusal.code != Refusal.REQUEST_TOO_LARGE) throw refusal
            if (report.texts.size == 1) refused(waiting.first { it.text.clientId == report.texts.single().clientId }) else most = report.texts.size / 2
            return true
        }
        most = MOST_TEXTS
        queue.answered(answer.texts.associate { it.clientId to it.files })
        return true
    }

    /** Sends the bytes of the oldest file Dispatch waits for, if any, once Texting lets it read them. */
    internal suspend fun upload(site: String, key: String): Boolean {
        val file = queue.upload() ?: return false
        val id = checkNotNull(file.dispatchId)
        texting.await()
        val parts = if (jpegs.holds(file.uri)) jpegs else provider
        // Measured as it's sent: the part may have changed since it was read, and Dispatch fails
        // an upload holding fewer bytes than it said.
        val size = withContext(io) { parts.size(file.uri) }
        if (size != null) {
            try {
                dispatch.file(site, key, id, size) { opened(parts, file.uri) }
            } catch (refusal: Refusal) {
                if (refusal.code != Refusal.FILE_NOT_FOUND && refusal.code != Refusal.UPLOAD_TOO_LARGE) throw refusal
            }
        }
        queue.uploaded(file.id)
        withContext(io) { jpegs.remove(file.uri) }
        return true
    }

    /**
     * [file] as Dispatch is to have it: a picture received as HEIF, which Dispatch's pages can't
     * show, as a JPEG made of it, once Texting lets its bytes be read; anything else, and one
     * Android can't make a JPEG of, as it is.
     */
    private suspend fun shown(file: WaitingFile): WaitingFile {
        if (!heif(file.name, file.mime) || !jpegs.possible) return file
        texting.await()
        val jpeg = withContext(io) {
            // A part gone, or grown past what Dispatch takes, goes as it is: its upload finds it so.
            val size = provider.size(file.uri)
            if (size == null || size > Inbox.LARGEST) null else jpegs.of(file, provider.open(file.uri).use(InputStream::readBytes))
        } ?: return file
        queue.converted(jpeg)
        return jpeg
    }

    /** Leaves [text], which Dispatch refuses alone, with the JPEGs made of its pictures. */
    private suspend fun refused(text: WaitingText) {
        queue.refused(text.text.clientId)
        withContext(io) { text.files.forEach { jpegs.remove(it.uri) } }
    }

    // Opened on the network's own thread, where anything but an IOException ends the app.
    private fun opened(parts: PartBytes, uri: String) = try {
        parts.open(uri)
    } catch (lost: SecurityException) {
        throw IOException("Texting was turned off", lost)
    }
}
