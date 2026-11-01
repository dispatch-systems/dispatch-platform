package com.dspdispatch.relay.texts

/**
 * Which numbers Dispatch takes as a text's other party, as its `numbers::normal` has it: a number
 * in North America, one with its country code, or a short code. Dispatch refuses a report holding
 * any other, as an alphanumeric sender's name, so such a text is never reported.
 */
object Numbers {
    /** The most characters Dispatch takes a number written with (`NUMBER_MOST`). */
    private const val MOST = 32
    private const val PUNCTUATION = " +-()."

    /** Whether Dispatch takes [raw], as the provider writes it, as a number. */
    fun accepted(raw: String): Boolean {
        val number = raw.trim()
        if (number.isEmpty() || number.length > MOST) return false
        if (!number.all { it in '0'..'9' || it in PUNCTUATION }) return false
        val plus = number.startsWith('+')
        val bare = number.all { it in '0'..'9' }
        val digits = number.count { it in '0'..'9' }
        return (digits == 10 && !plus) ||
            (digits == 11 && number.first { it in '0'..'9' } == '1') ||
            (digits in 8..15 && plus) ||
            (digits in 3..7 && bare)
    }
}
