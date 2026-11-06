package com.dspdispatch.relay.texts

import androidx.room3.AutoMigration
import androidx.room3.Database
import androidx.room3.RoomDatabase

/**
 * The texts queue's storage, and the sends', in the app's own: nothing of it leaves the phone in a
 * backup.
 */
@Database(
    entities = [
        Mark::class, Seen::class, Unread::class, Waiting::class, WaitingFile::class, OnPhone::class,
        Send::class, Report::class, SendRow::class, Told::class, SmsParts::class,
    ],
    version = 3,
    autoMigrations = [AutoMigration(from = 1, to = 2), AutoMigration(from = 2, to = 3)],
)
abstract class TextsDatabase : RoomDatabase() {
    abstract fun queue(): StoredQueue

    abstract fun sends(): StoredSends

    companion object {
        const val NAME = "texts.db"
    }
}
