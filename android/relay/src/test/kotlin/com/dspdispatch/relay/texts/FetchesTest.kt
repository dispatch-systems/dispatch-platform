package com.dspdispatch.relay.texts

import com.dspdispatch.relay.dispatch.AppVersion
import com.dspdispatch.relay.dispatch.Direction
import com.dspdispatch.relay.dispatch.Dispatch
import kotlin.coroutines.EmptyCoroutineContext
import kotlinx.coroutines.test.runTest
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import okhttp3.OkHttpClient
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Before
import org.junit.Test

class FetchesTest {
    private val server = MockWebServer()
    private val queue = MemoryQueue()
    private val provider = FakeProvider()
    private val fetches = Fetches(
        Dispatch(OkHttpClient(), AppVersion("1.4.0", 14)),
        queue,
        provider,
        Texting { provider.allowed },
        EmptyCoroutineContext,
    )
    private lateinit var site: String

    @Before
    fun start() {
        server.start()
        site = server.url("/").toString()
    }

    @After
    fun stop() {
        server.close()
    }

    @Test
    fun sendsAVideoAskedForAndSaysOfOneTheProviderHoldsNoMore() = runTest {
        queue.keptOnPhone(1, OnPhone("file_1", "content://mms/part/1"))
        queue.keptOnPhone(2, OnPhone("file_2", "content://mms/part/2"))
        provider.bytes["content://mms/part/1"] = byteArrayOf(1, 2, 3)
        repeat(3) { server.enqueue(ok()) }

        fetches.send(site, KEY, "file_1")
        val sent = server.takeRequest()
        assertEquals("/api/communications/phone/files/file_1", sent.url.encodedPath)
        assertEquals(listOf<Byte>(1, 2, 3), sent.body?.toByteArray()?.toList())
        assertNotNull("kept, to send whenever it's asked for", queue.onPhone("file_1"))

        // The texting app deleted it.
        fetches.send(site, KEY, "file_2")
        assertEquals("/api/communications/phone/files/file_2/gone", server.takeRequest().url.encodedPath)
        assertNull(queue.onPhone("file_2"))

        // Never kept here, as one of the phone's before it paired again.
        fetches.send(site, KEY, "file_3")
        assertEquals("/api/communications/phone/files/file_3/gone", server.takeRequest().url.encodedPath)
    }

    @Test
    fun sendsAVideoAskedForBeforeItsFrameWentAndNeverCallsOneGoneWhileReportsWait() = runTest {
        queue.read(
            Seen("mms-1-1", 1, Direction.IN),
            listOf(Waiting("mms-1-1", Direction.IN, "(206) 555-0101", "Defect", 1)),
            listOf(WaitingFile("mms-1-1", 0, "content://mms/part/1", "defect.mp4", "video/mp4", 3, onPhone = true)),
        )
        provider.bytes["content://mms/part/1"] = byteArrayOf(1, 2, 3)
        // Dispatch kept the report, its answer lost: the phone doesn't know the video's ID yet.
        fetches.send(site, KEY, "file_1")
        assertEquals(0, server.requestCount)

        queue.answered(mapOf("mms-1-1" to listOf("file_1")))
        server.enqueue(ok())
        fetches.send(site, KEY, "file_1")
        assertEquals("/api/communications/phone/files/file_1", server.takeRequest().url.encodedPath)
    }

    @Test
    fun sendsAVideoWhoseReportIsAnsweredAsItsLookedFor() = runTest {
        queue.read(
            Seen("mms-1-1", 1, Direction.IN),
            listOf(Waiting("mms-1-1", Direction.IN, "(206) 555-0101", "Defect", 1)),
            listOf(WaitingFile("mms-1-1", 0, "content://mms/part/1", "defect.mp4", "video/mp4", 3, onPhone = true)),
        )
        provider.bytes["content://mms/part/1"] = byteArrayOf(1, 2, 3)
        // Dispatch's answer comes between looking for the video and looking for reports waiting.
        val answering = object : Queue by queue {
            override suspend fun waiting(most: Int): List<WaitingText> {
                queue.answered(mapOf("mms-1-1" to listOf("file_1")))
                return queue.waiting(most)
            }
        }
        val fetches = Fetches(Dispatch(OkHttpClient(), AppVersion("1.4.0", 14)), answering, provider, Texting { true }, EmptyCoroutineContext)
        server.enqueue(ok())
        fetches.send(site, KEY, "file_1")
        assertEquals("/api/communications/phone/files/file_1", server.takeRequest().url.encodedPath)
    }

    @Test
    fun leavesAVideoItCantSendNowForWhoeverPlaysItToAskAgain() = runTest {
        queue.keptOnPhone(1, OnPhone("file_1", "content://mms/part/1"))
        provider.bytes["content://mms/part/1"] = byteArrayOf(1, 2, 3)
        provider.away += "content://mms/part/1"
        fetches.send(site, KEY, "file_1")
        assertEquals(0, server.requestCount)
        assertNotNull(queue.onPhone("file_1"))
    }

    private fun ok() = MockResponse.Builder().code(200).addHeader("Content-Type", "application/json").body("""{"ok":true}""").build()

    private companion object {
        val KEY = "k".repeat(64)
    }
}
