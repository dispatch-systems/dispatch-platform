package com.dspdispatch.relay.texts

import com.dspdispatch.relay.dispatch.Direction
import java.io.IOException
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

/** What reading a text's row found. */
sealed interface Reading {
    /** The text, whole. */
    data class Whole(val message: Message) : Reading

    /** Not whole yet: still being written, or the provider away a moment. It's read again. */
    data object Later : Reading

    /** The row's gone. */
    data object Gone : Reading
}

/**
 * The provider can't be read just now, as when it crashed a moment ago: what it holds is still
 * there, to read again.
 */
class Unavailable(what: String) : IOException("The texting provider can't read $what now")

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

    /** The text [row] holds, if it's whole yet. */
    fun read(row: Row): Reading

    /** How many bytes the part at [uri] holds now, or null once it's gone; [Unavailable] if it can't say. */
    fun size(uri: String): Long?

    /** The bytes of the part at [uri]; [Unavailable] if they can't be read now. */
    fun open(uri: String): InputStream
}
