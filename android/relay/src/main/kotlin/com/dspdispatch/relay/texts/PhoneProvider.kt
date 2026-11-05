package com.dspdispatch.relay.texts

import android.content.ContentResolver
import android.database.Cursor
import android.net.Uri
import android.provider.BaseColumns
import android.provider.Telephony.BaseMmsColumns
import android.provider.Telephony.Mms
import android.provider.Telephony.Sms
import android.provider.Telephony.TextBasedSmsColumns
import androidx.core.net.toUri
import androidx.core.text.HtmlCompat
import com.dspdispatch.relay.dispatch.Direction
import java.io.FileNotFoundException
import java.io.IOException
import java.io.InputStream
import java.util.concurrent.ConcurrentHashMap

/**
 * The phone's texts in Android's own texting provider, `Telephony.Sms` and `Telephony.Mms`. From
 * Android 17 it holds back a text with a one-time code from an app like this one for up to three
 * hours, accepted: such a text is reported once the provider lets it be read.
 */
class PhoneProvider(private val resolver: ContentResolver) : Provider {
    private val subscribed = ConcurrentHashMap<Table, Boolean>()

    override fun rows(scan: Scan): List<Row> = (smsRows(scan) + mmsRows(scan)).sortedWith(compareBy(Row::at, Row::id))

    override fun read(row: Row): Reading = try {
        when (row.table) {
            Table.SMS -> readSms(row)
            Table.MMS -> readMms(row)
        }
    } catch (_: IOException) {
        // The provider away, or failing as a part's read: it's all still there.
        Reading.Later
    }

    override fun message(uri: String): Message? {
        val (table, id) = rowAt(uri) ?: return null
        val row = when (table) {
            Table.SMS -> sms(Uri.withAppendedPath(Sms.CONTENT_URI, id.toString()), null, null)
            Table.MMS -> mms(Uri.withAppendedPath(Mms.CONTENT_URI, id.toString()), null, null)
        }.firstOrNull()?.first ?: return null
        return (read(row) as? Reading.Whole)?.message
    }

    override fun tried(table: Table, since: Long): List<Tried> = when (table) {
        Table.SMS -> sms(
            Sms.CONTENT_URI,
            "${Sms.DATE} >= ? AND ${Sms.TYPE} IN (${TextBasedSmsColumns.MESSAGE_TYPE_SENT}, ${TextBasedSmsColumns.MESSAGE_TYPE_FAILED})",
            arrayOf(since.toString()),
        ).map { (row, type) -> Tried(row, type == TextBasedSmsColumns.MESSAGE_TYPE_SENT) }
        Table.MMS -> mms(
            Mms.CONTENT_URI,
            "${Mms.DATE} >= ? AND ${Mms.MESSAGE_BOX} IN (${BaseMmsColumns.MESSAGE_BOX_SENT}, ${BaseMmsColumns.MESSAGE_BOX_FAILED})",
            arrayOf((since / MS_PER_SECOND).toString()),
        ).map { (row, box) -> Tried(row, box == BaseMmsColumns.MESSAGE_BOX_SENT) }
    }

    /**
     * How many picture messages received from [since] on, of the SIM [sim] where it's one of
     * several, the texting app hasn't downloaded: word of them alone.
     */
    fun undownloaded(since: Long, sim: Int?): Int {
        val only = sim(Table.MMS, sim)
        return mms(
            Mms.CONTENT_URI,
            "${Mms.DATE} >= ? AND ${Mms.MESSAGE_BOX} = ${BaseMmsColumns.MESSAGE_BOX_INBOX} AND ${Mms.MESSAGE_TYPE} = $NOTIFICATION_IND" + only.first,
            arrayOf(seconds(since).toString()) + only.second,
        ).size
    }

    // Android answers nothing for a provider that crashed a moment ago, and says a part's not
    // found for one gone as for a provider it can't reach, or one that failed the call: a part's
    // gone only once the provider, answering, holds no such part.
    override fun size(uri: String): Long? = try {
        val file = resolver.openAssetFileDescriptor(uri.toUri(), "r") ?: throw Unavailable(uri)
        file.use { it.length.takeIf { length -> length >= 0 } ?: open(uri).use(::count) }
    } catch (_: FileNotFoundException) {
        if (absent(uri.toUri())) null else throw Unavailable(uri)
    }

    override fun open(uri: String): InputStream = resolver.openInputStream(uri.toUri()) ?: throw Unavailable(uri)

    private fun smsRows(scan: Scan): List<Row> {
        val sim = sim(Table.SMS, scan.sim)
        return sms(
            Sms.CONTENT_URI,
            "${Sms.DATE} >= ? AND (${Sms.DATE} >= ? OR ${Sms._ID} > ?) AND " +
                "${Sms.TYPE} IN (${TextBasedSmsColumns.MESSAGE_TYPE_INBOX}, ${TextBasedSmsColumns.MESSAGE_TYPE_SENT})" + sim.first,
            arrayOf(scan.paired.toString(), scan.since.toString(), scan.sms.toString()) + sim.second,
        ).map { it.first }
    }

