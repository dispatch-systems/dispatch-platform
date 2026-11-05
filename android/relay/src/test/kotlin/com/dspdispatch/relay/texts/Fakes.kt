package com.dspdispatch.relay.texts

import androidx.room3.Room
import androidx.sqlite.driver.bundled.BundledSQLiteDriver
import androidx.sqlite.execSQL
import com.dspdispatch.relay.dispatch.Direction
import java.io.ByteArrayInputStream
import java.io.File
import java.io.FileNotFoundException
import java.io.InputStream
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/** The [Queue] as [StoredQueue] keeps it, in memory. */
class MemoryQueue : Queue {
    var mark: Mark? = null
    val seen = mutableMapOf<String, Seen>()
    val unread = mutableMapOf<String, Unread>()
    val texts = mutableMapOf<String, Waiting>()
    val files = mutableListOf<WaitingFile>()
    private var nextFile = 1L

    override suspend fun mark() = mark

    override suspend fun begin(mark: Mark) {
        seen.clear()
        unread.clear()
        texts.clear()
        files.clear()
        this.mark = mark
    }

    override suspend fun seen() = seen.keys.toList()

    override suspend fun read(row: Seen, texts: List<Waiting>, files: List<WaitingFile>) {
        seen[row.row] = row
        unread.remove(row.row)
        texts.forEach { this.texts[it.clientId] = it }
        files.forEach { this.files += it.copy(id = nextFile++) }
    }

    override suspend fun unread() = unread.values.toList()

    override suspend fun later(row: Unread) {
        unread.putIfAbsent(row.key, row)
    }

    override suspend fun scanned(mark: Mark, forget: Long) {
        this.mark = mark
        seen.values.removeAll { it.at < forget }
    }

    override suspend fun waiting(most: Int) = texts.values.sortedWith(compareBy(Waiting::at, Waiting::clientId)).take(most)
        .map { text -> WaitingText(text, files.filter { it.clientId == text.clientId }) }

    override suspend fun answered(answered: Map<String, List<String>>) {
        for ((clientId, ids) in answered) {
            texts.remove(clientId)
            files.replaceAll { if (it.clientId == clientId) it.copy(dispatchId = ids.getOrNull(it.position)) else it }
            files.removeAll { it.clientId == clientId && it.dispatchId == null }
        }
    }

    override suspend fun refused(clientId: String) {
        texts.remove(clientId)
        files.removeAll { it.clientId == clientId }
    }

    override suspend fun settled(files: List<WaitingFile>) {
        val settled = files.associateBy(WaitingFile::id)
        this.files.replaceAll { settled[it.id] ?: it }
    }

    override suspend fun uris() = files.map(WaitingFile::uri)

    override suspend fun upload() = files.filter { it.dispatchId != null }.minByOrNull(WaitingFile::id)

    override suspend fun uploaded(id: Long) {
        files.removeAll { it.id == id }
    }
}

/** A phone's texting provider, holding [messages], and its parts' bytes in [bytes] by their URI. */
class FakeProvider : Provider {
    val messages = mutableListOf<Message>()
    val bytes = mutableMapOf<String, ByteArray>()

    /** Rows the texting app is still writing: there, but not read whole yet. */
    val writing = mutableSetOf<Row>()

    /** Rows of texts sent that are in Failed, not Sent: never read as texts the phone sent. */
    val failed = mutableSetOf<Row>()

    /** Whether the provider answers at all: not while it crashed a moment ago. */
    var answering = true

    /** Whether the phone lets the app read its texts: reading them while it doesn't fails, as on a phone. */
    var allowed = true

    /** Parts the provider can't read the next time, as when it crashed a moment before. */
    val away = mutableSetOf<String>()

    /** Parts whose bytes fail to open in a way the provider doesn't say they may, every time. */
    val broken = mutableSetOf<String>()

    /** Rows the provider fails on in a way it doesn't say it may, every time. */
    val failing = mutableSetOf<Row>()

    /** Whether Texting is turned off as the next read starts, just after the app saw it on. */
    var turnedOffAtRead = false

    /** How many reads failed as the phone didn't allow them. */
    var refused = 0

    override fun rows(scan: Scan) = (messages.map(Message::row) + writing)
        .filter { it !in failed && it.at >= scan.paired && (it.at >= scan.since || it.id > if (it.table == Table.SMS) scan.sms else scan.mms) }
        .sortedBy(Row::at)

    override fun read(row: Row): Reading {
        if (row in failing) throw IllegalStateException("Couldn't read row 0, col -1 from CursorWindow")
        if (row in writing) return Reading.Later
        val message = messages.firstOrNull { it.row == row } ?: return Reading.Gone
        return if (message.parts.any { away.remove(it.uri) }) Reading.Later else Reading.Whole(message)
    }

    override fun message(uri: String): Message? {
        val (table, id) = rowAt(uri) ?: return null
        return messages.firstOrNull { it.row.table == table && it.row.id == id }
    }

    override fun tried(table: Table, since: Long): List<Tried> {
        if (!answering) throw Unavailable("its texts")
        return messages.map(Message::row).filter { it.table == table && it.direction == Direction.OUT && it.at >= since }
            .map { Tried(it, it !in failed) }
    }

