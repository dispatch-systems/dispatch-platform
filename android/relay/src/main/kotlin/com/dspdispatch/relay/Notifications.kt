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

    /** A newer version of the app to install. */
    private const val UPDATE_ID = 2
    private const val UPDATES_CHANNEL = "updates"

    fun channels(context: Context) {
        context.getSystemService(NotificationManager::class.java).createNotificationChannels(
            listOf(
                NotificationChannel(RELAY_CHANNEL, context.getString(R.string.relay_channel), NotificationManager.IMPORTANCE_LOW),
                NotificationChannel(SETUP_CHANNEL, context.getString(R.string.setup_channel), NotificationManager.IMPORTANCE_DEFAULT),
                NotificationChannel(UPDATES_CHANNEL, context.getString(R.string.updates_channel), NotificationManager.IMPORTANCE_DEFAULT),
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
    fun undone(context: Context, step: Step): Boolean = post(context, SETUP_CHANNEL, UNDONE_ID + step.ordinal, context.getString(step.undone))

    /** Says [version] of the app is there to install, where the app may notify. */
    fun update(context: Context, version: String) {
        post(context, UPDATES_CHANNEL, UPDATE_ID, context.getString(R.string.update_version, version))
    }

    /** Takes away the notification of a newer version, if there is one. */
    fun cancelUpdate(context: Context) = NotificationManagerCompat.from(context).cancel(UPDATE_ID)

    /** Posts [title] on [channel] as notification [id], opening the app, where the app may notify there; whether it did. */
    private fun post(context: Context, channel: String, id: Int, title: String): Boolean {
        val manager = NotificationManagerCompat.from(context)
        val allowed = Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ||
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED
        if (!allowed || !manager.areNotificationsEnabled() || isBlocked(context, channel)) return false
        val notification = NotificationCompat.Builder(context, channel)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(title)
            .setAutoCancel(true)
            .setContentIntent(open(context))
            .build()
        manager.notify(id, notification)
        return true
    }

    /** The app's channels the person has blocked, of the relay's and Set up's, by ID. */
    fun blocked(context: Context): List<String> = listOf(RELAY_CHANNEL, SETUP_CHANNEL).filter { isBlocked(context, it) }

    private fun isBlocked(context: Context, channel: String) =
        context.getSystemService(NotificationManager::class.java).getNotificationChannel(channel)?.importance ==
            NotificationManager.IMPORTANCE_NONE

    /** Takes away the notification of [step] undone, if there is one. */
    fun cancelUndone(context: Context, step: Step) = NotificationManagerCompat.from(context).cancel(UNDONE_ID + step.ordinal)

    private fun open(context: Context) = PendingIntent.getActivity(
        context,
        0,
        Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
        PendingIntent.FLAG_IMMUTABLE,
    )
}
