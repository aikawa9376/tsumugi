package io.github.aikawa9376.tsumugi.android

/** How long before [nowMs] the waits shown were told, in the largest whole unit. */
fun age(nowMs: Long, asOfMs: Long): String {
    val minutes = (nowMs - asOfMs).coerceAtLeast(0) / 60_000
    return when {
        minutes < 1 -> "just now"
        minutes < 60 -> "$minutes min ago"
        minutes < 24 * 60 -> "${minutes / 60} h ago"
        else -> "${minutes / (24 * 60)} d ago"
    }
}
