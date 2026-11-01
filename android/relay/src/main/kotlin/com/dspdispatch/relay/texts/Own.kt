package com.dspdispatch.relay.texts

/**
 * Which of the phone's sent texts the app sent itself, for Dispatch: Dispatch has them already,
 * so they're never reported back as sent from the phone's texting app.
 */
fun interface Own {
    /** Whether the app sent the text [row] holds. */
    fun sent(row: Row): Boolean

    companion object {
        /** Until the app sends texts for Dispatch: none of the phone's texts are its own. */
        val NONE = Own { false }
    }
}
