package com.dspdispatch.relay.texts

/** Who sent a text the phone sent: the app, for Dispatch; the phone's texting app; or not known yet. */
enum class Whose {
    DISPATCH,
    PHONE,

    /** The app sent a text to its number that Android hasn't said more of: read it again later. */
    UNSURE,
}

/**
 * Which of the phone's sent texts the app sent itself, for Dispatch: Dispatch has them already,
 * so they're never reported back as sent from the phone's texting app.
 */
fun interface Own {
    /** Who sent [message], a text the phone sent. */
    suspend fun whose(message: Message): Whose
}

/**
 * The texts the app sent, as Android wrote them into the provider for it, an app that isn't the
 * phone's texting app: by the row each send's result names; by the provider's own word, the
 * row's creator, when it's the app; or else by its number and words, near when the app sent one.
 */
class SentByApp(private val sends: Sends, private val app: String) : Own {
    override suspend fun whose(message: Message): Whose {
        val row = message.row
        if (sends.wrote(row.ref)) return Whose.DISPATCH
        // Sent to the same number about then, with no row of its own yet: each send claims one row.
        val unplaced = sends.unplaced(row.at - RESULT_WAIT_MS - SLACK_MS, row.at + SLACK_MS)
            .filter { send -> message.numbers.any { Numbers.same(it, send.number) } }
        val match = unplaced.firstOrNull { it.body == message.body }
        if (match != null || row.creator == app) {
            match?.let { sends.placed(it, row.ref) }
            return Whose.DISPATCH
        }
        return if (unplaced.any { it.outcome == null }) Whose.UNSURE else Whose.PHONE
    }

    private companion object {
        /** How far a row's time may be from its send's: a picture message's is in whole seconds. */
        const val SLACK_MS = 60_000L
    }
}