    // One received counts once the texting app has downloaded it, when it holds the message
    // itself and not only word of it.
    private fun mmsRows(scan: Scan): List<Row> {
        val sim = sim(Table.MMS, scan.sim)
        return mms(
            Mms.CONTENT_URI,
            "${Mms.DATE} >= ? AND (${Mms.DATE} >= ? OR ${Mms._ID} > ?) AND " +
                "((${Mms.MESSAGE_BOX} = ${BaseMmsColumns.MESSAGE_BOX_INBOX} AND ${Mms.MESSAGE_TYPE} = $RETRIEVE_CONF) OR " +
                "${Mms.MESSAGE_BOX} = ${BaseMmsColumns.MESSAGE_BOX_SENT})" + sim.first,
            arrayOf(seconds(scan.paired).toString(), seconds(scan.since).toString(), scan.mms.toString()) + sim.second,
        ).map { it.first }
    }

    /**
     * What keeps a read of [table] to the SIM [subscription]'s texts, and those whose SIM Android
     * didn't note: nothing where it's null, or where the provider doesn't say which SIM a text came
     * by, as on a phone that never had two.
     */
    private fun sim(table: Table, subscription: Int?): Pair<String, Array<String>> =
        if (subscription == null || !subscribed(table)) {
            "" to emptyArray()
        } else {
            " AND ($SUB_ID = ? OR $SUB_ID < 0 OR $SUB_ID IS NULL)" to arrayOf(subscription.toString())
        }

    /** Whether the provider's [table] says which SIM each text came by: asked once, as it's the same throughout. */
    private fun subscribed(table: Table): Boolean = subscribed.getOrPut(table) {
        val uri = if (table == Table.SMS) Sms.CONTENT_URI else Mms.CONTENT_URI
        (resolver.query(uri, null, "0", null, null) ?: throw Unavailable("its columns")).use { SUB_ID in it.columnNames }
    }

    /** The plain texts' rows at [uri] that [where] selects, each with its type: received, sent, failed. */
    private fun sms(uri: Uri, where: String?, args: Array<String>?): List<Pair<Row, Int>> = resolver.query(
        uri,
        arrayOf(Sms._ID, Sms.DATE, Sms.TYPE, Sms.CREATOR),
        where,
        args,
        null,
    ).rows { cursor ->
        val type = cursor.getInt(2)
        val direction = if (type == TextBasedSmsColumns.MESSAGE_TYPE_INBOX) Direction.IN else Direction.OUT
        Row(Table.SMS, cursor.getLong(0), cursor.getLong(1), direction, cursor.getString(3)) to type
    }

    /**
     * The picture messages' rows at [uri] that [where] selects, each with its box: received, sent,
     * failed. Their time is in seconds.
     */
    private fun mms(uri: Uri, where: String?, args: Array<String>?): List<Pair<Row, Int>> = resolver.query(
        uri,
        arrayOf(Mms._ID, Mms.DATE, Mms.MESSAGE_BOX, Mms.CREATOR),
        where,
        args,
        null,
    ).rows { cursor ->
        val box = cursor.getInt(2)
        val direction = if (box == BaseMmsColumns.MESSAGE_BOX_INBOX) Direction.IN else Direction.OUT
        Row(Table.MMS, cursor.getLong(0), cursor.getLong(1) * MS_PER_SECOND, direction, cursor.getString(3)) to box
    }

    // By its ID and time both: a row's ID is used again once it's deleted.
    private fun readSms(row: Row): Reading = resolver.query(
        Uri.withAppendedPath(Sms.CONTENT_URI, row.id.toString()),
        arrayOf(Sms.ADDRESS, Sms.BODY),
        "${Sms.DATE} = ?",
        arrayOf(row.at.toString()),
        null,
    ).rows { Message(row, listOfNotNull(it.getString(0)), it.getString(1).orEmpty(), emptyList()) }
        .firstOrNull()?.let(Reading::Whole) ?: Reading.Gone

    // The texting app writes a picture message's row first, then its parts and addresses: one
    // without them is still being written, and read again once it's whole.
    private fun readMms(row: Row): Reading {
        if (!there(row)) return Reading.Gone
        val numbers = addresses(row)
        val parts = parts(row)
        if (numbers.isEmpty() || parts.isEmpty()) return Reading.Later
        // A part of words, plain or HTML, is the message's words: Android keeps them in the part's
        // own row (on Android 8 beside an empty file), else in its file.
        val body = parts.filter { it.mime in WORDS }.joinToString("\n") { part ->
            val text = part.text ?: readText(part.uri)
            if (part.mime == HTML) HtmlCompat.fromHtml(text, HtmlCompat.FROM_HTML_MODE_COMPACT).toString().trim() else text
        }
        val files = parts.filter { it.mime !in WORDS && it.mime != SMIL }.mapNotNull { part ->
            size(part.uri)?.let { Part(part.uri, part.name, part.mime, it) }
        }
        return Reading.Whole(Message(row, numbers, body, files))
    }

