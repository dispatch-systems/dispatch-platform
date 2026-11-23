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

    /** A North American number's digits but its country code. */
    private const val NATIONAL = 10

    /** Whether Dispatch takes [raw], as the provider writes it, as a number. */
    fun accepted(raw: String): Boolean = normal(raw) != null

    /**
     * [raw] as Dispatch writes it: `+1` and ten digits in North America, `+` and its digits with
     * its country code, a short code's digits alone; null for one Dispatch doesn't take.
     */
    fun normal(raw: String): String? {
        val number = raw.trim()
        if (number.isEmpty() || number.length > MOST) return null
        if (!number.all { it in '0'..'9' || it in PUNCTUATION }) return null
        val plus = number.startsWith('+')
        val bare = number.all { it in '0'..'9' }
        val digits = number.filter { it in '0'..'9' }
        return when {
            digits.length == 10 && !plus -> "+1$digits"
            digits.length == 11 && digits.first() == '1' -> "+$digits"
            digits.length in 8..15 && plus -> "+$digits"
            digits.length in 3..7 && bare -> digits
            else -> null
        }
    }

    /**
     * Whether [a] and [b] are the same number, however each is written: their last ten digits, as
     * one may carry its country code and the other not, or all of a shorter one's.
     */
    fun same(a: String, b: String): Boolean {
        val (first, second) = digitsOf(a) to digitsOf(b)
        if (first.isEmpty()) return false
        return if (first.length >= NATIONAL && second.length >= NATIONAL) first.takeLast(NATIONAL) == second.takeLast(NATIONAL) else first == second
    }
    private fun digitsOf(number: String) = number.filter { it in '0'..'9' }
}
