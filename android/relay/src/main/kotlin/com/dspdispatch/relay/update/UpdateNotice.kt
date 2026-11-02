package com.dspdispatch.relay.update

import android.content.Context
import androidx.core.content.edit
import com.dspdispatch.relay.Notifications
import com.dspdispatch.relay.dispatch.AppVersion

/** The notification asking to confirm an update, in the phone's notifications; the version it asks about kept in the app's storage. */
class UpdateNotice(private val context: Context) : Notice {
    private val prefs = context.getSharedPreferences(NAME, Context.MODE_PRIVATE)

    override var showing: Int?
        get() = if (prefs.contains(SHOWING)) prefs.getInt(SHOWING, 0) else null
        set(versionCode) = prefs.edit { if (versionCode == null) remove(SHOWING) else putInt(SHOWING, versionCode) }

    override fun show(app: AppVersion) = Notifications.update(context, app.version)

    override fun cancel() = Notifications.cancelUpdate(context)

    private companion object {
        const val NAME = "updates"
        const val SHOWING = "showing_version"
    }
}
