package com.dspdispatch.relay.setup

import org.junit.Assert.assertEquals
import org.junit.Test

class NotifiedTest {
    private val posted = mutableListOf<Step>()
    private val canPost = { step: Step -> posted.add(step) }
    private val cannotPost = { _: Step -> false }

    @Test
    fun `keeps a step not posted out, and posts it once the app may notify`() {
        val denied = notified(emptySet(), setOf(Step.BATTERY), cannotPost)
        assertEquals(emptySet<Step>(), denied)
        assertEquals(setOf(Step.BATTERY), notified(denied, setOf(Step.BATTERY), canPost))
        assertEquals(listOf(Step.BATTERY), posted)
    }

    @Test
    fun `posts a step once while it stays undone`() {
        val once = notified(emptySet(), setOf(Step.BATTERY), canPost)
        assertEquals(setOf(Step.BATTERY), notified(once, setOf(Step.BATTERY), canPost))
        assertEquals(listOf(Step.BATTERY), posted)
    }

    @Test
    fun `forgets a step done again on a check that doesn't notify, and posts it when undone again`() {
        val once = notified(emptySet(), setOf(Step.BATTERY), canPost)
        val repaired = notified(once, emptySet(), null)
        assertEquals(emptySet<Step>(), repaired)
        assertEquals(setOf(Step.BATTERY), notified(repaired, setOf(Step.BATTERY), canPost))
        assertEquals(listOf(Step.BATTERY, Step.BATTERY), posted)
    }

    @Test
    fun `posts nothing on a check that doesn't notify`() {
        assertEquals(emptySet<Step>(), notified(emptySet(), setOf(Step.BATTERY), null))
        assertEquals(emptyList<Step>(), posted)
    }
}
