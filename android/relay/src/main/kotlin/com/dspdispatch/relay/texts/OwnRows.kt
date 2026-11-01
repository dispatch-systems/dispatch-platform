package com.dspdispatch.relay.texts

import android.database.sqlite.SQLiteException
import com.dspdispatch.relay.dispatch.Direction

/**
 * The provider rows Android wrote the app's sends to, for [app], an app that isn't the phone's
 * texting app, as the provider holds them now: whether the row a send's result names is still that
 * send's, and whether a send Android never said more of is in Sent.
 */
class OwnRows(private val provider: Provider, private val sends: Sends, private val app: String) {
    /**
     * The [Row.key] of the row at [uri], where Android said it wrote [heard]'s send, or null when
     * it can't be told it's that send's: gone, not whole yet, or its ID used again for another text
     * since, the provider using a deleted row's ID again. The inbox places such a send as it reads.
     */
    suspend fun row(heard: Heard, uri: String): String? {
        val send = sends.find(heard.id, heard.attempt) ?: return null
        val message = reading { provider.message(uri) } ?: return null
        return message.row.key.takeIf { send.mayBeIn(message) }
    }

    /**
     * Whether [send], which Android took and never said more of, was sent: its row is in Sent, the
     * one its results named or the inbox placed, else one with its words. Not when that row's in
     * Failed or the provider holds none; null when the provider can't say now.
     */
    suspend fun sent(send: Send): Boolean? {
        val handed = send.handed ?: return null
        val tried = reading { provider.tried(send.table, handed - ROW_SLACK_MS) } ?: return null
        val known = sends.rows(send).toSet()
        val placed = tried.filter { it.row.key in known }
        if (placed.isNotEmpty()) return placed.all(Tried::sent)
        // Not placed yet, as when the app restarted before the inbox read it. Another app's rows
        // aren't read, so one it's still writing holds nothing up.
        for (candidate in tried) {
            val row = candidate.row
            if ((row.creator != null && row.creator != app) || sends.wrote(row.key)) continue
            when (val reading = reading { provider.read(row) }) {
                is Reading.Whole -> if (reading.message.body == send.body && send.mayBeIn(reading.message)) return candidate.sent
                Reading.Gone -> Unit
                Reading.Later, null -> return null
            }
        }
        return false
    }

    /**
     * Whether [message] may be the row Android wrote this send to: the same kind of text, sent to
     * its number once it was handed, by the app as the provider says, or, where it names no app,
     * with its words.
     */
    private fun Send.mayBeIn(message: Message): Boolean {
        val row = message.row
        val handed = handed ?: return false
        if (row.table != table || row.direction != Direction.OUT || row.at < handed - ROW_SLACK_MS) return false
        if (message.numbers.none { Numbers.same(it, number) }) return false
        return row.creator == app || (row.creator == null && message.body == body)
    }

    /** What [read] of the provider answers, or null when it can't be read now. */
    private inline fun <T> reading(read: () -> T): T? = try {
        read()
    } catch (_: SecurityException) {
        null
    } catch (_: SQLiteException) {
        null
    } catch (_: Unavailable) {
        null
    }
}
