package com.dspdispatch.relay.update

import android.content.Context
import androidx.core.content.edit

/** The update held back from trying again, in the app's own storage, kept across restarts. */
class StoredHold(context: Context) : Holds {
    private val prefs = context.getSharedPreferences(NAME, Context.MODE_PRIVATE)

    override var hold: Hold?
        get() = if (prefs.contains(VERSION)) {
            Hold(prefs.getInt(VERSION, 0), prefs.getLong(UNTIL, NEVER).takeIf { it != NEVER }, prefs.getInt(TRIES, 1))
        } else {
            null
        }
        set(hold) = prefs.edit {
            if (hold == null) {
                remove(VERSION)
                remove(UNTIL)
                remove(TRIES)
            } else {
                putInt(VERSION, hold.versionCode)
                putLong(UNTIL, hold.until ?: NEVER)
                putInt(TRIES, hold.tries)
            }
        }

    private companion object {
        const val NAME = "updates"
        const val VERSION = "held_version"
        const val UNTIL = "held_until"
        const val TRIES = "held_tries"
        const val NEVER = -1L
    }
}
