package com.dspdispatch.relay.connection

import kotlin.random.Random

/**
 * How long to pause before calling again after [failures] calls in a row went unanswered: doubling
 * from a second to a minute, each a little shorter at random so phones don't call back as one.
 */
fun backoff(failures: Int, random: Random = Random.Default): Long {
    val full = (FIRST_MS shl (failures - 1).coerceIn(0, DOUBLINGS)).coerceAtMost(MOST_MS)
    return full - random.nextLong(full / JITTER_PARTS + 1)
}

private const val FIRST_MS = 1_000L
private const val MOST_MS = 60_000L
private const val DOUBLINGS = 6
private const val JITTER_PARTS = 5
