//! Putting a session to sleep once its turn is done and no one has looked at
//! or written to it for the minutes the settings name: its program is
//! stopped, its row and tabs stay, and focusing it or sending to it wakes it
//! with its conversation, as a restart does.

use std::collections::HashMap;

use tsumugi_model::{Session, State};

use crate::delegation::app::{root, with_store};
use crate::watch::Watched;
use crate::work::Seen;

const MINUTE_MS: u64 = 60_000;

/// How long each session has been done, out of view and left alone, as the
/// listing has shown it; kept between listings by the watch.
#[derive(Debug, Default)]
pub struct Quiet {
    since: HashMap<String, u64>,
}

impl Quiet {
    /// Look at `sessions` at `now_ms`, with `shown` the ones some window
    /// shows: a session that is not done or is shown starts counting anew
    /// when it is next done and out of view, and one no longer listed is
    /// forgotten.
    pub fn look(&mut self, sessions: &[Session], shown: &dyn Fn(&str) -> bool, now_ms: u64) {
        self.since.retain(|id, _| {
            sessions
                .iter()
                .any(|s| s.id == *id && finished(&s.state) && !shown(&s.id))
        });
        for s in sessions {
            if finished(&s.state) && !shown(&s.id) {
                self.since.entry(s.id.clone()).or_insert(now_ms);
            }
        }
    }

    /// For how long `session` has been quiet at `now_ms`: done, out of view
    /// and not written to, which counts from the later of the three.
    pub fn quiet_ms(&self, session: &Session, now_ms: u64) -> u64 {
        let Some(since) = self.since.get(&session.id) else {
            return 0;
        };
        let from = (*since).max(session.last_input_ms.unwrap_or(0));
        now_ms.saturating_sub(from)
    }
}

/// Whether `state` says its turn is done, with nothing running behind it.
fn finished(state: &State) -> bool {
    matches!(state, State::Done | State::Idle)
}

/// Whether `session` is to be put to sleep, `quiet_ms` having passed with it
/// done, out of view and not written to, `after` minutes being what the
/// settings wait, none for never, and `opened_running` whether a session it
/// opened still runs.
pub fn due(session: &Session, opened_running: bool, quiet_ms: u64, after: u32) -> bool {
    after != 0
        && finished(&session.state)
        && !opened_running
        // One that kept no conversation could not be woken into it, and
        // Cursor's cannot be resumed from Tsumugi.
        && session.wakeable()
        && quiet_ms >= u64::from(after) * MINUTE_MS
}

/// Whether a window shows `id` or reads its conversation.
fn shown_by(seen: &[Seen], id: &str) -> bool {
    seen.iter()
        .any(|s| s.sight.session.as_deref() == Some(id) || s.sight.conversations.contains(id))
}

/// Put to sleep each of `sessions` that is due; one the terminal host cannot
/// end is left as it was.
pub async fn tick(watched: &Watched, quiet: &mut Quiet, sessions: &[Session]) {
    let after = crate::settings::current().hibernate_after;
    let now = tsumugi_model::time::now_ms();
    if after == 0 {
        *quiet = Quiet::default();
        return;
    }
    let seen = watched.seen();
    quiet.look(sessions, &|id| shown_by(&seen, id), now);
    for session in sessions {
        let opened_running = session.delegation.is_some_and(|id| {
            sessions
                .iter()
                .any(|child| child.parent == Some(id) && child.rest.is_none())
        });
        if due(session, opened_running, quiet.quiet_ms(session, now), after)
            && let Err(error) = hibernate(session).await
        {
            tracing::warn!(%error, session = %session.name, "cannot put a session to sleep");
        }
    }
}

/// Of `sessions`, those `ids` name that can be woken again, and the names of
/// those that cannot. A session is put to sleep as the person asked, whether
/// or not it is done.
fn asked<'a>(sessions: &'a [Session], ids: &[String]) -> (Vec<&'a Session>, Vec<String>) {
    sessions.iter().filter(|s| ids.contains(&s.id)).fold(
        (Vec::new(), Vec::new()),
        |(mut slept, mut left), s| {
            if s.wakeable() {
                slept.push(s);
            } else {
                left.push(s.name.clone());
            }
            (slept, left)
        },
    )
}

/// Put to sleep the sessions `ids` name, as a person asked for a worktree to
/// sleep, and say how many slept and the names of those that cannot be woken
/// again and so run on.
pub async fn sleep_now(ids: &[String]) -> Result<(usize, Vec<String>), String> {
    let daemon = crate::daemon::daemon();
    let listed = daemon.sessions().await.map_err(|e| e.to_string())?;
    let sessions: Vec<Session> = listed
        .iter()
        .filter_map(crate::sessions::read)
        .filter_map(|kept| match kept {
            crate::sessions::Kept::Agent(session) => Some(session),
            crate::sessions::Kept::Terminal(_) => None,
        })
        .collect();
    let (slept, mut left) = asked(&sessions, ids);
    let mut count = 0;
    for session in slept {
        match hibernate(session).await {
            Ok(()) => count += 1,
            Err(error) => {
                tracing::warn!(%error, session = %session.name, "cannot put a session to sleep");
                left.push(session.name.clone());
            }
        }
    }
    Ok((count, left))
}

