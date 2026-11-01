package com.dspdispatch.relay.setup

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import com.dspdispatch.relay.RelayService
import com.dspdispatch.relay.pairing.Standing
import com.dspdispatch.relay.relay

/**
 * Starts the relay again when the phone starts and when the app was updated, while paired. Both
 * let an app start its foreground service from the background, and Android 15's limits on
 * starting one at boot leave out the relay's remote messaging type. Accepted limitation: the
 * relay starts at the first unlock after a reboot, as neither it nor its storage is Direct Boot aware.
 */
class StartReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action !in STARTS) return
        if (context.relay.pairings.standing.value is Standing.Pairing) RelayService.start(context)
    }

    private companion object {
        val STARTS = setOf(Intent.ACTION_BOOT_COMPLETED, Intent.ACTION_MY_PACKAGE_REPLACED)
    }
}
