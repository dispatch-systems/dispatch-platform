package com.dspdispatch.relay

import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import androidx.lifecycle.LifecycleService
import androidx.lifecycle.lifecycleScope
import com.dspdispatch.relay.connection.Link
import com.dspdispatch.relay.pairing.Standing
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * The relay, running for as long as the phone is paired: Android's own kind of service for
 * relaying text messages, which it keeps running beside its notification.
 */
class RelayService : LifecycleService() {
    private var running: Job? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        super.onStartCommand(intent, flags, startId)
        val paired = (relay.pairings.standing.value as? Standing.Pairing)?.paired
        val key = relay.pairings.key()
        if (paired == null || key == null) {
            stopSelf()
            return START_NOT_STICKY
        }
        show(paired.dsp, relay.connection.link.value)
        if (running?.isActive != true) {
            running = lifecycleScope.launch {
                launch { relay.connection.link.collect { show(paired.dsp, it) } }
                relay.connection.run(paired, key) { outbox ->
                    // Sending Dispatch's texts comes with the outbox's sender. Until then a text
                    // waiting stays in each answer, which comes at once: pause, well within the
                    // two minutes that keep the phone online, instead of asking again at once.
                    if (outbox.texts.isNotEmpty()) delay(HELD_MS)
                }
                relay.pairings.removed(paired)
                stopSelf()
            }
        }
        return START_STICKY
    }

    private fun show(dsp: String, link: Link) {
        val state = getString(
            when (link) {
                is Link.Connected -> R.string.connected
                is Link.Reconnecting -> R.string.reconnecting
                Link.Connecting -> R.string.connecting
            },
        )
        ServiceCompat.startForeground(this, Notifications.RELAY_ID, Notifications.relay(this, dsp, state), TYPE)
    }

    companion object {
        private const val HELD_MS = 30_000L
        private val TYPE = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            ServiceInfo.FOREGROUND_SERVICE_TYPE_REMOTE_MESSAGING
        } else {
            0
        }

        fun start(context: Context) = ContextCompat.startForegroundService(context, Intent(context, RelayService::class.java))
    }
}
