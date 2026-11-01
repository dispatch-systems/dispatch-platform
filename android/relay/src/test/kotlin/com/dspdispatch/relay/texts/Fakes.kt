package com.dspdispatch.relay.texts

import androidx.room3.Room
import androidx.sqlite.driver.bundled.BundledSQLiteDriver
import java.io.ByteArrayInputStream
import java.io.File
import java.io.FileNotFoundException
import java.io.InputStream

/** The [Queue] as [StoredQueue] keeps it, in memory. */
class MemoryQueue : Queue {
    var mark: Mark? = null
    val seen = mutableMapOf<String, Seen>()
    val texts = mutableMapOf<String, Waiting>()
    val files = mutableListOf<WaitingFile>()
    private var nextFile = 1L

    override suspend fun mark() = mark

    override suspend fun begin(mark: Mark) {
        seen.clear()
        texts.clear()
        files.clear()
        this.mark = mark
    }

    override suspend fun seen(since: Long) = seen.values.filter { it.at >= since }.map(Seen::row)

    override suspend fun read(row: Seen, texts: List<Waiting>, files: List<WaitingFile>) {
        seen[row.row] = row
        texts.forEach { this.texts[it.clientId] = it }
        files.forEach { this.files += it.copy(id = nextFile++) }
    }

    override suspend fun scanned(at: Long, forget: Long) {
        mark = mark?.copy(scanned = at)
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

    override fun rows(since: Long) = (messages.map(Message::row) + writing).filter { it.at >= since }.sortedBy(Row::at)

    override fun read(row: Row) = messages.firstOrNull { it.row == row }

    override fun size(uri: String) = bytes[uri]?.size?.toLong()

    override fun open(uri: String): InputStream = ByteArrayInputStream(bytes[uri] ?: throw FileNotFoundException(uri))
}

/** Android's texting service, sending nothing: each text handed to it is kept in [handed]. */
class FakeRadio : Radio {
    var allowed = true
    var mmsMost: Int? = 300 * 1024

    /** How the carrier splits a text: in parts of this many characters. */
    var partLength = 160

    /** What Android throws as it's handed a text, if anything. */
    var refusing: RuntimeException? = null
    val handed = mutableListOf<Pair<Send, List<String>>>()
    val pictureMessages = mutableListOf<Pair<Send, ByteArray>>()

    /** The number each line was asked for. */
    val lines = mutableListOf<String>()

    override fun allowed() = allowed

    override fun line(number: String): Line {
        lines += number
        return object : Line {
            override fun divide(body: String) = body.chunked(partLength).ifEmpty { listOf("") }

            override fun mmsMost() = mmsMost

            override fun sms(send: Send, parts: List<String>) {
                refusing?.let { throw it }
                handed += send to parts
            }

            override fun mms(send: Send, pdu: File) {
                refusing?.let { throw it }
                pictureMessages += send to pdu.readBytes()
            }
        }
    }
}

/** The app's texts database, in memory, on SQLite itself. */
fun memoryDatabase(): TextsDatabase = Room.inMemoryDatabaseBuilder<TextsDatabase>().setDriver(BundledSQLiteDriver()).build()
