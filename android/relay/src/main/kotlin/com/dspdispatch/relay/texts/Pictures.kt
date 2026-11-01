package com.dspdispatch.relay.texts

import java.io.File
import kotlin.math.sqrt

/** Scales pictures down to fit a picture message. */
fun interface Pictures {
    /** The picture in [file] as a JPEG of at most [most] bytes, or null when it can't be one. */
    fun shrink(file: File, most: Int): ByteArray?
}

/**
 * The first of [encode]'s JPEGs, of a picture [longest] pixels on its longest side, within [most]
 * bytes: at its full size or [LARGEST_SIDE], then smaller each time, each in a high quality first.
 */
internal fun fit(most: Int, longest: Int, encode: (side: Int, quality: Int) -> ByteArray): ByteArray? {
    var side = minOf(longest, LARGEST_SIDE)
    while (side >= SMALLEST_SIDE) {
        var size = 0
        for (quality in QUALITIES) {
            val bytes = encode(side, quality)
            if (bytes.size <= most) return bytes
            size = bytes.size
        }
        // A JPEG's size goes about with its pixels: aim for a little under what fits.
        side = (side * minOf(sqrt(most.toDouble() / size) * AIM, STEP)).toInt()
    }
    return null
}

/** The longest side a picture is sent at, as phones show a picture message's. */
internal const val LARGEST_SIDE = 1920

private const val SMALLEST_SIDE = 160
private val QUALITIES = listOf(85, 70)
private const val AIM = 0.9
private const val STEP = 0.9
