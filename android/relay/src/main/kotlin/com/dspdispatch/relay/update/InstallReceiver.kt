package com.dspdispatch.relay.update

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.os.Build
import androidx.core.content.IntentCompat
import com.dspdispatch.relay.relay

/** Hears what Android says of an install session, for the app's [SessionInstaller]. */
class InstallReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val id = intent.getIntExtra(PackageInstaller.EXTRA_SESSION_ID, NO_SESSION)
        val status = intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE)
        val confirmation = IntentCompat.getParcelableExtra(intent, Intent.EXTRA_INTENT, Intent::class.java)
        context.relay.installer.said(id, status, confirmation)
    }

    companion object {
        private const val NO_SESSION = -1

        /** Where Android tells of session [id]: this receiver, mutable for the extras Android adds. */
        fun statusOf(context: Context, id: Int): PendingIntent = PendingIntent.getBroadcast(
            context,
            id,
            Intent(context, InstallReceiver::class.java),
            PendingIntent.FLAG_UPDATE_CURRENT or if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) PendingIntent.FLAG_MUTABLE else 0,
        )
    }
}
