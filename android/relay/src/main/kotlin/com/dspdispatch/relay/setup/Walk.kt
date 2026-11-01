package com.dspdispatch.relay.setup

import androidx.annotation.StringRes
import com.dspdispatch.relay.R
import kotlinx.serialization.Serializable

/**
 * The guided Set up under way: the steps it walks, in Set up's order, each undone when it joined,
 * and why Android last refused texting without asking, if it did.
 */
@Serializable
data class Walk(val steps: List<Step> = emptyList(), val blocked: Blocked? = null) {
    /**
     * The walk after a check found [items]: with each step undone now that it lacks, and no
     * longer blocked once what blocked texting is gone; null once every step it walks is done.
     */
    fun next(items: List<Item>): Walk? {
        val undone = items.undone()
        // Restricted settings joins no walk: it shows in Texting's place, only while it blocks texting.
        val joined = undone.keys - Step.RESTRICTED_SETTINGS
        val walk = Walk((steps + joined).distinct().sorted(), blocked?.takeIf { it.stands(undone.keys) })
        return if (walk.steps.any(undone::containsKey)) walk else null
    }

    /** The step to show of [items], with its place in the walk; null where none it walks is undone. */
    fun at(items: List<Item>): Place? {
        val undone = items.undone()
        val step = steps.firstOrNull(undone::containsKey) ?: return null
        val number = steps.indexOf(step) + 1
        val instead = when (blocked.takeIf { step == Step.TEXTING }) {
            Blocked.RESTRICTED -> undone[Step.RESTRICTED_SETTINGS]?.let { Place(it, number, steps.size) }
            Blocked.REFUSED -> Place(Item(Step.TEXTING, false, listOf(Fix.APP_INFO)), number, steps.size, R.string.step_texting_refused)
            null -> null
        }
        return instead ?: Place(undone.getValue(step), number, steps.size)
    }

    private fun List<Item>.undone() = filterNot(Item::done).associateBy(Item::step)
}

/** Why Android refused texting without asking, which decides the way through the walk shows. */
enum class Blocked {
    /** Android 15+ restricts it: Restricted settings shows in Texting's place until the person is back from it. */
    RESTRICTED,

    /** The person refused it for good: the walk opens App info, where it's given, until it is. */
    REFUSED,
    ;

    /** Whether it still blocks texting, with [undone] undone. */
    fun stands(undone: Set<Step>): Boolean =
        Step.TEXTING in undone && (this != RESTRICTED || Step.RESTRICTED_SETTINGS in undone)

    companion object {
        /** What blocked texting, Android refusing it at once with [items] as checked then. */
        fun of(items: List<Item>): Blocked =
            if (items.any { it.step == Step.RESTRICTED_SETTINGS && !it.done }) RESTRICTED else REFUSED
    }
}

/** A step of the guided Set up as it shows: the step, under [heading], and that it's [number] of [count]. */
data class Place(val item: Item, val number: Int, val count: Int, @param:StringRes val heading: Int = item.step.heading)
