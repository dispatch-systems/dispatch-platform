package com.dspdispatch.relay.texts

import androidx.room3.Dao
import androidx.room3.Delete
import androidx.room3.Insert
import androidx.room3.OnConflictStrategy
import androidx.room3.Query
import androidx.room3.Transaction
import androidx.room3.Update
import com.dspdispatch.relay.dispatch.SendStatus

/** The [Sends], kept in the [TextsDatabase]. */
@Dao
abstract class StoredSends : Sends {
    @Transaction
    override suspend fun start(send: Send): Send {
        keep(send)
        return checkNotNull(find(send.id, send.attempt))
    }

    @Query("UPDATE sends SET handed = :at, parts = :parts, joined = :joined, mms = :mms WHERE id = :id AND attempt = :attempt")
    protected abstract suspend fun handed(id: String, attempt: Long, parts: Int, joined: Boolean, mms: Boolean, at: Long)

    @Transaction
    override suspend fun handed(send: Send, parts: Int, joined: Boolean, at: Long, mms: Boolean) {
        handed(send.id, send.attempt, parts, joined, mms, at)
        if (!mms) keep(SmsParts(send.id, send.attempt, at, parts))
    }

    @Query("SELECT COALESCE(SUM(parts), 0) FROM sms_parts WHERE at >= :since")
    abstract override suspend fun smsParts(since: Long): Int

    @Transaction
    override suspend fun failed(send: Send, error: String, at: Long) = end(send, SendStatus.FAILED, at, error)

    @Transaction
    override suspend fun heard(heard: Heard) {
        val send = find(heard.id, heard.attempt) ?: return
        keep(heard.report)
        if (heard.row != null) keep(SendRow(heard.row, send.id, send.attempt))
        val (settled, statuses) = send.settled(reports(send.id, send.attempt), heard.at)
        update(settled)
        statuses.forEach { keepStatus(it) }
    }

    @Query("SELECT * FROM sends WHERE handed < :before AND outcome IS NULL")
    abstract override suspend fun unanswered(before: Long): List<Send>

    @Transaction
    override suspend fun expired(send: Send, sent: Boolean, at: Long) =
        if (sent) end(send, SendStatus.SENT, at, null) else end(send, SendStatus.FAILED, at, Failure.NO_RESULT)

    /** Ends [send] [at] as [outcome], for [error] if it failed, unless it ended already. */
    private suspend fun end(send: Send, outcome: SendStatus, at: Long, error: String?) {
        val kept = find(send.id, send.attempt) ?: return
        if (kept.outcome != null) return
        update(kept.copy(outcome = outcome, finished = at))
        keepStatus(kept.status(outcome, at, error))
    }

    @Transaction
    override suspend fun forget(before: Long, undelivered: Long) {
        forgetSends(before, undelivered)
        forgetReports()
        forgetRows()
        forgetSmsParts(before)
    }

    @Query("SELECT * FROM statuses ORDER BY at, id, attempt LIMIT :most")
    abstract override suspend fun statuses(most: Int): List<Told>

    @Delete
    abstract override suspend fun told(told: List<Told>)

    @Query("SELECT row FROM send_rows WHERE id = :id AND attempt = :attempt")
    protected abstract suspend fun rows(id: String, attempt: Long): List<String>

    override suspend fun rows(send: Send) = rows(send.id, send.attempt)

    @Query("SELECT EXISTS (SELECT 1 FROM send_reports WHERE id = :id AND attempt = :attempt AND joined)")
    protected abstract suspend fun joined(id: String, attempt: Long): Boolean

    override suspend fun joined(send: Send) = joined(send.id, send.attempt)

    @Query("SELECT EXISTS (SELECT 1 FROM send_rows WHERE row = :row)")
    abstract override suspend fun wrote(row: String): Boolean

    @Query(
        """
        SELECT * FROM sends WHERE handed BETWEEN :from AND :to AND NOT EXISTS
            (SELECT 1 FROM send_rows WHERE send_rows.id = sends.id AND send_rows.attempt = sends.attempt)
        ORDER BY handed
        """,
    )
    abstract override suspend fun unplaced(from: Long, to: Long): List<Send>

    override suspend fun placed(send: Send, row: String) = keep(SendRow(row, send.id, send.attempt))

    @Insert(onConflict = OnConflictStrategy.IGNORE)
    protected abstract suspend fun keep(send: Send)

    @Insert(onConflict = OnConflictStrategy.IGNORE)
    protected abstract suspend fun keep(report: Report)

    @Insert(onConflict = OnConflictStrategy.IGNORE)
    protected abstract suspend fun keep(row: SendRow)

    @Insert(onConflict = OnConflictStrategy.IGNORE)
    protected abstract suspend fun keep(parts: SmsParts)

    @Insert(onConflict = OnConflictStrategy.IGNORE)
    protected abstract suspend fun keepStatus(status: Told)

    @Update
    protected abstract suspend fun update(send: Send)

    @Query("SELECT * FROM sends WHERE id = :id AND attempt = :attempt")
    abstract override suspend fun find(id: String, attempt: Long): Send?

    @Query("SELECT * FROM send_reports WHERE id = :id AND attempt = :attempt")
    protected abstract suspend fun reports(id: String, attempt: Long): List<Report>

    // A send Android took and never settled is settled once it expired, so it's kept until then. A
    // text not yet delivered may be, failed or sent, as long as the carrier keeps trying: picture
    // messages' deliveries aren't heard.
    @Query(
        """
        DELETE FROM sends WHERE started < :before AND (outcome IS NOT NULL OR handed IS NULL)
            AND (delivered OR mms OR handed IS NULL OR handed < :undelivered) AND NOT EXISTS
            (SELECT 1 FROM statuses WHERE statuses.id = sends.id AND statuses.attempt = sends.attempt)
        """,
    )
    protected abstract suspend fun forgetSends(before: Long, undelivered: Long)

    @Query(
        """
        DELETE FROM send_reports WHERE NOT EXISTS
            (SELECT 1 FROM sends WHERE sends.id = send_reports.id AND sends.attempt = send_reports.attempt)
        """,
    )
    protected abstract suspend fun forgetReports()

    @Query(
        """
        DELETE FROM send_rows WHERE NOT EXISTS
            (SELECT 1 FROM sends WHERE sends.id = send_rows.id AND sends.attempt = send_rows.attempt)
        """,
    )
    protected abstract suspend fun forgetRows()

    @Query("DELETE FROM sms_parts WHERE at < :before")
    protected abstract suspend fun forgetSmsParts(before: Long)
}
