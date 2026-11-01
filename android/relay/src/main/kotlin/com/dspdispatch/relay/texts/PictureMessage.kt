package com.dspdispatch.relay.texts

import java.io.File

/** A file of a text to send, its bytes fetched into [file]. */
class Attachment(val name: String, val mime: String, val file: File)

/**
 * A text with pictures or files as the picture message carriers take: its files, its words, and a
 * layout of them, within the carrier's largest message, its pictures scaled down to fit.
 */
class PictureMessage(private val pictures: Pictures) {
    /**
     * The M-Send.req of [body] and [attachments] to [to], within the carrier's [limits], or null
     * when they can't fit: too large a file that isn't a picture, or a picture that won't shrink.
     * A picture Android can't read the size of goes as it is, if its bytes fit.
     */
    fun compose(to: String, body: String, attachments: List<Attachment>, limits: MmsLimits, transaction: String, date: Long): ByteArray? {
        val files = attachments.map { Item(it.name, it.mime, it.file.length()) { it.file.readBytes() } }
        val fixed = encode(to, body, files.map { it.copy(read = { ByteArray(0) }) }, transaction, date).size + MARGIN
        val room = limits.most - fixed - files.filterNot(Item::picture).sumOf(Item::size)
        if (room < 0) return null
        val pictures = files.indices.filter { files[it].picture }
        val fitted = files.toMutableList()
        for ((picture, share) in pictures.zip(allot(room, pictures.map { files[it].size }))) {
            val file = attachments[picture].file
            if (files[picture].size <= share && this.pictures.size(file)?.fits(limits.picture) != false) continue
            val bytes = this.pictures.shrink(file, share.toInt(), limits.picture) ?: return null
            fitted[picture] = Item("${files[picture].name.substringBeforeLast('.')}.jpg", JPEG, bytes.size.toLong()) { bytes }
        }
        return encode(to, body, fitted, transaction, date).takeIf { it.size <= limits.most }
    }

    /** A file as it goes into the message: [read] gives its bytes. */
    private data class Item(val name: String, val mime: String, val size: Long, val read: () -> ByteArray) {
        val picture get() = mime.startsWith(IMAGE)
    }

    private fun encode(to: String, body: String, files: List<Item>, transaction: String, date: Long): ByteArray {
        val names = mutableSetOf(LAYOUT, WORDS)
        val parts = files.map { PduPart(it.mime, unique(names, safe(it.name)), it.read()) }
        val words = listOfNotNull(body.takeIf(String::isNotEmpty)?.let { PduPart(TEXT, WORDS, it.toByteArray(Charsets.UTF_8)) })
        return Pdu.sendReq(to, transaction, date, LAYOUT, listOf(PduPart(SMIL, LAYOUT, layout(parts, words).toByteArray())) + parts + words)
    }

    /** The message's layout: each file on a slide of its own, then the words. */
    private fun layout(files: List<PduPart>, words: List<PduPart>): String {
        val slides = (files + words).joinToString("") { part ->
            val element = when {
                part.mime.startsWith(IMAGE) -> "img"
                part.mime.startsWith("video/") -> "video"
                part.mime.startsWith("audio/") -> "audio"
                part.mime == TEXT -> "text"
                else -> "ref"
            }
            val region = if (element == "text") "Text" else "Image"
            """<par dur="5000ms"><$element src="${part.name}" region="$region"/></par>"""
        }
        return "<smil><head><layout><root-layout/>" +
            """<region id="Image" top="0" left="0" height="80%" width="100%" fit="meet"/>""" +
            """<region id="Text" top="80%" left="0" height="20%" width="100%" fit="scroll"/>""" +
            "</layout></head><body>$slides</body></smil>"
    }

    private companion object {
        const val IMAGE = "image/"
        const val JPEG = "image/jpeg"
        const val TEXT = "text/plain"
        const val SMIL = "application/smil"
        const val LAYOUT = "smil.xml"
        const val WORDS = "text.txt"
        const val NAME_MOST = 40

        /** Room for what a file's header grows by as its picture is scaled and renamed. */
        const val MARGIN = 512

        /** [name] as carriers take a part's: plain letters, digits, dots, dashes and underscores. */
        fun safe(name: String) =
            String(name.map { if (it in 'a'..'z' || it in 'A'..'Z' || it in '0'..'9' || it in "._-") it else '_' }.toCharArray())
                .takeLast(NAME_MOST).trimStart('.').ifEmpty { "file" }

        /** [name], numbered when another part of the message has it already. */
        fun unique(taken: MutableSet<String>, name: String): String {
            val unique = generateSequence(1) { it + 1 }.map { if (it == 1) name else "$it-$name" }.first { it !in taken }
            taken += unique
            return unique
        }
    }
}

/**
 * How much of [room] bytes each of pictures of [sizes] gets: an even share, a picture smaller than
 * its share leaving the rest to the larger ones.
 */
internal fun allot(room: Long, sizes: List<Long>): List<Long> {
    val allotted = LongArray(sizes.size)
    var left = room
    for ((index, i) in sizes.indices.sortedBy { sizes[it] }.withIndex()) {
        val share = left / (sizes.size - index)
        allotted[i] = minOf(sizes[i], share)
        left -= allotted[i]
    }
    return allotted.toList()
}
