package com.dspdispatch.relay.texts

import androidx.room3.Embedded
import androidx.room3.Entity
import androidx.room3.Index
import androidx.room3.PrimaryKey
import androidx.room3.Relation
import com.dspdispatch.relay.dispatch.Direction

/**
 * Where reading the phone's texts stands: for which pairing ([phone], Dispatch's ID of it), from
 * when ([paired]), when the provider was last read through ([scanned]), and the highest row IDs
 * read of its texts and picture messages ([sms], [mms]).
 */
@Entity(tableName = "mark")
data class Mark(
    val phone: String,
    val paired: Long,
    val scanned: Long,
    val sms: Long = 0,
    val mms: Long = 0,
    @PrimaryKey val id: Int = ONLY,
) {
    private companion object {
        const val ONLY = 0
    }
}

/**
 * A provider row read, by its [Row.key], so it's read once: [direction] is null for one never
 * reported, as one the app sent itself or from a sender Dispatch doesn't take.
 */
@Entity(tableName = "seen")
data class Seen(@PrimaryKey val row: String, val at: Long, val direction: Direction?)

/**
 * A provider row not read whole yet, as a picture message still being written or the provider
 * away a moment: read again, by itself, from when it was [found] until it is.
 */
@Entity(tableName = "unread")
data class Unread(@Embedded val row: Row, val found: Long, @PrimaryKey val key: String = row.key)

/** A text read and not yet answered by Dispatch, as it's reported. */
@Entity(tableName = "texts")
data class Waiting(
    @PrimaryKey val clientId: String,
    val direction: Direction,
    val number: String,
    val body: String,
    val at: Long,
)

/**
 * A picture or file of a text, its bytes still to send: [dispatchId] comes once Dispatch answered
 * its text, at its [position] among the text's files.
 */
@Entity(tableName = "files", indices = [Index("clientId"), Index("dispatchId")])
data class WaitingFile(
    val clientId: String,
    val position: Int,
    val uri: String,
    val name: String,
    val mime: String,
    val size: Long,
    val dispatchId: String? = null,
    @PrimaryKey(autoGenerate = true) val id: Long = 0,
)

/** A text waiting to be reported, with its files. */
data class WaitingText(
    @Embedded val text: Waiting,
    @Relation(parentColumns = ["clientId"], entityColumns = ["clientId"]) val files: List<WaitingFile>,
)

/**
 * What's read from the phone's texts and not yet with Dispatch, kept on the phone so nothing is
 * lost as the app stops: texts until Dispatch answers them, then their files until their bytes
 * are sent.
 */
interface Queue {
    /** Where reading stands, if it started. */
    suspend fun mark(): Mark?

    /** Starts reading for a new pairing at [mark], leaving behind everything of any before it. */
    suspend fun begin(mark: Mark)

    /** The keys of the rows read, kept until they can't come up again. */
    suspend fun seen(): List<String>

    /** Keeps [row] as read, and the [texts] it's reported as with their [files], at once: no longer [unread]. */
    suspend fun read(row: Seen, texts: List<Waiting>, files: List<WaitingFile>)

    /** The rows to read again, not whole as they were read. */
    suspend fun unread(): List<Unread>

    /** Keeps [row] to read again, as found the first time it was. */
    suspend fun later(row: Unread)

    /** The provider was read through as [mark] has it; rows written before [forget] can't come up again. */
    suspend fun scanned(mark: Mark, forget: Long)

    /** The [most] oldest texts waiting to be reported, with their files. */
    suspend fun waiting(most: Int): List<WaitingText>

    /** Dispatch answered [answered] with its IDs: leaves each text, and gives its files theirs. */
    suspend fun answered(answered: Map<String, List<String>>)

    /** Leaves [clientId], a text Dispatch refuses alone, and its files, never to be reported. */
    suspend fun refused(clientId: String)

    /** Keeps [file], one waiting, as it's now to be reported and sent, as a JPEG made of the picture it was. */
    suspend fun converted(file: WaitingFile)

    /** The URIs of every file waiting, reported or not. */
    suspend fun uris(): List<String>

    /** The oldest file whose bytes Dispatch waits for. */
    suspend fun upload(): WaitingFile?

    /** Leaves the file [id], its bytes sent, or never to be. */
    suspend fun uploaded(id: Long)
}