/// Stop `session`'s program, recording first that it sleeps, so it is never
/// taken for one that ended.
async fn hibernate(session: &Session) -> Result<(), String> {
    let (Some(record), Some(terminal)) = (session.delegation, crate::sessions::id_of(&session.id))
    else {
        return Ok(());
    };
    let root = root();
    with_store(&root, move |store| store.hibernate(record))
        .await
        .map_err(|e| e.to_string())?;
    if let Err(error) = crate::daemon::daemon().end(terminal).await {
        let _ = with_store(&root, move |store| store.stay_awake(record)).await;
        return Err(error.to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use tsumugi_model::testing::session;
    use tsumugi_model::{Agent, SessionId, Wait};

    use super::*;

    fn done(id: &str) -> Session {
        let mut s = session(id).session_id("c1").state(State::Done).build();
        s.delegation = Some(SessionId::new());
        s
    }

    const HOUR: u64 = 60 * MINUTE_MS;

    #[test]
    fn a_finished_session_left_alone_sleeps_once_the_minutes_have_passed() {
        let s = done("a");
        assert!(!due(&s, false, 29 * MINUTE_MS, 30));
        assert!(due(&s, false, 30 * MINUTE_MS, 30));
        let mut idle = s.clone();
        idle.state = State::Idle;
        assert!(due(&idle, false, HOUR, 30));
    }

    #[test]
    fn no_minutes_set_means_no_session_ever_sleeps() {
        assert!(!due(&done("a"), false, 1000 * HOUR, 0));
    }

    #[test]
    fn a_session_that_works_waits_on_a_dialog_or_is_still_starting_never_sleeps() {
        for state in [
            State::Working,
            State::Background,
            State::Starting,
            State::Failed,
            State::Waiting(Wait::Permission),
            State::Waiting(Wait::Input),
        ] {
            let mut s = done("a");
            s.state = state.clone();
            assert!(!due(&s, false, 1000 * HOUR, 1), "{state:?}");
        }
    }

    #[test]
    fn a_session_whose_children_still_run_never_sleeps() {
        assert!(!due(&done("a"), true, 1000 * HOUR, 1));
    }

    #[test]
    fn a_session_that_cannot_be_woken_into_its_conversation_never_sleeps() {
        let mut none = done("a");
        none.session_id.clear();
        assert!(!due(&none, false, 1000 * HOUR, 1));
        let mut cursor = done("b");
        cursor.agent = Agent::Cursor;
        assert!(!due(&cursor, false, 1000 * HOUR, 1));
        let mut command = done("e");
        command.agent = Agent::Generic;
        assert!(!due(&command, false, 1000 * HOUR, 1));
        let mut unrecorded = done("c");
        unrecorded.delegation = None;
        assert!(!due(&unrecorded, false, 1000 * HOUR, 1));
        let mut asleep = done("d");
        asleep.rest = Some(tsumugi_model::Rest::Hibernated);
        assert!(!due(&asleep, false, 1000 * HOUR, 1));
    }

    #[test]
    fn putting_to_sleep_by_hand_takes_the_asked_sessions_that_can_be_woken_and_names_the_rest() {
        let mut working = done("a");
        working.state = State::Working;
        let mut cursor = done("b");
        cursor.agent = Agent::Cursor;
        let other = done("c");
        let sessions = [working, cursor, other];

        let (slept, left) = asked(&sessions, &["a".into(), "b".into()]);

        assert_eq!(
            slept.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            ["a"],
            "a session mid-turn sleeps too, as the person asked"
        );
        assert_eq!(left, ["b"]);
    }

    #[test]
    fn the_clock_counts_from_when_it_was_done_out_of_view_or_from_the_last_input() {
        let mut quiet = Quiet::default();
        let mut s = done("a");
        quiet.look(std::slice::from_ref(&s), &|_| false, 1_000);
        assert_eq!(quiet.quiet_ms(&s, 61_000), 60_000);

        s.last_input_ms = Some(50_000);
        assert_eq!(quiet.quiet_ms(&s, 61_000), 11_000);
    }

    #[test]
    fn working_or_being_shown_starts_the_clock_anew() {
        let mut quiet = Quiet::default();
        let mut s = done("a");
        quiet.look(std::slice::from_ref(&s), &|_| false, 0);

        quiet.look(std::slice::from_ref(&s), &|_| true, 10_000);
        quiet.look(std::slice::from_ref(&s), &|_| false, 20_000);
        assert_eq!(quiet.quiet_ms(&s, 30_000), 10_000, "looked at, then left");

        s.state = State::Working;
        quiet.look(std::slice::from_ref(&s), &|_| false, 40_000);
        s.state = State::Done;
        quiet.look(std::slice::from_ref(&s), &|_| false, 50_000);
        assert_eq!(quiet.quiet_ms(&s, 60_000), 10_000, "worked, then done");
    }

    #[test]
    fn a_session_no_longer_listed_is_forgotten() {
        let mut quiet = Quiet::default();
        let s = done("a");
        quiet.look(std::slice::from_ref(&s), &|_| false, 0);
        quiet.look(&[], &|_| false, 10_000);
        quiet.look(std::slice::from_ref(&s), &|_| false, 20_000);
        assert_eq!(quiet.quiet_ms(&s, 30_000), 10_000);
    }
}
