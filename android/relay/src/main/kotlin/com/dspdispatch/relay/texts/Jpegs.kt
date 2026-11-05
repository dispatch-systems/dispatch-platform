package com.dspdispatch.relay.texts

import java.io.File
import java.io.IOException
import java.io.InputStream
import java.net.URI

/**
 * JPEGs made of pictures received as HEIF, as iPhones send them, which Dispatch's pages can't
 * show: made by [pictures], each kept in [folder] as its file's until its bytes are sent; and of
 * videos' first frames, each video copied there while it's read. Calls block on the disk: never on
 * the main thread.
 */
class Jpegs(private val folder: File, private val pictures: Pictures) : PartBytes {
    private val prefix = "${folder.absoluteFile.toURI().toString().removeSuffix("/")}/"

    /** Whether Android here can make them. */
    val possible get() = pictures.decodesHeif

    /**
     * [file] as a JPEG made of its picture, read from [picture], kept until it's sent: or null when
     * Android can't make one, the phone hasn't the memory to, or it can't be kept.
     */
    fun of(file: WaitingFile, picture: () -> InputStream): WaitingFile? {
        val jpeg = try {
            pictures.jpeg(picture().use(InputStream::readBytes), Inbox.LARGEST.toInt(), LONGEST_SIDE)
        } catch (_: OutOfMemoryError) {
            // A picture too large for what memory the phone has: it goes as it is.
            null
        } catch (lost: SecurityException) {
            throw lost
        } catch (_: RuntimeException) {
            // A picture Android fails on in a way it doesn't say it may: it goes as it is too.
            null
        } ?: return null
        val kept = File(folder.absoluteFile, "${file.id}$JPG")
        try {
            folder.mkdirs()
            kept.writeBytes(jpeg)
        } catch (_: IOException) {
            // Written in part, as when the phone's storage fills: none of it kept.
            kept.delete()
            return null
        }
        return file.copy(uri = kept.toURI().toString(), name = jpegName(file.name), mime = JPEG, size = jpeg.size.toLong())
    }

    /**
     * The first frame of [file], a video read from [video], as a JPEG small enough to show beside
     * its text: or null when Android can't make one, the phone hasn't the memory to, or it can't be
     * copied here to read. The provider away as it's read, it throws [Unavailable], to try again.
     */
    fun frame(file: WaitingFile, video: () -> InputStream): ByteArray? {
        val copy = File(folder.absoluteFile, "${file.id}$VIDEO")
        return try {
            folder.mkdirs()
            video().use { input -> copy.outputStream().use(input::copyTo) }
            pictures.frame(copy, FRAME_MOST, FRAME_SIDE)
        } catch (away: Unavailable) {
            throw away
        } catch (_: IOException) {
            // Gone from the provider, or the phone's storage full: it's kept on the phone without one.
            null
        } catch (_: OutOfMemoryError) {
            null
        } finally {
            copy.delete()
        }
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

        /** The most bytes a video's first frame is, as Dispatch takes it (`PREVIEW_MOST`). */
        const val FRAME_MOST = 256 * 1024

        /** The longest side a video's first frame has, as large as a page shows it, and twice that. */
        const val FRAME_SIDE = 640

        /** What a video copied here to read is named after its file's ID. */
        const val VIDEO = ".video"
    }
}

/**
 * Whether a part named [name], of type [mime], is a still HEIF picture: by its type, or by its
 * name when its type says only that it's bytes. A HEIF sequence, more than one picture, isn't:
 * a JPEG would keep only one.
 */
internal fun heif(name: String, mime: String): Boolean =
    mime in HEIF_TYPES || (mime == BYTES && HEIF_NAME.containsMatchIn(name))

/** [name] as a JPEG's: `.jpg` in place of `.heic` or `.heif`, within what Dispatch takes. */
internal fun jpegName(name: String): String =
    HEIF_NAME.replace(name, "").limited(Inbox.NAME_MOST - JPG.length) + JPG

private val HEIF_TYPES = setOf("image/heic", "image/heif")
private val HEIF_NAME = Regex("\\.hei[cf]$", RegexOption.IGNORE_CASE)
private const val BYTES = "application/octet-stream"
private const val JPEG = "image/jpeg"
private const val JPG = ".jpg"
