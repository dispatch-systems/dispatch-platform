package com.dspdispatch.relay.setup

/**
 * The guided Set up under way: the steps it walks, in Set up's order, each undone when it joined,
 * and whether Android just blocked texting.
 */
data class Walk(val steps: List<Step> = emptyList(), val blocked: Boolean = false) {
    /**
     * The walk after a check found [items]: with each step undone now that it lacks, and no
     * longer blocked once texting is allowed; null once every step it walks is done.
     */
    fun next(items: List<Item>): Walk? {
        val undone = items.undone()
        // Restricted settings joins no walk: it shows in Texting's place, only once Android blocked texting.
        val joined = undone.keys - Step.RESTRICTED_SETTINGS
        val walk = Walk((steps + joined).distinct().sorted(), blocked && Step.TEXTING in undone)
        return if (walk.steps.any(undone::containsKey)) walk else null
    }

    /** The step to show of [items], with its place in the walk; null where none it walks is undone. */
    fun at(items: List<Item>): Place? {
        val undone = items.undone()
        val step = steps.firstOrNull(undone::containsKey) ?: return null
        val restricted = undone[Step.RESTRICTED_SETTINGS].takeIf { blocked && step == Step.TEXTING }
        return Place(restricted ?: undone.getValue(step), steps.indexOf(step) + 1, steps.size)
    }

    private fun List<Item>.undone() = filterNot(Item::done).associateBy(Item::step)
}

/** A step of the guided Set up as it shows: the step, and that it's [number] of [count]. */
data class Place(val item: Item, val number: Int, val count: Int)
