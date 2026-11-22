//! The conversations every agent kept in each login, listed while the
//! window shows the list of them: read again each time it changes which directory it lists, or
//! asks for everything read again, and every half minute meanwhile.

use std::time::Duration;

use tsumugi_model::newest_first;
use tsumugi_protocol::PastChange;

use super::{Watched, distinct};

pub(super) async fn watch(mut watched: Watched) {
    loop {
        for sight in distinct(&watched.sights(), |s| s.past.clone()) {
            let dir = sight.dir.clone();
            let read = crate::environment::spawn_blocking(move || {
                let (settings, home) = (crate::settings::current(), crate::paths::home());
                let pasts = crate::programs::logins(&settings)
                    .iter()
                    .flat_map(|c| tsumugi_agents::adapter(c.agent()).past(c, &home, dir.as_deref()))
                    .collect();
                newest_first(pasts)
            })
            .await;
            if let Ok(conversations) = read {
                watched.record([PastChange::Listed {
                    sight,
                    conversations,
                }]);
            }
        }
        watched
            .wait(Duration::from_secs(30), |seen| {
                seen.iter()
                    .map(|s| s.sight.past.clone())
                    .collect::<Vec<_>>()
            })
            .await;
    }
}

/// What the conversation `past` says, read from the file its agent keeps it
/// in. The conversation is found again among those its login keeps, so a
/// file named by whoever asks is never read.
pub(crate) async fn preview(past: tsumugi_model::Past) -> Option<tsumugi_model::Preview> {
    let read = crate::environment::spawn_blocking(move || {
        let (settings, home) = (crate::settings::current(), crate::paths::home());
        found(&crate::programs::logins(&settings), &home, &past)
    });
    tokio::time::timeout(Duration::from_secs(5), read)
        .await
        .ok()?
        .ok()?
}

fn found(
    logins: &[tsumugi_agents::AgentContext],
    home: &std::path::Path,
    past: &tsumugi_model::Past,
) -> Option<tsumugi_model::Preview> {
    logins
        .iter()
        .filter(|c| c.agent() == past.agent && c.home == past.home)
        .find_map(|c| {
            let adapter = tsumugi_agents::adapter(c.agent());
            let kept = adapter
                .past(c, home, Some(&past.cwd))
                .into_iter()
                .find(|kept| kept.id == past.id)?;
            adapter.preview(&kept)
        })
}

#[cfg(test)]
mod tests {
    use tsumugi_agents::AgentContext;
    use tsumugi_model::{Agent, Past, Speaker};

    use super::found;

    fn kept(home: &std::path::Path) -> std::path::PathBuf {
        let folder = home.join(".claude/projects/-r");
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("c1.jsonl");
        let line = |v: serde_json::Value| format!("{v}\n");
        std::fs::write(
            &path,
            line(serde_json::json!({"type": "user", "cwd": "/r",
                "message": {"content": "hello"}}))
                + &line(serde_json::json!({"type": "assistant",
                    "message": {"content": [{"type": "text", "text": "hi"}]}})),
        )
        .unwrap();
        path
    }

    fn asked(id: &str, log: Option<std::path::PathBuf>) -> Past {
        Past {
            agent: Agent::Claude,
            id: id.into(),
            cwd: "/r".into(),
            log,
            ..Default::default()
        }
    }

    #[test]
    fn a_conversation_is_previewed_from_the_file_its_login_keeps_it_in() {
        let home = tempfile::tempdir().unwrap();
        kept(home.path());
        let logins = [AgentContext::builtin(Agent::Claude)];
        let got = found(&logins, home.path(), &asked("c1", None)).unwrap();
        assert_eq!(got.first, "hello");
        assert_eq!(got.recent.last().map(|t| t.speaker), Some(Speaker::Agent));
    }

    #[test]
    fn a_file_the_asker_names_is_not_read_for_a_conversation_no_login_keeps() {
        let home = tempfile::tempdir().unwrap();
        let secret = home.path().join("secret.jsonl");
        std::fs::write(
            &secret,
            "{\"type\":\"user\",\"message\":{\"content\":\"private\"}}\n",
        )
        .unwrap();
        kept(home.path());
        let logins = [AgentContext::builtin(Agent::Claude)];
        assert_eq!(
            found(&logins, home.path(), &asked("nope", Some(secret))),
            None
        );
        // Nor is one kept by another agent's login.
        let codex = [AgentContext::builtin(Agent::Codex)];
        assert_eq!(found(&codex, home.path(), &asked("c1", None)), None);
    }
}
