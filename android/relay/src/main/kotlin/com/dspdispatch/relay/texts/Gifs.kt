package com.dspdispatch.relay.texts

import com.dspdispatch.relay.dispatch.GifVersion
import com.dspdispatch.relay.dispatch.await
import java.io.File
import java.io.IOException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.CookieJar
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.OkHttpClient
import okhttp3.Request
import okio.buffer
import okio.sink

/**
 * KLIPY's GIFs, fetched as a text with one is sent, from where KLIPY delivers them ([klipy]):
 * never from anywhere else, a redirect elsewhere included, which isn't followed.
 */
class Gifs(http: OkHttpClient) {
    private val http = http.newBuilder()
        .cookieJar(CookieJar.NO_COOKIES)
        .followRedirects(false)
        .followSslRedirects(false)
        .build()

    /**
     * Writes the GIF at [url] into [to], streamed as it comes, answering whether it was at most
     * [most] bytes: what's past that isn't read. Throws [Gone] when KLIPY no longer has it, and an
     * [IOException] when it couldn't be fetched.
     */
    suspend fun download(url: String, to: File, most: Long): Boolean {
        val address = url.toHttpUrl()
        require(klipy(address)) { "Not where KLIPY delivers its GIFs: ${address.host}" }
        return withContext(Dispatchers.IO) {
            http.newCall(Request.Builder().url(address).build()).await().use { response ->
                if (response.code == NOT_FOUND || response.code == GONE) throw Gone()
                if (!response.isSuccessful) throw IOException("KLIPY answered ${response.code}")
                val source = response.body.source()
                to.sink().buffer().use { sink ->
                    var read = 0L
                    while (read <= most) {
                        val more = source.read(sink.buffer, SEGMENT)
                        if (more == -1L) return@use true
                        read += more
                        sink.emitCompleteSegments()
                    }
                    false
                }
            }
        }
    }

    private companion object {
        const val NOT_FOUND = 404
        const val GONE = 410
        const val SEGMENT = 8192L
    }
}

/** KLIPY no longer has the GIF. */
class Gone : Exception()

/** Whether [url] is one KLIPY delivers its GIFs from, over HTTPS. */
internal fun klipy(url: HttpUrl) = url.isHttps && url.host in KLIPY_HOSTS

/**
 * The largest of [versions] a picture message has room for: at most [room] bytes, and within the
 * carrier's [picture] size either way up. Null when none is.
 */
internal fun pick(versions: List<GifVersion>, room: Long, picture: Size): GifVersion? =
    versions.filter { it.size <= room && Size(it.width, it.height).fits(picture) }.maxByOrNull(GifVersion::size)

private val KLIPY_HOSTS = setOf("static.klipy.com", "static1.klipy.com", "static2.klipy.com")
