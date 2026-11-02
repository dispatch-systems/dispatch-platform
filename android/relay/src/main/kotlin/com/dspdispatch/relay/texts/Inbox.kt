package com.dspdispatch.relay.texts

import com.dspdispatch.relay.dispatch.Direction

/**
 * Reads the phone's texts into the [queue]: each received, and each sent from its texting app but
 * the app's [own], once, from when the phone paired on. Each read goes back over the texts written
 * since the last one, so a text written while the app was stopped is read at its next start, over
 * each not whole the last time, and over each row added since, whatever its time.
 */
class Inbox(
    private val provider: Provider,
    private val queue: Queue,
    private val own: Own,
    private val now: () -> Long = System::currentTimeMillis,
) {
    /**
     * Starts reading for the pairing Dispatch knows as [phone], unless it already did: from when
     * it [paired], however long before the relay first ran.
     */
    suspend fun begin(phone: String, paired: Long) {
        if (queue.mark()?.phone == phone) return
        queue.begin(Mark(phone, paired, paired))
    }

    /**
     * Reads the texts written since the last read, and those not whole then, keeping each new one
     * to report. Whether to leave the next read to its regular time: not while a text that couldn't
     * be read is less than [UNREAD_MS] old, as one being written soon is whole.
     */
    suspend fun read(): Boolean {
        val mark = queue.mark() ?: return true
        val started = now()
        // A text may come into the provider after its time. A text sent leaves the outbox in its
        // row, found looking back a while, as is a row whose ID was used again after one before it
        // was deleted. A picture message downloaded late is a new row, its time when it came,
        // however long before: found above the highest ID read.
        val since = maxOf(mark.paired, mark.scanned - LOOK_BACK_MS)
        val rows = provider.rows(Scan(mark.paired, since, mark.sms, mark.mms))
        val seen = queue.seen().toHashSet()
        val unread = queue.unread()
        val found = unread.associate { it.key to it.found }
        var whole = true
        for (row in (unread.map(Unread::row) + rows).distinctBy(Row::key)) {
            if (row.key in seen) continue
            whole = read(row, found[row.key] ?: started, started) && whole
        }
        queue.scanned(mark.copy(scanned = started, sms = rows.highest(Table.SMS, mark.sms), mms = rows.highest(Table.MMS, mark.mms)), since)
        return whole
    }

    /**
     * Reads [row], first [found] unread then, at [now]: whether it's done with as far as the quick
     * retries go. One that can't be read yet is kept until it's whole or gone, however long that
     * takes; after a day, only the regular reads try it again.
     */
    private suspend fun read(row: Row, found: Long, now: Long): Boolean {
        return when (val reading = provider.read(row)) {
            is Reading.Whole -> {
                if (row.direction == Direction.OUT) {
                    when (own.whose(reading.message)) {
                        Whose.DISPATCH -> return left(row)
                        // Read again by a later read, once Android said how the app's text went.
                        Whose.UNSURE -> return true
                        Whose.PHONE -> Unit
                    }
                }
                val texts = reports(reading.message)
                val reported = Seen(row.key, row.at, row.direction.takeIf { texts.isNotEmpty() })
                queue.read(reported, texts.map { it.first }, texts.flatMap { it.second })
                true
            }
            Reading.Gone -> left(row)
            Reading.Later -> {
                queue.later(Unread(row, found))
                now - found >= UNREAD_MS
            }
        }
    }

    /** Keeps [row] as read, never reported. */
    private suspend fun left(row: Row): Boolean {
        queue.read(Seen(row.key, row.at, null), emptyList(), emptyList())
        return true
    }

    /**
     * The texts [message] is reported as, each with its files: one received from its sender, one
     * sent to each recipient, as Dispatch answers each one to one. None for numbers Dispatch
     * doesn't take, which it would refuse the whole report for.
     */
    private fun reports(message: Message): List<Pair<Waiting, List<WaitingFile>>> {
        val row = message.row
        val numbers = message.numbers.filter(Numbers::accepted).distinct()
            .let { if (row.direction == Direction.IN) it.take(1) else it }
        val body = message.body.limited(BODY_MOST)
        val parts = message.parts.filter { it.size <= LARGEST }.take(FILES_MOST)
        return numbers.mapIndexed { index, number ->
            val clientId = if (row.table == Table.MMS && row.direction == Direction.OUT) "${row.key}-$index" else row.key
            val files = parts.mapIndexed { position, part ->
                WaitingFile(clientId, position, part.uri, part.name.limited(NAME_MOST), part.mime.limited(MIME_MOST), part.size)
            }
            Waiting(clientId, row.direction, number.trim(), body, row.at) to files
        }
    }

    companion object {
        /** How long after its time a text may still come into the provider. */
        const val LOOK_BACK_MS = 24 * 60 * 60 * 1000L

        /** How long a text not whole is read again, from when it was found, before it's left. */
        const val UNREAD_MS = 24 * 60 * 60 * 1000L

        // What Dispatch takes of a text (`LIMITS`, `NAME_MOST`, `MIME_MOST`), in characters as it
        // counts them, each code point one.
        private const val BODY_MOST = 10_000
        private const val FILES_MOST = 10
        internal const val LARGEST = 20L * 1024 * 1024
        internal const val NAME_MOST = 200
        private const val MIME_MOST = 100

        /** The highest ID of these rows of [table], or of those [read] before. */
        private fun List<Row>.highest(table: Table, read: Long): Long = filter { it.table == table }.maxOfOrNull(Row::id)?.coerceAtLeast(read) ?: read
    }
}

/** At most [most] characters of this, counting as Dispatch does. */
internal fun String.limited(most: Int): String =
    if (codePointCount(0, length) <= most) this else substring(0, offsetByCodePoints(0, most))
