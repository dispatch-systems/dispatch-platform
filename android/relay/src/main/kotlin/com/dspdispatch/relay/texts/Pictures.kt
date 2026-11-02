package com.dspdispatch.relay.texts

import java.io.File
import kotlin.math.sqrt

/** Android's pictures: scaled down to fit a picture message, and made JPEGs Dispatch's pages show. */
interface Pictures {
    /** Whether Android decodes HEIF pictures here, as it does from Android 9. */
    val decodesHeif: Boolean

    /** The size of the picture in [file], or null when it can't be read as one. */
    fun size(file: File): Size?

    /** The picture in [file] as a JPEG of at most [most] bytes and within [within], or null when it can't be one. */
    fun shrink(file: File, most: Int, within: Size): ByteArray?

    /**
     * The picture [picture] holds, of a kind Android decodes, as a JPEG upright of at most [most]
     * bytes and [longest] pixels on its longest side, or null when it can't be one. Throws
     * [OutOfMemoryError] when the phone hasn't the memory for it.
     */
    fun jpeg(picture: ByteArray, most: Int, longest: Int): ByteArray?
}

/** A picture's size, or the largest a carrier takes, in pixels. */
data class Size(val width: Int, val height: Int) {
    /** Whether a picture this size fits within [limit], either way up. */
    fun fits(limit: Size): Boolean {
        val upright = limit.turnedTo(this)
        return width <= upright.width && height <= upright.height
    }

    /** The longest side a picture this size has, scaled down as far as [limit] needs, either way up. */
    fun longestWithin(limit: Size): Int {
        val upright = limit.turnedTo(this)
        val scale = minOf(1.0, upright.width.toDouble() / width, upright.height.toDouble() / height)
        return (maxOf(width, height) * scale).toInt()
    }

    /** The longest side a picture this size is sent at in a picture message: within [limit], and [LARGEST_SIDE] at most. */
    fun sentWithin(limit: Size): Int = minOf(longestWithin(limit), LARGEST_SIDE)

    // A carrier's limit is for a picture either way up: turned to match it, it's scaled as little
    // as it can be, as Android's own texting app does.
    private fun turnedTo(picture: Size) = if ((picture.height > picture.width) != (height > width)) Size(height, width) else this
}

/**
 * The first of [encode]'s JPEGs, of a picture at most [longest] pixels on its longest side, within
 * [most] bytes: at [longest], then smaller each time, each in a high quality first. Null when none
 * fits, or [encode] can't make one.
 */
internal fun fit(most: Int, longest: Int, encode: (side: Int, quality: Int) -> ByteArray?): ByteArray? {
    var side = longest
    // A picture already smaller than any it's shrunk to is tried as it is, never scaled up.
    do {
        var size = 0
        for (quality in QUALITIES) {
            val bytes = encode(side, quality) ?: return null
            if (bytes.size <= most) return bytes
            size = bytes.size
        }
        // A JPEG's size goes about with its pixels: aim for a little under what fits.
        side = (side * minOf(sqrt(most.toDouble() / size) * AIM, STEP)).toInt()
    } while (side >= SMALLEST_SIDE)
    return null
}

/** The longest side a picture is sent at, as phones show a picture message's. */
internal const val LARGEST_SIDE = 1920

private const val SMALLEST_SIDE = 160
private val QUALITIES = listOf(85, 70)
private const val AIM = 0.9
private const val STEP = 0.9