    /** Whether the picture message [row] is still there, by its ID and time both. */
    private fun there(row: Row): Boolean = resolver.query(
        Uri.withAppendedPath(Mms.CONTENT_URI, row.id.toString()),
        arrayOf(Mms._ID),
        "${Mms.DATE} = ?",
        arrayOf((row.at / MS_PER_SECOND).toString()),
        null,
    ).rows {}.isNotEmpty()

    /** Whether the provider answers that it holds nothing at [uri]; not when it can't answer. */
    private fun absent(uri: Uri): Boolean =
        resolver.query(uri, arrayOf(BaseColumns._ID), null, null, null)?.use { it.count == 0 } ?: false

    /** A picture message's parts, in order: its words, its layout, and its pictures and files. */
    private fun parts(row: Row): List<MmsPart> = resolver.query(
        PARTS,
        arrayOf(Mms.Part._ID, Mms.Part.CONTENT_TYPE, Mms.Part.NAME, Mms.Part.CONTENT_LOCATION, Mms.Part.FILENAME, Mms.Part.TEXT),
        "${Mms.Part.MSG_ID} = ?",
        arrayOf(row.id.toString()),
        Mms.Part._ID,
    ).rows { cursor ->
        val id = cursor.getLong(0)
        // Senders name a part in any of three places, some within <>, and some in none.
        val name = (2..4).firstNotNullOfOrNull { cursor.getString(it)?.trim('<', '>', ' ')?.takeIf(String::isNotEmpty) }
        MmsPart(id, cursor.getString(1)?.trim()?.lowercase()?.takeIf(String::isNotEmpty) ?: BYTES, name ?: "part-$id", cursor.getString(5))
    }

    /** The sender of a received picture message, or each recipient of a sent one. */
    private fun addresses(row: Row): List<String> {
        val wanted = if (row.direction == Direction.IN) setOf(FROM) else setOf(TO, CC, BCC)
        return resolver.query(
            Uri.withAppendedPath(Mms.CONTENT_URI, "${row.id}/addr"),
            arrayOf(Mms.Addr.ADDRESS, Mms.Addr.TYPE),
            null,
            null,
            Mms.Addr._ID,
        ).rows { cursor -> cursor.getString(0)?.takeIf { cursor.getInt(1) in wanted } }
            .filterNotNull().filter { it.isNotBlank() && it != INSERT_ADDRESS_TOKEN }
    }

    private fun readText(uri: String) = try {
        open(uri).use { it.readBytes().decodeToString() }
    } catch (_: FileNotFoundException) {
        ""
    }

    private class MmsPart(val id: Long, val mime: String, val name: String, val text: String?) {
        val uri get() = Uri.withAppendedPath(PARTS, id.toString()).toString()
    }

    private companion object {
        const val MS_PER_SECOND = 1_000L

        /** The SIM a text came by, named the same in both tables. */
        const val SUB_ID = TextBasedSmsColumns.SUBSCRIPTION_ID

        /** A picture message as received, downloaded whole (`PduHeaders.MESSAGE_TYPE_RETRIEVE_CONF`). */
        const val RETRIEVE_CONF = 132

        /** Word of a picture message, not yet downloaded (`PduHeaders.MESSAGE_TYPE_NOTIFICATION_IND`). */
        const val NOTIFICATION_IND = 130

        // Which party of a picture message an address is (`PduHeaders`).
        const val FROM = 137
        const val TO = 151
        const val CC = 130
        const val BCC = 129

        /** What stands for the phone's own number among a sent picture message's addresses. */
        const val INSERT_ADDRESS_TOKEN = "insert-address-token"

        const val TEXT = "text/plain"
        const val HTML = "text/html"
        val WORDS = setOf(TEXT, HTML)
        const val SMIL = "application/smil"
        const val BYTES = "application/octet-stream"

        /** Picture messages' parts: `Telephony.Mms.Part.CONTENT_URI`, there only from Android 10. */
        val PARTS: Uri = "content://mms/part".toUri()

        /** [ms] in whole seconds, rounded up, as a picture message's time is. */
        fun seconds(ms: Long) = (ms + MS_PER_SECOND - 1) / MS_PER_SECOND

        fun count(stream: InputStream): Long {
            val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
            var total = 0L
            while (true) {
                val read = stream.read(buffer)
                if (read < 0) return total
                total += read
            }
        }

        /** Each row's value, closing the cursor; [Unavailable] when the provider answers nothing. */
        inline fun <T> Cursor?.rows(value: (Cursor) -> T): List<T> = (this ?: throw Unavailable("its texts")).use { cursor ->
            buildList { while (cursor.moveToNext()) add(value(cursor)) }
        }
    }
}
