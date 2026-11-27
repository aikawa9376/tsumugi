package io.github.aikawa9376.tsumugi.android

import org.junit.Assert.assertEquals
import org.junit.Test

class AgeTest {
    private val minute = 60_000L

    @Test
    fun how_old_the_waits_are_is_told_in_the_largest_whole_unit() {
        val cases = listOf(
            0L to "just now",
            59_000L to "just now",
            minute to "1 min ago",
            59 * minute to "59 min ago",
            60 * minute to "1 h ago",
            25 * 60 * minute to "1 d ago",
        )
        for ((ms, want) in cases) {
            assertEquals("$ms", want, age(nowMs = 10_000_000_000L, asOfMs = 10_000_000_000L - ms))
        }
    }

    @Test
    fun a_time_ahead_of_now_is_just_now() {
        assertEquals("just now", age(nowMs = 0L, asOfMs = minute))
    }
}
