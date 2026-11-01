package com.dspdispatch.relay.texts

import androidx.room3.Entity
import androidx.room3.PrimaryKey
import com.dspdispatch.relay.dispatch.OutgoingText
import com.dspdispatch.relay.dispatch.ReportedStatus
import com.dspdispatch.relay.dispatch.SendStatus

/**
 * An attempt at a text of the outbox, kept from before it's sent so it's sent once: [handed] is
 * when Android took it, in [parts] parts, and null while its files are still being fetched, when
 * it may start again; [outcome] is whether it was sent or failed, at [finished].
 */
@Entity(tableName = "sends", primaryKeys = ["id", "attempt"])
data class Send(
    val id: String,
    val attempt: Long,
    val number: String,
    val body: String,
    val started: Long,
    val handed: Long? = null,
    val parts: Int = 0,
    val outcome: SendStatus? = null,
    val finished: Long? = null,
    val delivered: Boolean = false,
) {
    /** The status Dispatch is to hear of this attempt. */
    fun status(status: SendStatus, at: Long, error: String? = null) = Told(id, attempt, status, at, error)

    /**
     * This attempt, and the statuses it moved to, with Android's [reports] of its parts: failed as
     * soon as one part failed, sent once every part was, delivered once every part was.
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
        if (!delivered && reports.count { it.event == Event.DELIVERED } >= parts) {
            send = send.copy(delivered = true)
            statuses += status(SendStatus.DELIVERED, at)
        }
        return send to statuses
    }

    companion object {
        /** The attempt at [text], started [at]. */
        fun of(text: OutgoingText, at: Long) = Send(text.id, text.attempt, text.number, text.body, at)
    }
}

/** What Android says of a send's part: sent, or delivered as the carrier reports. */
enum class Event {
    SENT,
    DELIVERED,
}

/** What Android said of [part] of a send: [error] says why it failed, null when it didn't. */
@Entity(tableName = "send_reports", primaryKeys = ["id", "attempt", "part", "event"])
data class Report(val id: String, val attempt: Long, val part: Int, val event: Event, val error: String? = null)

/** The provider row ([Row.key]) a send was written to, so it's never reported as the phone's own. */
@Entity(tableName = "send_rows")
data class SendRow(@PrimaryKey val row: String, val id: String, val attempt: Long)

/** A status Dispatch is still to hear, kept until it answers. */
@Entity(tableName = "statuses", primaryKeys = ["id", "attempt", "status"])
data class Told(val id: String, val attempt: Long, val status: SendStatus, val at: Long, val error: String? = null) {
    val reported get() = ReportedStatus(id, attempt, status, at, error)
}

/**
 * What Android said of a part of a send, as it reaches the app, possibly after it restarted:
 * [row] is the provider row it was written to ([Row.key]), where Android says.
 */
data class Heard(
    val id: String,
    val attempt: Long,
    val part: Int,
    val event: Event,
    val error: String?,
    val row: String?,
    val at: Long,
) {
    val report get() = Report(id, attempt, part, event, error)
}

/**
 * The attempts at the outbox's texts, kept on the phone so each is sent once, across restarts,
 * and the statuses Dispatch is still to hear of them.
 */
interface Sends {
    /** Keeps [send] as started, unless its attempt already is: answers the attempt as kept. */
    suspend fun start(send: Send): Send

    /** Android took [send], in [parts] parts, [at]. */
    suspend fun handed(send: Send, parts: Int, at: Long)

    /** [send] failed [at], for [error], before Android took it or as it refused it. */
    suspend fun failed(send: Send, error: String, at: Long)

    /** Keeps what Android said of a part of a send, and the statuses it moves the send to. */
    suspend fun heard(heard: Heard)

    /** Fails each send Android took before [before] and said nothing more of, [at]; answers them. */
    suspend fun expire(before: Long, at: Long): List<Send>

    /** Leaves the sends started before [before] that Dispatch heard the end of, or never handed. */
    suspend fun forget(before: Long)

    /** The [most] oldest statuses Dispatch is still to hear. */
    suspend fun statuses(most: Int): List<Told>

    /** Dispatch heard [told]. */
    suspend fun told(told: List<Told>)

    /** Whether a send was written to the provider row [row] ([Row.key]). */
    suspend fun wrote(row: String): Boolean

    /** The sends Android took between [from] and [to] whose provider row isn't known. */
    suspend fun unplaced(from: Long, to: Long): List<Send>

    /** [send] was written to the provider row [row] ([Row.key]). */
    suspend fun placed(send: Send, row: String)
}

/** How long Android may take to say how a text it took went; past it, the text failed. */
internal const val RESULT_WAIT_MS = 10 * 60 * 1000L
