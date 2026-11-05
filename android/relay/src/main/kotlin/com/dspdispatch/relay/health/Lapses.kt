package com.dspdispatch.relay.health

import android.content.Context
import androidx.core.content.edit
import com.dspdispatch.relay.connection.Logcat
import com.dspdispatch.relay.connection.Unexpected
import com.dspdispatch.relay.dispatch.Lapse

/**
 * What last went wrong on the phone, for its health report: why a text last failed to send, and
 * the last error the relay didn't expect, told to the phone's log too. Kept in the app's own
 * storage, so a restart doesn't lose them; never a text's words or a number, but the app's own.
 */
class Lapses(context: Context) : Unexpected {
    private val prefs = context.getSharedPreferences("lapses", Context.MODE_PRIVATE)

    /** Why a text last failed to send, and when. */
    val sendError: Lapse? get() = read(SEND_ERROR)

    /** The last error the relay didn't expect: its class, what it was doing, and when. */
    val unexpected: Lapse? get() = read(UNEXPECTED)

    override fun happened(doing: String, error: Throwable) {
        Logcat.happened(doing, error)
        val name = error.javaClass.name.filter { it in WORDS || it in '0'..'9' || it in "._$" }
        keep(UNEXPECTED, name.take(CLASS_MOST), doing.filter { it in WORDS || it == ' ' }.take(DOING_MOST))
    }

    /** A text failed to send at [at], for [error], as Dispatch is told it. */
    fun failed(error: String, at: Long) = keep(SEND_ERROR, error.filter { it in ' '..'~' }.take(ERROR_MOST), null, at)

    private fun keep(key: String, error: String, doing: String?, at: Long = System.currentTimeMillis()) = prefs.edit {
        putString("$key.error", error)
        putString("$key.where", doing)
        putLong("$key.at", at)
    }

    private fun read(key: String): Lapse? {
        val error = prefs.getString("$key.error", null) ?: return null
        return Lapse(error, prefs.getLong("$key.at", 0), prefs.getString("$key.where", null))
    }

    private companion object {
        const val SEND_ERROR = "send_error"
        const val UNEXPECTED = "unexpected"

        // As Dispatch holds them (`ERROR_MOST`, `CLASS_MOST`, `DOING_MOST`): a class by its name's
        // letters, digits, `.`, `_` and `$`, and what the app was doing in letters and spaces
        // alone, so a number never goes with it.
        const val ERROR_MOST = 500
        const val CLASS_MOST = 200
        const val DOING_MOST = 100
        val WORDS = ('a'..'z') + ('A'..'Z')
    }
}
