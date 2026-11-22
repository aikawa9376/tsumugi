use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;

fn key(commit: &str) -> Key {
    Key {
        primary: "/r/api".into(),
        commit: commit.into(),
    }
}

fn passing(commit: &str) -> Report {
    Report {
        commit: commit.into(),
        passed: true,
        exit: Some(0),
        failed: Vec::new(),
        known: Vec::new(),
        terminal: None,
        tail: String::new(),
    }
}

fn words(list: &[&str]) -> Vec<String> {
    list.iter().map(ToString::to_string).collect()
}

#[test]
fn the_failed_tests_are_read_from_what_the_common_runners_print() {
    let log = "running 3 tests\n\
               test a::ok ... ok\n\
               test a::dns_lookup ... FAILED\n\
               test b::real_bug ... FAILED\n\
               test a::dns_lookup ... FAILED\n\
               --- FAIL: TestGo (0.01s)\n\
               not ok 4 - tap case\n";
    assert_eq!(
        failed_tests(log),
        ["a::dns_lookup", "b::real_bug", "TestGo", "4 - tap case"]
    );
}

#[test]
fn a_command_that_succeeds_passes_whatever_its_output_says() {
    let (passed, failed, known) = judge(0, "test x ... FAILED\n", &words(&["x"]));
    assert!(passed);
    assert!(failed.is_empty() && known.is_empty());
}

#[test]
fn a_command_that_fails_only_in_known_tests_passes_and_names_them() {
    let log = "test net::dns_lookup ... FAILED\ntest net::proxy ... FAILED\n";
    let (passed, failed, known) = judge(101, log, &words(&["dns_lookup", "proxy"]));
    assert!(passed);
    assert!(failed.is_empty());
    assert_eq!(known, ["net::dns_lookup", "net::proxy"]);
}

#[test]
fn one_failure_that_is_not_known_fails_the_gate() {
    let log = "test net::dns_lookup ... FAILED\ntest core::real_bug ... FAILED\n";
    let (passed, failed, known) = judge(101, log, &words(&["dns_lookup"]));
    assert!(!passed);
    assert_eq!(failed, ["core::real_bug"]);
    assert_eq!(known, ["net::dns_lookup"]);
}

#[test]
fn a_failure_that_names_no_test_is_never_known() {
    let (passed, failed, known) = judge(2, "error: could not compile\n", &words(&["error"]));
    assert!(!passed);
    assert!(failed.is_empty() && known.is_empty());
}

#[test]
fn an_empty_known_word_excuses_nothing() {
    let (passed, ..) = judge(1, "test a ... FAILED\n", &words(&[""]));
    assert!(!passed);
}

#[test]
fn the_tail_is_the_last_lines() {
    assert_eq!(tail("a\nb\nc\nd", 2), "c\nd");
    assert_eq!(tail("a", 5), "a");
}

#[test]
fn the_command_is_run_in_a_subshell_that_keeps_its_status_in_a_file() {
    let text = script("just ci");
    assert!(text.contains("( just ci\n )"));
    assert!(text.contains("$TSUMUGI_GATE_STATUS"));
    assert!(text.contains("tee \"$TSUMUGI_GATE_LOG\""));
}

#[tokio::test]
async fn a_second_ask_for_the_commit_being_checked_joins_the_run_and_does_not_start_another() {
    let gate = Arc::new(Gate::default());
    let runs = Arc::new(AtomicUsize::new(0));
    let release = Arc::new(tokio::sync::Notify::new());
    let ask = |gate: Arc<Gate>| {
        let (runs, release) = (runs.clone(), release.clone());
        tokio::spawn(async move {
            gate.request(key("abc"), Duration::from_secs(5), move || async move {
                runs.fetch_add(1, Ordering::SeqCst);
                release.notified().await;
                Ok(passing("abc"))
            })
            .await
        })
    };
    let first = ask(gate.clone());
    tokio::time::sleep(Duration::from_millis(50)).await;
    let second = ask(gate.clone());
    tokio::time::sleep(Duration::from_millis(50)).await;
    release.notify_one();

    let (first, second) = (first.await.unwrap(), second.await.unwrap());

    assert_eq!(runs.load(Ordering::SeqCst), 1);
    assert!(!first.joined && second.joined);
    assert_eq!(first.progress, Progress::Done(Ok(passing("abc"))));
    assert_eq!(second.progress, first.progress);
}

