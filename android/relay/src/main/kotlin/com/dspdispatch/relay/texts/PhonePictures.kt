package com.dspdispatch.relay.texts

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.ImageDecoder
import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.Rect
import android.os.Build
import androidx.core.graphics.createBitmap
import androidx.exifinterface.media.ExifInterface
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.IOException
import java.nio.ByteBuffer

/** [Pictures] by Android's own decoders, turned upright as the camera that took them said. */
object PhonePictures : Pictures {
    override val decodesHeif get() = Build.VERSION.SDK_INT >= Build.VERSION_CODES.P

    override fun size(file: File): Size? {
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeFile(file.path, bounds)
        return Size(bounds.outWidth, bounds.outHeight).takeIf { it.width > 0 && it.height > 0 }
    }

    override fun shrink(file: File, most: Int, within: Size): ByteArray? {
        val size = size(file) ?: return null
        val longest = minOf(size.longestWithin(within), LARGEST_SIDE)
        // Decoded no larger than it's sent: a phone's photo is far larger, and its pixels many.
        var sample = 1
        while (maxOf(size.width, size.height) / (sample * 2) >= longest) sample *= 2
        val decoded = BitmapFactory.decodeFile(file.path, BitmapFactory.Options().apply { inSampleSize = sample }) ?: return null
        val picture = upright(decoded, rotation(file))
        return try {
            fit(most, minOf(longest, maxOf(picture.width, picture.height))) { side, quality -> jpeg(picture, side, quality) }
        } finally {
            picture.recycle()
        }
    }

    override fun jpeg(picture: ByteArray, most: Int, longest: Int): ByteArray? {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.P) return null
        // Android's decoder turns the picture upright itself, as its own rotation says: turned by
        // its EXIF orientation too, which an iPhone's says the same, it would be turned twice.
        val decoded = try {
            ImageDecoder.decodeBitmap(ImageDecoder.createSource(ByteBuffer.wrap(picture))) { decoder, info, _ ->
                // Drawn on the app's own canvas, which a picture in graphics memory can't be.
                decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
                val scale = longest.toDouble() / maxOf(info.size.width, info.size.height)
                if (scale < 1) {
                    decoder.setTargetSize((info.size.width * scale).toInt().coerceAtLeast(1), (info.size.height * scale).toInt().coerceAtLeast(1))
                }
            }
        } catch (_: IOException) {
            return null
        }
        return try {
            fit(most, maxOf(decoded.width, decoded.height)) { side, quality -> jpeg(decoded, side, quality) }
        } finally {
            decoded.recycle()
        }
    }

    private fun rotation(file: File): Int = try {
        ExifInterface(file).rotationDegrees
    } catch (_: IOException) {
        0
    }

    private fun upright(bitmap: Bitmap, degrees: Int): Bitmap {
        if (degrees == 0) return bitmap
        val turned = Bitmap.createBitmap(bitmap, 0, 0, bitmap.width, bitmap.height, Matrix().apply { postRotate(degrees.toFloat()) }, true)
        if (turned != bitmap) bitmap.recycle()
        return turned
    }

    /** [picture] as a JPEG [side] pixels on its longest side: on white, as a JPEG has no transparency. */
    private fun jpeg(picture: Bitmap, side: Int, quality: Int): ByteArray {
        val scale = side.toFloat() / maxOf(picture.width, picture.height)
        val width = (picture.width * scale).toInt().coerceAtLeast(1)
        val height = (picture.height * scale).toInt().coerceAtLeast(1)
        val flat = createBitmap(width, height)
        Canvas(flat).apply {
            drawColor(Color.WHITE)
            drawBitmap(picture, null, Rect(0, 0, width, height), Paint(Paint.FILTER_BITMAP_FLAG))
        }
        return try {
            ByteArrayOutputStream().also { flat.compress(Bitmap.CompressFormat.JPEG, quality, it) }.toByteArray()
        } finally {
            flat.recycle()
        }
    }
}
