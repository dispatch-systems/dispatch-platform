package com.dspdispatch.relay.texts

import android.database.sqlite.SQLiteException
import com.dspdispatch.relay.dispatch.Direction

/**
 * The provider rows Android wrote the app's sends to, for [app], an app that isn't the phone's
 * texting app, as the provider holds them now: whether the row a send's result names is still that
 * send's, and whether the row Android said holds all of a send it never said more of is in Sent.
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
     * Whether [send], which Android took and never said all of, was sent, by Android's own word
     * alone: it said it sent the parts joined, as one message, naming the row it wrote all of it to,
     * and that row is in Sent now, the app's, as Android moves it there only once every part went.
     * A text whose every part's result came is settled already; no row is ever matched to one by its
     * words. Not when the provider can't say now either: nothing waits past the timeout.
     */
    suspend fun sent(send: Send): Boolean {
        val whole = sends.whole(send) ?: return false
        val handed = send.handed ?: return false
        val tried = reading { provider.tried(send.table, handed - ROW_SLACK_MS) } ?: return false
        return tried.any { it.sent && it.row.key == whole && it.row.creator == app }
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
