use std::time::Duration;

use super::*;

const IDLE: Duration = Duration::from_secs(4 * 3600);

fn at(hours: u64) -> Duration {
    Duration::from_secs(hours * 3600)
}

fn free_all(_: BrowserId) -> bool {
    true
}

/// `retention` after the pages in `shown` are the ones displayed at `now`.
fn watch(retention: &mut Retention, now: Duration, open: &[BrowserId], shown: &[BrowserId]) {
    retention.observe(now, open, |id| shown.contains(&id));
}

#[test]
fn a_hidden_page_is_due_only_once_it_has_gone_unviewed_for_the_idle_time() {
    let (a, b) = (BrowserId::new(), BrowserId::new());
    let mut retention = Retention::default();
    watch(&mut retention, at(0), &[a, b], &[b]);
    watch(&mut retention, at(3), &[a, b], &[b]);
    assert!(retention.due(at(3), IDLE, free_all).is_empty());
    watch(&mut retention, at(4), &[a, b], &[b]);
    assert_eq!(retention.due(at(4), IDLE, free_all), vec![a]);
}

#[test]
fn a_page_that_is_shown_is_never_due_however_long_it_has_been_open() {
    let a = BrowserId::new();
    let mut retention = Retention::default();
    watch(&mut retention, at(0), &[a], &[a]);
    watch(&mut retention, at(100), &[a], &[a]);
    assert!(retention.due(at(100), IDLE, free_all).is_empty());
}

#[test]
fn viewing_a_page_starts_its_idle_time_again() {
    let a = BrowserId::new();
    let mut retention = Retention::default();
    watch(&mut retention, at(0), &[a], &[]);
    watch(&mut retention, at(3), &[a], &[a]);
    watch(&mut retention, at(4), &[a], &[]);
    assert!(retention.due(at(7), IDLE, free_all).is_empty());
    assert_eq!(retention.due(at(8), IDLE, free_all), vec![a]);
}

#[test]
fn the_page_unviewed_longest_comes_first_and_only_it_when_the_others_are_younger() {
    let (old, young) = (BrowserId::new(), BrowserId::new());
    let mut retention = Retention::default();
    watch(&mut retention, at(0), &[old, young], &[old, young]);
    watch(&mut retention, at(1), &[old, young], &[young]);
    watch(&mut retention, at(3), &[old, young], &[]);
    assert_eq!(retention.due(at(5), IDLE, free_all), vec![old]);
    assert_eq!(retention.due(at(7), IDLE, free_all), vec![old, young]);
}

#[test]
fn a_page_is_told_apart_by_its_identity_so_another_worktrees_page_is_not_taken_with_it() {
    // Layouts count their windows each from 1, so two worktrees' pages can
    // sit in "window 1"; the policy only ever sees page identities.
    let (mine, theirs) = (BrowserId::new(), BrowserId::new());
    let mut retention = Retention::default();
    watch(&mut retention, at(0), &[mine, theirs], &[]);
    watch(&mut retention, at(3), &[mine, theirs], &[theirs]);
    watch(&mut retention, at(4), &[mine, theirs], &[theirs]);
    assert_eq!(retention.due(at(4), IDLE, free_all), vec![mine]);
    retention.free(mine);
    assert!(retention.is_free(mine));
    assert!(!retention.is_free(theirs));
    assert!(retention.due(at(40), IDLE, free_all).is_empty());
}

#[test]
fn a_page_in_use_is_left_alone_until_it_can_be_freed() {
    let a = BrowserId::new();
    let mut retention = Retention::default();
    watch(&mut retention, at(0), &[a], &[]);
    assert!(retention.due(at(9), IDLE, |_| false).is_empty());
    assert_eq!(retention.due(at(9), IDLE, free_all), vec![a]);
}

#[test]
fn a_freed_page_is_not_due_again_and_is_thawed_when_it_is_shown() {
    let a = BrowserId::new();
    let mut retention = Retention::default();
    watch(&mut retention, at(0), &[a], &[]);
    retention.free(a);
    assert!(retention.is_free(a));
    assert!(retention.due(at(50), IDLE, free_all).is_empty());
    retention.thaw(a);
    assert!(!retention.is_free(a));
}

#[test]
fn a_page_that_closed_is_forgotten() {
    let a = BrowserId::new();
    let mut retention = Retention::default();
    watch(&mut retention, at(0), &[a], &[]);
    retention.free(a);
    watch(&mut retention, at(1), &[], &[]);
    assert!(!retention.is_free(a));
    assert!(retention.due(at(9), IDLE, free_all).is_empty());
}

#[test]
fn a_page_swapped_in_and_out_often_is_kept_as_the_user_plainly_wants_it() {
    let a = BrowserId::new();
    let mut retention = Retention::default();
    let mut hour = 0;
    for _ in 0..=REVISITS {
        watch(&mut retention, at(hour), &[a], &[]);
        hour += 1;
        watch(&mut retention, at(hour), &[a], &[a]);
        hour += 1;
    }
    watch(&mut retention, at(hour), &[a], &[]);
    assert!(retention.due(at(hour + 10), IDLE, free_all).is_empty());
}

#[test]
fn a_page_loaded_again_while_hidden_starts_its_idle_time_over() {
    // An agent that drives a page the user does not look at brings it back;
    // freeing it again at once would throw away what the agent just did.
    let a = BrowserId::new();
    let mut retention = Retention::default();
    watch(&mut retention, at(0), &[a], &[]);
    retention.free(a);
    retention.thaw(a);
    watch(&mut retention, at(10), &[a], &[]);
    assert!(retention.due(at(10), IDLE, free_all).is_empty());
    assert!(retention.due(at(13), IDLE, free_all).is_empty());
    assert_eq!(retention.due(at(14), IDLE, free_all), vec![a]);
}
