package com.dspdispatch.relay.update

import com.dspdispatch.relay.dispatch.AppDetails
import java.io.File
import java.io.InputStream
import java.security.DigestInputStream
import java.security.MessageDigest

/** A download that isn't the APK the DSP's details describe: another size or SHA-256. */
class Mismatch(message: String) : Exception(message)

/**
 * Saves the APK [input] streams to [to], and throws a [Mismatch] unless it's [expected]'s size and
 * SHA-256. Stops as soon as it's larger, so a wrong
 * answer never fills the phone. Blocks: call it off the main thread.
 */
fun save(input: InputStream, to: File, expected: AppDetails) {
    val digest = MessageDigest.getInstance(SHA_256)
    var saved = 0L
    to.outputStream().use { output ->
        val buffer = ByteArray(BUFFER_BYTES)
        while (true) {
            val read = input.read(buffer)
            if (read < 0) break
            saved += read
            if (saved > expected.size) throw Mismatch("Larger than ${expected.size} bytes")
            digest.update(buffer, 0, read)
            output.write(buffer, 0, read)
        }
    }
    if (saved != expected.size) throw Mismatch("$saved bytes of ${expected.size}")
    val sha256 = digest.hex()
    if (sha256 != expected.sha256) throw Mismatch("SHA-256 $sha256, not ${expected.sha256}")
}

/**
 * Whether [file] is the whole APK [expected] describes, as kept from a try before: its size and
 * SHA-256. Blocks: call it off the main thread.
 */
fun saved(file: File, expected: AppDetails): Boolean {
    if (!file.isFile || file.length() != expected.size) return false
    val digest = MessageDigest.getInstance(SHA_256)
    DigestInputStream(file.inputStream(), digest).use { input ->
        val buffer = ByteArray(BUFFER_BYTES)
        while (input.read(buffer) >= 0) continue
    }
    return digest.hex() == expected.sha256
}

private fun MessageDigest.hex() = digest().joinToString("") { "%02x".format(it) }

private const val SHA_256 = "SHA-256"
private const val BUFFER_BYTES = 64 * 1024
