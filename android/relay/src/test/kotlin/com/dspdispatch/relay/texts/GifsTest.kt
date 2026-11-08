package com.dspdispatch.relay.texts

import com.dspdispatch.relay.dispatch.GifVersion
import java.nio.file.Files
import kotlinx.coroutines.test.runTest
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.OkHttpClient
import okio.Buffer
import org.junit.After
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class GifsTest {
    private val server = MockWebServer()
    private val folder = Files.createTempDirectory("gifs").toFile()
    private val gifs = Gifs(OkHttpClient()) { it.host == server.hostName }

    @Before
    fun start() = server.start()

    @After
    fun stop() {
        server.close()
        folder.deleteRecursively()
    }

    @Test
    fun picksTheLargestSizeTheMessageHasRoomFor() {
        val hd = GifVersion("https://static.klipy.com/hd.gif", 498, 280, 2_600_000)
        val md = GifVersion("https://static.klipy.com/md.gif", 498, 280, 900_000)
        val sm = GifVersion("https://static.klipy.com/sm.gif", 220, 124, 210_000)
        val xs = GifVersion("https://static.klipy.com/xs.gif", 90, 50, 60_000)
        val all = listOf(xs, hd, sm, md)
        assertEquals(md, pick(all, 1_000_000, Size(640, 480)))
        assertEquals(sm, pick(all, 300_000, Size(640, 480)))
        // Within the carrier's picture size, either way up.
        assertEquals(sm, pick(all, 1_000_000, Size(240, 400)))
        assertNull(pick(all, 50_000, Size(640, 480)))
    }

    @Test
    fun fetchesOnlyFromKlipyOverHttps() {
        assertTrue(klipy("https://static.klipy.com/ii/a/b.gif".toHttpUrl()))
        assertTrue(klipy("https://static2.klipy.com/ii/a/b.gif".toHttpUrl()))
        assertFalse(klipy("http://static.klipy.com/ii/a/b.gif".toHttpUrl()))
        assertFalse(klipy("https://static.klipy.com.example.com/b.gif".toHttpUrl()))
        assertFalse(klipy("https://example.com/b.gif".toHttpUrl()))
        assertThrows(IllegalArgumentException::class.java) {
            runTest { Gifs(OkHttpClient()).download("https://example.com/b.gif", folder.resolve("b.gif"), 10) }
        }
        assertEquals(0, server.requestCount)
    }

    @Test
    fun writesTheGifWhileItFits() = runTest {
        val bytes = ByteArray(20_000) { it.toByte() }
        server.enqueue(MockResponse.Builder().code(200).body(Buffer().write(bytes)).build())
        val into = folder.resolve("wave.gif")
        assertTrue(gifs.download(server.url("/ii/wave.gif").toString(), into, 20_000))
        assertArrayEquals(bytes, into.readBytes())
        assertEquals("/ii/wave.gif", server.takeRequest().url.encodedPath)

        // Larger than KLIPY said: what's past the room isn't kept.
        server.enqueue(MockResponse.Builder().code(200).body(Buffer().write(bytes)).build())
        assertFalse(gifs.download(server.url("/ii/wave.gif").toString(), into, 10_000))
    }

    @Test
    fun tellsAGifKlipyNoLongerHasFromOneItCouldntSend() {
        server.enqueue(MockResponse.Builder().code(404).build())
        assertThrows(Gone::class.java) { runTest { gifs.download(server.url("/gone.gif").toString(), folder.resolve("g.gif"), 10) } }
        server.enqueue(MockResponse.Builder().code(503).build())
        assertThrows(java.io.IOException::class.java) { runTest { gifs.download(server.url("/busy.gif").toString(), folder.resolve("g.gif"), 10) } }
    }
}
