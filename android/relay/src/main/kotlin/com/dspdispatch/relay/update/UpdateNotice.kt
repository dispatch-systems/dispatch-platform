package com.dspdispatch.relay.update

import android.content.Context
import com.dspdispatch.relay.Notifications
import com.dspdispatch.relay.dispatch.AppVersion

/** The notification of a newer version, in the phone's notifications. */
class UpdateNotice(private val context: Context) : Notice {
    override fun show(app: AppVersion) = Notifications.update(context, app.version)

    override fun cancel() = Notifications.cancelUpdate(context)
}
