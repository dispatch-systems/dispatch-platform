package com.dspdispatch.relay.texts

import androidx.room3.Entity
import androidx.room3.PrimaryKey
import com.dspdispatch.relay.dispatch.OutgoingText
import com.dspdispatch.relay.dispatch.ReportedStatus
import com.dspdispatch.relay.dispatch.SendStatus

/**
 * An attempt at a text of the outbox, kept from before it's sent so it's sent once: [mms] when it
 * goes as a picture message, as one with files does, and one without near Android's SMS limit;
 * [handed] is when Android took it, in [parts] parts, handed [joined] as one message or each its
 * own, and null while its files are still being fetched, when it may start again; [outcome] is
 * whether it was sent or failed, at [finished].
 */
@Entity(tableName = "sends", primaryKeys = ["id", "attempt"])
data class Send(
    val id: String,
    val attempt: Long,
    val number: String,
    val body: String,
    val started: Long,
    val mms: Boolean = false,
    val handed: Long? = null,
    val parts: Int = 0,
    val joined: Boolean = false,
    val outcome: SendStatus? = null,
    val finished: Long? = null,
    val delivered: Boolean = false,
) {
    /** The provider table Android writes this attempt to. */
    val table get() = if (mms) Table.MMS else Table.SMS

    /** The status Dispatch is to hear of this attempt. */
    fun status(status: SendStatus, at: Long, error: String? = null) = Told(id, attempt, status, at, error)

    /**
     * This attempt, and the statuses it moved to, with Android's [reports] of its parts: failed as
     * soon as one part failed, sent once every part was, delivered once every part was. A text
     * Android says it sent joined is reported delivered on CDMA by its last part's report alone, once
     * every part's result says it went: Android says it of the last part even when one before failed.
     */
    fun settled(reports: List<Report>, at: Long): Pair<Send, List<Told>> {
        if (handed == null) return this to emptyList()
        var send = this
        val statuses = mutableListOf<Told>()
        val sent = reports.filter { it.event == Event.SENT }
        val error = sent.firstNotNullOfOrNull(Report::error)
        if (outcome == null && (error != null || sent.size >= parts)) {
            val outcome = if (error == null) SendStatus.SENT else SendStatus.FAILED
            send = send.copy(outcome = outcome, finished = at)
            statuses += status(outcome, at, error)
        }
        val reached = reports.filter { it.event == Event.DELIVERED }
        val joined = joined && error == null && sent.size >= parts && sent.any(Report::joined)
        val whole = reached.size >= parts || (joined && reached.any { it.format == CDMA && it.part == parts - 1 })
        if (!delivered && whole) {
            send = send.copy(delivered = true)
            statuses += status(SendStatus.DELIVERED, at)
        }
        return send to statuses
    }

    companion object {
        /** The attempt at [text], started [at]. */
        fun of(text: OutgoingText, at: Long) = Send(text.id, text.attempt, text.number, text.body, at, mms = text.files.isNotEmpty())
    }
}

/** What Android says of a send's part: sent, or delivered as the carrier reports. */
enum class Event {
    SENT,
    DELIVERED,
}

/**
 * What Android said of [part] of a send: [error] says why it failed, null when it didn't; [joined]
 * when Android says it was the last of a text's parts it sent joined, as one message; [format] is
 * a delivery report's, `3gpp` or `3gpp2`; [row] is the [Row.key] of the provider row it named.
 */
@Entity(tableName = "send_reports", primaryKeys = ["id", "attempt", "part", "event"])
data class Report(
    val id: String,
    val attempt: Long,
    val part: Int,
    val event: Event,
    val error: String? = null,
    val joined: Boolean = false,
    val format: String? = null,
    val row: String? = null,
)

/**
 * The [parts] of the attempt [attempt] at the text [id] the app handed Android as SMS [at], as
 * Android counts them against its limit ([SmsLimit]): kept apart from the sends, as a new pairing
 * leaves Android's count as it was.
 */
@Entity(tableName = "sms_parts", primaryKeys = ["id", "attempt"])
data class SmsParts(val id: String, val attempt: Long, val at: Long, val parts: Int)

