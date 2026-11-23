package com.dspdispatch.relay.contacts

import android.content.Context
import androidx.core.content.edit
import androidx.room3.Room
import com.dspdispatch.relay.connection.backoff
import com.dspdispatch.relay.dispatch.ContactsReport
import com.dspdispatch.relay.dispatch.Dispatch
import com.dspdispatch.relay.dispatch.NamedNumber
import com.dspdispatch.relay.dispatch.Outbox
import com.dspdispatch.relay.dispatch.Refusal
import com.dspdispatch.relay.dispatch.SavedContact
import com.dspdispatch.relay.dispatch.SavedNumber
import com.dspdispatch.relay.pairing.Paired
import java.io.IOException
import java.util.concurrent.ConcurrentHashMap
import kotlin.coroutines.CoroutineContext
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerializationException

/** Which pairing's line keeps its contacts on its phones, as its outbox last said: by its phone's ID. */
interface Keeping {
    fun read(): String?

    fun write(phone: String?)
}

/**
 * The phone's contacts, for a line that keeps its contacts on its phones (every line but Dispatch
 * Phone's own): the names saved at the DSP, as the outbox hands them, saved in the phone's own
 * contacts ([book]); and each number's name as its contacts change, told to Dispatch as it
 * changes, against what Dispatch was last [told], so only what changed goes. One loop does it
 * all, in turn, so a name saved is never told back as the phone's own.
 */
class Contacts internal constructor(
    private val dispatch: Dispatch,
    private val book: Book,
    private val told: Told,
    private val keeping: Keeping,
    private val settle: Long = SETTLE_MS,
    private val io: CoroutineContext = Dispatchers.IO,
) {
    constructor(context: Context, dispatch: Dispatch) : this(
        dispatch,
        PhoneBook(context),
        Room.databaseBuilder<ContactsDatabase>(context, ContactsDatabase.NAME).build().told(),
        Kept(context),
    )

    private val wake = Channel<Unit>(Channel.CONFLATED)

    // The names handed to save, the latest of each number's, until the phone saved them and said so.
    private val saving = ConcurrentHashMap<String, SavedContact>()

    // Whether the phone's contacts may have changed since they were last read.
    @Volatile private var changed = true

    /** Whether [paired]'s line keeps its contacts on its phones, as its outbox last said. */
    fun kept(paired: Paired) = keeping.read() == paired.phoneId

    /**
     * Takes what [outbox], Dispatch's answer to [paired]'s wait, says of contacts: the names to
     * save, and whether its line keeps them. Whether that's news, as Set up's steps then change.
     */
    fun heard(paired: Paired, outbox: Outbox): Boolean {
        val savings = outbox.contacts
        val news = (savings != null) != kept(paired)
        if (news) keeping.write(paired.phoneId.takeIf { savings != null })
        savings?.forEach { saving.merge(it.number, it) { kept, new -> if (new.at >= kept.at) new else kept } }
        if (savings != null) wake.trySend(Unit)
        return news
    }

    /**
     * Keeps [paired]'s contacts and Dispatch's alike, with its [key], while its line keeps them and
     * Android lets the app at them, waiting for a change or a name to save between; until cancelled.
     * A call that fails is made again after a growing pause.
     */
    @OptIn(FlowPreview::class)
    suspend fun run(paired: Paired, key: String): Unit = coroutineScope {
        told.only(paired.phoneId)
        var watching: Job? = null
        var failures = 0
        while (true) {
            if (!kept(paired) || !book.allowed()) {
                watching?.cancel()
                watching = null
                wake.receive()
                continue
            }
            if (watching == null) {
                changed = true
                watching = launch {
                    book.changes().debounce(settle).collect {
                        changed = true
                        wake.trySend(Unit)
                    }
                }
            }
            try {
                sync(paired, key)
                failures = 0
            } catch (refusal: Refusal) {
                if (refusal.code == Refusal.CONTACTS_NOT_KEPT) keeping.write(null) else delay(backoff(++failures))
                continue
            } catch (_: IOException) {
                delay(backoff(++failures))
                continue
            } catch (_: SerializationException) {
                // An answer that isn't Dispatch's, as from something between the phone and it.
                delay(backoff(++failures))
                continue
            } catch (_: SecurityException) {
                // Android stopped letting the app at the contacts as it read or saved them.
                delay(backoff(++failures))
                continue
            }
            wake.receive()
        }
    }

    /**
     * Saves the names handed, then tells Dispatch what changed of the phone's contacts since it
     * was last told, with the names saved, in batches, each kept as told once answered.
     */
    internal suspend fun sync(paired: Paired, key: String) {
        val phone = paired.phoneId
        val saves = saving.values.sortedBy(SavedContact::at)
        if (saves.isNotEmpty()) {
            val named = saves.map { NamedNumber(it.number, it.name) }
            withContext(io) { book.save(named) }
            // Told as Dispatch has it already, so its own name never goes back as the phone's.
            told.told(phone, named, emptyList())
            changed = true
        }
        if (!changed) return
        changed = false
        val now = withContext(io) { book.read() }
        val saved = saves.map { SavedNumber(it.number, it.at) }
        for (batch in report(told.all(phone), now, saved)) {
            dispatch.contacts(paired.site, key, batch)
            told.told(phone, batch.changed, batch.removed)
            val done = batch.saved.associate { it.number to it.at }
            saves.filter { done[it.number] == it.at }.forEach { saving.remove(it.number, it) }
        }
    }

    /** Which pairing keeps its contacts on its phone, in the app's own storage. */
    private class Kept(context: Context) : Keeping {
        private val prefs = context.getSharedPreferences("contacts", Context.MODE_PRIVATE)

        override fun read(): String? = prefs.getString(KEPT, null)

        override fun write(phone: String?) = prefs.edit { if (phone == null) remove(KEPT) else putString(KEPT, phone) }
    }

    private companion object {
        /** How long the contacts stay unchanged before they're read, so a run of changes is read once. */
        const val SETTLE_MS = 2_000L
        const val KEPT = "kept"
    }
}

