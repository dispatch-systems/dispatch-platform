package com.dspdispatch.relay.texts

import android.content.Context
import android.database.sqlite.SQLiteException
import androidx.room3.Room
import com.dspdispatch.relay.dispatch.Dispatch
import com.dspdispatch.relay.dispatch.Outbox
import com.dspdispatch.relay.pairing.Paired
import java.io.File
import java.time.LocalDate
import java.time.ZoneId
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.conflate
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.merge
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Relaying the phone's texts to its DSP: those it receives, and those sent from its texting app,
 * with their pictures, read into a queue kept on the phone and reported from it; and sending the
 * DSP's from the phone, telling it how each went.
 */
class Texts(private val context: Context, dispatch: Dispatch) {
    private val database = Room.databaseBuilder<TextsDatabase>(context, TextsDatabase.NAME).build()
    private val queue = database.queue()
    private val sends = database.sends()
    private val provider = PhoneProvider(context.contentResolver)
    private val inbox = Inbox(provider, queue, SentByApp(sends, context.packageName))
    private val reporter = Reporter(dispatch, queue, provider)
    private val statuses = Statuses(dispatch, sends)
    private val sender = Sender(
        dispatch,
        sends,
        PhoneRadio(context),
        PictureMessage(PhonePictures),
        statuses,
        File(context.cacheDir, SENDING),
    )

    // Android said how a text the app sent went: the row it wrote of it may be read for sure now.
    private val heard = Channel<Unit>(Channel.CONFLATED)

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
        inbox.begin(paired.phoneId)
        val wake = Channel<Unit>(Channel.CONFLATED)
        launch { reporter.run(paired.site, key, wake) }
        launch { statuses.run(paired.site, key) }
        launch { sender.run(paired, key) }
        // The setup asks for Texting; the relay runs from pairing, so it waits for it here.
        while (!textingAllowed(context)) delay(ALLOWED_CHECK_MS)
        merge(changes(context), heard.receiveAsFlow()).conflate().collect {
            // The texting app writes a text in steps: read once it's done.
            delay(SETTLE_MS)
            try {
                withContext(Dispatchers.IO) { inbox.read() }
                wake.trySend(Unit)
            } catch (_: SecurityException) {
                // Texting was turned off, which ends the app; nothing to read until it's back.
            } catch (_: SQLiteException) {
                // The provider failed this once; its next change reads again.
            }
        }
    }

    /**
     * Sends what [outbox], Dispatch's answer to [paired]'s wait, holds, and returns once it's worth
     * waiting again.
     */
    suspend fun send(paired: Paired, outbox: Outbox) = sender.take(paired, outbox)

    /** Keeps what Android said of a part of a text the app sent, as it says it. */
    suspend fun heard(heard: Heard) {
        sender.heard(heard)
        this.heard.trySend(Unit)
    }

    private companion object {
        const val ALLOWED_CHECK_MS = 5_000L
        const val SETTLE_MS = 1_000L

        /** Where picture messages' files wait as they're sent (`mms_files.xml`). */
        const val SENDING = "sending"
    }
}
