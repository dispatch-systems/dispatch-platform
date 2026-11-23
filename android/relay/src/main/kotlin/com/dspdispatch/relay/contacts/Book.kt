package com.dspdispatch.relay.contacts

import android.Manifest
import android.content.ContentProviderOperation
import android.content.ContentResolver
import android.content.Context
import android.content.pm.PackageManager
import android.database.ContentObserver
import android.os.Handler
import android.os.Looper
import android.provider.ContactsContract
import android.provider.ContactsContract.CommonDataKinds.Phone
import android.provider.ContactsContract.CommonDataKinds.StructuredName
import android.provider.ContactsContract.Data
import android.provider.ContactsContract.RawContacts
import androidx.core.content.ContextCompat
import com.dspdispatch.relay.dispatch.NamedNumber
import com.dspdispatch.relay.texts.Numbers
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.flow.conflate

/** The phone's own contacts, as the app reads and changes them ([PhoneBook], Android's). */
interface Book {
    /** Whether Android lets the app read and change the phone's contacts. */
    fun allowed(): Boolean

    /**
     * Each number the phone's contacts hold that Dispatch takes, as Dispatch writes it, by its name
     * there; of several contacts holding one number, the first made's.
     */
    fun read(): Map<String, String>

    /** Names each number's contacts as [contacts] has it, adding one so named where none holds it. */
    fun save(contacts: List<NamedNumber>)

    /** Each time the phone's contacts may have changed. */
    fun changes(): Flow<Unit>
}

/** The phone's contacts, as Android keeps them: read and changed through its contacts provider. */
class PhoneBook(private val context: Context) : Book {
    private val resolver: ContentResolver = context.contentResolver

    override fun allowed() = PERMISSIONS.all {
        ContextCompat.checkSelfPermission(context, it) == PackageManager.PERMISSION_GRANTED
    }

    override fun read(): Map<String, String> {
        val numbers = mutableMapOf<String, String>()
        query(arrayOf(Phone.NUMBER, Phone.DISPLAY_NAME_PRIMARY)) { number, name ->
            val named = name?.let(::named) ?: return@query
            if (Numbers.normal(named) == number) return@query
            numbers.putIfAbsent(number, named)
        }
        return numbers
    }

    override fun save(contacts: List<NamedNumber>) {
        if (contacts.isEmpty()) return
        val holding = mutableMapOf<String, MutableSet<String>>()
        query(arrayOf(Phone.NUMBER, Phone.RAW_CONTACT_ID)) { number, raw ->
            if (raw != null) holding.getOrPut(number, ::mutableSetOf) += raw
        }
        // Android takes a few hundred operations in one batch: a handful of contacts at a time.
        for (some in contacts.chunked(SAVED_AT_ONCE)) {
            val operations = ArrayList<ContentProviderOperation>()
            for (contact in some) {
                val raws = holding[contact.number]
                if (raws == null) operations.added(contact) else raws.forEach { operations.renamed(it, contact.name) }
            }
            resolver.applyBatch(ContactsContract.AUTHORITY, operations)
        }
    }

    override fun changes(): Flow<Unit> = callbackFlow {
        val observer = object : ContentObserver(Handler(Looper.getMainLooper())) {
            override fun onChange(selfChange: Boolean) {
                trySend(Unit)
            }
        }
        resolver.registerContentObserver(ContactsContract.AUTHORITY_URI, true, observer)
        awaitClose { resolver.unregisterContentObserver(observer) }
    }.conflate()

    /** Each phone number row of the contacts, oldest contact first, as Dispatch writes its number, with its [column]. */
    private fun query(columns: Array<String>, each: (number: String, column: String?) -> Unit) {
        resolver.query(Phone.CONTENT_URI, columns, null, null, "${Phone.CONTACT_ID} ASC")?.use { rows ->
            while (rows.moveToNext()) {
                val number = rows.getString(0)?.let(Numbers::normal) ?: continue
                each(number, rows.getString(1))
            }
        }
    }

    /** Names the raw contact [raw] [name] alone, its name's parts as Android makes them of it. */
    private fun MutableList<ContentProviderOperation>.renamed(raw: String, name: String) {
        add(
            ContentProviderOperation.newDelete(Data.CONTENT_URI)
                .withSelection("${Data.RAW_CONTACT_ID}=? AND ${Data.MIMETYPE}=?", arrayOf(raw, StructuredName.CONTENT_ITEM_TYPE))
                .build(),
        )
        add(
            ContentProviderOperation.newInsert(Data.CONTENT_URI)
                .withValue(Data.RAW_CONTACT_ID, raw.toLong())
                .withValue(Data.MIMETYPE, StructuredName.CONTENT_ITEM_TYPE)
                .withValue(StructuredName.DISPLAY_NAME, name)
                .build(),
        )
    }

    /** Adds a contact of the phone's own, of no account, holding [contact]'s number by its name. */
    private fun MutableList<ContentProviderOperation>.added(contact: NamedNumber) {
        val at = size
        add(
            ContentProviderOperation.newInsert(RawContacts.CONTENT_URI)
                .withValue(RawContacts.ACCOUNT_TYPE, null)
                .withValue(RawContacts.ACCOUNT_NAME, null)
                .build(),
        )
        add(
            ContentProviderOperation.newInsert(Data.CONTENT_URI)
                .withValueBackReference(Data.RAW_CONTACT_ID, at)
                .withValue(Data.MIMETYPE, StructuredName.CONTENT_ITEM_TYPE)
                .withValue(StructuredName.DISPLAY_NAME, contact.name)
                .build(),
        )
        add(
            ContentProviderOperation.newInsert(Data.CONTENT_URI)
                .withValueBackReference(Data.RAW_CONTACT_ID, at)
                .withValue(Data.MIMETYPE, Phone.CONTENT_ITEM_TYPE)
                .withValue(Phone.NUMBER, contact.number)
                .withValue(Phone.TYPE, Phone.TYPE_MOBILE)
                .build(),
        )
    }

    companion object {
        /** What reading and changing the phone's contacts needs. */
        val PERMISSIONS = arrayOf(Manifest.permission.READ_CONTACTS, Manifest.permission.WRITE_CONTACTS)

        /** How many contacts are saved in one batch: at most three operations each. */
        private const val SAVED_AT_ONCE = 50
    }
}

/** The most characters Dispatch keeps of a name (`NAME_MOST`). */
private const val NAME_MOST = 80

/** [name] as Dispatch keeps it: trimmed, and cut to its most characters; null for a blank one. */
internal fun named(name: String): String? {
    val trimmed = name.trim()
    if (trimmed.isEmpty()) return null
    if (trimmed.codePointCount(0, trimmed.length) <= NAME_MOST) return trimmed
    return trimmed.substring(0, trimmed.offsetByCodePoints(0, NAME_MOST)).trimEnd()
}
