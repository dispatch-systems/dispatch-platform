package com.dspdispatch.relay.texts

import androidx.room3.Room
import androidx.sqlite.driver.bundled.BundledSQLiteDriver
import com.dspdispatch.relay.dispatch.Direction
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The queue's queries, on SQLite itself. */
class StoredQueueTest {
    private val database = Room.inMemoryDatabaseBuilder<TextsDatabase>().setDriver(BundledSQLiteDriver()).build()
    private val queue = database.queue()

    @After
    fun close() = database.close()

    @Test
    fun keepsTextsUntilAnsweredThenTheirFilesUntilSent() = runTest {
        queue.begin(Mark("phone_1", 0, 0))
        queue.read(
            Seen("mms-1-10", 10, Direction.IN),
            listOf(Waiting("mms-1-10", Direction.IN, "(206) 555-0101", "Door", 10)),
            listOf(1, 0).map { WaitingFile("mms-1-10", it, "content://mms/part/$it", "p$it.jpg", "image/jpeg", 5) },
        )
        queue.read(Seen("sms-2-5", 5, Direction.OUT), listOf(Waiting("sms-2-5", Direction.OUT, "(206) 555-0102", "Hi", 5)), emptyList())
        assertEquals(listOf("sms-2-5", "mms-1-10"), queue.waiting(10).map { it.text.clientId })
        assertEquals(2, queue.waiting(10).last().files.size)
        assertNull(queue.upload())

        // Dispatch answered one file's ID alone: the other has nowhere to go.
        queue.answered(mapOf("mms-1-10" to listOf("file_a")))
        assertEquals(listOf("sms-2-5"), queue.waiting(10).map { it.text.clientId })
        val file = queue.upload()!!
        assertEquals(0 to "file_a", file.position to file.dispatchId)
        queue.uploaded(file.id)
        assertNull(queue.upload())

        queue.refused("sms-2-5")
        assertTrue(queue.waiting(10).isEmpty())
    }

    @Test
    fun keepsAFileAsTheJpegMadeOfIt() = runTest {
        queue.begin(Mark("phone_1", 0, 0))
        queue.read(
            Seen("mms-1-10", 10, Direction.IN),
            listOf(Waiting("mms-1-10", Direction.IN, "(206) 555-0101", "Door", 10)),
            listOf(WaitingFile("mms-1-10", 0, "content://mms/part/1", "IMG_0001.HEIC", "image/heic", 731)),
        )
        val file = queue.waiting(10).single().files.single()
        val jpeg = file.copy(uri = "file:/jpegs/${file.id}.jpg", name = "IMG_0001.jpg", mime = "image/jpeg", size = 512)
        queue.converted(jpeg)
        assertEquals(listOf(jpeg), queue.waiting(10).single().files)
        assertEquals(listOf(jpeg.uri), queue.uris())
    }

    @Test
    fun keepsARowToReadAgainUntilItsRead() = runTest {
        queue.begin(Mark("phone_1", 0, 0))
        val row = Row(Table.MMS, 5, 10, Direction.IN)
        queue.later(Unread(row, 20))
        queue.later(Unread(row, 30))
        assertEquals(listOf(Unread(row, 20)), queue.unread())
        queue.read(Seen(row.key, row.at, Direction.IN), emptyList(), emptyList())
        assertTrue(queue.unread().isEmpty())
    }

    @Test
    fun countsTodaysTextsAndThoseWaiting() = runTest {
        queue.begin(Mark("phone_1", 0, 0))
        queue.read(Seen("sms-1-50", 50, Direction.IN), listOf(Waiting("sms-1-50", Direction.IN, "(206) 555-0101", "", 50)), emptyList())
        queue.read(Seen("sms-2-150", 150, Direction.IN), emptyList(), emptyList())
        queue.read(Seen("sms-3-160", 160, Direction.OUT), emptyList(), emptyList())
        queue.read(Seen("sms-4-170", 170, null), emptyList(), emptyList())
        assertEquals(Today(received = 1, sent = 1, waiting = 1, latest = 160), queue.today(100).first())
        assertEquals(Today(waiting = 1), queue.today(1_000).first())
    }

    @Test
    fun forgetsRowsBeforeTheLookBackAndEverythingOfAnEarlierPairing() = runTest {
        queue.begin(Mark("phone_1", 0, 0))
        queue.read(Seen("sms-1-50", 50, Direction.IN), listOf(Waiting("sms-1-50", Direction.IN, "(206) 555-0101", "", 50)), emptyList())
        queue.read(Seen("sms-2-150", 150, Direction.IN), emptyList(), emptyList())
        queue.scanned(Mark("phone_1", 0, 200, sms = 2, mms = 7), 100)
        assertEquals(listOf("sms-2-150"), queue.seen())
        assertEquals(Mark("phone_1", 0, 200, sms = 2, mms = 7), queue.mark())
        queue.begin(Mark("phone_2", 300, 300))
        assertEquals("phone_2", queue.mark()?.phone)
        assertTrue(queue.seen().isEmpty() && queue.waiting(10).isEmpty())
    }
}
