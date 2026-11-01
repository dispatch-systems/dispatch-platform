package com.dspdispatch.relay.texts

import com.dspdispatch.relay.dispatch.Direction
import com.dspdispatch.relay.dispatch.SendStatus
import java.io.File
import java.nio.file.Files
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** The sends' queries, on SQLite itself. */
class StoredSendsTest {
    private val database = memoryDatabase()
    private val sends = database.sends()

    @After
    fun close() = database.close()

    @Test
    fun settlesASendOnceEveryPartIsHeardOf() = runTest {
        val send = handed("text_1", parts = 2)
        heard(send, 0, Event.SENT)
        heard(send, 0, Event.SENT)
        assertTrue("one part of two", sends.statuses(10).isEmpty())
        heard(send, 1, Event.DELIVERED)
        heard(send, 1, Event.SENT)
        assertEquals(listOf(SendStatus.SENT), sends.statuses(10).map(Told::status))
        heard(send, 0, Event.DELIVERED)
        assertEquals(listOf(SendStatus.SENT, SendStatus.DELIVERED), sends.statuses(10).map(Told::status))
        heard(send, 0, Event.DELIVERED)
        assertEquals(2, sends.statuses(10).size)
    }

    @Test
    fun deliversAJoinedTextOnCdmaByItsLastPartsReport() = runTest {
        // Android asks a CDMA carrier to report on a joined text's last part only, and says it sent
        // the text joined with that part's result.
        val joined = handed("joined", parts = 2)
        heard(joined, 0, Event.SENT)
        heard(joined, 1, Event.SENT, joined = true)
        heard(joined, 1, Event.DELIVERED, format = "3gpp2")
        // Handed joined, but sent by Android each as a text of its own, as where the phone can't
        // join them: none says it went joined, whether their results name a row each, or one, as
        // when Android couldn't write the other.
        val split = handed("split", parts = 2)
        heard(split, 0, Event.SENT)
        heard(split, 1, Event.SENT)
        heard(split, 1, Event.DELIVERED, format = "3gpp2")
        // Each part its own text: each is reported, as on GSM.
        val apart = handed("apart", parts = 2, joined = false)
        heard(apart, 0, Event.SENT)
        heard(apart, 1, Event.SENT)
        heard(apart, 1, Event.DELIVERED, format = "3gpp2")
        val gsm = handed("gsm", parts = 2)
        heard(gsm, 0, Event.SENT)
        heard(gsm, 1, Event.SENT, joined = true)
        heard(gsm, 1, Event.DELIVERED, format = "3gpp")
        // Android says the last part went joined even when one before it failed: its report stands
        // for the text only once every part's result says it went.
        val unheard = handed("unheard", parts = 2)
        heard(unheard, 1, Event.SENT, joined = true)
        heard(unheard, 1, Event.DELIVERED, format = "3gpp2")
        val failed = handed("failed", parts = 2)
        heard(failed, 0, Event.SENT, "No service")
        heard(failed, 1, Event.SENT, joined = true)
        heard(failed, 1, Event.DELIVERED, format = "3gpp2")
        assertEquals(listOf("joined"), delivered())
        heard(split, 0, Event.DELIVERED, format = "3gpp2")
        assertEquals(listOf("joined", "split"), delivered())
    }

    private suspend fun delivered() = sends.statuses(10).filter { it.status == SendStatus.DELIVERED }.map(Told::id)

    @Test
    fun failsASendAsSoonAsAPartFails() = runTest {
        val send = handed("text_1", parts = 3)
        heard(send, 1, Event.SENT, "No service")
        heard(send, 0, Event.SENT)
        heard(send, 2, Event.SENT)
        assertEquals(listOf(Told("text_1", 1, SendStatus.FAILED, AT, "No service")), sends.statuses(10))
    }

    @Test
    fun keepsStatusesUntilDispatchHearsThem() = runTest {
        val first = handed("text_1")
        val second = handed("text_2")
        heard(second, 0, Event.SENT)
        heard(first, 0, Event.SENT, at = AT - 1)
        val waiting = sends.statuses(10)
        assertEquals(listOf("text_1", "text_2"), waiting.map(Told::id))
        sends.told(waiting.take(1))
        assertEquals(listOf("text_2"), sends.statuses(10).map(Told::id))
    }

