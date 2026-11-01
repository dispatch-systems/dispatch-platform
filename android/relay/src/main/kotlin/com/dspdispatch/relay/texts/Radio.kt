package com.dspdispatch.relay.texts

import java.io.File

/** Android's texting service, which sends the outbox's texts from the phone's own number. */
interface Radio {
    /** Whether the phone lets the app send texts. */
    fun allowed(): Boolean

    /**
     * The line to send from: the SIM whose number is [number], the phone's as it paired on
     * [subscription].
     */
    fun line(number: String, subscription: Int): Line
}

/**
 * One of the phone's SIMs, as its carrier sends texts. Each send's results reach the app later,
 * by [Send]'s ID and attempt and each part's number: Android never answers at once.
 */
interface Line {
    /** [body] in the parts the carrier sends it in. */
    fun divide(body: String): List<String>

    /** The largest picture message the carrier takes, in bytes, or null when it takes none. */
    fun mmsMost(): Int?

    /** Hands Android [parts] of [send] to send as a text, each reported sent and delivered. */
    fun sms(send: Send, parts: List<String>)

    /** Hands Android the picture message of [send], its M-Send.req in [pdu], reported sent. */
    fun mms(send: Send, pdu: File)
}

/**
 * Which of the phone's [subscriptions], each with its number, to text from: the one whose number
 * is [number], else Android's [default] for texts, else any, as a phone with no default asks the
 * person holding it each time. Null when the phone knows none, to send as Android does by itself.
 */
internal fun subscription(subscriptions: Map<Int, String>, number: String, default: Int): Int? =
    subscriptions.entries.firstOrNull { number.isNotBlank() && Numbers.same(it.value, number) }?.key
        ?: default.takeIf { it in subscriptions || (subscriptions.isEmpty() && it >= 0) }
        ?: subscriptions.keys.firstOrNull()
