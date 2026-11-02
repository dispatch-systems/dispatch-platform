package com.dspdispatch.relay.texts

import com.dspdispatch.relay.connection.backoff
import com.dspdispatch.relay.dispatch.Dispatch
import com.dspdispatch.relay.dispatch.OutgoingText
import com.dspdispatch.relay.dispatch.Outbox
import com.dspdispatch.relay.dispatch.Refusal
import com.dspdispatch.relay.pairing.Paired
import java.io.File
import java.io.IOException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull

/**
 * Sends the texts Dispatch has for the phone from its own number, each attempt once: kept as
 * started before anything of it goes, a picture message's files fetched first into [folder]. While
 * [mmsWhenBusy], a text that would cross Android's SMS limit goes as a picture message instead, or
 * fails where the carrier takes none.
 */
class Sender(
    private val dispatch: Dispatch,
    private val sends: Sends,
    private val own: OwnRows,
    private val radio: Radio,
    private val message: PictureMessage,
    private val statuses: Statuses,
    private val folder: File,
    private val mmsWhenBusy: () -> Boolean,
    private val now: () -> Long = System::currentTimeMillis,
) {
    private val answers = Channel<Pair<Paired, Outbox>>(Channel.CONFLATED)

    // The attempts whose files couldn't be fetched, each with how many times in a row and when to
    // try again. Only the sender's own coroutine touches it.
    private val retries = mutableMapOf<Pair<String, Long>, Retry>()

    /** Held while an answer's texts are sent, and while the app's update replaces it ([hold]). */
    private val gate = Mutex()

    /** How many times Android has said more of a send, to wake [hold]. */
    private val answered = MutableStateFlow(0)

    /**
     * Takes [outbox], Dispatch's answer to [paired]'s wait, and returns once it's worth waiting
     * again: at once when it holds nothing. A text stays in each answer until Dispatch hears how
     * it went, so else once Dispatch heard how one went, or after a moment, for any new one.
     */
    suspend fun take(paired: Paired, outbox: Outbox) {
        if (outbox.texts.isEmpty()) return
        val told = statuses.told.value
        answers.send(paired to outbox)
        withTimeoutOrNull(HOLD_MS) { statuses.told.first { it != told } }
    }

    /** Sends the texts of each answer [paired] takes, with its [key], until the coroutine is cancelled. */
    suspend fun run(paired: Paired, key: String) {
        for ((from, outbox) in answers) {
            // An answer to an earlier pairing, whose texts Dispatch failed as it removed the phone.
            if (from == paired) send(paired, key, outbox)
        }
    }

    /** Keeps what Android said of a part of a send, done with its picture message's file. */
    suspend fun heard(heard: Heard) {
        sends.heard(heard)
        if (heard.event == Event.SENT) File(folder, pduName(heard.id, heard.attempt)).delete()
        answered.value++
        statuses.more()
    }

    /**
     * Holds sending back, once no text is mid-send, until [release]d: none being handed to Android,
     * and none it took in the last [RESULT_WAIT_MS] without saying yet whether it went, as a picture
     * message whose file Android may still be reading from the app. For the app to be replaced.
     */
    suspend fun hold() {
        while (true) {
            val last = answered.value
            gate.lock(UPDATE)
            val wait = unanswered() ?: return
            gate.unlock(UPDATE)
            withTimeoutOrNull(wait) { answered.first { it != last } }
        }
    }

    /** Lets sending go on, if [hold] held it back. */
    fun release() {
        if (gate.holdsLock(UPDATE)) gate.unlock(UPDATE)
    }

    /**
     * How long until the last text Android took in the last [RESULT_WAIT_MS] without saying yet
     * whether it went is no longer waited on; null if none is.
     */
    internal suspend fun unanswered(): Long? {
        val since = now() - RESULT_WAIT_MS
        return sends.unanswered(now() + 1).mapNotNull(Send::handed).filter { it > since }.maxOrNull()?.let { it - since }
    }

    /**
     * Sends each text of [outbox] that wasn't yet, settling those Android never said more of. A
     * text whose file couldn't be fetched goes with a later answer, after a growing pause of its
     * own, the others going on.
     */
    internal suspend fun send(paired: Paired, key: String, outbox: Outbox) = withContext(Dispatchers.IO) {
        gate.withLock { sendEach(paired, key, outbox) }
    }

    private suspend fun sendEach(paired: Paired, key: String, outbox: Outbox) {
        try {
            val at = now()
            expire(at)
            sends.forget(at - KEEP_MS, at - DELIVERY_WAIT_MS)
            retries.keys.retainAll(outbox.texts.map { it.id to it.attempt }.toSet())
            for (text in outbox.texts) {
                val attempt = text.id to text.attempt
                val retry = retries[attempt]
                if (retry != null && now() < retry.at) continue
                val send = sends.start(Send.of(text, at))
                if (send.handed != null || send.outcome != null) continue
                try {
                    sendText(paired, key, text, send)
                    retries -= attempt
                } catch (_: IOException) {
                    retries[attempt] = Retry.after(retry, now())
                } catch (_: Refusal) {
                    retries[attempt] = Retry.after(retry, now())
                }
            }
        } finally {
            statuses.more()
        }
    }

    /**
     * Settles each send Android took over [RESULT_WAIT_MS] before [at] and never said all of: sent
     * when Android's own word says it went ([OwnRows.sent]), else failed.
     */
    private suspend fun expire(at: Long) {
        for (send in sends.unanswered(at - RESULT_WAIT_MS)) {
            sends.expired(send, own.sent(send), at)
            File(folder, pduName(send.id, send.attempt)).delete()
        }
    }

    private suspend fun sendText(paired: Paired, key: String, text: OutgoingText, send: Send) {
        if (!radio.allowed()) return sends.failed(send, Failure.NOT_ALLOWED, now())
        val line = telephony { radio.line(paired.number, paired.subscription) }.getOrElse { return notSent(send) }
        if (text.files.isEmpty()) {
            val parts = telephony { line.divide(send.body) }.getOrElse { return notSent(send) }
            // Near Android's SMS limit it goes as a picture message, or fails where it can't: as SMS,
            // Android would hold it behind the phone's prompt.
            if (!busy(parts.texts.size)) {
                hand(send, parts.texts.size, parts.joined, mms = false) { line.sms(send, parts) }
                return
            }
        }
        val limits = mmsLimits(line, send) ?: return
        sendMms(paired, key, line, limits, text, send)
    }

    /**
     * Whether a text of [parts] parts goes as a picture message: while [mmsWhenBusy], once its parts
     * would cross Android's SMS limit.
     */
    private suspend fun busy(parts: Int): Boolean {
        if (!mmsWhenBusy()) return false
        val limit = radio.smsLimit()
        return limit.crossedBy(sends.smsParts(limit.since(now())), parts)
    }

    /**
     * What the carrier of [line] takes of a picture message; null, [send] failed, when it takes none
     * or Android couldn't say.
     */
    private suspend fun mmsLimits(line: Line, send: Send): MmsLimits? {
        val limits = telephony { line.mmsLimits() }.getOrElse { return null.also { notSent(send) } }
        return limits ?: null.also { sends.failed(send, Failure.MMS_OFF, now()) }
    }

    /** Sends [text] as a picture message on [line], within the carrier's [limits]. */
    private suspend fun sendMms(paired: Paired, key: String, line: Line, limits: MmsLimits, text: OutgoingText, send: Send) {
        val pdu = compose(paired, key, limits, text, send) ?: return
        if (!hand(send, 1, joined = false, mms = true) { line.mms(send, pdu) }) pdu.delete()
    }

    /**
     * What [ask] of Android's telephony answers: it may fail as it's asked, as a phone's picture
     * message service while it's away, which fails that text and leaves the others to go.
     */
    private inline fun <T> telephony(ask: () -> T): Result<T> = try {
        Result.success(ask())
    } catch (failed: RuntimeException) {
        Result.failure(failed)
    }

    private suspend fun notSent(send: Send) = sends.failed(send, Failure.NOT_SENT, now())

    /**
     * Hands [send] to Android in [parts] parts, [joined] as one message or not, as a picture message
     * if [mms], by [go], kept as handed first, so it's never sent twice; answers whether Android
     * took it.
     */
    private suspend fun hand(send: Send, parts: Int, joined: Boolean, mms: Boolean, go: () -> Unit): Boolean {
        sends.handed(send, parts, joined, now(), mms)
        return try {
            go()
            true
        } catch (_: RuntimeException) {
            // Android refused it as it was handed, as a number it can't send to, so it never went.
            sends.failed(send, Failure.NOT_SENT, now())
            false
        }
    }

    /**
     * The file of [text]'s picture message, its files fetched from Dispatch first and its pictures
     * scaled to the carrier's [limits]; null when it failed.
     */
    private suspend fun compose(paired: Paired, key: String, limits: MmsLimits, text: OutgoingText, send: Send): File? {
        folder.mkdirs()
        val fetched = text.files.indices.map { File(folder, "${send.id}-${send.attempt}-$it") }
        try {
            for ((file, into) in text.files.zip(fetched)) {
                try {
                    dispatch.download(paired.site, key, file.id, into)
                } catch (refusal: Refusal) {
                    if (refusal.code != Refusal.FILE_NOT_FOUND) throw refusal
                    return null.also { sends.failed(send, Failure.FILE_GONE, now()) }
                }
            }
            val attachments = text.files.zip(fetched) { file, into -> Attachment(file.name, file.mime, into) }
            val at = now()
            val bytes = message.compose(send.number, send.body, attachments, limits, "T${at.toString(HEX)}", at / MS_PER_SECOND)
                ?: return null.also { sends.failed(send, Failure.TOO_LARGE, now()) }
            return File(folder, pduName(send.id, send.attempt)).apply { writeBytes(bytes) }
        } finally {
            fetched.forEach(File::delete)
        }
    }

    /** When to fetch an attempt's files again, after [failures] tries in a row failed. */
    private class Retry(val failures: Int, val at: Long) {
        companion object {
            /** The retry after another failure, at [now], following [last]. */
            fun after(last: Retry?, now: Long): Retry {
                val failures = (last?.failures ?: 0) + 1
                return Retry(failures, now + backoff(failures))
            }
        }
    }

    private companion object {
        /** The owner of [gate] while the app's update holds sending back. */
        val UPDATE = Any()

        /** How long an answer holding only texts under way waits before the next. */
        const val HOLD_MS = 5_000L

        /** How long a send is kept: its text's row may be read as long as the inbox looks back. */
        const val KEEP_MS = Inbox.LOOK_BACK_MS + RESULT_WAIT_MS

        const val MS_PER_SECOND = 1_000L
        const val HEX = 16

        fun pduName(id: String, attempt: Long) = "$id-$attempt.pdu"
    }
}
