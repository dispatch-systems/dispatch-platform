package com.dspdispatch.relay.connection

import com.dspdispatch.relay.dispatch.Dispatch
import com.dspdispatch.relay.dispatch.Outbox
import com.dspdispatch.relay.pairing.Paired
import kotlinx.coroutines.test.runTest
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import okhttp3.OkHttpClient
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test

class ConnectionTest {
    private val server = MockWebServer()
    private var clock = 1_000L
    private val connection = Connection(Dispatch(OkHttpClient())) { clock }
    private lateinit var paired: Paired

    @Before
    fun start() {
        server.start()
        paired = Paired(server.url("/").toString(), "Blue Heron", "phone_1", "Dispatch", 0)
    }

    @After
    fun stop() = server.close()

    @Test
    fun waitsAgainAsEachWaitEndsUntilThePhoneIsRemoved() = runTest {
        server.enqueue(json(200, """{"texts":[]}"""))
        server.enqueue(json(200, """{"texts":[]}"""))
        server.enqueue(json(401, """{"error":"phones_removed"}"""))
        val received = mutableListOf<Outbox>()
        val links = mutableListOf<Link>()
        connection.run(paired, "key") {
            received += it
            links += connection.link.value
        }
        assertEquals(2, received.size)
        assertEquals(listOf<Link>(Link.Connected(1_000), Link.Connected(1_000)), links)
        assertEquals(3, server.requestCount)
    }

    @Test
    fun reconnectsFromWhenCallsFirstWentUnanswered() = runTest {
        server.enqueue(json(503, """{"error":"platform_busy"}"""))
        server.enqueue(json(503, """{"error":"platform_busy"}"""))
        server.enqueue(json(200, """{"texts":[]}"""))
        server.enqueue(json(401, """{"error":"phones_removed"}"""))
        val links = mutableListOf<Link>()
        connection.run(paired, "key") { links += connection.link.value }
        assertEquals(listOf<Link>(Link.Connected(1_000)), links)
        assertEquals(4, server.requestCount)
    }

    @Test
    fun keepsItsKeyWhileItsDspIsSuspended() = runTest {
        server.enqueue(json(401, """{"error":"sign_in_required"}"""))
        server.enqueue(json(401, """{"error":"sign_in_required"}"""))
        server.enqueue(json(200, """{"texts":[]}"""))
        server.enqueue(json(401, """{"error":"phones_removed"}"""))
        val links = mutableListOf<Link>()
        connection.run(paired, "key") { links += connection.link.value }
        assertEquals(listOf<Link>(Link.Connected(1_000)), links)
        assertEquals(4, server.requestCount)
    }

    @Test
    fun saysWhyWhileDispatchKeepsRefusing() = runTest {
        server.enqueue(json(403, """{"error":"phones_off"}"""))
        server.enqueue(json(401, """{"error":"phones_removed"}"""))
        clock = 5_000L
        connection.run(paired, "key") {}
        assertEquals(Link.Reconnecting(5_000, "phones_off"), connection.link.value)
    }

    private fun json(code: Int, body: String) =
        MockResponse.Builder().code(code).addHeader("Content-Type", "application/json").body(body).build()
}
