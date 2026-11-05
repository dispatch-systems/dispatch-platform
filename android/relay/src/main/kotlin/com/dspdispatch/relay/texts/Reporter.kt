package com.dspdispatch.relay.texts

import android.database.SQLException
import com.dspdispatch.relay.connection.Unexpected
import com.dspdispatch.relay.connection.backoff
import com.dspdispatch.relay.dispatch.Dispatch
import com.dspdispatch.relay.dispatch.Refusal
import java.io.IOException
import kotlin.coroutines.CoroutineContext
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.ReceiveChannel
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerializationException

/**
 * Reports what the [queue] holds to the phone's DSP: its texts in batches, each until Dispatch
 * answers it, then each file's bytes, streamed from the [provider] while [texting] allows it, or
 * from the [jpegs] made of its HEIF pictures, both read on [io]. A file whose bytes can't be read
 * as nothing expected is told to [unexpected], and left.
 */
class Reporter(
    private val dispatch: Dispatch,
    private val queue: Queue,
    private val provider: Provider,
    private val texting: Texting,
    private val jpegs: Jpegs,
    private val unexpected: Unexpected,
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
        val waiting = settled(queue.waiting(most))
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

    /**
     * Sends the bytes of the oldest file Dispatch waits for, if any, once Texting lets it read them.
     * One Dispatch would refuse again however often it's sent, or that can't be read, is left, for
     * those after it to go.
     */
    internal suspend fun upload(site: String, key: String): Boolean {
        val file = queue.upload() ?: return false
        val id = checkNotNull(file.dispatchId)
        texting.await()
        val parts = if (jpegs.holds(file.uri)) jpegs else provider
        try {
            // Measured as it's sent: the part may have changed since it was read, and Dispatch fails
            // an upload holding fewer bytes than it said.
            val size = withContext(io) { reading { parts.size(file.uri) } }
            if (size != null) dispatch.file(site, key, id, size) { reading { parts.open(file.uri) } }
        } catch (refusal: Refusal) {
            if (!refusal.lasting) throw refusal
        } catch (unreadable: Unreadable) {
            unexpected.happened("Reading a file to send", unreadable)
        }
        queue.uploaded(file.id)
        withContext(io) { jpegs.remove(file.uri) }
        return true
    }

    /**
     * [texts] with each file as it's reported every time: one not yet [WaitingFile.settled] as
     * Dispatch is to have it, kept so before it's first reported. Dispatch keeps a file as it's
     * first told of it, though the answer is lost, so it never changes after: not when a JPEG
     * could be made of it later, nor in another version of the app.
     */
    private suspend fun settled(texts: List<WaitingText>): List<WaitingText> {
        val settled = texts.flatMap(WaitingText::files).filterNot(WaitingFile::settled).associate { it.id to shown(it).copy(settled = true) }
        if (settled.isEmpty()) return texts
        queue.settled(settled.values.toList())
        return texts.map { text -> text.copy(files = text.files.map { settled[it.id] ?: it }) }
    }

    /**
     * [file] as Dispatch is to have it: a still picture received as HEIF, which Dispatch's pages
     * can't show, as a JPEG made of it, once Texting lets its bytes be read; anything else, and
     * one Android can't make a JPEG of, as it is.
     */
    private suspend fun shown(file: WaitingFile): WaitingFile {
        if (!heif(file.name, file.mime) || !jpegs.possible) return file
        texting.await()
        return withContext(io) {
            // A part gone, or grown past what Dispatch takes, goes as it is: its upload finds it so.
            val size = provider.size(file.uri)
            if (size == null || size > Inbox.LARGEST) null else jpegs.of(file) { provider.open(file.uri) }
        } ?: file
    }

    /** Leaves [text], which Dispatch refuses alone, with the JPEGs made of its pictures. */
    private suspend fun refused(text: WaitingText) {
        queue.refused(text.text.clientId)
        withContext(io) { text.files.forEach { jpegs.remove(it.uri) } }
    }

    // Opened on the network's own thread too, where anything but an IOException ends the app.
    private inline fun <T> reading(read: () -> T): T = try {
        read()
    } catch (lost: SecurityException) {
        throw IOException("Texting was turned off", lost)
    } catch (away: SQLException) {
        // The provider's database failing a moment, as it may across Binder: the file is still there.
        throw IOException("The texting provider failed", away)
    } catch (failed: RuntimeException) {
        throw Unreadable(failed)
    }

    /** A file's bytes that can't be read as nothing expected, which reading again won't change. */
    private class Unreadable(cause: RuntimeException) : IOException(cause)

    private companion object {
        /**
         * Refusals of a file's bytes worth sending them again for: while the DSP, or its
         * Communications, is away, and for bytes that came too slowly.
         */
        val AGAIN = setOf(Refusal.SIGN_IN_REQUIRED, Refusal.PHONES_OFF, Refusal.PHONES_REMOVED, Refusal.UPLOAD_INCOMPLETE)

        /** Whether Dispatch would refuse the same file's bytes again, however often they're sent. */
        val Refusal.lasting get() = !passing && code !in AGAIN
    }
}
