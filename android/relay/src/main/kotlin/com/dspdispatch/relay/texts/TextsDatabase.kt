package com.dspdispatch.relay.texts

import androidx.room3.Dao
import androidx.room3.Database
import androidx.room3.Insert
import androidx.room3.OnConflictStrategy
import androidx.room3.Query
import androidx.room3.RoomDatabase
import androidx.room3.Transaction
import kotlinx.coroutines.flow.Flow

/** The texts queue's storage, in the app's own: nothing of it leaves the phone in a backup. */
@Database(entities = [Mark::class, Seen::class, Unread::class, Waiting::class, WaitingFile::class], version = 1)
abstract class TextsDatabase : RoomDatabase() {
    abstract fun queue(): StoredQueue

    companion object {
        const val NAME = "texts.db"
    }
}

/** Today's texts so far: received, sent, and the latest one's time; and those waiting to be reported. */
data class Today(val received: Int = 0, val sent: Int = 0, val waiting: Int = 0, val latest: Long? = null)

/** The [Queue], kept in the [TextsDatabase]. */
@Dao
abstract class StoredQueue : Queue {
    @Query("SELECT * FROM mark LIMIT 1")
    abstract override suspend fun mark(): Mark?

    @Transaction
    override suspend fun begin(mark: Mark) {
        clearMark()
        clearSeen()
        clearUnread()
        clearTexts()
        clearFiles()
        keep(mark)
    }

    @Query("SELECT row FROM seen")
    abstract override suspend fun seen(): List<String>

    @Transaction
    override suspend fun read(row: Seen, texts: List<Waiting>, files: List<WaitingFile>) {
        keep(row)
        leaveUnread(row.row)
        keepTexts(texts)
        keepFiles(files)
    }

    @Query("SELECT * FROM unread")
    abstract override suspend fun unread(): List<Unread>

    @Insert(onConflict = OnConflictStrategy.IGNORE)
    abstract override suspend fun later(row: Unread)

    @Transaction
    override suspend fun scanned(mark: Mark, forget: Long) {
        keep(mark)
        forgetBefore(forget)
    }

    @Transaction
    @Query("SELECT * FROM texts ORDER BY at, clientId LIMIT :most")
    abstract override suspend fun waiting(most: Int): List<WaitingText>

    @Transaction
    override suspend fun answered(answered: Map<String, List<String>>) {
        for ((clientId, ids) in answered) {
            leaveText(clientId)
            for (file in filesOf(clientId)) {
                val id = ids.getOrNull(file.position)
                if (id == null) uploaded(file.id) else answerFile(file.id, id)
            }
        }
    }

    @Transaction
    override suspend fun refused(clientId: String) {
        leaveText(clientId)
        leaveFiles(clientId)
    }

    @Query("SELECT * FROM files WHERE dispatchId IS NOT NULL ORDER BY id LIMIT 1")
    abstract override suspend fun upload(): WaitingFile?

    @Query("DELETE FROM files WHERE id = :id")
    abstract override suspend fun uploaded(id: Long)

    /** Today's texts from [since], the day's start, as they change. */
    @Query(
        """
        SELECT COUNT(CASE WHEN direction = 'IN' THEN 1 END) AS received,
            COUNT(CASE WHEN direction = 'OUT' THEN 1 END) AS sent,
            (SELECT COUNT(*) FROM texts) AS waiting,
            MAX(at) AS latest
        FROM seen WHERE at >= :since AND direction IS NOT NULL
        """,
    )
    abstract fun today(since: Long): Flow<Today>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    protected abstract suspend fun keep(mark: Mark)

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    protected abstract suspend fun keep(seen: Seen)

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    protected abstract suspend fun keepTexts(texts: List<Waiting>)

    @Insert
    protected abstract suspend fun keepFiles(files: List<WaitingFile>)

    @Query("DELETE FROM seen WHERE at < :before")
    protected abstract suspend fun forgetBefore(before: Long)

    @Query("SELECT * FROM files WHERE clientId = :clientId")
    protected abstract suspend fun filesOf(clientId: String): List<WaitingFile>

    @Query("UPDATE files SET dispatchId = :dispatchId WHERE id = :id")
    protected abstract suspend fun answerFile(id: Long, dispatchId: String)

    @Query("DELETE FROM texts WHERE clientId = :clientId")
    protected abstract suspend fun leaveText(clientId: String)

    @Query("DELETE FROM files WHERE clientId = :clientId")
    protected abstract suspend fun leaveFiles(clientId: String)

    @Query("DELETE FROM unread WHERE `key` = :key")
    protected abstract suspend fun leaveUnread(key: String)

    @Query("DELETE FROM mark")
    protected abstract suspend fun clearMark()

    @Query("DELETE FROM seen")
    protected abstract suspend fun clearSeen()

    @Query("DELETE FROM unread")
    protected abstract suspend fun clearUnread()

    @Query("DELETE FROM texts")
    protected abstract suspend fun clearTexts()

    @Query("DELETE FROM files")
    protected abstract suspend fun clearFiles()
}
