package com.dspdispatch.relay.setup

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ChecklistTest {
    private val allDone = Facts(
        texting = true,
        phoneNumber = true,
        notificationsAllowed = true,
        notificationsOn = true,
        channelsOn = true,
        backgroundRestricted = false,
        ignoringBatteryOptimizations = true,
        makerBattery = emptyList(),
        unusedApps = UnusedApps.OFF,
        backgroundData = true,
        restricted = false,
        textingApp = true,
        chatFeaturesByHand = ByHand.DONE,
        installUpdates = true,
    )

    private fun Facts.item(step: Step) = checklist(this).single { it.step == step }

    @Test
    fun `lists every step that applies, in order, done`() {
        val items = checklist(allDone)
        assertEquals(Step.entries, items.map(Item::step))
        assertTrue(items.all { it.done && it.fixes.isEmpty() && it.fix == null })
    }

    @Test
    fun `leaves out the steps a phone's Android doesn't have`() {
        val older = allDone.copy(notificationsAllowed = null, unusedApps = UnusedApps.NOT_AVAILABLE, restricted = null, textingApp = false)
        assertEquals(
            listOf(Step.TEXTING, Step.PHONE_NUMBER, Step.BATTERY, Step.BACKGROUND_DATA, Step.INSTALL_UPDATES),
            checklist(older).map(Item::step),
        )
    }

    @Test
    fun `asks for a permission not given`() {
        assertEquals(listOf(Fix.ASK_TEXTING), allDone.copy(texting = false).item(Step.TEXTING).fixes)
        assertEquals(listOf(Fix.ASK_PHONE_NUMBER), allDone.copy(phoneNumber = false).item(Step.PHONE_NUMBER).fixes)
    }

    @Test
    fun `asks for notifications, or opens their settings once allowed but off`() {
        assertEquals(Fix.ASK_NOTIFICATIONS, allDone.copy(notificationsAllowed = false).item(Step.NOTIFICATIONS).fix)
        val off = allDone.copy(notificationsOn = false).item(Step.NOTIFICATIONS)
        assertEquals(false, off.done)
        assertEquals(Fix.NOTIFICATION_SETTINGS, off.fix)
    }

    @Test
    fun `takes notifications as done only once their channels are on too`() {
        val blocked = allDone.copy(channelsOn = false).item(Step.NOTIFICATIONS)
        assertEquals(false, blocked.done)
        assertEquals(listOf(Fix.CHANNEL_SETTINGS), blocked.fixes)
        assertEquals(Fix.NOTIFICATION_SETTINGS, allDone.copy(notificationsOn = false, channelsOn = false).item(Step.NOTIFICATIONS).fix)
    }

    @Test
    fun `lifts Android's battery restriction before the maker's`() {
        val both = allDone.copy(ignoringBatteryOptimizations = false, makerBattery = listOf(ByHand.NONE))
        assertEquals(listOf(Fix.BATTERY_REQUEST), both.item(Step.BATTERY).fixes)
        val restricted = both.copy(backgroundRestricted = true)
        assertEquals(listOf(Fix.APP_INFO), restricted.item(Step.BATTERY).fixes)
    }

    @Test
    fun `takes the maker's battery screen as done only once marked done by hand`() {
        val maker = allDone.copy(makerBattery = listOf(ByHand.NONE))
        assertEquals(listOf(Fix.MAKER_BATTERY), maker.item(Step.BATTERY).fixes)
        val opened = maker.copy(makerBattery = listOf(ByHand.OPENED)).item(Step.BATTERY)
        assertEquals(listOf(Fix.MAKER_BATTERY, Fix.CONFIRM), opened.fixes)
        assertEquals(Fix.CONFIRM, opened.fix)
        assertTrue(maker.copy(makerBattery = listOf(ByHand.DONE)).item(Step.BATTERY).done)
    }

    @Test
    fun `offers each of the maker's controls in turn, done once all are`() {
        val first = allDone.copy(makerBattery = listOf(ByHand.DONE, ByHand.NONE)).item(Step.BATTERY)
        assertEquals(false, first.done)
        assertEquals(listOf(Fix.MAKER_BATTERY), first.fixes)
        val second = allDone.copy(makerBattery = listOf(ByHand.DONE, ByHand.OPENED)).item(Step.BATTERY)
        assertEquals(listOf(Fix.MAKER_BATTERY, Fix.CONFIRM), second.fixes)
        assertTrue(allDone.copy(makerBattery = listOf(ByHand.DONE, ByHand.DONE)).item(Step.BATTERY).done)
    }

    @Test
    fun `takes chat features as done only once marked done by hand`() {
        assertEquals(listOf(Fix.TEXTING_APP), allDone.copy(chatFeaturesByHand = ByHand.NONE).item(Step.CHAT_FEATURES).fixes)
        assertEquals(
            listOf(Fix.TEXTING_APP, Fix.CONFIRM),
            allDone.copy(chatFeaturesByHand = ByHand.OPENED).item(Step.CHAT_FEATURES).fixes,
        )
    }

    @Test
    fun `opens the screen of each setting Android reports`() {
        assertEquals(Fix.UNUSED_APPS, allDone.copy(unusedApps = UnusedApps.ON).item(Step.UNUSED_APPS).fix)
        assertEquals(Fix.BACKGROUND_DATA, allDone.copy(backgroundData = false).item(Step.BACKGROUND_DATA).fix)
        assertEquals(Fix.APP_INFO, allDone.copy(restricted = true).item(Step.RESTRICTED_SETTINGS).fix)
        assertEquals(Fix.INSTALL_UPDATES, allDone.copy(installUpdates = false).item(Step.INSTALL_UPDATES).fix)
    }

    @Test
    fun `a done step has no fix`() {
        assertNull(allDone.item(Step.BATTERY).fix)
    }
}
