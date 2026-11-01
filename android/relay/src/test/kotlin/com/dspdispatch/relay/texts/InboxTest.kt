package com.dspdispatch.relay.texts

import com.dspdispatch.relay.dispatch.Direction
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class InboxTest {
    private val provider = FakeProvider()
    private val queue = MemoryQueue()
    private val ownRows = mutableSetOf<Row>()
    private var clock = PAIRED
    private val inbox = Inbox(provider, queue, { it in ownRows }) { clock }

    @Test
    fun readsEachTextOnceFromWhenThePhonePaired() = runTest {
        sms(1, PAIRED - 1, Direction.IN, "(206) 555-0101", "Before pairing")
        inbox.begin("phone_1", PAIRED)
        sms(2, PAIRED + 10, Direction.IN, "(206) 555-0102", "Gate code?")
        sms(3, PAIRED + 20, Direction.OUT, "(206) 555-0102", "4412")
        read()
        read()
        assertEquals(
            listOf(
                Waiting("sms-2-${PAIRED + 10}", Direction.IN, "(206) 555-0102", "Gate code?", PAIRED + 10),
                Waiting("sms-3-${PAIRED + 20}", Direction.OUT, "(206) 555-0102", "4412", PAIRED + 20),
            ),
            queue.texts.values.sortedBy(Waiting::at),
        )
    }

    @Test
    fun readsFromWhenThePhonePairedNotWhenTheRelayFirstRan() = runTest {
        sms(2, PAIRED + 1, Direction.IN, "(206) 555-0102", "Paired a moment ago")
        clock = PAIRED + 2
        inbox.begin("phone_1", PAIRED)
        read()
        assertEquals(setOf("sms-2-${PAIRED + 1}"), queue.texts.keys)
    }

    @Test
    fun beginsAgainForANewPairingOnly() = runTest {
        inbox.begin("phone_1", PAIRED)
        sms(2, PAIRED + 10, Direction.IN, "(206) 555-0102", "Gate code?")
        read()
        inbox.begin("phone_1", PAIRED)
        assertEquals(1, queue.texts.size)
        clock += HOUR
        inbox.begin("phone_2", clock)
        assertTrue(queue.texts.isEmpty())
        read()
        assertTrue("a text before the new pairing", queue.texts.isEmpty())
    }

    @Test
    fun readsTextsWrittenWhileStoppedAndLateOnesAtItsNextStart() = runTest {
        inbox.begin("phone_1", PAIRED)
        sms(2, PAIRED + 10, Direction.IN, "(206) 555-0102", "First")
        clock = PAIRED + 3 * HOUR
        read()
        // While the app was stopped: a text, and a picture message downloaded hours after its time.
        sms(3, PAIRED + 4 * HOUR, Direction.IN, "(206) 555-0103", "While stopped")
        mms(4, PAIRED + HOUR, Direction.IN, listOf("(206) 555-0104"), "Late")
        clock = PAIRED + 5 * HOUR
        read()
        assertEquals(setOf("sms-2-${PAIRED + 10}", "sms-3-${PAIRED + 4 * HOUR}", "mms-4-${PAIRED + HOUR}"), queue.texts.keys)
    }

    @Test
    fun knowsARowWhoseIdWasUsedAgain() = runTest {
        inbox.begin("phone_1", PAIRED)
        sms(2, PAIRED + 10, Direction.IN, "(206) 555-0102", "Deleted later")
        read()
        provider.messages.clear()
        sms(2, PAIRED + 20, Direction.IN, "(206) 555-0102", "Its ID again")
        read()
        assertEquals(setOf("sms-2-${PAIRED + 10}", "sms-2-${PAIRED + 20}"), queue.texts.keys)
    }

    @Test
    fun readsAPictureMessageOnceItsWhole() = runTest {
        inbox.begin("phone_1", PAIRED)
        val row = Row(Table.MMS, 5, PAIRED + 10, Direction.IN)
        provider.writing += row
        read()
        assertTrue(queue.texts.isEmpty())
        provider.writing.clear()
        mms(5, PAIRED + 10, Direction.IN, listOf("(206) 555-0105"), "Door", Part("content://mms/part/9", "door.jpg", "image/jpeg", 48_213))
        read()
        assertEquals(listOf("mms-5-${PAIRED + 10}"), queue.texts.keys.toList())
        assertEquals(
            listOf(WaitingFile("mms-5-${PAIRED + 10}", 0, "content://mms/part/9", "door.jpg", "image/jpeg", 48_213, id = 1)),
            queue.files,
        )
    }

    @Test
    fun readsAPictureMessageAgainAfterTheProviderWasAway() = runTest {
        inbox.begin("phone_1", PAIRED)
        val door = Part("content://mms/part/9", "door.jpg", "image/jpeg", 48_213)
        mms(5, PAIRED + 10, Direction.IN, listOf("(206) 555-0105"), "Door", door)
        provider.away += door.uri
        read()
        assertTrue(queue.texts.isEmpty())
        assertTrue("not marked read", queue.seen.isEmpty())
        read()
        assertEquals(listOf("mms-5-${PAIRED + 10}"), queue.texts.keys.toList())
        assertEquals(listOf(door.uri), queue.files.map(WaitingFile::uri))
    }

    @Test
    fun reportsAGroupTextFromItsSenderAndOneSentToEachRecipient() = runTest {
        inbox.begin("phone_1", PAIRED)
        mms(6, PAIRED + 10, Direction.IN, listOf("(206) 555-0106"), "To the group")
        mms(7, PAIRED + 20, Direction.OUT, listOf("(206) 555-0106", "(206) 555-0107"), "To both", Part("content://mms/part/3", "map.png", "image/png", 10))
        read()
        val key = "mms-7-${PAIRED + 20}"
        assertEquals(
            listOf("mms-6-${PAIRED + 10}" to "(206) 555-0106", "$key-0" to "(206) 555-0106", "$key-1" to "(206) 555-0107"),
            queue.texts.values.sortedBy(Waiting::clientId).map { it.clientId to it.number },
        )
        assertEquals(listOf("$key-0", "$key-1"), queue.files.map(WaitingFile::clientId))
    }

    @Test
    fun leavesOutWhatDispatchWouldRefuseAndTheAppsOwn() = runTest {
        inbox.begin("phone_1", PAIRED)
        sms(2, PAIRED + 10, Direction.IN, "Carrier", "Your bill is ready")
        sms(3, PAIRED + 20, Direction.OUT, "(206) 555-0103", "Sent for Dispatch")
        ownRows += provider.messages.last().row
        sms(4, PAIRED + 30, Direction.IN, "(206) 555-0104", "Still here")
        read()
        assertEquals(listOf("sms-4-${PAIRED + 30}"), queue.texts.keys.toList())
        assertEquals(3, queue.seen.size)
    }

    @Test
    fun keepsATextWithinWhatDispatchTakes() = runTest {
        inbox.begin("phone_1", PAIRED)
        val parts = (0..11).map { Part("content://mms/part/$it", "p$it.jpg", "image/jpeg", 1) } +
            Part("content://mms/part/big", "big.mp4", "video/mp4", 21L * 1024 * 1024)
        mms(8, PAIRED + 10, Direction.IN, listOf("(206) 555-0108"), "😀".repeat(10_001), *parts.reversed().toTypedArray())
        read()
        val text = queue.texts.values.single()
        assertEquals(10_000, text.body.codePointCount(0, text.body.length))
        assertEquals(10, queue.files.size)
        assertTrue(queue.files.none { it.name == "big.mp4" })
    }

    private suspend fun read() {
        inbox.read()
        clock += 1
    }

    private fun sms(id: Long, at: Long, direction: Direction, number: String, body: String) {
        provider.messages += Message(Row(Table.SMS, id, at, direction), listOf(number), body, emptyList())
    }

    private fun mms(id: Long, at: Long, direction: Direction, numbers: List<String>, body: String, vararg parts: Part) {
        provider.messages += Message(Row(Table.MMS, id, at, direction), numbers, body, parts.toList())
    }

    private companion object {
        const val PAIRED = 1_793_041_515_000L
        const val HOUR = 60 * 60 * 1000L
    }
}
