package com.dspdispatch.relay.health

import android.content.Context
import android.provider.Telephony
import com.dspdispatch.relay.Phone
import com.dspdispatch.relay.dispatch.Health
import com.dspdispatch.relay.pairing.Paired
import com.dspdispatch.relay.setup.Item
import com.dspdispatch.relay.setup.Setup
import com.dspdispatch.relay.texts.Texts
import com.dspdispatch.relay.texts.textingAllowed
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * How the phone stands, as it says with each wait on its outbox: Set up's steps as last checked,
 * texting, its SIM, what waits on it, and what last went wrong ([lapses]).
 */
class Checkup(private val context: Context, private val setup: Setup, private val texts: Texts, private val lapses: Lapses) {
    /**
     * How the phone paired as [paired] stands now, whether or not the phone lets the app read its
     * texts; null where reading it fails in a way nothing expected, told to [lapses]: the wait goes
     * on without it.
     */
    suspend fun now(paired: Paired): Health? = try {
        val steps = setup.items.value ?: setup.check()
        withContext(Dispatchers.IO) {
            Health(
                setup = steps.filterNot(Item::done).map { it.step.name.lowercase() },
                read = textingAllowed(context),
                send = texts.sending(),
                sim = Phone.sim(context, texts.line(paired)),
                waiting = texts.backlog(),
                undownloaded = texts.undownloaded(paired),
                textingApp = textingApp(context),
                sendError = lapses.sendError,
                unexpected = lapses.unexpected,
            )
        }
    } catch (cancelled: CancellationException) {
        throw cancelled
    } catch (failed: RuntimeException) {
        lapses.happened("Checking the phone", failed)
        null
    }
}

/**
 * The phone's default texting app, by its package, as Dispatch holds it (`APP_MOST`); null where
 * it has none.
 */
private fun textingApp(context: Context): String? =
    Telephony.Sms.getDefaultSmsPackage(context)?.filter { (it.isLetterOrDigit() && it < '\u0080') || it == '.' || it == '_' }?.take(APP_MOST)

private const val APP_MOST = 200
