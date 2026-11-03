package com.dspdispatch.relay.setup

import com.dspdispatch.relay.R
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class WalkingTest {
    /** The app's storage, which outlives the process. */
    private class Kept(var text: String? = null) : Walking.Saved {
        override fun read() = text

        override fun write(text: String?) {
            this.text = text
        }
    }

    private fun done(step: Step) = Item(step, true, emptyList())
    private fun undone(step: Step, vararg fixes: Fix) = Item(step, false, fixes.toList())

    private val phone = listOf(
        undone(Step.TEXTING, Fix.ASK_TEXTING),
        undone(Step.BATTERY, Fix.BATTERY_REQUEST),
        undone(Step.CHAT_FEATURES, Fix.TEXTING_APP),
    )

    @Test
    fun `goes on where it stood after Android ends the process`() {
        val kept = Kept()
        val before = Walking(kept).apply {
            start()
            on(phone)
            refused(phone)
        }
        val after = Walking(kept)
        assertEquals(before.walk.value, after.walk.value)
        assertEquals(Blocked.REFUSED, after.walk.value?.blocked)
        assertEquals(before.walk.value?.at(phone), after.walk.value?.at(phone))
    }

    @Test
    fun `checked again on return, moves on to what's undone now`() {
        val kept = Kept()
        Walking(kept).apply {
            start()
            on(phone)
        }
        // Texting was allowed and the battery set while the app wasn't running.
        val now = listOf(done(Step.TEXTING), done(Step.BATTERY), undone(Step.CHAT_FEATURES, Fix.TEXTING_APP, Fix.CONFIRM))
        val walking = Walking(kept)
        assertFalse(walking.on(now))
        assertEquals(Place(now[2], 3, 3), walking.walk.value?.at(now))
    }

    @Test
    fun `ends on return where the last step was done meanwhile, keeping nothing`() {
        val kept = Kept()
        Walking(kept).apply {
            start()
            on(phone)
        }
        val walking = Walking(kept)
        assertTrue(walking.on(phone.map { done(it.step) }))
        assertNull(walking.walk.value)
        assertNull(kept.text)
    }

    @Test
    fun `left by Back, keeps nothing`() {
        val kept = Kept()
        Walking(kept).apply {
            start()
            on(phone)
            stop()
        }
        assertNull(kept.text)
        assertNull(Walking(kept).walk.value)
    }

    /** A phone on Android 15+, the app installed from a file, before Android let the person choose on texting. */
    private val restricted = phone + undone(Step.RESTRICTED_SETTINGS, Fix.APP_INFO)

    @Test
    fun `back from Restricted settings, asks for texting again`() {
        val walking = Walking(Kept()).apply {
            start()
            on(restricted)
            refused(restricted)
        }
        assertEquals(Step.RESTRICTED_SETTINGS, walking.walk.value?.at(restricted)?.item?.step)
        walking.back()
        assertEquals(Place(phone[0], 1, 3), walking.walk.value?.at(restricted))
    }

    @Test
    fun `texting refused for good after Android let the person choose stays on App info, never Restricted settings`() {
        // Android asked once, so restricted settings no longer stand.
        val lifted = phone + done(Step.RESTRICTED_SETTINGS)
        val walking = Walking(Kept()).apply {
            start()
            on(lifted)
            refused(lifted)
            back()
            on(lifted)
        }
        val place = walking.walk.value?.at(lifted)
        assertEquals(Place(undone(Step.TEXTING, Fix.APP_INFO), 1, 3, R.string.step_texting_refused), place)
    }

    /**
     * A phone on Android 15+, the app installed from a file and updated in place from a build that
     * kept no record of texting's restriction, with texting refused for good: as it checks now,
     * Restricted settings undone, as the app can't tell they were allowed.
     */
    private val updated = checklist(
        Facts(
            texting = false,
            phoneNumber = true,
            notificationsAllowed = true,
            notificationsOn = true,
            channelsOn = true,
            backgroundRestricted = false,
            ignoringBatteryOptimizations = false,
            makerBattery = emptyList(),
            unusedApps = UnusedApps.OFF,
            backgroundData = true,
            restricted = restriction(fromFile = true, texting = false, unrestricted = false),
            textingApp = null,
            chatFeaturesByHand = ByHand.NONE,
            installUpdates = true,
        ),
    )

    private val permissions = Place(undone(Step.TEXTING, Fix.APP_INFO), 1, 2, R.string.step_texting_refused)

    @Test
    fun `with no record of texting's restriction, shows Restricted settings once, then Permissions, never looping`() {
        val walking = Walking(Kept()).apply {
            start()
            on(updated)
            refused(updated)
        }
        assertEquals(Step.RESTRICTED_SETTINGS, walking.walk.value?.at(updated)?.item?.step)
        walking.back()
        walking.on(updated)
        assertEquals(Place(updated[0], 1, 2), walking.walk.value?.at(updated))
        walking.refused(updated)
        assertEquals(permissions, walking.walk.value?.at(updated))
        repeat(2) {
            walking.back()
            walking.on(updated)
            assertEquals(permissions, walking.walk.value?.at(updated))
            walking.refused(updated)
            assertEquals(permissions, walking.walk.value?.at(updated))
        }
    }

    @Test
    fun `keeps that Restricted settings were opened after Android ends the process`() {
        val kept = Kept()
        Walking(kept).apply {
            start()
            on(updated)
            refused(updated)
            back()
        }
        val walking = Walking(kept)
        walking.on(updated)
        walking.refused(updated)
        assertEquals(permissions, walking.walk.value?.at(updated))
    }

    @Test
    fun `goes on with a walk kept by the build before, in Restricted settings`() {
        val walking = Walking(Kept("""{"steps":["TEXTING","BATTERY"],"blocked":"RESTRICTED"}"""))
        assertEquals(Step.RESTRICTED_SETTINGS, walking.walk.value?.at(updated)?.item?.step)
        walking.back()
        walking.on(updated)
        walking.refused(updated)
        assertEquals(permissions, walking.walk.value?.at(updated))
    }

    @Test
    fun `reports no end where no walk was under way`() {
        val walking = Walking(Kept())
        assertFalse(walking.on(phone))
        assertFalse(walking.refused(phone))
        assertNull(walking.walk.value)
    }

    @Test
    fun `starts with none where what was kept can't be read`() {
        assertNull(Walking(Kept("{")).walk.value)
    }
}
