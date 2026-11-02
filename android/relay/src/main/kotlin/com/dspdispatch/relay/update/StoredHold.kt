package com.dspdispatch.relay.update

import android.content.Context
import androidx.core.content.edit

/** The update held back from trying again, in the app's own storage, kept across restarts. */
class StoredHold(context: Context) : Holds {
    private val prefs = context.getSharedPreferences(NAME, Context.MODE_PRIVATE)

    override var hold: Hold?
        get() = if (prefs.contains(VERSION)) Hold(prefs.getInt(VERSION, 0), prefs.getLong(UNTIL, NEVER).takeIf { it != NEVER }) else null
        set(hold) = prefs.edit {
            if (hold == null) {
                remove(VERSION)
                remove(UNTIL)
            } else {
                putInt(VERSION, hold.versionCode)
                putLong(UNTIL, hold.until ?: NEVER)
            }
        }

    private companion object {
        const val NAME = "updates"
        const val VERSION = "held_version"
        const val UNTIL = "held_until"
        const val NEVER = -1L
    }
}