#[tokio::test]
async fn an_ask_for_another_commit_waits_until_the_run_before_it_has_ended() {
    let gate = Arc::new(Gate::default());
    let order = Arc::new(Mutex::new(Vec::new()));
    let release = Arc::new(tokio::sync::Notify::new());
    let slow = {
        let (gate, order, release) = (gate.clone(), order.clone(), release.clone());
        tokio::spawn(async move {
            gate.request(key("one"), Duration::from_secs(5), move || async move {
                order.lock().unwrap().push("one starts");
                release.notified().await;
                order.lock().unwrap().push("one ends");
                Ok(passing("one"))
            })
            .await
        })
    };
    tokio::time::sleep(Duration::from_millis(50)).await;
    let waiting = {
        let (gate, order) = (gate.clone(), order.clone());
        tokio::spawn(async move {
            gate.request(key("two"), Duration::from_secs(5), move || async move {
                order.lock().unwrap().push("two starts");
                Ok(passing("two"))
            })
            .await
        })
    };
    tokio::time::sleep(Duration::from_millis(50)).await;
    // Nothing of the second has run while the first holds the gate.
    assert_eq!(*order.lock().unwrap(), ["one starts"]);
    assert_eq!(gate.pending(), 2);
    release.notify_one();

    slow.await.unwrap();
    waiting.await.unwrap();

    assert_eq!(
        *order.lock().unwrap(),
        ["one starts", "one ends", "two starts"]
    );
    assert_eq!(gate.pending(), 0);
}

#[tokio::test]
async fn an_ask_that_waits_too_long_is_told_how_far_the_run_has_got_and_the_run_goes_on() {
    let gate = Arc::new(Gate::default());
    let release = Arc::new(tokio::sync::Notify::new());
    let held = release.clone();
    let early = gate
        .request(key("abc"), Duration::from_millis(50), move || async move {
            held.notified().await;
            Ok(passing("abc"))
        })
        .await;
    assert_eq!(early.progress, Progress::Running);
    assert_eq!(told(&key("abc"), &early)["state"], "running");

    release.notify_one();
    // Asking again joins the run that went on.
    let later = gate
        .request(key("abc"), Duration::from_secs(5), || async {
            Err("started a second run".to_string())
        })
        .await;
    assert!(later.joined || matches!(later.progress, Progress::Done(Err(_))));
    if later.joined {
        assert_eq!(later.progress, Progress::Done(Ok(passing("abc"))));
    }
}

#[tokio::test]
async fn once_a_run_has_ended_the_next_ask_for_the_same_commit_runs_again() {
    let gate = Gate::default();
    let runs = Arc::new(AtomicUsize::new(0));
    for _ in 0..2 {
        let counted = runs.clone();
        let answer = gate
            .request(key("abc"), Duration::from_secs(5), move || async move {
                counted.fetch_add(1, Ordering::SeqCst);
                Ok(passing("abc"))
            })
            .await;
        assert!(!answer.joined);
    }
    assert_eq!(runs.load(Ordering::SeqCst), 2);
}

#[test]
fn the_answer_says_how_the_commit_stands() {
    let done = Answer {
        progress: Progress::Done(Ok(passing("abc"))),
        joined: true,
    };
    let told = told(&key("abc"), &done);
    assert_eq!(told["state"], "done");
    assert_eq!(told["report"]["passed"], true);
    assert_eq!(told["joined"], true);
    let queued = Answer {
        progress: Progress::Queued { ahead: 2 },
        joined: false,
    };
    assert_eq!(told_of(&queued)["ahead"], 2);
    let failed = Answer {
        progress: Progress::Done(Err("no shell".into())),
        joined: false,
    };
    assert_eq!(told_of(&failed)["error"], "no shell");
}

fn told_of(answer: &Answer) -> serde_json::Value {
    told(&key("abc"), answer)
}
