package com.dspdispatch.relay.texts

import java.io.File
import java.io.IOException
import java.io.InputStream
import java.net.URI

/**
 * JPEGs made of pictures received as HEIF, as iPhones send them, which Dispatch's pages can't
 * show: made by [pictures], each kept in [folder] as its file's until its bytes are sent. Calls
 * block on the disk: never on the main thread.
 */
class Jpegs(private val folder: File, private val pictures: Pictures) : PartBytes {
    private val prefix = "${folder.absoluteFile.toURI().toString().removeSuffix("/")}/"

    /** Whether Android here can make them. */
    val possible get() = pictures.decodesHeif

    /**
     * [file] as a JPEG made of [picture], its bytes, kept until they're sent: or null when Android
     * can't make one, or it can't be kept.
     */
    fun of(file: WaitingFile, picture: ByteArray): WaitingFile? {
        val jpeg = pictures.jpeg(picture, Inbox.LARGEST.toInt(), LONGEST_SIDE) ?: return null
        val kept = File(folder.absoluteFile, "${file.id}$JPG")
        try {
            folder.mkdirs()
            kept.writeBytes(jpeg)
        } catch (_: IOException) {
            return null
        }
        return file.copy(uri = kept.toURI().toString(), name = jpegName(file.name), mime = JPEG, size = jpeg.size.toLong())
    }

    /** Whether [uri] is of a JPEG made here. */
    fun holds(uri: String) = uri.startsWith(prefix)

    override fun size(uri: String): Long? = File(URI(uri)).takeIf(File::isFile)?.length()

    override fun open(uri: String): InputStream = File(URI(uri)).inputStream()

    /** Lets go of the JPEG at [uri], its bytes sent or never to be; nothing for a part not made here. */
    fun remove(uri: String) {
        if (holds(uri)) File(URI(uri)).delete()
    }

    /** Lets go of every JPEG but those at [uris], as an earlier pairing's, or one left as the app stopped. */
    fun keepOnly(uris: Collection<String>) {
        val kept = uris.toSet()
        folder.absoluteFile.listFiles()?.filter { it.toURI().toString() !in kept }?.forEach(File::delete)
    }

    private companion object {
        /** The longest side a JPEG made has: a phone's 12-megapixel photo whole, one larger scaled to it. */
        const val LONGEST_SIDE = 4096
    }
}

/**
 * Whether a part named [name], of type [mime], is a HEIF picture: by its type, or by its name
 * when its type says only that it's bytes.
 */
internal fun heif(name: String, mime: String): Boolean =
    mime in HEIF_TYPES || (mime == BYTES && HEIF_NAME.containsMatchIn(name))

/** [name] as a JPEG's: `.jpg` in place of `.heic` or `.heif`, within what Dispatch takes. */
internal fun jpegName(name: String): String =
    HEIF_NAME.replace(name, "").limited(Inbox.NAME_MOST - JPG.length) + JPG

private val HEIF_TYPES = setOf("image/heic", "image/heif", "image/heic-sequence", "image/heif-sequence")
private val HEIF_NAME = Regex("\\.hei[cf]$", RegexOption.IGNORE_CASE)
private const val BYTES = "application/octet-stream"
private const val JPEG = "image/jpeg"
private const val JPG = ".jpg"
