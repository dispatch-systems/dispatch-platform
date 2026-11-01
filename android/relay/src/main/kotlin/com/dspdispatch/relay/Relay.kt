package com.dspdispatch.relay

import android.content.Context
import com.dspdispatch.relay.connection.Connection
import com.dspdispatch.relay.dispatch.Dispatch
import com.dspdispatch.relay.dispatch.PairRequest
import com.dspdispatch.relay.pairing.Paired
import com.dspdispatch.relay.pairing.Pairings
import com.dspdispatch.relay.pairing.PairingLink
import java.net.InetAddress
import okhttp3.Dns
import okhttp3.OkHttpClient

/** The app's parts, one of each for as long as the app runs (`RelayApp`). */
class Relay(private val context: Context) {
    val pairings = Pairings(context)
    val dispatch = Dispatch(http())
    val connection = Connection(dispatch)

    /** Pairs this phone with the DSP [link] names; throws as `Dispatch.pair` does. */
    suspend fun pair(link: PairingLink): Paired {
        val answer = dispatch.pair(
            link.site,
            PairRequest(link.code, Phone.number(context), Phone.model, BuildConfig.VERSION_NAME),
        )
        val paired = Paired(link.site.trimEnd('/'), answer.dsp.name, answer.phone.id, answer.phone.name)
        pairings.save(paired, answer.key)
        connection.paired()
        RelayService.start(context)
        return paired
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
