package com.dspdispatch.relay.texts

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
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

    @Test
    fun writesEachNumberAsDispatchDoes() {
        // Dispatch's own cases (numbers.rs).
        for ((raw, kept) in listOf(
            "(206) 555-0148" to "+12065550148",
            "206.555.0148" to "+12065550148",
            "+1 206 555 0148" to "+12065550148",
            "12065550148" to "+12065550148",
            "+44 20 7946 0958" to "+442079460958",
            "+2065550148" to "+2065550148",
            " 72166 " to "72166",
            "" to null,
            "call me" to null,
            "555-01" to null,
            "22065550148" to null,
        )) {
            assertEquals(raw, kept, Numbers.normal(raw))
        }
    }

    @Test
    fun knowsANumberHoweverItsWritten() {
        assertTrue(Numbers.same("+14255550199", "(425) 555-0199"))
        assertTrue(Numbers.same("+1 425 555 0199", "14255550199"))
        assertTrue(Numbers.same("72166", "72166"))
        assertFalse(Numbers.same("+14255550199", "+14255550198"))
        assertFalse(Numbers.same("72166", "+14255572166"))
        assertFalse(Numbers.same("", ""))
    }
}
