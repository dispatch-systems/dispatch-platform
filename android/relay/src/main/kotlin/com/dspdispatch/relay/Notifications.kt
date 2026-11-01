package com.dspdispatch.relay

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import androidx.core.app.NotificationCompat

/** What the app shows in the phone's notifications. */
object Notifications {
    /** The relay's own, always there while it runs, as Android requires of a service that never stops. */
    const val RELAY_ID = 1
    private const val RELAY_CHANNEL = "relay"

    fun channels(context: Context) {
        context.getSystemService(NotificationManager::class.java).createNotificationChannel(
            NotificationChannel(RELAY_CHANNEL, context.getString(R.string.relay_channel), NotificationManager.IMPORTANCE_LOW),
        )
    }

    fun relay(context: Context, title: String, text: String) =
        NotificationCompat.Builder(context, RELAY_CHANNEL)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(title)
            .setContentText(text)
            .setOngoing(true)
            .setSilent(true)
            .setContentIntent(open(context))
            .build()

    private fun open(context: Context) = PendingIntent.getActivity(
        context,
        0,
        Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
        PendingIntent.FLAG_IMMUTABLE,
    )
}
