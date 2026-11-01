package com.dspdispatch.relay.update

import android.content.Context
import androidx.core.content.edit
import com.dspdispatch.relay.Notifications
import com.dspdispatch.relay.dispatch.AppVersion

/** The notification of a newer version, in the phone's notifications; the one last notified kept in the app's storage. */
class UpdateNotice(private val context: Context) : Notice {
    private val prefs = context.getSharedPreferences(NAME, Context.MODE_PRIVATE)

    override var notified: Int
        get() = prefs.getInt(NOTIFIED, 0)
        set(versionCode) = prefs.edit { putInt(NOTIFIED, versionCode) }

    override fun show(app: AppVersion) = Notifications.update(context, app.version)

    override fun cancel() = Notifications.cancelUpdate(context)

    private companion object {
        const val NAME = "updates"
        const val NOTIFIED = "notified"
    }
}
