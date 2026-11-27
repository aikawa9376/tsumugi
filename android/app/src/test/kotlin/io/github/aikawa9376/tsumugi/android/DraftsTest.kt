package io.github.aikawa9376.tsumugi.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class DraftsTest {
    private val wait = WaitKey("a", 1)

    @Test
    fun a_draft_is_kept_while_its_reply_is_on_its_way() {
        val drafts = Drafts().edit(wait, "go on").sending(wait)

        assertEquals("go on", drafts[wait].text)
        assertTrue(drafts[wait].sending)
        assertFalse(drafts.canSend(wait))
    }

    @Test
    fun a_reply_that_was_sent_lets_its_draft_go() {
        val drafts = Drafts().edit(wait, "go on").sending(wait).settled(wait, Delivery.Sent)

        assertEquals("", drafts[wait].text)
        assertFalse(drafts[wait].sending)
        assertNull(drafts[wait].note)
    }

    @Test
    fun a_reply_that_did_not_reach_the_session_keeps_its_draft_to_send_again() {
        for (delivery in listOf(Delivery.NotSent("offline"), Delivery.Refused("moved on"))) {
            val drafts = Drafts().edit(wait, "go on").sending(wait).settled(wait, delivery)

            assertEquals("go on", drafts[wait].text)
            assertEquals(delivery, drafts[wait].note)
            assertTrue(drafts.canSend(wait))
        }
    }

    @Test
    fun a_reply_whose_outcome_was_lost_keeps_its_draft_and_is_not_sent_again() {
        val drafts = Drafts().edit(wait, "go on").sending(wait).settled(wait, Delivery.Unknown("lost"))

        assertEquals("go on", drafts[wait].text)
        assertFalse(drafts.canSend(wait))
        assertFalse(drafts.edit(wait, "go on, please").canSend(wait))
    }

    @Test
    fun an_empty_draft_is_not_sent() {
        assertFalse(Drafts().edit(wait, "  ").canSend(wait))
        assertTrue(Drafts().edit(wait, "y").canSend(wait))
    }

    @Test
    fun the_draft_of_a_session_moves_to_its_next_wait() {
        val next = WaitKey("a", 2)
        val drafts = Drafts().edit(wait, "go on").sending(wait).settled(wait, Delivery.Unknown("lost"))

        val moved = drafts.keep(listOf(next))

        assertEquals("go on", moved[next].text)
        assertTrue(moved.canSend(next))
    }

    @Test
    fun the_drafts_of_sessions_that_wait_no_more_are_kept_until_they_wait_again() {
        val drafts = Drafts().edit(wait, "go on")

        val kept = drafts.keep(emptyList()).keep(listOf(WaitKey("a", 3)))

        assertEquals("go on", kept[WaitKey("a", 3)].text)
    }
}
