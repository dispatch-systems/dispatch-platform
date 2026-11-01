package com.dspdispatch.relay.texts

import com.dspdispatch.relay.dispatch.Direction
import com.dspdispatch.relay.dispatch.Dispatch
import com.dspdispatch.relay.dispatch.Outbox
import com.dspdispatch.relay.dispatch.OutgoingFile
import com.dspdispatch.relay.dispatch.OutgoingText
import com.dspdispatch.relay.dispatch.SendStatus
import com.dspdispatch.relay.pairing.Paired
import java.nio.file.Files
import kotlin.time.Duration.Companion.seconds
import kotlin.time.measureTime
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.test.runTest
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import okhttp3.OkHttpClient
import okio.Buffer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class SenderTest {
    private val server = MockWebServer()
    private val database = memoryDatabase()
    private val sends = database.sends()
    private val radio = FakeRadio()
    private val dispatch = Dispatch(OkHttpClient())
    private val statuses = Statuses(dispatch, sends)
    private val folder = Files.createTempDirectory("sending").toFile()
    private val pictures = FakePictures { ByteArray(it) }
    private var clock = 1_793_041_515_000L
    private lateinit var paired: Paired

    private fun sender() = Sender(
        dispatch,
        sends,
        radio,
        PictureMessage(pictures),
        statuses,
        folder,
    ) { clock }

    @Before
    fun start() {
        server.start()
        paired = Paired(server.url("/").toString(), "Blue Heron", "phone_1", "Dispatch", 0, "+14255550100")
    }

    @After
    fun stop() {
        server.close()
        database.close()
        folder.deleteRecursively()
    }

    @Test
    fun sendsEachAttemptOnceEvenAcrossARestart() = runTest {
        val first = text("text_1", 41, "On my way")
        sender().send(paired, KEY, Outbox(listOf(first)))
        sender().send(paired, KEY, Outbox(listOf(first)))
        assertEquals(listOf("text_1" to 41L), radio.handed.map { it.first.id to it.first.attempt })
        assertEquals(listOf("+14255550100"), radio.lines)

        // Tried again from Dispatch: a new attempt, sent again.
        sender().send(paired, KEY, Outbox(listOf(first.copy(attempt = 42))))
        assertEquals(listOf(41L, 42L), radio.handed.map { it.first.attempt })
    }

    @Test
    fun sendsALongTextInTheCarriersParts() = runTest {
        radio.partLength = 4
        sender().send(paired, KEY, Outbox(listOf(text("text_1", 1, "Gate code 4412"))))
        assertEquals(listOf("Gate", " cod", "e 44", "12"), radio.handed.single().second)
        assertEquals(4, sends.start(Send("text_1", 1, "", "", 0)).parts)
    }

    @Test
    fun fetchesAPicturesBytesThenSendsItAsAPictureMessage() = runTest {
        val picture = byteArrayOf(9, 8, 7, 6)
        server.enqueue(MockResponse.Builder().code(200).body(Buffer().write(picture)).build())
        val text = text("text_1", 1, "Door", OutgoingFile("file_1", "door.jpg", "image/jpeg", 4))
        sender().send(paired, KEY, Outbox(listOf(text)))

        val request = server.takeRequest()
        assertEquals("GET", request.method)
        assertEquals("/api/communications/phone/files/file_1", request.url.encodedPath)
        assertEquals("Bearer $KEY", request.headers["Authorization"])
        val (send, pdu) = radio.pictureMessages.single()
        assertEquals("text_1", send.id)
        assertTrue("the picture, whole", pdu.contains(picture))
        assertTrue("the words", pdu.contains("Door".toByteArray()))
        assertTrue(pictures.asked.isEmpty())
        // Only the message waits for Android: the picture's bytes are gone.
        assertEquals(listOf("text_1-1.pdu"), folder.list()!!.toList())
        sender().heard(Heard("text_1", 1, 0, Event.SENT, null, "mms-7", clock))
        assertTrue(folder.list()!!.isEmpty())
    }

    @Test
    fun scalesPicturesToTheCarriersLargestMessage() = runTest {
        radio.mmsLimits = MmsLimits(4_000, Size(640, 480))
        server.enqueue(MockResponse.Builder().code(200).body(Buffer().write(ByteArray(10_000))).build())
        sender().send(paired, KEY, Outbox(listOf(text("text_1", 1, "Door", OutgoingFile("file_1", "door.png", "image/png", 10_000)))))
        val pdu = radio.pictureMessages.single().second
        assertTrue(pdu.size <= 4_000)
        assertTrue(pdu.contains("door.jpg".toByteArray()) && pdu.contains("image/jpeg".toByteArray()))
        assertEquals(Size(640, 480), pictures.asked.single().third)
    }

    @Test
    fun triesAPictureMessageAgainWhileItsFileCantBeFetchedSendingTheRest() = runTest {
        val sender = sender()
        val picture = text("text_1", 1, "", OutgoingFile("file_1", "door.jpg", "image/jpeg", 3))
        val outbox = Outbox(listOf(picture, text("text_2", 1, "On my way")))
        server.enqueue(json(503, """{"error":"uploads_busy"}"""))
        sender.send(paired, KEY, outbox)
        assertEquals(listOf("text_2"), radio.handed.map { it.first.id })
        assertTrue(radio.pictureMessages.isEmpty())
        assertTrue(sends.statuses(10).isEmpty())

        // Not fetched again until its pause is over.
        sender.send(paired, KEY, outbox)
        assertEquals(1, server.requestCount)
        clock += 60_000
        server.enqueue(MockResponse.Builder().code(200).body(Buffer().write(byteArrayOf(1, 2, 3))).build())
        sender.send(paired, KEY, outbox)
        assertEquals(listOf("text_1"), radio.pictureMessages.map { it.first.id })
        assertEquals("each sent once", 1, radio.handed.size)
    }

    @Test
    fun failsWhatCantGo() = runTest {
        server.enqueue(json(404, """{"error":"phones_file_not_found"}"""))
        sender().send(paired, KEY, Outbox(listOf(text("gone", 1, "", OutgoingFile("file_1", "door.jpg", "image/jpeg", 3)))))
        radio.mmsLimits = null
        sender().send(paired, KEY, Outbox(listOf(text("mms_off", 1, "", OutgoingFile("file_2", "door.jpg", "image/jpeg", 3)))))
        server.enqueue(MockResponse.Builder().code(200).body(Buffer().write(ByteArray(2_000))).build())
        radio.mmsLimits = MmsLimits(1_500, Size(640, 480))
        sender().send(paired, KEY, Outbox(listOf(text("too_large", 1, "", OutgoingFile("file_3", "notes.pdf", "application/pdf", 2_000)))))
        radio.refusing = IllegalArgumentException("Invalid destinationAddress")
        sender().send(paired, KEY, Outbox(listOf(text("refused", 1, "Hi"))))
        radio.allowed = false
        sender().send(paired, KEY, Outbox(listOf(text("not_allowed", 1, "Hi"))))

        assertEquals(
            mapOf(
                "gone" to Failure.FILE_GONE,
                "mms_off" to Failure.MMS_OFF,
                "too_large" to Failure.TOO_LARGE,
                "refused" to Failure.NOT_SENT,
                "not_allowed" to Failure.NOT_ALLOWED,
            ),
            sends.statuses(10).associate { it.id to it.error },
        )
        assertTrue(sends.statuses(10).all { it.status == SendStatus.FAILED })
        assertTrue(radio.handed.isEmpty() && radio.pictureMessages.isEmpty())
    }

    @Test
    fun failsOnlyTheTextAndroidFailedToPrepare() = runTest {
        // As Android 8's carrier settings, null while its picture message service is away.
        radio.failing = NullPointerException("carrierConfigValues")
        val picture = text("text_1", 1, "Door", OutgoingFile("file_1", "door.jpg", "image/jpeg", 3))
        sender().send(paired, KEY, Outbox(listOf(picture, text("text_2", 1, "On my way"))))
        assertEquals(listOf(Told("text_1", 1, SendStatus.FAILED, clock, Failure.NOT_SENT)), sends.statuses(10))
        assertEquals(listOf("text_2"), radio.handed.map { it.first.id })
    }

    @Test
    fun hearsATextDeliveredLongAfterDispatchHeardHowItWent() = runTest {
        val sender = sender()
        sender.send(paired, KEY, Outbox(listOf(text("sent", 1, "On my way"), text("failed", 1, "Gate code?"))))
        sender.heard(Heard("sent", 1, 0, Event.SENT, null, null, clock))
        sender.heard(Heard("failed", 1, 0, Event.SENT, "No service", null, clock))
        sends.told(sends.statuses(10))

        // The recipient's phone off for two days, past how long the inbox looks back.
        clock += 2 * DAY_MS
        sender.send(paired, KEY, Outbox(emptyList()))
        sender.heard(Heard("sent", 1, 0, Event.DELIVERED, null, null, clock, "3gpp"))
        sender.heard(Heard("failed", 1, 0, Event.DELIVERED, null, null, clock, "3gpp"))
        assertEquals(listOf("sent", "failed").map { Told(it, 1, SendStatus.DELIVERED, clock) }.toSet(), sends.statuses(10).toSet())
    }

    @Test
    fun settlesATextAndroidNeverSaidMoreOfByItsRowInSent() = runTest {
        sender().send(paired, KEY, Outbox(listOf(text("in_sent", 1, "On my way"), text("in_failed", 1, "Gate code?"))))
        // The inbox reads the app's row in Sent; one in Failed it never reads.
        val row = Row(Table.SMS, 7, clock + 1_000, Direction.OUT, APP)
        assertEquals(Whose.DISPATCH, SentByApp(sends, APP).whose(Message(row, listOf("(425) 555-0199"), "On my way", emptyList())))
        clock += RESULT_WAIT_MS + 1
        sender().send(paired, KEY, Outbox(emptyList()))
        assertEquals(
            setOf(Told("in_sent", 1, SendStatus.SENT, clock), Told("in_failed", 1, SendStatus.FAILED, clock, Failure.NO_RESULT)),
            sends.statuses(10).toSet(),
        )
    }

    @Test
    fun failsATextAndroidNeverSaidMoreOf() = runTest {
        sender().send(paired, KEY, Outbox(listOf(text("text_1", 1, "Hi"))))
        clock += RESULT_WAIT_MS - 1
        sender().send(paired, KEY, Outbox(emptyList()))
        assertTrue(sends.statuses(10).isEmpty())
        clock += 2
        sender().send(paired, KEY, Outbox(emptyList()))
        assertEquals(listOf(Told("text_1", 1, SendStatus.FAILED, clock, Failure.NO_RESULT)), sends.statuses(10))
    }

    @Test
    fun waitsAgainAtOnceForNothingElseOnceDispatchHearsHowATextWent() = runBlocking {
        val sender = sender()
        assertTrue(measureTime { sender.take(paired, Outbox(emptyList())) } < 1.seconds)

        sends.start(Send("text_1", 1, "+14255550199", "Hi", clock))
        sends.failed(Send("text_1", 1, "+14255550199", "Hi", clock), Failure.NOT_SENT, clock)
        server.enqueue(json(200, """{"ok":true}"""))
        val took = measureTime {
            launch { statuses.tell(paired.site, KEY) }
            sender.take(paired, Outbox(listOf(text("text_1", 1, "Hi"))))
        }
        assertTrue(took < 4.seconds)
    }

    private fun text(id: String, attempt: Long, body: String, vararg files: OutgoingFile) =
        OutgoingText(id, "+14255550199", body, attempt, files.toList())

    private fun ByteArray.contains(part: ByteArray) = (0..size - part.size).any { at -> part.indices.all { this[at + it] == part[it] } }

    private fun json(code: Int, body: String) =
        MockResponse.Builder().code(code).addHeader("Content-Type", "application/json").body(body).build()

    private companion object {
        val KEY = "k".repeat(64)
        const val DAY_MS = 24 * 60 * 60 * 1000L
        const val APP = "com.dspdispatch.relay"
    }
}
