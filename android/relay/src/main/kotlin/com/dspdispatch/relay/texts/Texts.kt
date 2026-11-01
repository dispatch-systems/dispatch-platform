package com.dspdispatch.relay.texts

import android.content.Context
import android.database.sqlite.SQLiteException
import androidx.room3.Room
import com.dspdispatch.relay.dispatch.Dispatch
import com.dspdispatch.relay.pairing.Paired
import java.time.LocalDate
import java.time.ZoneId
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Relaying the phone's texts to its DSP: those it receives, and those sent from its texting app,
 * with their pictures, read into a queue kept on the phone and reported from it.
 */
class Texts(private val context: Context, dispatch: Dispatch) {
    private val queue = Room.databaseBuilder<TextsDatabase>(context, TextsDatabase.NAME).build().queue()
    private val provider = PhoneProvider(context.contentResolver)
    private val inbox = Inbox(provider, queue, Own.NONE)
    private val texting = Texting { textingAllowed(context) }
    private val reporter = Reporter(dispatch, queue, provider, texting)

    /** Today's texts as they change, a new day from each midnight. */
    @OptIn(ExperimentalCoroutinesApi::class)
    val today: Flow<Today> = flow {
        while (true) {
            val zone = ZoneId.systemDefault()
            val day = LocalDate.now(zone)
            emit(day.atStartOfDay(zone).toInstant().toEpochMilli())
            delay(day.plusDays(1).atStartOfDay(zone).toInstant().toEpochMilli() - System.currentTimeMillis())
        }
    }.flatMapLatest(queue::today)

    /**
     * Relays the texts of [paired], with its [key], from when it paired: reporting whatever
     * waits at once, and reading the phone's texts once the phone allows it. Runs until cancelled.
     */
    suspend fun run(paired: Paired, key: String): Unit = coroutineScope {
        inbox.begin(paired.phoneId, paired.at)
        val wake = Channel<Unit>(Channel.CONFLATED)
        launch { reporter.run(paired.site, key, wake) }
        texting.await()
        changes(context).collect {
            // The texting app writes a text in steps: read once it's done.
            delay(SETTLE_MS)
            try {
                withContext(Dispatchers.IO) { inbox.read() }
                wake.trySend(Unit)
            } catch (_: SecurityException) {
                // Texting was turned off, which ends the app; nothing to read until it's back.
            } catch (_: SQLiteException) {
                // The provider failed this once; its next change reads again.
            } catch (_: Unavailable) {
                // The provider's away a moment; its next change reads again.
            }
        }
    }

    private companion object {
        const val SETTLE_MS = 1_000L
    }
}
