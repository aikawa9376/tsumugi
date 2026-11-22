use serde::Serialize;
use serde::de::DeserializeOwned;

/// A line that does not hold the message it was read as.
#[derive(Debug, thiserror::Error)]
#[error("cannot read the message: {0}")]
pub struct DecodeError(#[from] serde_json::Error);

/// `message` as one line of JSON, ending in a newline, as it goes on the
/// wire.
pub fn encode<T: Serialize>(message: &T) -> String {
    let mut line =
        serde_json::to_string(message).expect("a protocol message always serializes to JSON");
    line.push('\n');
    line
}

/// The message one line holds, with or without its newline.
pub fn decode<T: DeserializeOwned>(line: impl AsRef<str>) -> Result<T, DecodeError> {
    Ok(serde_json::from_str(line.as_ref().trim_end_matches('\n'))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::Path;

    use tsumugi_model::Past;
    use tsumugi_model::files::Entry;
    use tsumugi_model::links::Links;
    use tsumugi_model::repository::Marks;
    use tsumugi_model::repository::{Listing, Rev, Spot};
    use tsumugi_model::testing::{session, shell};
    use tsumugi_model::usage::{Limit, Severity, Window};

    use crate::testing::{listed, patch, snapshot};
    use crate::{
        AgentFacts, AgentsWork, Change, Colours, CommandsRead, CommandsSight, Composed,
        ConfigChange, ConfigFile, ConfigWork, Configured, ConsoleChange, ConsoleWork,
        ConversationRead, DetailsChange, DetailsWork, Dialog, Event, FileText, HistoryRead, Offers,
        PastChange, PastSight, PastWork, PickChange, PickSight, PickWork, PlaceChange, PlaceRead,
        PlaceWork, PromptChange, PromptWork, RepositoryRead, RepositorySight, Request, Seq,
        SessionRead, Sight, SystemUsage, ThemeKind, ThemeListed, ThemeRead, Told, TreeChange,
        TreeSight, Typing, Version, WorkSnapshot,
    };

    fn round_trips<T>(message: T)
    where
        T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug,
    {
        let line = encode(&message);
        assert!(line.ends_with('\n'));
        assert_eq!(line.matches('\n').count(), 1, "one line: {line:?}");
        assert_eq!(decode::<T>(&line).unwrap(), message);
    }

    #[test]
    fn a_picture_a_diff_compares_is_asked_by_revision_and_path_and_changes_nothing() {
        use crate::{Answer, Ask};
        let ask = Ask::DiffPicture {
            worktree: "/project with spaces/日本語".into(),
            rev: Some("abc^".into()),
            path: "画像/a b.png".into(),
        };
        assert!(!ask.mutates());
        round_trips(ask);
        round_trips(Ask::DiffPicture {
            worktree: "/w".into(),
            rev: None,
            path: "a.png".into(),
        });
        round_trips(Answer::Picture {
            url: Some("data:image/png;base64,AA==".into()),
        });
    }

    #[test]
    fn review_comments_asked_and_changed_round_trip_and_only_changing_them_mutates() {
        use crate::{Answer, Ask};
        use tsumugi_model::repository::Rev;
        use tsumugi_model::review::{Anchor, Change, Comment, Freshness, Judged, NewComment, Side};
        let anchor = Anchor {
            path: "日本語/メモ 1.md".into(),
            side: Side::Old,
            start: 3,
            end: 4,
        };
        let worktree: std::path::PathBuf = "/project with spaces/日本語".into();
        assert!(
            !Ask::Review {
                worktree: worktree.clone()
            }
            .mutates()
        );
        let change = Ask::ChangeReview {
            worktree: worktree.clone(),
            change: Box::new(Change::Add(NewComment {
                rev: Rev::Commit("abc".into()),
                anchor: anchor.clone(),
                quoted: vec!["a\tb".into(), "".into()],
                text: "なぜ?\n理由".into(),
            })),
        };
        assert!(change.mutates());
        round_trips(change);
        round_trips(Ask::Review { worktree });
        round_trips(Answer::Review {
            comments: vec![Judged {
                comment: Comment {
                    id: 7,
                    rev: Rev::WorkingCopy,
                    anchor,
                    quoted: vec!["x".into()],
                    text: "t".into(),
                    resolved: true,
                    sent: true,
                },
                freshness: Freshness::Outdated,
            }],
        });
    }

    #[test]
    fn schedule_requests_and_listings_round_trip_and_all_change_the_host() {
        use crate::Ask;
        use tsumugi_model::{
            Cadence, NewSchedule, RunOutcome, Schedule, ScheduleEdit, ScheduleRun, ScheduleTarget,
            Weekday,
        };
        let id = tsumugi_model::SessionId::new().to_string();
        let new = NewSchedule {
            id: id.clone(),
            name: "週次の点検".into(),
            prompt: "First line\nSecond line".into(),
            dir: "/project with spaces/日本語".into(),
            profile: Some("work".into()),
            timezone: "Asia/Tokyo".into(),
            cadence: Cadence::Weekly {
                days: vec![Weekday::Mon, Weekday::Thu],
                hour: 9,
                minute: 30,
            },
            target: ScheduleTarget::Session {
                code: "ab12".into(),
            },
            grace_minutes: 720,
        };
        for ask in [
            Ask::SaveSchedule(new.clone()),
            Ask::EditSchedule {
                id: id.clone(),
                edit: ScheduleEdit {
                    profile: Some(None),
                    cadence: Some(Cadence::Hourly { minute: 5 }),
                    ..Default::default()
                },
            },
            Ask::SetScheduleEnabled {
                id: id.clone(),
                enabled: true,
            },
            Ask::RunSchedule { id: id.clone() },
            Ask::DeleteSchedule { id: id.clone() },
        ] {
            assert!(ask.mutates());
            round_trips(ask);
        }
        round_trips(TreeChange::Schedules {
            schedules: vec![Schedule {
                id,
                number: 2,
                name: new.name,
                prompt: new.prompt,
                dir: new.dir,
                profile: None,
                timezone: new.timezone,
                cadence: new.cadence,
                target: ScheduleTarget::Queue,
                enabled: true,
                grace_minutes: 720,
                next_ms: Some(1),
                last: Some(ScheduleRun {
                    due_ms: 0,
                    at_ms: 1,
                    outcome: RunOutcome::Failed,
                    issue: None,
                    note: Some("The directory is gone".into()),
                }),
                held_ms: Some(2),
            }],
        });
    }

    #[test]
    fn local_issue_requests_and_progress_round_trip_without_losing_the_request() {
        use crate::Ask;
        use tsumugi_model::{LocalIssue, LocalIssueState, NewLocalIssue};
        let new = NewLocalIssue {
            id: tsumugi_model::SessionId::new().to_string(),
            dir: "/project with spaces/日本語".into(),
            title: "直してほしい".into(),
            body: "First line\nSecond line".into(),
        };
        let ask = Ask::QueueLocalIssue(new.clone());
        assert!(ask.mutates());
        round_trips(ask);
        round_trips(Ask::CreateLocalIssue(new.clone()));
        round_trips(Ask::StartLocalIssue { id: new.id.clone() });
        round_trips(Ask::StartLeader {
            issues: vec![new.id.clone()],
            max_workers: Some(3),
        });
        round_trips(Ask::CancelLocalIssue { id: new.id.clone() });
        round_trips(Ask::RetryLocalIssue { id: new.id.clone() });
        round_trips(Ask::CloseLocalIssue { id: new.id.clone() });
        round_trips(Ask::ReopenLocalIssue { id: new.id.clone() });
        round_trips(Ask::DeleteLocalIssue { id: new.id.clone() });
        let command = tsumugi_model::NewSavedCommand {
            id: new.id.clone(),
            dir: "/project with spaces/日本語".into(),
            label: "Checks".into(),
            command: "cd 'a b'\njust ci".into(),
        };
        let ask = Ask::SaveCommand(command.clone());
        assert!(ask.mutates());
        round_trips(ask);
        let ask = Ask::DeleteSavedCommand { id: new.id.clone() };
        assert!(ask.mutates());
        round_trips(ask);
        round_trips(TreeChange::SavedCommands {
            commands: vec![tsumugi_model::SavedCommand {
                id: command.id,
                project: command.dir,
                label: command.label,
                command: command.command,
            }],
        });
        round_trips(Ask::KeepResource {
            dir: "/project with spaces/日本語".into(),
            target: "docs/設計.md".into(),
            label: Some("設計".into()),
            note: None,
        });
        round_trips(Ask::DescribeResource {
            id: "r".into(),
            label: None,
            note: Some("first line\nsecond".into()),
        });
        round_trips(Ask::DropResource { id: "r".into() });
        round_trips(Ask::KeepScreenshot {
            dir: "/project".into(),
            data: "iVBORw0KGgo=".into(),
            label: Some("Screenshot".into()),
            note: Some("https://example.com/".into()),
        });
        round_trips(TreeChange::Resources {
            resources: vec![tsumugi_model::Resource {
                id: "r".into(),
                project: "/project with spaces/日本語".into(),
                target: "https://example.com/a?b=c".into(),
                label: Some("Guide".into()),
                note: None,
            }],
        });
        round_trips(TreeChange::Notices {
            notices: vec![tsumugi_model::Notice {
                id: 7,
                kind: tsumugi_model::NoticeKind::Failed,
                session: "ffff".into(),
                name: "日本語 name".into(),
                workspace: "api".into(),
                at_ms: 1_700_000_000_000,
                reason: "line\nbreak".into(),
            }],
        });
        round_trips(TreeChange::LocalIssues {
            issues: vec![LocalIssue {
                id: new.id,
                dir: new.dir,
                title: new.title,
                body: new.body,
                state: LocalIssueState::Waiting,
                record: Some(tsumugi_model::SessionId::new()),
                session: Some("ffff".into()),
                error: None,
                schedule: None,
            }],
        });
    }

    #[test]
    fn every_request_round_trips() {
        round_trips(Request::Hello {
            versions: vec![Version(1), Version(2)],
        });
        round_trips(Request::Snapshot);
        round_trips(Request::Watch(Box::default()));
        round_trips(Request::ReadAgain);
        round_trips(Request::Watch(Box::new(Sight {
            session: Some("s".into()),
            conversations: BTreeSet::from(["s".into(), "other".into()]),
            details: true,
            overview: true,
            repositories: vec![RepositorySight {
                worktree: "/w".into(),
                spot: Spot::Files(Rev::Commit("abc".into())),
            }],
            usage: Some(crate::Account {
                agent: tsumugi_model::Agent::Codex,
                home: Some("/w".into()),
            }),
            agents: true,
            system: Some(tsumugi_model::config::UsageSettings {
                gpu: false,
                ..Default::default()
            }),
            tree: TreeSight {
                every_worktree: true,
                marked: BTreeSet::from(["/w".into()]),
                listed: BTreeSet::from(["/w".into(), "/w/src".into()]),
                github: BTreeSet::from([("/w".into(), tsumugi_model::github::Listing::Issues)]),
            },
            past: Some(PastSight {
                dir: Some("/w".into()),
            }),
            pick: Some(PickSight {
                listed: BTreeSet::from(["/home".into()]),
                owners: BTreeSet::from(["aikawa9376".into()]),
                accounts: true,
                files: BTreeSet::from(["/w".into()]),
            }),
            place: Some(crate::HostPath::local("/w")),
            commands: Some(CommandsSight {
                agent: tsumugi_model::Agent::Codex,
                home: Some("/w".into()),
                dir: "/w".into(),
            }),
            terminals: BTreeSet::from([u128::MAX, 7]),
        })));
        round_trips(Request::Relist);
        round_trips(Request::Call {
            name: "作業".into(),
        });
        round_trips(Request::Typed {
            inputs: BTreeMap::from([(u128::MAX, 1_700_000_000_000)]),
        });
        round_trips(Request::TerminalOpened {
            terminal: shell(u128::MAX - 1, "/w"),
        });
        round_trips(Request::TerminalClosed { id: u128::MAX });
        round_trips(Request::Ask {
            asked: 9,
            ask: Box::new(crate::Ask::LoginStanding {
                profile: "claude-work".parse().unwrap(),
                after: Some(u128::MAX),
            }),
        });
        round_trips(Request::Ask {
            asked: 10,
            ask: Box::new(crate::Ask::OpenLogin {
                profile: "claude".parse().unwrap(),
                cols: 80,
                rows: 24,
            }),
        });
        round_trips(Request::Ask {
            asked: 11,
            ask: Box::new(crate::Ask::DeleteLogin {
                profile: "claude-work".parse().unwrap(),
            }),
        });
        round_trips(Request::Ask {
            asked: 12,
            ask: Box::new(crate::Ask::Clone {
                typed: "git@github.com:o/r.git".into(),
                cols: 80,
                rows: 24,
            }),
        });
        round_trips(Request::Ask {
            asked: 13,
            ask: Box::new(crate::Ask::Cloned {
                path: "github.com/o/r".into(),
                after: u128::MAX,
            }),
        });
        round_trips(Request::Ask {
            asked: 12,
            ask: Box::new(crate::Ask::PullWorktree {
                repository: "/w".into(),
                number: 7,
            }),
        });
        let change = tsumugi_model::repository::Change {
            status: "R ".into(),
            path: "新しい name.rs".into(),
            from: Some("old.rs".into()),
        };
        for op in [
            crate::GitOp::Stage {
                changes: vec![change.clone()],
            },
            crate::GitOp::Unstage {
                changes: vec![change.clone()],
            },
            crate::GitOp::Discard {
                changes: vec![change],
            },
            crate::GitOp::StageAll,
            crate::GitOp::Commit {
                message: "subject\n\n日本語 body".into(),
                amend: true,
            },
            crate::GitOp::Push { lease: true },
            crate::GitOp::Pull,
            crate::GitOp::Abort,
            crate::GitOp::Resume,
            crate::GitOp::ChooseBase {
                base: Some("origin/日本語".into()),
            },
            crate::GitOp::ChooseBase { base: None },
            crate::GitOp::CherryPick {
                commit: "a".repeat(40),
            },
            crate::GitOp::Revert {
                commit: "b".repeat(40),
            },
            crate::GitOp::CreateBranch {
                name: "feat/日本語".into(),
                commit: "c".repeat(40),
            },
            crate::GitOp::CreatePull {
                base: Some("main".into()),
                title: "core: 日本語".into(),
                body: "Why.\n\n- one".into(),
                draft: true,
            },
        ] {
            round_trips(Request::Ask {
                asked: 14,
                ask: Box::new(crate::Ask::Git {
                    worktree: "/w".into(),
                    op,
                }),
            });
        }
        round_trips(Request::Ask {
            asked: 14,
            ask: Box::new(crate::Ask::CreateWorktree {
                repository: "/w".into(),
                new: tsumugi_model::NewWorktree {
                    name: "fix login".into(),
                    start: tsumugi_model::StartPoint::Remote {
                        remote: "origin".into(),
                        branch: "main".into(),
                    },
                    branch: Some("feat/login".into()),
                },
            }),
        });
        round_trips(Request::Ask {
            asked: 18,
            ask: Box::new(crate::Ask::SetupWorktree {
                repository: "/w".into(),
                worktree: "/w-wt/fix-login".into(),
                choice: tsumugi_model::SetupChoice::Run,
            }),
        });
        round_trips(Request::Ask {
            asked: 15,
            ask: Box::new(crate::Ask::DeleteWorktree {
                path: "/w-wt/fix-login".into(),
                force: true,
                branch: tsumugi_model::BranchFate::DeleteUnmerged,
            }),
        });
        round_trips(Request::Ask {
            asked: 16,
            ask: Box::new(crate::Ask::WorktreeStarts {
                repository: "/w".into(),
            }),
        });
        round_trips(Request::Ask {
            asked: 17,
            ask: Box::new(crate::Ask::WorktreeStanding {
                path: "/w-wt/fix-login".into(),
            }),
        });
        round_trips(Request::Ask {
            asked: 20,
            ask: Box::new(crate::Ask::Sleep {
                ids: vec!["7".into(), "9".into()],
            }),
        });
        round_trips(Event::Answer {
            asked: 20,
            answer: crate::Answer::Slept {
                count: 1,
                left: vec!["review".into()],
            },
        });
        round_trips(Request::Ask {
            asked: 19,
            ask: Box::new(crate::Ask::MergeStanding {
                path: "/w-wt/fix-login".into(),
            }),
        });
        for changes in [
            tsumugi_model::MainChanges::Refuse,
            tsumugi_model::MainChanges::Stash,
            tsumugi_model::MainChanges::Commit,
        ] {
            round_trips(Request::Ask {
                asked: 20,
                ask: Box::new(crate::Ask::MergeWorktree {
                    path: "/w-wt/fix-login".into(),
                    changes,
                    message: Some("treewide: merge 日本語".into()),
                    close_issue: true,
                }),
            });
        }
        round_trips(Request::Ask {
            asked: 21,
            ask: Box::new(crate::Ask::ResolveMerge {
                path: "/w-wt/fix-login".into(),
            }),
        });
        round_trips(Request::Ask {
            asked: 24,
            ask: Box::new(crate::Ask::CleanupEntries {
                repository: "/w".into(),
            }),
        });
        round_trips(Event::Answer {
            asked: 24,
            answer: crate::Answer::CleanupEntries {
                entries: vec![tsumugi_model::CleanupEntry {
                    path: "/w-wt/fix-login".into(),
                    standing: tsumugi_model::WorktreeStanding {
                        branch: Some("fix-login".into()),
                        changed: 1,
                        ..Default::default()
                    },
                    bytes: 4096,
                    merged: true,
                    last_commit: Some(1_700_000_000),
                    pull: Some(tsumugi_model::CleanupPull {
                        number: 4,
                        state: tsumugi_model::PullState::Merged,
                    }),
                }],
            },
        });
        round_trips(Request::Ask {
            asked: 13,
            ask: Box::new(crate::Ask::GitHubThread {
                repository: "/w".into(),
                listing: tsumugi_model::github::Listing::Issues,
                number: 3,
            }),
        });
        round_trips(Request::Ask {
            asked: 14,
            ask: Box::new(crate::Ask::GenerateText {
                worktree: "/w-wt/日本語".into(),
                action: tsumugi_model::AiAction::PullRequest,
                profile: Some("codex".parse().unwrap()),
                args: vec!["--model".into(), "x y".into()],
                template: "{basePrompt}\n日本語".into(),
            }),
        });
        round_trips(Request::Ask {
            asked: 15,
            ask: Box::new(crate::Ask::GitHubFixContext {
                repository: "/w".into(),
                number: 3,
                checks: true,
            }),
        });
        round_trips(Event::Answer {
            asked: 15,
            answer: crate::Answer::FixContext {
                context: tsumugi_model::github::FixContext {
                    head: "abc".into(),
                    checks: vec![
                        tsumugi_model::github::FixCheck {
                            check: tsumugi_model::github::Check {
                                name: "テスト".into(),
                                ..Default::default()
                            },
                            log: tsumugi_model::github::Log::Excerpt {
                                text: "error".into(),
                                cut: true,
                            },
                        },
                        tsumugi_model::github::FixCheck {
                            check: Default::default(),
                            log: tsumugi_model::github::Log::Unread { why: "no".into() },
                        },
                    ],
                },
            },
        });
        round_trips(Request::Ask {
            asked: 16,
            ask: Box::new(crate::Ask::GitHubCheckLog {
                repository: "/w".into(),
                job: 42,
            }),
        });
        round_trips(Event::Answer {
            asked: 16,
            answer: crate::Answer::CheckLog {
                log: tsumugi_model::github::CheckLog {
                    log: tsumugi_model::github::Log::Excerpt {
                        text: "error: 日本語".into(),
                        cut: true,
                    },
                    first_error: Some(0),
                },
            },
        });
        for write in [
            tsumugi_model::github::Write::Rerun { run: 9 },
            tsumugi_model::github::Write::Comment {
                body: "日本語 -x".into(),
            },
            tsumugi_model::github::Write::Reply {
                thread: "PRRT_1".into(),
                body: "Done.".into(),
            },
            tsumugi_model::github::Write::Resolve {
                thread: "PRRT_1".into(),
                resolved: true,
            },
            tsumugi_model::github::Write::React {
                subject: "IC_1".into(),
                content: "+1".into(),
                add: false,
            },
            tsumugi_model::github::Write::Review {
                verdict: tsumugi_model::github::Verdict::RequestChanges,
                body: "Fix.".into(),
            },
            tsumugi_model::github::Write::Viewed {
                pull: "PR_1".into(),
                path: "a.rs".into(),
                viewed: true,
            },
            tsumugi_model::github::Write::Merge {
                method: tsumugi_model::github::Method::Squash,
                head: "abc".into(),
            },
            tsumugi_model::github::Write::Auto {
                method: tsumugi_model::github::Method::Rebase,
                enabled: true,
            },
            tsumugi_model::github::Write::Close { closed: true },
            tsumugi_model::github::Write::Draft { draft: false },
        ] {
            round_trips(Request::Ask {
                asked: 14,
                ask: Box::new(crate::Ask::GitHubWrite {
                    repository: "/w".into(),
                    listing: tsumugi_model::github::Listing::Pulls,
                    number: 3,
                    write,
                }),
            });
        }
        round_trips(Request::History {
            asked: 3,
            agent: tsumugi_model::Agent::Claude,
            home: Some("/w".into()),
            project: "/w".into(),
        });
        round_trips(Request::CommandsWanted);
        round_trips(Request::Redrawn {
            session: "0a".into(),
            agent: tsumugi_model::Agent::Codex,
        });
        let session = Box::new(session("0a").name("fix").build());
        round_trips(Request::Stop {
            session: session.clone(),
        });
        round_trips(Request::Kill {
            session,
            process: tsumugi_model::Process {
                pid: 7,
                ..Default::default()
            },
        });
        round_trips(Request::Rename {
            id: "0a".into(),
            name: "fixer".into(),
            was: "fix".into(),
            agent: tsumugi_model::Agent::Claude,
        });
        round_trips(Request::HandBackName { id: "0a".into() });
        round_trips(Request::Compose {
            asked: 4,
            agent: tsumugi_model::Agent::Codex,
            text: "look".into(),
            images: vec!["/p/shot.png".into()],
        });
    }

    #[test]
    fn every_event_round_trips() {
        round_trips(Event::Welcome {
            version: Version(1),
        });
        round_trips(Event::Refused {
            versions: vec![Version(3)],
        });
        round_trips(Event::Told(Told::Notice {
            text: "ended fix".into(),
        }));
        round_trips(Event::Answer {
            asked: 12,
            answer: crate::Answer::PullCreated {
                number: 7,
                url: "https://github.com/o/r/pull/7".into(),
            },
        });
        round_trips(Event::Answer {
            asked: 13,
            answer: crate::Answer::Thread {
                thread: Box::new(tsumugi_model::github::Thread {
                    state: "open".into(),
                    body: tsumugi_model::github::Post {
                        author: "me".into(),
                        body: "修正 ![a](https://x/a.png)".into(),
                        ..Default::default()
                    },
                    said: vec![tsumugi_model::github::Post {
                        did: tsumugi_model::github::Did::ChangesRequested,
                        ..Default::default()
                    }],
                    images: vec![tsumugi_model::github::Image {
                        address: "https://x/a.png".into(),
                        mime: "image/png".into(),
                        data: "iVBORw0K".into(),
                    }],
                    files: vec![tsumugi_model::github::FileChange {
                        path: "a.rs".into(),
                        patch: Some("@@ -1 +1 @@\n-a\n+b".into()),
                        ..Default::default()
                    }],
                    checks: vec![tsumugi_model::github::Check {
                        name: "lint".into(),
                        outcome: tsumugi_model::github::Outcome::Failed,
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
            },
        });
        round_trips(Event::Told(Told::Failure {
            text: "cannot end fix".into(),
        }));
        round_trips(Event::Type {
            session: "0a".into(),
            typing: vec![Typing::Paste {
                text: "/rename fixer".into(),
            }],
        });
        round_trips(Event::from(snapshot().seq(7)));
        round_trips(Event::Snapshot(Box::new(WorkSnapshot {
            seq: Seq(7),
            agents: AgentsWork {
                agents: vec![AgentFacts {
                    agent: tsumugi_model::Agent::Codex,
                    program: "codex".into(),
                    usage_url: Some("https://chatgpt.com/settings/usage".into()),
                    usage_limits: true,
                }],
            },
            details: DetailsWork {
                sessions: BTreeMap::from([(
                    "s".to_string(),
                    SessionRead {
                        links: Some(Links::default()),
                        thumbs: BTreeMap::from([("/w/a.png".to_string(), Some("data:".into()))]),
                        ..Default::default()
                    },
                )]),
                usage: BTreeMap::from([(
                    crate::Account {
                        agent: tsumugi_model::Agent::Claude,
                        home: Some("/w".into()),
                    },
                    Vec::new(),
                )]),
                unavailable: [crate::Account {
                    agent: tsumugi_model::Agent::Codex,
                    home: None,
                }]
                .into(),
                system: Some(SystemUsage::default()),
                ..Default::default()
            },
            tree: Default::default(),
            past: PastWork {
                listed: Some((
                    PastSight::default(),
                    vec![Past {
                        agent: tsumugi_model::Agent::Codex,
                        id: "019f7a4b".into(),
                        cwd: "/w".into(),
                        title: "review".into(),
                        at_ms: 1,
                        ..Default::default()
                    }],
                )),
            },
            pick: PickWork {
                offers: Some(Offers::default()),
                owned: [("aikawa9376".into(), vec!["tsumugi".into()])].into(),
                accounts: Some(vec!["aikawa9376".into()]),
                files: [("/w".into(), vec!["src/main.rs".into()])].into(),
            },
            place: PlaceWork {
                read: Some(PlaceRead::default()),
            },
            prompt: PromptWork {
                history: Some(HistoryRead::default()),
                commands: Some(CommandsRead {
                    agent: tsumugi_model::Agent::Claude,
                    home: None,
                    dir: "/w".into(),
                    commands: vec![tsumugi_model::Command {
                        name: "compact".into(),
                        description: "Clear history but keep a summary".into(),
                        argument_hint: String::new(),
                    }],
                }),
                composed: Some(Composed::default()),
            },
            config: ConfigWork {
                settings: Some(ConfigFile::default()),
                keys: None,
                configured: Some(Configured {
                    scratch: "/s".into(),
                    theme: ThemeRead {
                        name: "Mine".into(),
                        colours: Colours::Theme {
                            text: "background = #010203\n".into(),
                        },
                    },
                    themes: vec![ThemeListed {
                        name: "Mine".into(),
                        kind: ThemeKind::Light,
                    }],
                    system: ThemeKind::Light,
                    themes_dir: "/c/themes".into(),
                    login_shell: "/bin/zsh".into(),
                    home: "/home/me".into(),
                }),
            },
            console: ConsoleWork {
                dialog: Some(Dialog {
                    session: "0a".into(),
                    open: false,
                }),
            },
        })));
        round_trips(Event::from(patch(8)));
        let details = |change| Change::Details(change);
        round_trips(Event::from(
            patch(9).changes([
                details(DetailsChange::Conversation {
                    session: "s".into(),
                    conversation: ConversationRead::default(),
                }),
                details(DetailsChange::Repository {
                    worktree: "/w".into(),
                    read: RepositoryRead {
                        spot: Spot::Log,
                        listing: Err("not a repository".into()),
                        revision: None,
                    },
                }),
                details(DetailsChange::Repository {
                    worktree: "/w".into(),
                    read: RepositoryRead {
                        spot: Spot::Log,
                        listing: Ok(Listing::Log(Vec::new())),
                        revision: None,
                    },
                }),
                details(DetailsChange::UsageUnavailable {
                    account: crate::Account {
                        agent: tsumugi_model::Agent::Codex,
                        home: None,
                    },
                }),
                details(DetailsChange::Usage {
                    account: crate::Account {
                        agent: tsumugi_model::Agent::Claude,
                        home: Some("/w".into()),
                    },
                    limits: vec![Limit {
                        window: Window::WeekOf("Opus".into()),
                        percent: 12.5,
                        resets_ms: Some(1),
                        severity: Severity::Warning,
                    }],
                }),
                details(DetailsChange::SessionLeft {
                    session: "s".into(),
                }),
                Change::Tree(TreeChange::Listed {
                    listed: Box::new(
                        listed()
                            .session(session("s").cwd("/w"))
                            .terminal(shell(u128::MAX, "/w/src"))
                            .build(),
                    ),
                }),
                Change::Tree(TreeChange::Failing {
                    error: Some("no daemon".into()),
                }),
                Change::Tree(TreeChange::Marks {
                    worktree: "/w".into(),
                    marks: Marks::new(
                        Path::new("/w"),
                        &[tsumugi_model::repository::Change {
                            status: " M".into(),
                            path: "a.rs".into(),
                            from: None,
                        }],
                        &["target/".into()],
                    ),
                }),
                Change::Tree(TreeChange::Entries {
                    dir: "/w".into(),
                    entries: vec![Entry {
                        path: "/w/a.rs".into(),
                        dir: false,
                    }],
                }),
                Change::Tree(TreeChange::EntriesLeft { dir: "/w".into() }),
                Change::Tree(TreeChange::GitHub {
                    repository: "/w".into(),
                    github: Some(Box::new(tsumugi_model::github::Repository {
                        slug: "o/r".into(),
                        pulls: Some(Ok(vec![tsumugi_model::github::Item {
                            number: 1,
                            title: "Draft".into(),
                            url: "https://github.com/o/r/pull/1".into(),
                            author: "me".into(),
                            draft: true,
                            created_at: "2026-11-01T00:00:00Z".into(),
                            labels: vec![tsumugi_model::github::Label {
                                name: "bug".into(),
                                color: "d73a4a".into(),
                            }],
                            pull: Some(tsumugi_model::github::Pull {
                                head: "fix".into(),
                                base: "main".into(),
                                review: Some(tsumugi_model::github::Review::Approved),
                                checks: tsumugi_model::github::Checks {
                                    failed: 1,
                                    failing: vec!["lint".into()],
                                    ..Default::default()
                                },
                                ..Default::default()
                            }),
                            ..Default::default()
                        }])),
                        issues: Some(Err("gh is not logged in".into())),
                        open: Some(tsumugi_model::github::Counts {
                            pulls: 1,
                            issues: 40,
                        }),
                    })),
                }),
                Change::Tree(TreeChange::GitHub {
                    repository: "/v".into(),
                    github: None,
                }),
                Change::Config(ConfigChange::Settings {
                    file: ConfigFile {
                        path: "/c/settings.toml".into(),
                        text: FileText::Read {
                            text: "zoom = 1.25\n".into(),
                        },
                    },
                }),
                Change::Config(ConfigChange::Keys {
                    file: ConfigFile {
                        path: "/c/keys.toml".into(),
                        text: FileText::Unreadable {
                            error: "denied".into(),
                        },
                    },
                }),
                Change::Config(ConfigChange::Keys {
                    file: ConfigFile::default(),
                }),
                Change::Tree(TreeChange::MarksLeft {
                    worktree: "/w".into(),
                }),
                Change::Past(PastChange::Listed {
                    sight: PastSight {
                        dir: Some("/w".into()),
                    },
                    conversations: Vec::new(),
                }),
                Change::Past(PastChange::Left),
                Change::Pick(PickChange::Offers {
                    offers: Offers {
                        recent: vec!["/w".into()],
                        ghq: vec!["/g/r".into()],
                        clones: true,
                        listed: vec!["/l/a".into()],
                        roots: vec!["/g".into(), "/l".into()],
                    },
                }),
                Change::Pick(PickChange::Left),
                Change::Place(PlaceChange::Read {
                    read: PlaceRead {
                        worktree: crate::HostPath::local("/w"),
                        files: vec!["src/a.rs".into()],
                    },
                }),
                Change::Place(PlaceChange::Left),
                Change::Prompt(PromptChange::History {
                    read: HistoryRead {
                        asked: 1,
                        project: "/w".into(),
                        sent: vec!["fix".into()],
                    },
                }),
                Change::Prompt(PromptChange::Commands {
                    read: CommandsRead {
                        agent: tsumugi_model::Agent::Codex,
                        home: None,
                        dir: "/w".into(),
                        commands: Vec::new(),
                    },
                }),
                Change::Console(ConsoleChange::Dialog {
                    dialog: Dialog {
                        session: "0a".into(),
                        open: true,
                    },
                }),
                Change::Prompt(PromptChange::Composed {
                    composed: Composed {
                        asked: 4,
                        shell: true,
                        image_missing: false,
                        typing: vec![
                            Typing::Text { text: "!".into() },
                            Typing::Pause { ms: 40 },
                            Typing::Paste { text: "ls".into() },
                            Typing::Key {
                                name: "Enter".into(),
                            },
                        ],
                    },
                }),
            ]),
        ));
    }

    #[test]
    fn the_handshake_reads_the_same_in_every_version() {
        // A peer reads the handshake before it knows which version the other
        // speaks, so its shape must never change.
        let cases = [
            (
                encode(&Request::Hello {
                    versions: vec![Version(1)],
                }),
                "{\"type\":\"hello\",\"versions\":[1]}\n",
            ),
            (
                encode(&Event::Welcome {
                    version: Version(1),
                }),
                "{\"type\":\"welcome\",\"version\":1}\n",
            ),
            (
                encode(&Event::Refused {
                    versions: vec![Version(2)],
                }),
                "{\"type\":\"refused\",\"versions\":[2]}\n",
            ),
        ];
        for (line, want) in cases {
            assert_eq!(line, want);
        }
    }

    #[test]
    fn a_line_that_is_not_a_message_is_an_error() {
        assert!(decode::<Request>("{\"type\":\"no_such\"}").is_err());
        assert!(decode::<Event>("not json").is_err());
    }
}
