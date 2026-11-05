package com.dspdispatch.relay.connection

import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

class UnexpectedTest {
    private val told = mutableListOf<Pair<String, Throwable>>()
    private val unexpected = Unexpected { doing, error -> told += doing to error }

    @Test
    fun runsALoopAgainAfterAnErrorNothingExpectedTellingIt() = runTest {
        var runs = 0
        val ended = keepRunning("Reporting texts", unexpected) {
            if (++runs < 3) throw IllegalStateException("database or disk is full")
            "ended"
        }
        assertEquals("ended", ended)
        assertEquals(List(2) { "Reporting texts" }, told.map { it.first })
        assertTrue(told.all { it.second is IllegalStateException })
    }

    @Test
    fun stopsAsItsCoroutineIsCancelled() = runTest {
        try {
            keepRunning("Reporting texts", unexpected) { throw CancellationException("stopped") }
            fail("Ran again")
        } catch (_: CancellationException) {
        }
        assertTrue(told.isEmpty())
    }
}
