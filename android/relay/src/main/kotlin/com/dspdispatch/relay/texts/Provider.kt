package com.dspdispatch.relay.texts

import com.dspdispatch.relay.dispatch.Direction
import java.io.InputStream

/** Which of the phone's texting tables a text is a row of: plain texts, or picture messages. */
enum class Table(val prefix: String) {
    SMS("sms"),
    MMS("mms"),
}

/**
 * A text's row in the phone's texting provider, received or sent: [id] is the provider's own and
 * may be used again once the row is deleted, so a row is known by [key], its ID with its time.
 */
data class Row(val table: Table, val id: Long, val at: Long, val direction: Direction) {
    val key get() = "${table.prefix}-$id-$at"
}

/**
 * A text as the provider holds it: [numbers], the sender of one received or each recipient of
 * one sent, as the provider writes them; its words, and its pictures and files.
 */
data class Message(val row: Row, val numbers: List<String>, val body: String, val parts: List<Part>)

/** A picture or file of a text, at [uri] in the provider, [size] bytes as it was read. */
data class Part(val uri: String, val name: String, val mime: String, val size: Long)

/**
 * The phone's texts, as its texting provider holds them, the texting app keeping them. Calls
 * block on the provider: never on the main thread.
 */
interface Provider {
    /**
     * The rows of texts written at or after [since], received or sent, oldest first: none still
     * waiting to be sent, nor a picture message the texting app hasn't downloaded yet.
     */
    fun rows(since: Long): List<Row>

    /** The text [row] holds, or null while it's gone or still being written. */
    fun read(row: Row): Message?

    /** How many bytes the part at [uri] holds now, or null once it's gone. */
    fun size(uri: String): Long?

    /** The bytes of the part at [uri]. */
    fun open(uri: String): InputStream
}
