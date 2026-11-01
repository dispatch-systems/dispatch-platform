package com.dspdispatch.relay.setup

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class WalkTest {
    private fun done(step: Step) = Item(step, true, emptyList())
    private fun undone(step: Step, vararg fixes: Fix) = Item(step, false, fixes.toList())

    /** A phone on Android 15+, the app installed from a file, before texting is allowed. */
    private val restrictedPhone = listOf(
        undone(Step.TEXTING, Fix.ASK_TEXTING),
        done(Step.PHONE_NUMBER),
        undone(Step.BATTERY, Fix.BATTERY_REQUEST),
        undone(Step.RESTRICTED_SETTINGS, Fix.APP_INFO),
        undone(Step.CHAT_FEATURES, Fix.TEXTING_APP),
    )

    private fun started(items: List<Item>) = Walk().next(items)!!

    @Test
    fun `walks only the steps undone, in Set up's order, leaving Restricted settings out`() {
        val walk = started(restrictedPhone)
        assertEquals(listOf(Step.TEXTING, Step.BATTERY, Step.CHAT_FEATURES), walk.steps)
        assertEquals(Place(restrictedPhone[0], 1, 3), walk.at(restrictedPhone))
    }

    @Test
    fun `ends at once where nothing is undone`() {
        assertNull(Walk().next(listOf(done(Step.TEXTING), done(Step.BATTERY))))
    }

    @Test
    fun `moves on as each step is done, keeping its count`() {
        val walk = started(restrictedPhone)
        val texting = listOf(done(Step.TEXTING), undone(Step.BATTERY, Fix.BATTERY_REQUEST), undone(Step.CHAT_FEATURES, Fix.TEXTING_APP))
        val moved = walk.next(texting)!!
        assertEquals(Place(texting[1], 2, 3), moved.at(texting))
        val battery = listOf(done(Step.TEXTING), done(Step.BATTERY), undone(Step.CHAT_FEATURES, Fix.TEXTING_APP, Fix.CONFIRM))
        assertEquals(Place(battery[2], 3, 3), moved.next(battery)!!.at(battery))
        assertNull(moved.next(listOf(done(Step.TEXTING), done(Step.BATTERY), done(Step.CHAT_FEATURES))))
    }

    @Test
    fun `stays on a step still undone`() {
        val walk = started(restrictedPhone)
        assertEquals(walk.at(restrictedPhone), walk.next(restrictedPhone)!!.at(restrictedPhone))
    }

    @Test
    fun `takes in a step undone during the walk, in its place`() {
        val walk = started(listOf(undone(Step.TEXTING, Fix.ASK_TEXTING), undone(Step.CHAT_FEATURES, Fix.TEXTING_APP)))
        val later = listOf(done(Step.TEXTING), undone(Step.BATTERY, Fix.APP_INFO), undone(Step.CHAT_FEATURES, Fix.TEXTING_APP))
        val next = walk.next(later)!!
        assertEquals(listOf(Step.TEXTING, Step.BATTERY, Step.CHAT_FEATURES), next.steps)
        assertEquals(Place(later[1], 2, 3), next.at(later))
    }

    @Test
    fun `shows Restricted settings in Texting's place only once blocked`() {
        val blocked = started(restrictedPhone).copy(blocked = true)
        assertEquals(Place(restrictedPhone[3], 1, 3), blocked.at(restrictedPhone))
        // Back from it, the walk asks for texting again.
        assertEquals(restrictedPhone[0], blocked.copy(blocked = false).at(restrictedPhone)?.item)
    }

    @Test
    fun `is no longer blocked once texting is allowed`() {
        val allowed = listOf(done(Step.TEXTING), undone(Step.BATTERY, Fix.BATTERY_REQUEST), done(Step.RESTRICTED_SETTINGS))
        val walk = started(restrictedPhone).copy(blocked = true).next(allowed)!!
        assertFalse(walk.blocked)
        assertEquals(Step.BATTERY, walk.at(allowed)?.item?.step)
    }

    @Test
    fun `stays blocked while texting is still refused`() {
        assertTrue(started(restrictedPhone).copy(blocked = true).next(restrictedPhone)!!.blocked)
    }

    @Test
    fun `shows Texting where Android blocked it without restricting it`() {
        val items = listOf(undone(Step.TEXTING, Fix.ASK_TEXTING), undone(Step.BATTERY, Fix.BATTERY_REQUEST))
        assertEquals(items[0], started(items).copy(blocked = true).at(items)?.item)
    }
}
