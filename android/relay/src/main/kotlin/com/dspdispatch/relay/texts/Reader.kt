package com.dspdispatch.relay.texts

import android.database.sqlite.SQLiteException
import com.dspdispatch.relay.connection.backoff
import kotlin.coroutines.CoroutineContext
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.produceIn
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull

/**
 * Reads the phone's texts through the [inbox] while [texting] allows it, on [io]: at each of the
 * provider's [changes], again after a growing pause while a read failed or left a text not whole,
 * and every [RECONCILE_MS] whatever the provider says, as it doesn't say everything: a picture
 * message's parts and addresses are written without a word.
 */
class Reader(
    private val inbox: Inbox,
    private val texting: Texting,
    private val changes: Flow<Unit>,
    private val io: CoroutineContext = Dispatchers.IO,
) {
    /** Reads until cancelled, calling [read] after each read. */
    suspend fun run(read: () -> Unit): Nothing {
        while (true) {
            texting.await()
            try {
                reading(read)
            } catch (_: SecurityException) {
                // Texting was turned off: nothing to read until it's back.
            }
        }
    }

    private suspend fun reading(read: () -> Unit): Unit = coroutineScope {
        val changed = changes.produceIn(this)
        var failures = 0
        while (true) {
            withTimeoutOrNull(if (failures == 0) RECONCILE_MS else backoff(failures)) { changed.receive() }
            // The texting app writes a text in steps: read once it's done.
            delay(SETTLE_MS)
            failures = try {
                if (withContext(io) { inbox.read() }) 0 else failures + 1
            } catch (_: Unavailable) {
                failures + 1
            } catch (_: SQLiteException) {
                failures + 1
            }
            read()
        }
    }

    internal companion object {
        /** How long the reader waits for the provider to say something changed before it reads anyway. */
        const val RECONCILE_MS = 15 * 60 * 1000L

        /** How long a text may take to be written whole. */
        const val SETTLE_MS = 1_000L
    }
}
