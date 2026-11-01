package com.dspdispatch.relay.texts

import com.dspdispatch.relay.dispatch.Direction

/**
 * Reads the phone's texts into the [queue]: each received, and each sent from its texting app but
 * the app's [own], once, from when the phone paired on. Each read goes back over the texts written
 * since the last one, so a text written while the app was stopped is read at its next start.
 */
class Inbox(
    private val provider: Provider,
    private val queue: Queue,
    private val own: Own,
    private val now: () -> Long = System::currentTimeMillis,
) {
    /** Starts reading for the pairing Dispatch knows as [phone], unless it already did. */
    suspend fun begin(phone: String) {
        if (queue.mark()?.phone == phone) return
        val at = now()
        queue.begin(Mark(phone, at, at))
    }

    /** Reads the texts written since the last read, keeping each new one to report. */
    suspend fun read() {
        val mark = queue.mark() ?: return
        val started = now()
        // A text may come into the provider a while after its time: one sent waits to go, and a
        // picture message to be downloaded. Looking back a while finds those, and a row whose ID
        // was used again after one before it was deleted.
        val since = maxOf(mark.paired, mark.scanned - LOOK_BACK_MS)
        val seen = queue.seen(since).toHashSet()
        for (row in provider.rows(since)) {
            if (row.key in seen) continue
            val message = provider.read(row) ?: continue
            if (row.direction == Direction.OUT) {
                when (own.whose(message)) {
                    Whose.DISPATCH -> {
                        queue.read(Seen(row.key, row.at, null), emptyList(), emptyList())
                        continue
                    }
                    // Read again by a later read, once Android said how the app's text went.
                    Whose.UNSURE -> continue
                    Whose.PHONE -> Unit
                }
            }
            val texts = reports(message)
            val reported = Seen(row.key, row.at, row.direction.takeIf { texts.isNotEmpty() })
            queue.read(reported, texts.map { it.first }, texts.flatMap { it.second })
        }
        queue.scanned(started, since)
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

        // What Dispatch takes of a text (`LIMITS`, `NAME_MOST`, `MIME_MOST`), in characters as it
        // counts them, each code point one.
        private const val BODY_MOST = 10_000
        private const val FILES_MOST = 10
        private const val LARGEST = 20L * 1024 * 1024
        private const val NAME_MOST = 200
        private const val MIME_MOST = 100

        /** At most [most] characters of this, counting as Dispatch does. */
        private fun String.limited(most: Int): String =
            if (codePointCount(0, length) <= most) this else substring(0, offsetByCodePoints(0, most))
    }
}
