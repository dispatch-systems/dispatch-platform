package com.dspdispatch.relay

import android.app.Application
import android.content.Context

class RelayApp : Application() {
    val relay by lazy { Relay(this) }

    override fun onCreate() {
        super.onCreate()
        Notifications.channels(this)
    }
}

/** The app's parts, from anything of it. */
val Context.relay: Relay get() = (applicationContext as RelayApp).relay
