//! Waiting for a page to be as an agent says. The conditions are checked by
//! asking the page, through the result of a script the window runs on it, and
//! by what the window itself knows of the page's address; the page is never
//! asked to say when they hold.

use std::future::Future;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tsumugi_protocol::{BrowserCommand, ElementState, LoadState, WAIT_DEFAULT_MS};

/// How often the conditions are looked at again.
const EVERY: Duration = Duration::from_millis(100);

/// What a wait waits for, all of it at once.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Conditions {
    pub selector: Option<String>,
    pub state: Option<ElementState>,
    pub text: Option<String>,
    pub url: Option<String>,
    pub load: Option<LoadState>,
}

impl Conditions {
    fn none(&self) -> bool {
        !self.asks_page() && self.url.is_none()
    }

    /// Whether the page itself has anything to say of the conditions: the
    /// address alone is the window's to judge.
    pub(super) fn asks_page(&self) -> bool {
        self.selector.is_some() || self.text.is_some() || self.load.is_some()
    }

    /// The conditions as the agent gave them, for a failure to name them.
    fn describe(&self) -> String {
        let mut parts = Vec::new();
        if let Some(selector) = &self.selector {
            let state = match self.state {
                Some(ElementState::Hidden) => "hidden",
                _ => "visible",
            };
            parts.push(format!("{selector:?} to be {state}"));
        }
        if let Some(text) = &self.text {
            parts.push(format!("the text {text:?}"));
        }
        if let Some(url) = &self.url {
            parts.push(format!("an address matching {url:?}"));
        }
        if let Some(load) = self.load {
            parts.push(
                match load {
                    LoadState::Load => "the page to load",
                    LoadState::DomContentLoaded => "the document to be read",
                    LoadState::NetworkIdle => "the page to load and its requests to stop",
                }
                .to_string(),
            );
        }
        parts.join(", ")
    }

    /// The script that tells what the page itself can say of the conditions;
    /// the address is the window's to match, not the page's.
    pub(super) fn script(&self) -> String {
        let asked = json!({
            "selector": self.selector,
            "state": self.state.map(|state| match state {
                ElementState::Visible => "visible",
                ElementState::Hidden => "hidden",
            }),
            "text": self.text,
            "load": self.load.map(|load| match load {
                LoadState::Load => "load",
                LoadState::DomContentLoaded => "domcontentloaded",
                LoadState::NetworkIdle => "networkidle",
            }),
        });
        format!("window.tsumugiAgent?.check({asked})")
    }

    /// Whether the page's address, as the agent writes it, is as asked.
    pub(super) fn address_holds(&self, address: &str) -> bool {
        self.url
            .as_deref()
            .is_none_or(|pattern| url_matches(pattern, address))
    }
}

/// What a `wait` command comes to.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Plan {
    /// Nothing is waited for: only a pause.
    Pause(Duration),
    Until {
        conditions: Conditions,
        timeout: Duration,
    },
}

/// What to do for `command`, which is a wait.
pub(super) fn plan(command: &BrowserCommand) -> Option<Plan> {
    let BrowserCommand::Wait {
        selector,
        state,
        text,
        url,
        load,
        timeout,
    } = command
    else {
        return None;
    };
    let conditions = Conditions {
        selector: selector.clone(),
        state: *state,
        text: text.clone(),
        url: url.clone(),
        load: *load,
    };
    Some(if conditions.none() {
        Plan::Pause(Duration::from_millis(timeout.unwrap_or(0)))
    } else {
        Plan::Until {
            conditions,
            timeout: Duration::from_millis(timeout.unwrap_or(WAIT_DEFAULT_MS)),
        }
    })
}

/// Whether `address` matches `pattern`: `*` stands for anything but a slash,
/// `**` for anything, and every other character for itself.
pub(super) fn url_matches(pattern: &str, address: &str) -> bool {
    fn at(pattern: &[char], address: &[char]) -> bool {
        match pattern.split_first() {
            None => address.is_empty(),
            Some(('*', rest)) => {
                let (any, rest) = match rest.split_first() {
                    Some(('*', rest)) => (true, rest),
                    _ => (false, rest),
                };
                (0..=address.len()).any(|taken| {
                    (any || !address[..taken].contains(&'/')) && at(rest, &address[taken..])
                })
            }
            Some((first, rest)) => address
                .split_first()
                .is_some_and(|(next, after)| next == first && at(rest, after)),
        }
    }
    let pattern: Vec<char> = pattern.chars().collect();
    let address: Vec<char> = address.chars().collect();
    at(&pattern, &address)
}

/// Look at the conditions every moment until `met` says they hold, which
/// answers with how they stand; give up when `timeout` has passed, naming
/// what was waited for.
pub(super) async fn until<F, Fut>(
    conditions: &Conditions,
    timeout: Duration,
    mut met: F,
) -> Result<(), String>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<bool, String>>,
{
    let start = Instant::now();
    loop {
        if met().await? {
            return Ok(());
        }
        if start.elapsed() >= timeout {
            return Err(format!(
                "Timed out after {} ms waiting for {}",
                timeout.as_millis(),
                conditions.describe()
            ));
        }
        tokio::time::sleep(
            EVERY.min(
                timeout
                    .saturating_sub(start.elapsed())
                    .max(Duration::from_millis(1)),
            ),
        )
        .await;
    }
}