    @Test
    fun forgetsASendOnlyOnceDispatchHeardItsEnd() = runTest {
        val told = handed("told")
        heard(told, 0, Event.SENT, row = "sms-1")
        sends.told(sends.statuses(10))
        val untold = handed("untold")
        heard(untold, 0, Event.SENT, row = "sms-2")
        handed("unsettled")
        sends.start(Send("fetching", 1, "+14255550199", "", AT))
        sends.forget(AT + 1, AT + 1)
        assertFalse(sends.wrote("sms-1"))
        assertTrue(sends.wrote("sms-2"))
        assertEquals(listOf("unsettled"), sends.unplaced(0, Long.MAX_VALUE).map(Send::id))
        // Never handed, it may start again, as Dispatch has it still.
        assertEquals(null, sends.start(Send("fetching", 1, "+14255550199", "", AT + 2)).handed)
        assertEquals(AT + 2, sends.start(Send("fetching", 1, "+14255550199", "", AT + 3)).started)
    }

    @Test
    fun keepsATextUntilItsDeliveryCantBeReportedAnyMore() = runTest {
        val sms = handed("sms")
        heard(sms, 0, Event.SENT)
        val picture = sends.start(Send("picture", 1, "+14255550199", "Door", AT, mms = true))
        sends.handed(picture, 1, false, AT)
        heard(picture, 0, Event.SENT)
        val delivered = handed("delivered")
        heard(delivered, 0, Event.SENT)
        heard(delivered, 0, Event.DELIVERED)
        sends.told(sends.statuses(10))

        sends.forget(AT + 1, AT)
        assertEquals("only the text still to be delivered", listOf("sms"), listOf("sms", "picture", "delivered").filter { kept(it) })
        sends.forget(AT + 1, AT + 1)
        assertFalse(kept("sms"))
    }

    @Test
    fun countsTheSmsPartsHandedAcrossANewPairing() = runTest {
        handed("long", parts = 3)
        handed("short")
        val picture = sends.start(Send("picture", 1, "+14255550199", "Door", AT, mms = true))
        sends.handed(picture, 1, false, AT)
        // A text without files that went as a picture message isn't counted either.
        sends.handed(sends.start(Send("busy", 1, "+14255550199", "Hi", AT)), 1, false, AT, mms = true)
        assertEquals(4, sends.smsParts(AT))
        assertEquals(0, sends.smsParts(AT + 1))
        assertTrue(checkNotNull(sends.find("busy", 1)).mms)

        // Android's count stands as the phone pairs again.
        database.queue().begin(Mark("phone_2", AT, AT))
        assertEquals(4, sends.smsParts(AT))
        sends.forget(AT + 1, AT + 1)
        assertEquals(0, sends.smsParts(0))
    }

    @Test
    fun keepsTheSmsPartsCountAcrossARestart() = runTest {
        val folder = Files.createTempDirectory("texts").toFile()
        try {
            val file = File(folder, TextsDatabase.NAME)
            fileDatabase(file).run {
                sends().run { handed(start(Send("long", 1, "+14255550199", "Hi", AT)), 3, true, AT) }
                close()
            }
            fileDatabase(file).run {
                assertEquals(3, sends().smsParts(AT))
                close()
            }
        } finally {
            folder.deleteRecursively()
        }
    }

    /** Whether the attempt at [id] is still kept: one forgotten starts again, never handed. */
    private suspend fun kept(id: String) = sends.start(Send(id, 1, "+14255550199", "Hi", AT)).handed != null

    @Test
    fun countsTodaysTextsSentForDispatch() = runTest {
        val queue = database.queue()
        queue.begin(Mark("phone_1", 0, 0))
        queue.read(Seen("sms-1-50", 50, Direction.OUT), emptyList(), emptyList())
        queue.read(Seen("sms-2-60", 60, null), emptyList(), emptyList())
        heard(handed("text_1"), 0, Event.SENT, at = 70)
        heard(handed("text_2"), 0, Event.SENT, "No service", at = 80)
        assertEquals(Today(sent = 2, latest = 70), queue.today(10).first())
        assertEquals(Today(sent = 1, latest = 70), queue.today(55).first())
    }

    private suspend fun handed(id: String, parts: Int = 1, joined: Boolean = parts > 1): Send {
        val send = sends.start(Send(id, 1, "+14255550199", "Hi", AT))
        sends.handed(send, parts, joined, AT)
        return send
    }

    private suspend fun heard(
        send: Send,
        part: Int,
        event: Event,
        error: String? = null,
        row: String? = null,
        at: Long = AT,
        format: String? = null,
        joined: Boolean = false,
    ) = sends.heard(Heard(send.id, send.attempt, part, event, error, row, at, format, joined))

    private companion object {
        const val AT = 1_793_041_515_000L
    }
}
