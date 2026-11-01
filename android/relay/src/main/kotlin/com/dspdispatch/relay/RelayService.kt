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
import com.dspdispatch.relay.setup.Watchdog
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch

/**
 * The relay, running for as long as the phone is paired: Android's own kind of service for
 * relaying text messages, which it keeps running beside its notification.
 */
class RelayService : LifecycleService() {
    private var relaying: Job? = null

    override fun onCreate() {
        super.onCreate()
        running = true
    }

    override fun onDestroy() {
        running = false
        super.onDestroy()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        super.onStartCommand(intent, flags, startId)
        val paired = (relay.pairings.standing.value as? Standing.Pairing)?.paired
        val key = relay.pairings.key()
        if (paired == null || key == null) {
            stopSelf()
            return START_NOT_STICKY
        }
        show(paired.dsp, relay.connection.link.value)
        Watchdog.schedule(this)
        if (relaying?.isActive != true) {
            relaying = lifecycleScope.launch {
                launch { relay.connection.link.collect { show(paired.dsp, it) } }
                launch { relay.texts.run(paired, key) }
                relay.connection.run(paired, key) { outbox ->
                    relay.updates.heard(outbox.app)
                    relay.texts.send(paired, outbox)
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
        private val TYPE = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            ServiceInfo.FOREGROUND_SERVICE_TYPE_REMOTE_MESSAGING
        } else {
            0
        }

        /** Whether the relay is running in this process. */
        @Volatile
        var running = false
            private set

        /**
         * Starts the relay, or leaves it running. From the background, Android 12+ lets an app start
         * its foreground service only in the cases it lists, among them boot, the app's update and
         * the battery left unrestricted: elsewhere the start is refused, and Set up's battery step
         * says so.
         */
        fun start(context: Context) {
            try {
                ContextCompat.startForegroundService(context, Intent(context, RelayService::class.java))
            } catch (_: IllegalStateException) {
                // Android 12+'s ForegroundServiceStartNotAllowedException, which older phones
                // don't know: refused from the background, to try again at the watchdog's next run.
            }
        }
    }
}
