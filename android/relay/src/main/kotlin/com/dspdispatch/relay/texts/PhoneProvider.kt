package com.dspdispatch.relay.texts

import android.content.ContentResolver
import android.database.Cursor
import android.net.Uri
import android.provider.Telephony.BaseMmsColumns
import android.provider.Telephony.Mms
import android.provider.Telephony.Sms
import android.provider.Telephony.TextBasedSmsColumns
import androidx.core.net.toUri
import com.dspdispatch.relay.dispatch.Direction
import java.io.FileNotFoundException
import java.io.InputStream

/** The phone's texts in Android's own texting provider, `Telephony.Sms` and `Telephony.Mms`. */
class PhoneProvider(private val resolver: ContentResolver) : Provider {
    override fun rows(since: Long): List<Row> = (smsRows(since) + mmsRows(since)).sortedWith(compareBy(Row::at, Row::id))

    override fun read(row: Row): Message? = when (row.table) {
        Table.SMS -> readSms(row)
        Table.MMS -> readMms(row)
    }

    override fun size(uri: String): Long? = try {
        resolver.openAssetFileDescriptor(uri.toUri(), "r")?.use { file ->
            file.length.takeIf { it >= 0 } ?: resolver.openInputStream(uri.toUri())?.use(::count)
        }
    } catch (_: FileNotFoundException) {
        null
    }

    override fun open(uri: String): InputStream =
        resolver.openInputStream(uri.toUri()) ?: throw FileNotFoundException(uri)

    private fun smsRows(since: Long): List<Row> = resolver.query(
        Sms.CONTENT_URI,
        arrayOf(Sms._ID, Sms.DATE, Sms.TYPE, Sms.CREATOR),
        "${Sms.DATE} >= ? AND ${Sms.TYPE} IN (${TextBasedSmsColumns.MESSAGE_TYPE_INBOX}, ${TextBasedSmsColumns.MESSAGE_TYPE_SENT})",
        arrayOf(since.toString()),
        null,
    ).rows { cursor ->
        val direction = if (cursor.getInt(2) == TextBasedSmsColumns.MESSAGE_TYPE_INBOX) Direction.IN else Direction.OUT
        Row(Table.SMS, cursor.getLong(0), cursor.getLong(1), direction, cursor.getString(3))
    }

    // A picture message's time is in seconds. One received counts once the texting app has
    // downloaded it, when it holds the message itself and not only word of it.
    private fun mmsRows(since: Long): List<Row> = resolver.query(
        Mms.CONTENT_URI,
        arrayOf(Mms._ID, Mms.DATE, Mms.MESSAGE_BOX, Mms.CREATOR),
        "${Mms.DATE} >= ? AND ((${Mms.MESSAGE_BOX} = ${BaseMmsColumns.MESSAGE_BOX_INBOX} AND " +
            "${Mms.MESSAGE_TYPE} = $RETRIEVE_CONF) OR ${Mms.MESSAGE_BOX} = ${BaseMmsColumns.MESSAGE_BOX_SENT})",
        arrayOf(((since + MS_PER_SECOND - 1) / MS_PER_SECOND).toString()),
        null,
    ).rows { cursor ->
        val direction = if (cursor.getInt(2) == BaseMmsColumns.MESSAGE_BOX_INBOX) Direction.IN else Direction.OUT
        Row(Table.MMS, cursor.getLong(0), cursor.getLong(1) * MS_PER_SECOND, direction, cursor.getString(3))
    }

    private fun readSms(row: Row): Message? = resolver.query(
        Uri.withAppendedPath(Sms.CONTENT_URI, row.id.toString()),
        arrayOf(Sms.ADDRESS, Sms.BODY),
        null,
        null,
        null,
    ).rows { Message(row, listOfNotNull(it.getString(0)), it.getString(1).orEmpty(), emptyList()) }.firstOrNull()

    // The texting app writes a picture message's row first, then its parts and addresses: one
    // without them is still being written, and read again once it's whole.
    private fun readMms(row: Row): Message? {
        val numbers = addresses(row)
        val parts = parts(row)
        if (numbers.isEmpty() || parts.isEmpty()) return null
        val body = parts.filter { it.mime == TEXT }.joinToString("\n") { it.text ?: readText(it.uri) }
        val files = parts.filter { it.mime != TEXT && it.mime != SMIL }.mapNotNull { part ->
            size(part.uri)?.let { Part(part.uri, part.name, part.mime, it) }
        }
        return Message(row, numbers, body, files)
    }

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

        /** A picture message as received, downloaded whole (`PduHeaders.MESSAGE_TYPE_RETRIEVE_CONF`). */
        const val RETRIEVE_CONF = 132

        // Which party of a picture message an address is (`PduHeaders`).
        const val FROM = 137
        const val TO = 151
        const val CC = 130
        const val BCC = 129

        /** What stands for the phone's own number among a sent picture message's addresses. */
        const val INSERT_ADDRESS_TOKEN = "insert-address-token"

        const val TEXT = "text/plain"
        const val SMIL = "application/smil"
        const val BYTES = "application/octet-stream"

        /** Picture messages' parts: `Telephony.Mms.Part.CONTENT_URI`, there only from Android 10. */
        val PARTS: Uri = "content://mms/part".toUri()

        fun count(stream: InputStream): Long {
            val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
            var total = 0L
            while (true) {
                val read = stream.read(buffer)
                if (read < 0) return total
                total += read
            }
        }

        /** Each row's value, closing the cursor; none when the provider answers nothing. */
        inline fun <T> Cursor?.rows(value: (Cursor) -> T): List<T> = this?.use { cursor ->
            buildList { while (cursor.moveToNext()) add(value(cursor)) }
        }.orEmpty()
    }
}