/** The most entries Dispatch takes in one report (`MOST_CONTACTS_REPORTED`). */
internal const val MOST_REPORTED = 200

/** About the most bytes of a report, well within Dispatch's 64 KiB. */
internal const val MOST_BYTES = 48_000

/**
 * What changed between what Dispatch was [told] and the phone's contacts [now], with the names
 * [saved], in batches of at most [most] entries and about [bytes] each: the saved first, then the
 * removed, then the changed.
 */
internal fun report(
    told: Map<String, String>,
    now: Map<String, String>,
    saved: List<SavedNumber>,
    most: Int = MOST_REPORTED,
    bytes: Int = MOST_BYTES,
): List<ContactsReport> {
    val batches = Batches(most, bytes)
    saved.forEach(batches::saved)
    told.keys.filterNot(now::containsKey).sorted().forEach(batches::removed)
    now.toSortedMap().forEach { (number, name) -> if (told[number] != name) batches.changed(NamedNumber(number, name)) }
    return batches.done()
}

/** Reports filled in turn, each of at most [most] entries and about [bytes]. */
private class Batches(private val most: Int, private val bytes: Int) {
    private val done = mutableListOf<ContactsReport>()
    private val saved = mutableListOf<SavedNumber>()
    private val changed = mutableListOf<NamedNumber>()
    private val removed = mutableListOf<String>()
    private var count = 0
    private var size = 0

    fun saved(entry: SavedNumber) = add(entry.number.length) { saved += entry }

    fun removed(number: String) = add(number.length) { removed += number }

    fun changed(entry: NamedNumber) = add(entry.number.length + entry.name.toByteArray().size) { changed += entry }

    /** Every report, the last one closed. */
    fun done(): List<ContactsReport> {
        if (count > 0) close()
        return done
    }

    /** Adds an entry of about [length] bytes but its keys, starting another report where it won't fit. */
    private fun add(length: Int, entry: () -> Unit) {
        if (count > 0 && (count == most || size + length + ENTRY_BYTES > bytes)) close()
        entry()
        count++
        size += length + ENTRY_BYTES
    }

    private fun close() {
        done += ContactsReport(saved.toList(), changed.toList(), removed.toList())
        saved.clear()
        changed.clear()
        removed.clear()
        count = 0
        size = 0
    }
}

/** About the most bytes an entry takes beside its number and name: its keys, quotes and commas. */
private const val ENTRY_BYTES = 40