    override fun size(uri: String): Long? {
        readable()
        if (away.remove(uri)) throw Unavailable(uri)
        return bytes[uri]?.size?.toLong()
    }

    override fun open(uri: String): InputStream {
        readable()
        if (uri in broken) throw IllegalArgumentException("Unknown URI $uri")
        return ByteArrayInputStream(bytes[uri] ?: throw FileNotFoundException(uri))
    }

    private fun readable() {
        if (turnedOffAtRead) {
            turnedOffAtRead = false
            allowed = false
        }
        if (allowed) return
        refused++
        throw SecurityException("Permission Denial: reading the texting provider")
    }
}

/** Android's texting service, sending nothing: each text handed to it is kept in [handed]. */
class FakeRadio : Radio {
    var allowed = true
    var smsLimit = SmsLimit.ANDROID
    var mmsLimits: MmsLimits? = MmsLimits(300 * 1024, Size(1920, 1920))

    /** How the carrier splits a text: in parts of this many characters. */
    var partLength = 160

    /** What Android throws as it's handed a text, if anything. */
    var refusing: RuntimeException? = null

    /** What Android throws as it's asked of the carrier's picture messages, if anything. */
    var failing: RuntimeException? = null
    val handed = mutableListOf<Pair<Send, List<String>>>()
    val pictureMessages = mutableListOf<Pair<Send, ByteArray>>()

    /** The number each line was asked for. */
    val lines = mutableListOf<String>()

    override fun allowed() = allowed

    override fun smsLimit() = smsLimit

    override fun relayed(number: String, subscription: Int): Int? = null

    override fun line(number: String, subscription: Int): Line {
        lines += number
        return object : Line {
            override fun divide(body: String): Parts {
                val texts = body.chunked(partLength).ifEmpty { listOf("") }
                return Parts(texts, texts.size > 1)
            }

            override fun mmsLimits(): MmsLimits? {
                failing?.let { throw it }
                return mmsLimits
            }

            override fun sms(send: Send, parts: Parts) {
                refusing?.let { throw it }
                handed += send to parts.texts
            }

            override fun mms(send: Send, pdu: File) {
                refusing?.let { throw it }
                pictureMessages += send to pdu.readBytes()
            }
        }
    }
}

/**
 * Pictures as Android would scale them, each [sizes] says, by [shrink]: what each shrinking was
 * asked is kept in [asked]. [decodesHeif] is whether it says Android here decodes HEIF; whatever
 * it says, each JPEG it's asked for is [jpeg]'s, and what it was asked, the picture and the most
 * bytes and pixels, is kept in [made].
 */
class FakePictures(
    private val sizes: Map<String, Size> = emptyMap(),
    override val decodesHeif: Boolean = true,
    private val jpeg: (picture: ByteArray) -> ByteArray? = { null },
    private val shrink: (most: Int) -> ByteArray? = { ByteArray(it / 2) },
) : Pictures {
    val asked = mutableListOf<Triple<String, Int, Size>>()
    val made = mutableListOf<Triple<List<Byte>, Int, Int>>()

    override fun jpeg(picture: ByteArray, most: Int, longest: Int): ByteArray? {
        made += Triple(picture.toList(), most, longest)
        return jpeg(picture)
    }

    override fun size(file: File) = sizes[file.name]

    override fun shrink(file: File, most: Int, within: Size): ByteArray? {
        asked += Triple(file.name, most, within)
        return shrink(most)
    }
}

/** The app's texts database, in memory, on SQLite itself. */
fun memoryDatabase(): TextsDatabase = Room.inMemoryDatabaseBuilder<TextsDatabase>().setDriver(BundledSQLiteDriver()).build()

/** The app's texts database in [file], on SQLite itself, as it's kept across the app's restarts. */
fun fileDatabase(file: File): TextsDatabase =
    Room.databaseBuilder<TextsDatabase>(file.path).setDriver(BundledSQLiteDriver()).build()

/**
 * Writes the app's texts database in [file] as an earlier app kept it, at [version] of its schema
 * (`schemas/`), holding what [rows] put in it: for the app as it is to open, as after an update.
 */
fun earlierDatabase(file: File, version: Int, vararg rows: String) {
    val schema = Json.parseToJsonElement(File("schemas/${TextsDatabase::class.qualifiedName}/$version.json").readText())
        .jsonObject.getValue("database").jsonObject
    val tables = schema.getValue("entities").jsonArray.map { it.jsonObject }.flatMap { table ->
        val name = table.getValue("tableName").jsonPrimitive.content
        val indices = table["indices"]?.jsonArray.orEmpty().map { it.jsonObject.getValue("createSql") }
        (listOf(table.getValue("createSql")) + indices).map { it.jsonPrimitive.content.replace("\${TABLE_NAME}", name) }
    }
    val setup = schema.getValue("setupQueries").jsonArray.map { it.jsonPrimitive.content }
    BundledSQLiteDriver().open(file.path).use { connection ->
        (tables + setup + "PRAGMA user_version = $version" + rows).forEach(connection::execSQL)
    }
}
