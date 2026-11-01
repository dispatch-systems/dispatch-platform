package com.dspdispatch.relay.texts

import org.junit.Assert.assertEquals
import org.junit.Test

class NumbersTest {
    @Test
    fun takesWhatDispatchTakes() {
        // Dispatch's own cases (numbers.rs), and those its rule falls through to.
        for ((raw, taken) in listOf(
            "(206) 555-0148" to true,
            "206.555.0148" to true,
            "+1 206 555 0148" to true,
            "12065550148" to true,
            "+44 20 7946 0958" to true,
            "+2065550148" to true,
            " 72166 " to true,
            "72166" to true,
            "" to false,
            "call me" to false,
            "555-01" to false,
            "Carrier" to false,
            "22065550148" to false,
            "+1 (206) 555-0148 ext 12" to false,
            "1".repeat(33) to false,
        )) {
            assertEquals(raw, taken, Numbers.accepted(raw))
        }
    }
}
