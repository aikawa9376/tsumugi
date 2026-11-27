package io.github.aikawa9376.tsumugi.android

/** A wait of a session, as the work machine numbers it. */
data class WaitKey(val session: String, val wait: Long)

/** What came of a reply. */
sealed interface Delivery {
    data object Sent : Delivery

    /** Never written to the work machine. */
    data class NotSent(val why: String) : Delivery

    /** The work machine typed nothing. */
    data class Refused(val why: String) : Delivery

    /** Written, and what came of it was lost: it may have been typed. */
    data class Unknown(val why: String) : Delivery

    /** Not sent, since a reply to this wait went already or may have. */
    data class Held(val why: String) : Delivery
}

/** What the user wrote to a session, and how its reply to [wait] went. */
data class Draft(
    val text: String = "",
    val wait: Long? = null,
    val sending: Boolean = false,
    val note: Delivery? = null,
)

/**
 * The drafts, one a session. A draft is kept until its reply is known to have
 * been typed; one whose outcome was lost is not sent again to the same wait.
 */
class Drafts(private val bySession: Map<String, Draft> = emptyMap()) {
    operator fun get(key: WaitKey): Draft {
        val draft = bySession[key.session] ?: return Draft(wait = key.wait)
        return if (draft.wait == key.wait) draft else draft.copy(wait = key.wait, sending = false, note = null)
    }

    fun edit(key: WaitKey, text: String) = change(key) { it.copy(text = text) }

    fun sending(key: WaitKey) = change(key) { it.copy(sending = true, note = null) }

    fun settled(key: WaitKey, delivery: Delivery) = change(key) {
        if (delivery == Delivery.Sent) Draft(wait = key.wait) else it.copy(sending = false, note = delivery)
    }

    fun canSend(key: WaitKey): Boolean {
        val draft = get(key)
        return !draft.sending && draft.text.isNotBlank() &&
            draft.note !is Delivery.Unknown && draft.note !is Delivery.Held
    }

    /** The drafts with each session that waits now at the wait it waits in. */
    fun keep(waits: List<WaitKey>): Drafts {
        val moved = bySession.toMutableMap()
        for (key in waits) {
            val draft = moved[key.session] ?: continue
            if (draft.wait != key.wait) moved[key.session] = get(key)
        }
        return Drafts(moved)
    }

    private fun change(key: WaitKey, change: (Draft) -> Draft) =
        Drafts(bySession + (key.session to change(get(key))))
}
