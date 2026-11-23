package com.dspdispatch.relay.contacts

import androidx.room3.Dao
import androidx.room3.Database
import androidx.room3.Entity
import androidx.room3.Insert
import androidx.room3.OnConflictStrategy
import androidx.room3.Query
import androidx.room3.RoomDatabase
import androidx.room3.Transaction
import com.dspdispatch.relay.dispatch.NamedNumber

/** A number by its [name], as Dispatch was last told the pairing [phone]'s contacts hold it. */
@Entity(tableName = "told", primaryKeys = ["phone", "number"])
data class ToldContact(val phone: String, val number: String, val name: String)

/** What Dispatch was last told each pairing's contacts hold, so only what changed since is told. */
interface Told {
    /** Each number Dispatch was told [phone]'s contacts hold, by its name. */
    suspend fun all(phone: String): Map<String, String>

    /** Dispatch was told [phone]'s contacts hold [changed] as named, and [removed] no more. */
    suspend fun told(phone: String, changed: List<NamedNumber>, removed: List<String>)

    /** Forgets what was told of any pairing but [phone]'s, an earlier one's. */
    suspend fun only(phone: String)
}

/** The [Told], kept in the [ContactsDatabase]. */
@Dao
abstract class StoredTold : Told {
    @Query("SELECT * FROM told WHERE phone = :phone")
    protected abstract suspend fun rows(phone: String): List<ToldContact>

    override suspend fun all(phone: String) = rows(phone).associate { it.number to it.name }

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    protected abstract suspend fun keep(rows: List<ToldContact>)

    @Query("DELETE FROM told WHERE phone = :phone AND number IN (:numbers)")
    protected abstract suspend fun forget(phone: String, numbers: List<String>)

    @Transaction
    override suspend fun told(phone: String, changed: List<NamedNumber>, removed: List<String>) {
        if (changed.isNotEmpty()) keep(changed.map { ToldContact(phone, it.number, it.name) })
        if (removed.isNotEmpty()) forget(phone, removed)
    }

    @Query("DELETE FROM told WHERE phone != :phone")
    abstract override suspend fun only(phone: String)
}

/** The contacts' storage, in the app's own: nothing of it leaves the phone in a backup. */
@Database(entities = [ToldContact::class], version = 1)
abstract class ContactsDatabase : RoomDatabase() {
    abstract fun told(): StoredTold

    companion object {
        const val NAME = "contacts.db"
    }
}
