package com.dspdispatch.relay.texts

import android.content.Context
import com.dspdispatch.relay.Phone
import androidx.room3.Room
import com.dspdispatch.relay.connection.keepRunning
import com.dspdispatch.relay.dispatch.Dispatch
import com.dspdispatch.relay.dispatch.Backlog
import com.dspdispatch.relay.dispatch.Outbox
import com.dspdispatch.relay.health.Lapses
import com.dspdispatch.relay.pairing.Paired
import java.io.File
import java.time.LocalDate
import java.time.ZoneId
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.merge
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.launch

/**
 * Relaying the phone's texts to its DSP: those it receives, and those sent from its texting app,
 * with their pictures, read into a queue kept on the phone and reported from it; and sending the
 * DSP's from the phone, telling it how each went.
 */
class Texts(private val context: Context, dispatch: Dispatch, gifs: Gifs, private val lapses: Lapses) {
    /** The running screen's switch: whether a text near Android's SMS limit goes as a picture message. */
    val mmsWhenBusy = MmsWhenBusy(context)

    private val database = Room.databaseBuilder<TextsDatabase>(context, TextsDatabase.NAME).build()
    private val queue = database.queue()
    private val sends = database.sends()
    private val provider = PhoneProvider(context.contentResolver)
    private val own = OwnRows(provider, sends, context.packageName)
    private val inbox = Inbox(provider, queue, SentByApp(sends, context.packageName), lapses)
    private val texting = Texting { textingAllowed(context) }
    private val reporter = Reporter(dispatch, queue, provider, texting, Jpegs(File(context.noBackupFilesDir, JPEGS), PhonePictures), lapses)
    private val statuses = Statuses(dispatch, sends, lapses::failed)
    private val fetches = Fetches(dispatch, queue, provider, texting)
    private val radio = PhoneRadio(context)
    private val sender = Sender(
        dispatch,
        sends,
        own,
        radio,
        PictureMessage(PhonePictures),
        gifs,
        statuses,
        File(context.cacheDir, SENDING),
        mmsWhenBusy.on::value,
        lapses,
    )

    // Android said how a text the app sent went: the row it wrote of it may be read for sure now.
    private val heard = Channel<Unit>(Channel.CONFLATED)
    private val reader = Reader(inbox, texting, merge(changes(context), heard.receiveAsFlow()))

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
     * waits at once, and reading the phone's texts once the phone allows it. Runs until cancelled,
     * each of its loops on through an error it didn't expect, the others going on meanwhile.
     */
    suspend fun run(paired: Paired, key: String): Unit = coroutineScope {
        inbox.begin(paired.phoneId, paired.at)
        val wake = Channel<Unit>(Channel.CONFLATED)
        launch { keepRunning("Reporting texts", lapses) { reporter.run(paired.site, key, wake) } }
        launch { keepRunning("Reporting statuses", lapses) { statuses.run(paired.site, key) } }
        launch { keepRunning("Sending texts", lapses) { sender.run(paired, key) } }
        launch { keepRunning("Sending videos", lapses) { fetches.run(paired.site, key) } }
        keepRunning("Reading texts", lapses) {
            reader.run({ radio.relayed(paired.number, paired.subscription) }) { wake.trySend(Unit) }
        }
    }

    /**
     * Sends what [outbox], Dispatch's answer to [paired]'s wait, holds, the videos it asks for
     * beside it, and returns once it's worth waiting again.
     */
    suspend fun send(paired: Paired, outbox: Outbox) {
        fetches.ask(outbox.wanted)
        sender.take(paired, outbox)
    }

    /** Whether Android lets the app send texts. */
    fun sending() = radio.allowed()

    /** The SIM [paired]'s texts go from, as Android names it; null where it names none. */
    fun line(paired: Paired) = subscription(Phone.subscriptions(context, paired.subscription), paired.number, Phone.textingSubscription())

    /** How many texts read, their files, and statuses wait to go to Dispatch. */
    suspend fun backlog(): Backlog = queue.backlog()

    /**
     * How many picture messages [paired]'s SIM received since it paired that the texting app hasn't
     * downloaded; null while the phone doesn't let the app read texts, or the provider can't say.
     */
    fun undownloaded(paired: Paired): Int? = try {
        if (textingAllowed(context)) provider.undownloaded(paired.at, radio.relayed(paired.number, paired.subscription)) else null
    } catch (_: SecurityException) {
        // Texting turned off between the check and the read.
        null
    } catch (_: Unavailable) {
        null
    }

    /** Holds sending back once no text is mid-send, for the app to be replaced by its update, until [release]d. */
    suspend fun hold() = sender.hold()

    /** Lets sending go on, if [hold] held it back. */
    fun release() = sender.release()

    /**
     * Keeps what Android said of a part of a text the app sent, as it says it, with the row at
     * [uri] it wrote the text to, where it says.
     */
    suspend fun heard(heard: Heard, uri: String?) {
        sender.heard(heard.copy(row = uri?.let { own.row(heard, it) }))
        this.heard.trySend(Unit)
    }

    private companion object {
        /** Where picture messages' files wait as they're sent (`mms_files.xml`). */
        const val SENDING = "sending"

        /** Where JPEGs made of pictures received as HEIF wait as they're reported and sent. */
        const val JPEGS = "jpegs"
    }
}
