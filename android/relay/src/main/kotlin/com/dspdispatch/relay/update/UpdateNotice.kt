package com.dspdispatch.relay.update

import android.content.Context
import androidx.core.content.edit
import com.dspdispatch.relay.Notifications
import com.dspdispatch.relay.dispatch.AppVersion

/** The notification of a newer version, in the phone's notifications; those notified kept in the app's storage. */
class UpdateNotice(private val context: Context) : Notice {
    private val prefs = context.getSharedPreferences(NAME, Context.MODE_PRIVATE)

    override var notified: Set<Int>
        get() = prefs.getStringSet(NOTIFIED, emptySet()).orEmpty().mapNotNull(String::toIntOrNull).toSet()
        set(versionCodes) = prefs.edit { putStringSet(NOTIFIED, versionCodes.map(Int::toString).toSet()) }

    override var showing: Int?
        get() = if (prefs.contains(SHOWING)) prefs.getInt(SHOWING, 0) else null
        set(versionCode) = prefs.edit { if (versionCode == null) remove(SHOWING) else putInt(SHOWING, versionCode) }

    override fun show(app: AppVersion) = Notifications.update(context, app.version)

    override fun cancel() = Notifications.cancelUpdate(context)

    private companion object {
        const val NAME = "updates"
        const val NOTIFIED = "notified_versions"
        const val SHOWING = "showing_version"
    }
}
