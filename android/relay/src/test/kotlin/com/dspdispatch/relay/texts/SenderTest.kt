package com.dspdispatch.relay.texts

import com.dspdispatch.relay.dispatch.Direction
import com.dspdispatch.relay.dispatch.AppVersion
import com.dspdispatch.relay.dispatch.Dispatch
import com.dspdispatch.relay.dispatch.GifVersion
import com.dspdispatch.relay.dispatch.Outbox
import com.dspdispatch.relay.dispatch.OutgoingFile
import com.dspdispatch.relay.dispatch.OutgoingGif
import com.dspdispatch.relay.dispatch.OutgoingText
import com.dspdispatch.relay.dispatch.SendStatus
import com.dspdispatch.relay.pairing.Paired
import java.nio.file.Files
import kotlin.time.Duration.Companion.seconds
import kotlin.time.measureTime
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.test.runTest
import mockwebserver3.Dispatcher
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import mockwebserver3.RecordedRequest
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
    private val dispatch = Dispatch(OkHttpClient(), AppVersion("1.4.0", 14))
    private val statuses = Statuses(dispatch, sends) { _, _ -> }
    private val folder = Files.createTempDirectory("sending").toFile()
    private var shrink: (Int) -> ByteArray? = { ByteArray(it) }
    private val pictures = FakePictures { shrink(it) }
    private val provider = FakeProvider()
    private val own = OwnRows(provider, sends, APP)
    private var clock = 1_793_041_515_000L
    private var mmsWhenBusy = true
    private val unexpected = mutableListOf<Throwable>()
    private lateinit var paired: Paired

    private fun sender(sends: Sends = this.sends) = Sender(
        dispatch,
        sends,
        own,
        radio,
        PictureMessage(pictures),
        Gifs(OkHttpClient()) { it.host == server.hostName },
        statuses,
        folder,
        { mmsWhenBusy },
        { _, error -> unexpected += error },
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
    fun sendsAGifFromKlipyWholeAtTheLargestSizeTheCarrierTakes() = runTest {
        radio.mmsLimits = MmsLimits(300 * 1024, Size(640, 480))
        val gif = ByteArray(200_000) { 7 }
        server.enqueue(MockResponse.Builder().code(200).body(Buffer().write(gif)).build())
        sender().send(paired, KEY, Outbox(listOf(gifText(version("hd", 800, 600, 2_000_000), version("md", 498, 280, 900_000), version("sm", 220, 124, 200_000)))))

        // The largest within the carrier's message and picture size, from KLIPY, not Dispatch.
        assertEquals("/ii/sm.gif", server.takeRequest().url.encodedPath)
        val (send, pdu) = radio.pictureMessages.single()
        assertEquals("text_1", send.id)
        assertTrue("the GIF, whole", pdu.contains(gif))
        assertTrue(pdu.contains("image/gif".toByteArray()) && pdu.contains("Nice work".toByteArray()))
        assertTrue("never a JPEG of it", pictures.asked.isEmpty())
        assertEquals(listOf("text_1-1.pdu"), folder.list()!!.toList())
    }

    @Test
    fun failsAGifTooLargeForTheCarrierOrGoneFromKlipy() = runTest {
        radio.mmsLimits = MmsLimits(100_000, Size(640, 480))
        sender().send(paired, KEY, Outbox(listOf(gifText(version("md", 498, 280, 900_000)))))
        assertEquals(0, server.requestCount)
        server.enqueue(MockResponse.Builder().code(404).build())
        sender().send(paired, KEY, Outbox(listOf(gifText(version("xs", 90, 50, 60_000)).copy(id = "text_2"))))
        assertEquals(setOf(Failure.TOO_LARGE, Failure.FILE_GONE), sends.statuses(10).map { it.error }.toSet())
        assertTrue(radio.pictureMessages.isEmpty())
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
    fun failsAPictureMessageWhoseFileCantBeFetchedAfterAFewTries() = runTest {
        val sender = sender()
        val outbox = Outbox(listOf(text("text_1", 1, "", OutgoingFile("file_1", "door.jpg", "image/jpeg", 3))))
        repeat(10) {
            server.enqueue(json(503, """{"error":"platform_busy"}"""))
            sender.send(paired, KEY, outbox)
            clock += 60_000
        }
        assertEquals(listOf(Failure.FILE_UNAVAILABLE), sends.statuses(10).map { it.error })
        assertTrue(server.requestCount < 10)
        assertTrue(radio.pictureMessages.isEmpty())
    }

    @Test
    fun neverHandsAndroidATextPastItsTime() = runTest {
        val late = text("late", 1, "On my way").copy(expires = clock - 1)
        val due = text("due", 1, "On my way").copy(expires = clock)
        // Its picture takes it past its time as it's fetched.
        val slow = text("slow", 1, "Door", OutgoingFile("file_1", "door.jpg", "image/jpeg", 3)).copy(expires = clock + 1_000)
        server.dispatcher = object : Dispatcher() {
            override fun dispatch(request: RecordedRequest): MockResponse {
                clock += 2_000
                return MockResponse.Builder().code(200).body(Buffer().write(byteArrayOf(1, 2, 3))).build()
            }
        }
        sender().send(paired, KEY, Outbox(listOf(late, due, slow)))
        assertEquals(listOf("due"), radio.handed.map { it.first.id })
        assertTrue(radio.pictureMessages.isEmpty())
        assertEquals(mapOf("late" to Failure.EXPIRED, "slow" to Failure.EXPIRED), sends.statuses(10).associate { it.id to it.error })
        assertTrue(folder.list()!!.isEmpty())
    }

    @Test
    fun neverHandsAndroidATextThatPassedItsTimeAsItWasKeptHanded() = runTest {
        // The phone's database busy a while as the text is kept handed.
        val busy = object : Sends by sends {
            override suspend fun handed(send: Send, parts: Int, joined: Boolean, at: Long, mms: Boolean) {
                clock += 2_000
                sends.handed(send, parts, joined, at, mms)
            }
        }
        sender(busy).send(paired, KEY, Outbox(listOf(text("text_1", 1, "On my way").copy(expires = clock + 1_000))))
        assertTrue(radio.handed.isEmpty())
        assertEquals(listOf(Told("text_1", 1, SendStatus.FAILED, clock, Failure.EXPIRED)), sends.statuses(10))
    }

    @Test
    fun failsAloneATextThatFailsInAWayNothingExpected() = runTest {
        radio.mmsLimits = MmsLimits(4_000, Size(640, 480))
        val picture = text("text_1", 1, "Door", OutgoingFile("file_1", "door.png", "image/png", 10_000))
        val outbox = Outbox(listOf(picture, text("text_2", 1, "On my way")))
        server.enqueue(MockResponse.Builder().code(200).body(Buffer().write(ByteArray(10_000))).build())
        shrink = { throw IllegalStateException("Software rendering doesn't support hardware bitmaps") }
        sender().send(paired, KEY, outbox)
        server.enqueue(MockResponse.Builder().code(200).body(Buffer().write(ByteArray(10_000))).build())
        shrink = { throw OutOfMemoryError() }
        sender().send(paired, KEY, outbox.copy(texts = listOf(picture.copy(attempt = 2))))
        assertEquals(listOf(Failure.NOT_SENT, Failure.TOO_LARGE), sends.statuses(10).map { it.error })
        assertEquals(1, unexpected.size)
        assertEquals(listOf("text_2"), radio.handed.map { it.first.id })
        // Never tried again: Dispatch hears it failed.
        sender().send(paired, KEY, outbox)
        assertEquals(2, server.requestCount)
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
    fun waitsOnEachTextAndroidTookUntilItSaysHowItWentOrItsWaitIsOver() = runTest {
        val sender = sender()
        sender.send(paired, KEY, Outbox(listOf(text("text_1", 1, "On my way"))))
        clock += 60_000
        sender.send(paired, KEY, Outbox(listOf(text("text_2", 1, "Gate code?"))))
        assertEquals(RESULT_WAIT_MS, sender.unanswered())
        sender.heard(Heard("text_2", 1, 0, Event.SENT, null, null, clock))
        assertEquals(RESULT_WAIT_MS - 60_000, sender.unanswered())
        clock += RESULT_WAIT_MS - 60_000
        assertEquals(null, sender.unanswered())
        sender.hold()
        sender.release()
    }

    @Test
    fun failsATextAndroidNeverSaidMoreOfThoughARowWithItsWordsIsInSent() = runTest {
        sender().send(paired, KEY, Outbox(listOf(text("text_1", 1, "On my way"))))
        // Android's word of it lost: the app's row with its words in Sent isn't Android's word.
        provider.messages += sms(7, "On my way")
        clock += RESULT_WAIT_MS - 1
        sender().send(paired, KEY, Outbox(emptyList()))
        assertTrue(sends.statuses(10).isEmpty())
        clock += 2
        sender().send(paired, KEY, Outbox(emptyList()))
        assertEquals(listOf(Told("text_1", 1, SendStatus.FAILED, clock, Failure.NO_RESULT)), sends.statuses(10))
    }

    @Test
    fun settlesATextAndroidSaysItSentJoinedByTheAppsRowItNamedInSent() = runTest {
        radio.partLength = 7
        val sender = sender()
        val texts = listOf(text("sent", 1, "Café door 4412"), text("failed", 1, "Gate code 4412"), text("unowned", 1, "Dock door 7712"))
        sender.send(paired, KEY, Outbox(texts))
        // Each first part's word lost; each last said it went joined, naming the row Android wrote the
        // whole text to: the app's in Sent, its words as a phone forcing 7-bit texts sends them; in
        // Failed, as Android says the last part went though one before it failed; in Sent, no app's.
        val rows = listOf(sms(7, "Cafe door 4412"), sms(8, "Gate code 4412"), sms(9, "Dock door 7712", creator = null))
        provider.messages += rows
        provider.failed += rows[1].row
        for ((text, row) in texts.zip(rows)) {
            val heard = Heard(text.id, 1, 1, Event.SENT, null, null, clock, joined = true)
            sender.heard(heard.copy(row = own.row(heard, "content://sms/sent/${row.row.id}")))
        }
        clock += RESULT_WAIT_MS + 1
        sender.send(paired, KEY, Outbox(emptyList()))
        assertEquals(
            setOf(Told("sent", 1, SendStatus.SENT, clock)) +
                listOf("failed", "unowned").map { Told(it, 1, SendStatus.FAILED, clock, Failure.NO_RESULT) },
            sends.statuses(10).toSet(),
        )
    }

    @Test
    fun settlesLikeTextsEachByItsOwnWordNeverByAnothersRow() = runTest {
        radio.partLength = 7
        val sender = sender()
        val ids = listOf("apart_failed", "apart_sent", "joined_failed", "joined_sent")
        sender.send(paired, KEY, Outbox(ids.map { text(it, 1, "Gate code 4412") }))
        // Two Android sent in parts apart, each to rows of its own: each first part's word heard,
        // naming its row; each last's lost, one's row never written, as where Android failed to, the
        // other's in Sent.
        val apart = listOf(sms(7, "Gate co"), sms(9, "Gate co"), sms(10, "de 4412"))
        // Two it sent joined: each first part's word lost; each last said it went, naming its own row,
        // one in Failed, the other in Sent.
        val joined = listOf(sms(11, "Gate code 4412"), sms(12, "Gate code 4412"))
        provider.messages += apart + joined
        provider.failed += joined[0].row
        for ((id, row) in ids.zip(apart.take(2) + joined)) {
            val last = id.startsWith("joined")
            val heard = Heard(id, 1, if (last) 1 else 0, Event.SENT, null, null, clock, joined = last)
            sender.heard(heard.copy(row = own.row(heard, "content://sms/sent/${row.row.id}")))
        }
        clock += RESULT_WAIT_MS + 1
        sender.send(paired, KEY, Outbox(emptyList()))
        assertEquals(
            setOf(Told("joined_sent", 1, SendStatus.SENT, clock)) +
                ids.dropLast(1).map { Told(it, 1, SendStatus.FAILED, clock, Failure.NO_RESULT) },
            sends.statuses(10).toSet(),
        )
    }

    @Test
    fun failsATextAtTheTimeoutWhileTheProviderCantSay() = runTest {
        radio.partLength = 7
        val sender = sender()
        sender.send(paired, KEY, Outbox(listOf(text("text_1", 1, "Gate code 4412"))))
        provider.messages += sms(7, "Gate code 4412")
        val heard = Heard("text_1", 1, 1, Event.SENT, null, null, clock, joined = true)
        sender.heard(heard.copy(row = own.row(heard, "content://sms/sent/7")))
        provider.answering = false
        clock += RESULT_WAIT_MS + 1
        sender.send(paired, KEY, Outbox(emptyList()))
        assertEquals(listOf(Told("text_1", 1, SendStatus.FAILED, clock, Failure.NO_RESULT)), sends.statuses(10))
    }

    @Test
    fun sendsATextAsAPictureMessageOnceItWouldComeNearAndroidsSmsLimit() = runTest {
        // Room for 8 parts, kept 2 short of Android's 10.
        radio.smsLimit = SmsLimit(10, WINDOW_MS)
        radio.partLength = 4
        sender().send(paired, KEY, Outbox((1..3).map { text("sms_$it", 1, "Gate 441") }))
        // A long text's parts all count: its third would cross, where a text of two still fits.
        sender().send(paired, KEY, Outbox(listOf(text("long", 1, "Gate code 4"))))
        sender().send(paired, KEY, Outbox(listOf(text("short", 1, "Gate 441"))))
        sender().send(paired, KEY, Outbox(listOf(text("busy", 1, "Hi"))))

        assertEquals(listOf("sms_1", "sms_2", "sms_3", "short"), radio.handed.map { it.first.id })
        assertEquals(listOf("long", "busy"), radio.pictureMessages.map { it.first.id })
        assertTrue("the words", radio.pictureMessages.first().second.contains("Gate code 4".toByteArray()))
        assertEquals(8, sends.smsParts(0))
        val busy = checkNotNull(sends.find("busy", 1))
        assertTrue(busy.mms)
        assertEquals(1, busy.parts)
        // Settled as any picture message: sent once Android says it went.
        sender().heard(Heard("busy", 1, 0, Event.SENT, null, null, clock))
        assertEquals(listOf(Told("busy", 1, SendStatus.SENT, clock)), sends.statuses(10))
    }

    @Test
    fun sendsAsSmsAgainOnceThePartsLeaveAndroidsWindow() = runTest {
        radio.smsLimit = SmsLimit(10, WINDOW_MS)
        sender().send(paired, KEY, Outbox(listOf(text("long", 1, "x".repeat(8 * 160)))))
        // Android stamps a part a moment after the app counts it: a thirtieth of the window more, it
        // may count it still.
        clock += WINDOW_MS + 1_999
        sender().send(paired, KEY, Outbox(listOf(text("busy", 1, "Hi"))))
        clock += 2
        sender().send(paired, KEY, Outbox(listOf(text("free", 1, "Hi"))))
        assertEquals(listOf("long", "free"), radio.handed.map { it.first.id })
        assertEquals(listOf("busy"), radio.pictureMessages.map { it.first.id })
    }

    @Test
    fun sendsAsSmsPastAndroidsLimitWhileTheSwitchIsOff() = runTest {
        radio.smsLimit = SmsLimit(10, WINDOW_MS)
        mmsWhenBusy = false
        sender().send(paired, KEY, Outbox(listOf(text("long", 1, "x".repeat(8 * 160)), text("past", 1, "Hi"))))
        assertEquals(listOf("long", "past"), radio.handed.map { it.first.id })
        assertTrue(radio.pictureMessages.isEmpty())

        mmsWhenBusy = true
        sender().send(paired, KEY, Outbox(listOf(text("busy", 1, "Hi"))))
        assertEquals(listOf("busy"), radio.pictureMessages.map { it.first.id })
    }

    @Test
    fun failsATextPastAndroidsLimitWhereNoPictureMessageCanGo() = runTest {
        // Room for one part.
        radio.smsLimit = SmsLimit(3, WINDOW_MS)
        radio.mmsLimits = null
        sender().send(paired, KEY, Outbox(listOf(text("first", 1, "Hi"), text("mms_off", 1, "Hi"))))
        // As Android 8's carrier settings, null while its picture message service is away.
        radio.failing = NullPointerException("carrierConfigValues")
        sender().send(paired, KEY, Outbox(listOf(text("unprepared", 1, "Hi"))))

        assertEquals(listOf("first"), radio.handed.map { it.first.id })
        assertTrue(radio.pictureMessages.isEmpty())
        assertEquals(
            listOf(Told("mms_off", 1, SendStatus.FAILED, clock, Failure.MMS_OFF), Told("unprepared", 1, SendStatus.FAILED, clock, Failure.NOT_SENT)),
            sends.statuses(10),
        )
        assertEquals(1, sends.smsParts(0))
    }

    @Test
    fun neverReportsATextSentAsAPictureMessageNearTheLimitAsThePhones() = runTest {
        // Room for one part.
        radio.smsLimit = SmsLimit(3, WINDOW_MS)
        val texts = listOf(text("sms", 1, "Hi"), text("busy", 1, "On my way"), text("unnamed", 1, "Gate code?"))
        sender().send(paired, KEY, Outbox(texts))
        assertEquals(listOf("busy", "unnamed"), radio.pictureMessages.map { it.first.id })

        // Android wrote each to the picture messages' table, naming the first's row in its result.
        val named = mms(3, "On my way", APP)
        val unnamed = mms(4, "Gate code?")
        provider.messages += listOf(named, unnamed)
        val heard = Heard("busy", 1, 0, Event.SENT, null, null, clock)
        sender().heard(heard.copy(row = own.row(heard, "content://mms/3")))
        assertTrue(sends.wrote(named.row.key))
        val whose = SentByApp(sends, APP)
        assertEquals(Whose.DISPATCH, whose.whose(named))
        assertEquals(Whose.DISPATCH, whose.whose(unnamed))
        assertEquals("the same words typed on the phone", Whose.PHONE, whose.whose(mms(5, "On my way", TEXTING_APP)))
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

    private fun gifText(vararg versions: GifVersion) =
        OutgoingText("text_1", "+14255550199", "Nice work", 1, gif = OutgoingGif("wave.gif", versions.toList()))

    private fun version(size: String, width: Int, height: Int, bytes: Long) =
        GifVersion(server.url("/ii/$size.gif").toString(), width, height, bytes)

    /** The app's row of a text it sent, as Android writes it, just after it was handed. */
    private fun sms(id: Long, body: String, creator: String? = APP) =
        Message(Row(Table.SMS, id, clock + 1_000, Direction.OUT, creator), listOf("(425) 555-0199"), body, emptyList())

    /** A picture message's row of a text sent, as Android writes it, its time in whole seconds. */
    private fun mms(id: Long, body: String, creator: String? = null) =
        Message(Row(Table.MMS, id, (clock + 1_000) / 1_000 * 1_000, Direction.OUT, creator), listOf("+14255550199"), body, emptyList())

    private fun ByteArray.contains(part: ByteArray) = (0..size - part.size).any { at -> part.indices.all { this[at + it] == part[it] } }

    private fun json(code: Int, body: String) =
        MockResponse.Builder().code(code).addHeader("Content-Type", "application/json").body(body).build()

    private companion object {
        val KEY = "k".repeat(64)
        const val DAY_MS = 24 * 60 * 60 * 1000L
        const val WINDOW_MS = 60_000L
        const val APP = "com.dspdispatch.relay"
        const val TEXTING_APP = "com.example.texting"
    }
}
