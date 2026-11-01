package com.dspdispatch.relay

import android.content.Context
import androidx.annotation.StringRes
import com.dspdispatch.relay.connection.Connection
import com.dspdispatch.relay.dispatch.Dispatch
import com.dspdispatch.relay.dispatch.PairRequest
import com.dspdispatch.relay.dispatch.Refusal
import com.dspdispatch.relay.pairing.Paired
import com.dspdispatch.relay.pairing.PairingLink
import com.dspdispatch.relay.pairing.Pairings
import com.dspdispatch.relay.texts.Texts
import java.io.IOException
import java.net.InetAddress
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.serialization.SerializationException
import okhttp3.Dns
import okhttp3.OkHttpClient

/** Where pairing stands: none under way, under way, or the last one's problem. */
sealed interface Attempt {
    data object None : Attempt

    data object Pairing : Attempt

    data class Failed(@param:StringRes val problem: Int) : Attempt
}

/** The app's parts, one of each for as long as the app runs (`RelayApp`). */
class Relay(private val context: Context) {
    val pairings = Pairings(context)
    val dispatch = Dispatch(http())
    val connection = Connection(dispatch)
    val texts = Texts(context, dispatch)

    // Pairing outlives the screen that started it: a code works once, so a pairing cancelled
    // after Dispatch took it would leave a phone no app holds the key of.
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val attempt = MutableStateFlow<Attempt>(Attempt.None)
    val attempting: StateFlow<Attempt> = attempt.asStateFlow()

    /** The pairing link [text] holds, if it's one this build pairs with; else says why not. */
    fun read(text: String): PairingLink? {
        val link = PairingLink.parse(text)
        val problem = when {
            link == null -> R.string.pairing_not_a_code
            !Server.THIS_BUILD.accepts(link.site) -> R.string.pairing_other_server
            else -> return link
        }
        attempt.value = Attempt.Failed(problem)
        return null
    }

    /** Leaves the last pairing's problem behind, as scanning again does. */
    fun again() {
        if (attempt.value is Attempt.Failed) attempt.value = Attempt.None
    }

    /** Pairs this phone with the DSP [link] names, unless a pairing is already under way. */
    fun pair(link: PairingLink) {
        if (attempt.value == Attempt.Pairing) return
        attempt.value = Attempt.Pairing
        scope.launch {
            attempt.value = try {
                paired(link)
                Attempt.None
            } catch (refusal: Refusal) {
                Attempt.Failed(if (refusal.code == Refusal.PAIRING_INVALID) R.string.pairing_invalid else R.string.pairing_refused)
            } catch (_: IOException) {
                Attempt.Failed(R.string.pairing_unreachable)
            } catch (_: SerializationException) {
                Attempt.Failed(R.string.pairing_unreachable)
            }
        }
    }

    private suspend fun paired(link: PairingLink) {
        val answer = dispatch.pair(
            link.site,
            PairRequest(link.code, Phone.number(context), Phone.model, BuildConfig.VERSION_NAME),
        )
        pairings.save(Paired(link.site.trimEnd('/'), answer.dsp.name, answer.phone.id, answer.phone.name), answer.key)
        connection.paired()
        RelayService.start(context)
    }

    private fun http(): OkHttpClient = OkHttpClient.Builder().apply {
        // A debug build reaches a development server on the computer it's attached to, its DSPs
        // at <code>.localhost, through `adb reverse`.
        if (BuildConfig.DEBUG) dns(LocalDns)
    }.build()

    private object LocalDns : Dns {
        override fun lookup(hostname: String): List<InetAddress> =
            if (hostname.endsWith(".${Server.LOCAL}")) listOf(InetAddress.getLoopbackAddress()) else Dns.SYSTEM.lookup(hostname)
    }
}
