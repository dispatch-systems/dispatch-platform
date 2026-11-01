package com.dspdispatch.relay.connection

import kotlin.random.Random
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class BackoffTest {
    /** A "random" that always picks the longest jitter it may. */
    private val longest = object : Random() {
        override fun nextBits(bitCount: Int) = 0
        override fun nextLong(until: Long) = until - 1
    }
    private val none = object : Random() {
        override fun nextBits(bitCount: Int) = 0
        override fun nextLong(until: Long) = 0L
    }

    @Test
    fun doublesFromASecondToAMinute() {
        assertEquals(listOf(1_000L, 2_000L, 4_000L, 8_000L, 16_000L, 32_000L, 60_000L, 60_000L), (1..8).map { backoff(it, none) })
        assertEquals(60_000L, backoff(1_000, none))
    }

    @Test
    fun isAtMostAFifthShorterAtRandom() {
        assertEquals(800L, backoff(1, longest))
        assertEquals(48_000L, backoff(9, longest))
        repeat(100) { assertTrue(backoff(3) in 3_200L..4_000L) }
    }
}