/** The provider row ([Row.key]) a send was written to, so it's never reported as the phone's own. */
@Entity(tableName = "send_rows")
data class SendRow(@PrimaryKey val row: String, val id: String, val attempt: Long)

/** A status Dispatch is still to hear, kept until it answers. */
@Entity(tableName = "statuses", primaryKeys = ["id", "attempt", "status"])
data class Told(val id: String, val attempt: Long, val status: SendStatus, val at: Long, val error: String? = null) {
    val reported get() = ReportedStatus(id, attempt, status, at, error)
}

/**
 * What Android said of a part of a send, as it reaches the app, possibly after it restarted: [row]
 * is the [Row.key] of the provider row Android says it wrote the text to; [joined] and [format] as
 * a [Report]'s.
 */
data class Heard(
    val id: String,
    val attempt: Long,
    val part: Int,
    val event: Event,
    val error: String?,
    val row: String?,
    val at: Long,
    val format: String? = null,
    val joined: Boolean = false,
) {
    val report get() = Report(id, attempt, part, event, error, joined, format, row)
}

/**
 * The attempts at the outbox's texts, kept on the phone so each is sent once, across restarts,
 * and the statuses Dispatch is still to hear of them.
 */
interface Sends {
    /** Keeps [send] as started, unless its attempt already is: answers the attempt as kept. */
    suspend fun start(send: Send): Send

    /**
     * Android took [send], in [parts] parts, handed [joined] as one message or each its own, [at],
     * as a picture message if [mms]; an SMS's parts are counted ([smsParts]).
     */
    suspend fun handed(send: Send, parts: Int, joined: Boolean, at: Long, mms: Boolean = send.mms)

    /** How many SMS parts the app handed Android from [since] on, whichever pairing they were for. */
    suspend fun smsParts(since: Long): Int

    /** [send] failed [at], for [error], before Android took it or as it refused it. */
    suspend fun failed(send: Send, error: String, at: Long)

    /** Keeps what Android said of a part of a send, and the statuses it moves the send to. */
    suspend fun heard(heard: Heard)

    /** The sends Android took before [before] and said nothing more of. */
    suspend fun unanswered(before: Long): List<Send>

    /**
     * Settles [send], which Android never said all of, [at]: [sent] when Android's own word says it
     * went, else failed. Not if Android said more of it meanwhile.
     */
    suspend fun expired(send: Send, sent: Boolean, at: Long)

    /**
     * Leaves the sends started before [before] that Dispatch heard the end of, or never handed. A
     * text whose delivery may still be reported is kept until it was handed before [undelivered].
     * The SMS parts handed before [before] are no longer counted.
     */
    suspend fun forget(before: Long, undelivered: Long)

    /** The [most] oldest statuses Dispatch is still to hear. */
    suspend fun statuses(most: Int): List<Told>

    /** Dispatch heard [told]. */
    suspend fun told(told: List<Told>)

    /** The attempt [attempt] at the text [id], if it's kept. */
    suspend fun find(id: String, attempt: Long): Send?

    /**
     * The provider row ([Row.key]) Android named as it said it sent [send]'s parts joined, as one
     * message ([Report.joined]): the row it wrote all of it to. Null when it said no such thing.
     */
    suspend fun whole(send: Send): String?

    /** Whether a send was written to the provider row [row] ([Row.key]). */
    suspend fun wrote(row: String): Boolean

    /** The sends Android took between [from] and [to] whose provider row isn't known. */
    suspend fun unplaced(from: Long, to: Long): List<Send>

    /** [send] was written to the provider row [row] ([Row.key]). */
    suspend fun placed(send: Send, row: String)
}

/** How far the time of a send's provider row may be from its own: a picture message's is in whole seconds. */
internal const val ROW_SLACK_MS = 60_000L

/** How long Android may take to say how a text it took went; past it, it's settled by what it said. */
internal const val RESULT_WAIT_MS = 10 * 60 * 1000L

/**
 * How long a carrier may still report a text delivered, failed or sent: as long as it keeps trying
 * to deliver it to a phone that's off, at most a week for nearly every carrier.
 */
internal const val DELIVERY_WAIT_MS = 7 * 24 * 60 * 60 * 1000L
