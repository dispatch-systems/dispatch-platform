package com.dspdispatch.relay

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import com.dspdispatch.relay.setup.Step

/** What the app shows in the phone's notifications. */
object Notifications {
    /** The relay's own, always there while it runs, as Android requires of a service that never stops. */
    const val RELAY_ID = 1
    private const val RELAY_CHANNEL = "relay"

    /** A step of Set up undone while the relay runs, one notification each, after the relay's. */
    private const val UNDONE_ID = 100
    private const val SETUP_CHANNEL = "setup"

    fun channels(context: Context) {
        context.getSystemService(NotificationManager::class.java).createNotificationChannels(
            listOf(
                NotificationChannel(RELAY_CHANNEL, context.getString(R.string.relay_channel), NotificationManager.IMPORTANCE_LOW),
                NotificationChannel(SETUP_CHANNEL, context.getString(R.string.setup_channel), NotificationManager.IMPORTANCE_DEFAULT),
            ),
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

    /** Says [step] was undone, where the app may notify on Set up's channel; whether it did. */
    fun undone(context: Context, step: Step): Boolean {
        val manager = NotificationManagerCompat.from(context)
        val allowed = Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ||
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED
        if (!allowed || !manager.areNotificationsEnabled() || SETUP_CHANNEL in blocked(context)) return false
        val notification = NotificationCompat.Builder(context, SETUP_CHANNEL)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(context.getString(step.undone))
            .setAutoCancel(true)
            .setContentIntent(open(context))
            .build()
        manager.notify(UNDONE_ID + step.ordinal, notification)
        return true
    }

    /** The app's channels the person has blocked, of the relay's and Set up's, by ID. */
    fun blocked(context: Context): List<String> {
        val manager = context.getSystemService(NotificationManager::class.java)
        return listOf(RELAY_CHANNEL, SETUP_CHANNEL).filter { manager.getNotificationChannel(it)?.importance == NotificationManager.IMPORTANCE_NONE }
    }

    /** Takes away the notification of [step] undone, if there is one. */
    fun cancelUndone(context: Context, step: Step) = NotificationManagerCompat.from(context).cancel(UNDONE_ID + step.ordinal)

    private fun open(context: Context) = PendingIntent.getActivity(
        context,
        0,
        Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
        PendingIntent.FLAG_IMMUTABLE,
    )
}