/// What the answer of the page's script says of the conditions: whether they
/// hold.
pub(super) fn held(answer: &Value) -> bool {
    answer["met"].as_bool().unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn wait(
        selector: Option<&str>,
        text: Option<&str>,
        url: Option<&str>,
        load: Option<LoadState>,
        timeout: Option<u64>,
    ) -> BrowserCommand {
        BrowserCommand::Wait {
            selector: selector.map(String::from),
            state: None,
            text: text.map(String::from),
            url: url.map(String::from),
            load,
            timeout,
        }
    }

    #[test]
    fn a_wait_with_no_condition_only_pauses_and_one_with_a_condition_waits_for_it() {
        assert_eq!(
            plan(&wait(None, None, None, None, Some(250))),
            Some(Plan::Pause(Duration::from_millis(250)))
        );
        assert_eq!(
            plan(&wait(None, None, None, None, None)),
            Some(Plan::Pause(Duration::ZERO))
        );
        let Some(Plan::Until {
            conditions,
            timeout,
        }) = plan(&wait(None, Some("Welcome"), None, None, None))
        else {
            panic!("a wait for text waits");
        };
        assert_eq!(conditions.text.as_deref(), Some("Welcome"));
        assert_eq!(timeout, Duration::from_millis(WAIT_DEFAULT_MS));
        assert_eq!(plan(&BrowserCommand::Pdf), None);
    }

    #[test]
    fn an_address_pattern_is_a_glob_where_one_star_stops_at_a_slash() {
        let yes = |pattern: &str, address: &str| {
            assert!(url_matches(pattern, address), "{pattern} {address}")
        };
        let no = |pattern: &str, address: &str| {
            assert!(!url_matches(pattern, address), "{pattern} {address}")
        };
        yes("http://localhost:3000/", "http://localhost:3000/");
        yes("**/dashboard", "http://localhost:3000/app/dashboard");
        yes("http://localhost:3000/*", "http://localhost:3000/home");
        yes("**", "anything at all");
        yes("**/items/*/edit", "http://x/items/42/edit");
        no("http://localhost:3000/*", "http://localhost:3000/a/b");
        no("**/dashboard", "http://localhost:3000/dashboard/more");
        no("http://localhost:3000", "http://localhost:3000/");
        no("", "http://x/");
        // Characters a regular expression would read stand for themselves.
        yes("http://x/a.b?c=1+1", "http://x/a.b?c=1+1");
        no("http://x/a.b", "http://x/aXb");
    }

    #[test]
    fn the_page_is_asked_in_a_script_that_carries_the_conditions_as_json_and_not_the_address() {
        let conditions = Conditions {
            selector: Some("#done'); alert(1); ('".into()),
            state: Some(ElementState::Hidden),
            text: Some("ok".into()),
            url: Some("**/secret".into()),
            load: Some(LoadState::NetworkIdle),
        };
        let script = conditions.script();
        assert!(
            script.starts_with("window.tsumugiAgent?.check({"),
            "{script}"
        );
        assert!(script.contains(r#""state":"hidden""#), "{script}");
        assert!(script.contains(r#""load":"networkidle""#), "{script}");
        assert!(script.contains(r#"#done'); alert(1); ('"#), "{script}");
        assert!(!script.contains("secret"), "{script}");
        assert!(conditions.address_holds("http://x/secret"));
        assert!(!conditions.address_holds("http://x/other"));
        assert!(Conditions::default().address_holds("anything"));
        // An address is the window's to judge: the page is not asked for it.
        assert!(
            !Conditions {
                url: Some("**".into()),
                ..Conditions::default()
            }
            .asks_page()
        );
        assert!(
            Conditions {
                load: Some(LoadState::Load),
                ..Conditions::default()
            }
            .asks_page()
        );
    }

    #[test]
    fn what_the_page_answers_holds_only_when_it_says_so() {
        assert!(held(&json!({"met": true})));
        assert!(!held(&json!({"met": false})));
        assert!(!held(&json!({"met": "yes"})));
        assert!(!held(&json!({})));
    }

    #[tokio::test]
    async fn a_wait_ends_when_the_conditions_hold_and_names_them_when_time_runs_out() {
        let conditions = Conditions {
            text: Some("Welcome".into()),
            selector: Some("#spinner".into()),
            state: Some(ElementState::Hidden),
            ..Conditions::default()
        };
        let looks = Cell::new(0);
        let held_on_third = until(&conditions, Duration::from_secs(5), || {
            looks.set(looks.get() + 1);
            let now = looks.get() >= 3;
            async move { Ok(now) }
        })
        .await;
        assert_eq!(held_on_third, Ok(()));
        assert_eq!(looks.get(), 3);

        let never = until(&conditions, Duration::from_millis(150), || async {
            Ok(false)
        })
        .await;
        let failed = never.unwrap_err();
        assert!(failed.contains("Timed out after 150 ms"), "{failed}");
        assert!(failed.contains(r##""#spinner" to be hidden"##), "{failed}");
        assert!(failed.contains(r#"the text "Welcome""#), "{failed}");

        // A page that cannot be read ends the wait with what it said.
        let broken = until(&conditions, Duration::from_secs(5), || async {
            Err("The page went away".to_string())
        })
        .await;
        assert_eq!(broken, Err("The page went away".into()));
    }
}
