package com.dspdispatch.relay.texts

import java.io.ByteArrayInputStream
import java.io.FileNotFoundException
import java.io.InputStream

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

    /** Whether the phone lets the app read its texts: reading them while it doesn't fails, as on a phone. */
    var allowed = true

    /** Parts the provider can't read the next time, as when it crashed a moment before. */
    val away = mutableSetOf<String>()

    /** Whether Texting is turned off as the next read starts, just after the app saw it on. */
    var turnedOffAtRead = false

    /** How many reads failed as the phone didn't allow them. */
    var refused = 0

    override fun rows(scan: Scan) = (messages.map(Message::row) + writing)
        .filter { it.at >= scan.paired && (it.at >= scan.since || it.id > if (it.table == Table.SMS) scan.sms else scan.mms) }
        .sortedBy(Row::at)

    override fun read(row: Row): Reading {
        if (row in writing) return Reading.Later
        val message = messages.firstOrNull { it.row == row } ?: return Reading.Gone
        return if (message.parts.any { away.remove(it.uri) }) Reading.Later else Reading.Whole(message)
    }

    override fun size(uri: String): Long? {
        readable()
        if (away.remove(uri)) throw Unavailable(uri)
        return bytes[uri]?.size?.toLong()
    }

    override fun open(uri: String): InputStream {
        readable()
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
