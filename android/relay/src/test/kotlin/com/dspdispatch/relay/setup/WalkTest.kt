package com.dspdispatch.relay.setup

import com.dspdispatch.relay.R
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
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
    fun `shows Restricted settings in Texting's place while it blocks texting`() {
        val blocked = started(restrictedPhone).copy(blocked = Blocked.RESTRICTED)
        assertEquals(Place(restrictedPhone[3], 1, 3), blocked.at(restrictedPhone))
        assertEquals(Place(restrictedPhone[3], 1, 3), blocked.next(restrictedPhone)!!.at(restrictedPhone))
    }

    @Test
    fun `asks for texting again once restricted settings no longer stand`() {
        val lifted = restrictedPhone.map { if (it.step == Step.RESTRICTED_SETTINGS) done(it.step) else it }
        val walk = started(restrictedPhone).copy(blocked = Blocked.RESTRICTED).next(lifted)!!
        assertNull(walk.blocked)
        assertEquals(Place(restrictedPhone[0], 1, 3), walk.at(lifted))
    }

    @Test
    fun `opens App info for texting refused for good, after restricted settings were allowed`() {
        val lifted = restrictedPhone.map { if (it.step == Step.RESTRICTED_SETTINGS) done(it.step) else it }
        val refused = started(restrictedPhone).copy(blocked = Blocked.of(lifted, restrictedSettingsOpened = false)).next(lifted)!!
        val place = refused.at(lifted)!!
        assertEquals(Place(undone(Step.TEXTING, Fix.APP_INFO), 1, 3, R.string.step_texting_refused), place)
        // Back from App info with texting still refused, it stays: never Restricted settings.
        assertEquals(place, refused.next(lifted)!!.at(lifted))
    }

    @Test
    fun `opens App info for texting refused for good where Android never restricted it`() {
        val items = listOf(undone(Step.TEXTING, Fix.ASK_TEXTING), undone(Step.BATTERY, Fix.BATTERY_REQUEST))
        val walk = started(items).copy(blocked = Blocked.of(items, restrictedSettingsOpened = false))
        assertEquals(Blocked.REFUSED, walk.blocked)
        assertEquals(R.string.step_texting_refused, walk.at(items)?.heading)
    }

    @Test
    fun `blames restricted settings only while they may be restricted, and only until the walk opened them`() {
        assertEquals(Blocked.RESTRICTED, Blocked.of(restrictedPhone, restrictedSettingsOpened = false))
        assertEquals(Blocked.REFUSED, Blocked.of(restrictedPhone, restrictedSettingsOpened = true))
    }

    @Test
    fun `opens App info for texting refused at once again after Restricted settings, which may still look restricted`() {
        val walk = started(restrictedPhone).copy(restrictedSettingsOpened = true)
        val refused = walk.copy(blocked = Blocked.of(restrictedPhone, walk.restrictedSettingsOpened))
        val place = Place(undone(Step.TEXTING, Fix.APP_INFO), 1, 3, R.string.step_texting_refused)
        assertEquals(place, refused.at(restrictedPhone))
        assertEquals(place, refused.next(restrictedPhone)!!.at(restrictedPhone))
        assertEquals(true, refused.next(restrictedPhone)!!.restrictedSettingsOpened)
    }

    @Test
    fun `is no longer blocked once texting is allowed`() {
        val allowed = listOf(done(Step.TEXTING), undone(Step.BATTERY, Fix.BATTERY_REQUEST), done(Step.RESTRICTED_SETTINGS))
        Blocked.entries.forEach { blocked ->
            val walk = started(restrictedPhone).copy(blocked = blocked).next(allowed)!!
            assertNull(walk.blocked)
            assertEquals(Place(allowed[1], 2, 3), walk.at(allowed))
        }
    }

    @Test
    fun `stays blocked while texting is still refused`() {
        Blocked.entries.forEach { assertEquals(it, started(restrictedPhone).copy(blocked = it).next(restrictedPhone)!!.blocked) }
    }
}
