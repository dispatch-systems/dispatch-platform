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
     * Whether [send], which Android took and never said more of, was sent: its rows are in Sent and
     * hold all of it, one the whole text, as Android writes a text it sent joined, else one each
     * part. Its rows are those its results named or the inbox placed, and those with its words no
     * other send may be in, which it takes. Not when one is in Failed or they don't hold all of it;
     * null when the provider can't say now, or while another send may be in those rows too.
     */
    suspend fun sent(send: Send): Boolean? {
        val handed = send.handed ?: return null
        val tried = reading { provider.tried(send.table, handed - ROW_SLACK_MS) } ?: return null
        val known = sends.rows(send).toSet()
        val placed = mutableListOf<Written>()
        val unplaced = mutableListOf<Written>()
        for (candidate in tried) {
            val row = candidate.row
            val its = row.key in known
            // Another app's rows aren't read, so one it's still writing holds nothing up.
            if (!its && ((row.creator != null && row.creator != app) || sends.wrote(row.key))) continue
            val message = when (val reading = reading { provider.read(row) }) {
                is Reading.Whole -> reading.message
                Reading.Gone -> continue
                Reading.Later, null -> return null
            }
            when {
                its -> placed += Written(message, candidate.sent)
                send.mayHold(message) -> unplaced += Written(message, candidate.sent)
            }
        }
        val joined = sends.joined(send)
        fun whole(rows: List<Written>) = rows.size >= send.parts || rows.any { joined || it.message.body == send.body }
        if (placed.any { !it.sent }) return false
        if (whole(placed)) return true
        // The rest of it is in rows not placed yet, as when the app restarted before the inbox read
        // them: with none in Sent, it failed whichever are its. Rows another send may be in too are
        // left for the inbox to place, as which came first doesn't tell like texts apart.
        if (unplaced.none { it.sent }) return false
        val others = sends.unplaced(0, Long.MAX_VALUE).filter { it.id != send.id || it.attempt != send.attempt }
        if (unplaced.any { row -> others.any { it.mayHold(row.message) } }) return null
        unplaced.forEach { sends.placed(send, it.message.row.key) }
        return unplaced.all { it.sent } && whole(placed + unplaced)
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

    /**
     * Whether [message], a row not placed yet, may hold this send: its words, or where Android sent
     * its parts each as a text of its own, one part's, in the app's own row.
     */
    private fun Send.mayHold(message: Message): Boolean {
        val words = message.body
        return mayBeIn(message) && (words == body || (parts > 1 && message.row.creator == app && words.isNotEmpty() && words in body))
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

    /** A row of the provider a send may be in, [sent] when it's in Sent, not Failed. */
    private class Written(val message: Message, val sent: Boolean)
}
