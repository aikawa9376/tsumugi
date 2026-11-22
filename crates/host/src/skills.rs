//! `tsumugi skills`: the skills tsumugi ships, listed, read and put into the
//! skills folders of the profiles' logins.

use std::io::Write;
use std::path::PathBuf;

use serde::Serialize;
use tsumugi_agents::skills::{Action, Outcome, Shipped, Status, Step};
use tsumugi_model::profile::AgentProfile;

pub const USAGE: &str = "usage: tsumugi skills <list | get <topic> [--full] | install | update> [--skill <name>]… [--all] [--profile <id>[,<id>…]]… [--force] [--dry-run] [--json]";

/// What the commands act on: the user's profiles and where things are kept.
pub struct Env {
    pub profiles: Vec<AgentProfile>,
    /// The user's home directory.
    pub home: PathBuf,
    /// Where a copy replaced is moved to.
    pub backups: PathBuf,
    /// The tsumugi the guides tell agents to run.
    pub program: PathBuf,
}

/// Run `tsumugi skills` with `args`; the exit code.
pub fn cli(args: &[String]) -> i32 {
    let settings = match crate::settings::try_load() {
        Ok(settings) => settings,
        Err(error) => return crate::cli::failed(error, 1),
    };
    let env = Env {
        profiles: settings.profiles,
        home: tsumugi_model::paths::home(),
        backups: tsumugi_model::paths::state_dir().join("skill-backups"),
        program: tsumugi_sys::process::executable().unwrap_or_else(|_| "tsumugi".into()),
    };
    run(args, &env, &mut std::io::stdout(), &mut std::io::stderr())
}

enum Asked {
    List {
        json: bool,
    },
    Get {
        topic: String,
        json: bool,
    },
    Change {
        update: bool,
        skills: Vec<String>,
        all: bool,
        profiles: Vec<String>,
        force: bool,
        dry_run: bool,
        json: bool,
    },
}

/// What `args` ask for; `None` for what the usage does not allow.
fn asked(args: &[String]) -> Option<Asked> {
    let (verb, mut rest) = args.split_first()?;
    let (mut json, mut full, mut all, mut force, mut dry_run) = (false, false, false, false, false);
    let (mut skills, mut profiles, mut topic) = (Vec::new(), Vec::new(), None);
    let change = matches!(verb.as_str(), "install" | "update");
    let get = matches!(verb.as_str(), "get" | "show");
    while let [word, after @ ..] = rest {
        rest = after;
        match word.as_str() {
            "--json" => json = true,
            "--full" if get => full = true,
            "--all" if change => all = true,
            "--force" if change => force = true,
            "--dry-run" if change => dry_run = true,
            "--skill" | "--profile" if change => {
                let (value, after) = rest.split_first()?;
                rest = after;
                if word == "--skill" {
                    skills.push(value.clone());
                } else {
                    profiles.extend(value.split(',').map(str::to_string));
                }
            }
            name if get && topic.is_none() && !name.starts_with("--") => {
                topic = Some(name.to_string());
            }
            _ => return None,
        }
    }
    // Every guide is whole: there are no references beside it to ask for.
    let _ = full;
    match verb.as_str() {
        "list" => Some(Asked::List { json }),
        "get" | "show" => Some(Asked::Get {
            topic: topic?,
            json,
        }),
        "install" | "update" => Some(Asked::Change {
            update: verb == "update",
            skills,
            all,
            profiles,
            force,
            dry_run,
            json,
        }),
        _ => None,
    }
}

/// Run `args` against `env`, writing the answer to `out` and what went wrong
/// to `err`; the exit code: 0, 1 when a folder failed or changed while it
/// ran, 2 for what was asked wrongly.
pub fn run(args: &[String], env: &Env, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let Some(asked) = asked(args) else {
        let _ = writeln!(err, "{USAGE}");
        return 2;
    };
    let shipped = tsumugi_agents::skills::shipped();
    let refused = |err: &mut dyn Write, text: String| {
        let _ = writeln!(err, "tsumugi: {text}");
        2
    };
    let known = || {
        shipped
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    match asked {
        Asked::List { json } => {
            show_names(&shipped, json, out);
            0
        }
        Asked::Get { topic, json } => {
            let Some(skill) = shipped.iter().find(|s| s.name == topic) else {
                return refused(
                    err,
                    format!("no skill `{topic}`; the skills are {}", known()),
                );
            };
            let guide = tsumugi_agents::skills::guide(skill, &env.program);
            if json {
                print_json(
                    out,
                    &serde_json::json!({
                        "name": skill.name,
                        "description": skill.description,
                        "content": guide,
                    }),
                );
            } else {
                let _ = write!(out, "{guide}");
            }
            0
        }
        Asked::Change {
            update,
            skills,
            all,
            profiles,
            force,
            dry_run,
            json,
        } => {
            if skills.is_empty() && !all {
                show_names(&shipped, json, out);
                return 0;
            }
            let chosen: Vec<Shipped> = if all {
                shipped.clone()
            } else {
                let mut chosen = Vec::new();
                for name in &skills {
                    match shipped.iter().find(|s| &s.name == name) {
                        Some(skill) => chosen.push(skill.clone()),
                        None => {
                            return refused(
                                err,
                                format!("no skill `{name}`; the skills are {}", known()),
                            );
                        }
                    }
                }
                chosen
            };
            let targets = match targets(env, &profiles) {
                Ok(targets) => targets,
                Err(text) => return refused(err, text),
            };
            let plan: Vec<(Vec<String>, Vec<Step>)> = targets
                .into_iter()
                .map(|(profiles, folder)| {
                    (
                        profiles,
                        tsumugi_agents::skills::plan(&chosen, &folder, update, force),
                    )
                })
                .collect();
            change(&chosen, plan, env, dry_run, json, out)
        }
    }
}

fn show_names(shipped: &[Shipped], json: bool, out: &mut dyn Write) {
    if json {
        let topics: Vec<_> = shipped
            .iter()
            .map(|s| serde_json::json!({"name": s.name, "description": s.description}))
            .collect();
        print_json(out, &serde_json::json!({"topics": topics}));
    } else {
        for skill in shipped {
            let _ = writeln!(out, "{}  {}", skill.name, skill.description);
        }
    }
}

fn print_json(out: &mut dyn Write, value: &serde_json::Value) {
    let _ = writeln!(
        out,
        "{}",
        serde_json::to_string_pretty(value).expect("JSON")
    );
}

/// The profiles a change reaches, grouped by the skills folder they share:
/// those `named`, else every one whose login folder is there already.
fn targets(env: &Env, named: &[String]) -> Result<Vec<(Vec<String>, PathBuf)>, String> {
    let mut chosen = Vec::new();
    if named.is_empty() {
        chosen.extend(env.profiles.iter());
    } else {
        for id in named {
            let Some(profile) = env.profiles.iter().find(|p| p.id.as_str() == id) else {
                let ids: Vec<_> = env.profiles.iter().map(|p| p.id.as_str()).collect();
                return Err(format!(
                    "no profile `{id}`; the profiles are {}",
                    ids.join(", ")
                ));
            };
            chosen.push(profile);
        }
    }
    let mut grouped: Vec<(Vec<String>, PathBuf)> = Vec::new();
    for profile in chosen {
        let context = tsumugi_agents::AgentContext::new(profile, "");
        let Some(folder) = tsumugi_agents::skills::skills_folder(&context, &env.home) else {
            continue;
        };
        if named.is_empty() && !folder.parent().is_some_and(|state| state.is_dir()) {
            continue;
        }
        let id = profile.id.to_string();
        match grouped.iter_mut().find(|(_, shared)| *shared == folder) {
            Some((ids, _)) => {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
            None => grouped.push((vec![id], folder)),
        }
    }
    if grouped.is_empty() {
        return Err("no profile has a login folder here yet; name one with --profile".to_string());
    }
    Ok(grouped)
}

/// A step, with the profiles whose agents read its folder.
#[derive(Serialize)]
struct Planned {
    profiles: Vec<String>,
    #[serde(flatten)]
    step: Step,
}

/// What came of a step, with the profiles whose agents read its folder.
#[derive(Serialize)]
struct Done {
    profiles: Vec<String>,
    #[serde(flatten)]
    outcome: Outcome,
}

/// 1 when a folder failed or changed while it ran; folders left alone are
/// warnings.
fn exit_code(outcomes: &[Outcome]) -> i32 {
    i32::from(
        outcomes
            .iter()
            .any(|o| matches!(o.status, Status::Failed | Status::Changed)),
    )
}

fn change(
    chosen: &[Shipped],
    plan: Vec<(Vec<String>, Vec<Step>)>,
    env: &Env,
    dry_run: bool,
    json: bool,
    out: &mut dyn Write,
) -> i32 {
    let planned: Vec<Planned> = plan
        .iter()
        .flat_map(|(profiles, steps)| {
            steps.iter().map(|step| Planned {
                profiles: profiles.clone(),
                step: step.clone(),
            })
        })
        .collect();
    let digest =
        tsumugi_agents::skills::digest(serde_json::to_string(&planned).expect("JSON").as_bytes());
    let done: Vec<Done> = if dry_run {
        Vec::new()
    } else {
        plan.iter()
            .flat_map(|(profiles, steps)| {
                tsumugi_agents::skills::apply(chosen, steps, &env.backups)
                    .into_iter()
                    .map(|outcome| Done {
                        profiles: profiles.clone(),
                        outcome,
                    })
            })
            .collect()
    };
    if json {
        let mut object = serde_json::json!({
            "dry_run": dry_run,
            "digest": digest,
            "plan": planned,
        });
        if !dry_run {
            object["results"] = serde_json::to_value(&done).expect("JSON");
        }
        print_json(out, &object);
    } else if dry_run {
        for item in &planned {
            let _ = writeln!(
                out,
                "{}  {}  {}  ({})",
                item.profiles.join(","),
                item.step.skill,
                planned_label(&item.step),
                item.step.folder.display()
            );
        }
        let _ = writeln!(out, "plan digest: {digest}");
    } else {
        for item in &done {
            let _ = writeln!(
                out,
                "{}  {}  {}  ({})",
                item.profiles.join(","),
                item.outcome.step.skill,
                done_label(&item.outcome),
                item.outcome.step.folder.display()
            );
        }
    }
    exit_code(&done.into_iter().map(|d| d.outcome).collect::<Vec<_>>())
}

fn planned_label(step: &Step) -> String {
    match step.action {
        Action::Write => "Will install".into(),
        Action::Keep => "No change".into(),
        Action::Update => "Will update".into(),
        Action::Replace => "Will replace, the copy moved to a backup first".into(),
        Action::Leave => left(step),
    }
}

fn done_label(outcome: &Outcome) -> String {
    let step = &outcome.step;
    match outcome.status {
        Status::Applied => match (&step.action, &outcome.backup) {
            (Action::Write, _) => "Installed".into(),
            (Action::Update, _) => "Updated".into(),
            (_, Some(backup)) => format!("Replaced, the copy kept at {}", backup.display()),
            (_, None) => "Replaced".into(),
        },
        Status::Unchanged => "No change".into(),
        Status::Left => left(step),
        Status::Changed => "Changed since it was planned; nothing written".into(),
        Status::Failed => format!(
            "Failed: {}",
            outcome.error.as_deref().unwrap_or("unknown error")
        ),
    }
}

fn left(step: &Step) -> String {
    format!(
        "Left alone: {}",
        step.reason.unwrap_or("not tsumugi's copy")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsumugi_agents::skills::Standing;
    use tsumugi_model::Agent;

    struct Setup {
        dir: tempfile::TempDir,
        env: Env,
    }

    fn profile(id: &str, agent: Agent, home: Option<PathBuf>) -> AgentProfile {
        let mut profile = AgentProfile::builtin(agent);
        profile.id = id.parse().unwrap();
        profile.name = id.into();
        profile.home = home;
        profile
    }

    /// Two logins whose state folders exist, and a third that does not.
    fn setup() -> Setup {
        let dir = tempfile::tempdir().unwrap();
        for name in ["work", "play"] {
            std::fs::create_dir_all(dir.path().join(name)).unwrap();
        }
        let env = Env {
            profiles: vec![
                profile("work", Agent::Claude, Some(dir.path().join("work"))),
                profile("play", Agent::Codex, Some(dir.path().join("play"))),
                profile("new", Agent::Claude, Some(dir.path().join("new"))),
            ],
            home: dir.path().join("home"),
            backups: dir.path().join("backups"),
            program: "/My Apps/tsumugi".into(),
        };
        Setup { dir, env }
    }

    fn ask(env: &Env, args: &str) -> (i32, String, String) {
        let args: Vec<String> = args.split_whitespace().map(String::from).collect();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = run(&args, env, &mut out, &mut err);
        (
            code,
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
        )
    }

    fn json(text: &str) -> serde_json::Value {
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn list_names_each_skill_with_its_description() {
        let Setup { env, .. } = setup();
        let (code, out, _) = ask(&env, "list --json");
        assert_eq!(code, 0);
        let topics = &json(&out)["topics"];
        assert_eq!(topics[0]["name"], "start-session");
        assert!(
            topics[0]["description"]
                .as_str()
                .unwrap()
                .contains("session")
        );
        let (_, text, _) = ask(&env, "list");
        assert!(text.starts_with("start-session"));
    }

    #[test]
    fn get_prints_the_guide_naming_the_tsumugi_that_prints_it() {
        let Setup { env, .. } = setup();
        let (code, out, _) = ask(&env, "get start-session");
        assert_eq!(code, 0);
        assert!(out.starts_with("# Work beside other sessions in Tsumugi"));
        assert!(out.contains("'/My Apps/tsumugi' session start"));
        let (_, shown, _) = ask(&env, "show start-session --full");
        assert_eq!(shown, out);
        let (_, out, _) = ask(&env, "get start-session --json");
        let object = json(&out);
        assert_eq!(object["name"], "start-session");
        assert!(
            object["content"]
                .as_str()
                .unwrap()
                .contains("session start")
        );
    }

    #[test]
    fn get_refuses_a_topic_there_is_none_of() {
        let Setup { env, .. } = setup();
        let (code, out, err) = ask(&env, "get nope");
        assert_eq!((code, out.as_str()), (2, ""));
        assert!(
            err.contains("nope") && err.contains("start-session"),
            "{err}"
        );
        assert_eq!(ask(&env, "get").0, 2);
    }

    #[test]
    fn install_without_a_skill_lists_what_can_be_installed_and_writes_nothing() {
        let Setup { dir, env } = setup();
        let (code, out, _) = ask(&env, "install");
        assert_eq!(code, 0);
        assert!(out.contains("start-session"));
        assert!(!dir.path().join("work/skills").exists());
    }

    #[test]
    fn a_dry_run_prints_the_plan_and_its_digest_and_writes_nothing() {
        let Setup { dir, env } = setup();
        let (code, out, _) = ask(&env, "install --all --dry-run --json");
        assert_eq!(code, 0);
        let plan = json(&out);
        assert_eq!(plan["dry_run"], true);
        assert_eq!(plan["digest"].as_str().unwrap().len(), 64);
        assert!(plan.get("results").is_none());
        let steps = plan["plan"].as_array().unwrap();
        // Each skill, in each place that is not shared.
        assert_eq!(steps.len(), 4, "{plan}");
        assert_eq!(steps[0]["profiles"], serde_json::json!(["work"]));
        assert_eq!(steps[0]["action"], "write");
        assert!(!dir.path().join("work/skills").exists());
        assert!(!dir.path().join("play/skills").exists());
        let (_, text, _) = ask(&env, "install --all --dry-run");
        assert!(
            text.contains("Will install") && text.contains("work"),
            "{text}"
        );
    }

    #[test]
    fn install_puts_the_stub_in_the_profiles_whose_login_folder_is_there() {
        let Setup { dir, env } = setup();
        let (code, out, _) = ask(&env, "install --skill start-session --json");
        assert_eq!(code, 0, "{out}");
        let stub =
            std::fs::read_to_string(dir.path().join("work/skills/start-session/SKILL.md")).unwrap();
        assert!(stub.contains("tsumugi skills get start-session"));
        assert!(
            dir.path()
                .join("play/skills/start-session/SKILL.md")
                .exists()
        );
        assert!(!dir.path().join("new").exists());
        let done = json(&out);
        assert_eq!(done["dry_run"], false);
        assert_eq!(done["results"][0]["status"], "applied");
        let (_, again, _) = ask(&env, "update --all");
        assert!(again.contains("No change"), "{again}");
    }

    #[test]
    fn a_profile_named_gets_its_folder_made() {
        let Setup { dir, env } = setup();
        let (code, _, _) = ask(&env, "install --all --profile new");
        assert_eq!(code, 0);
        assert!(
            dir.path()
                .join("new/skills/start-session/SKILL.md")
                .exists()
        );
        assert!(!dir.path().join("work/skills").exists());
    }

    #[test]
    fn profiles_that_share_a_folder_are_one_target() {
        let Setup { dir, mut env } = setup();
        env.profiles.push(profile(
            "twin",
            Agent::Claude,
            Some(dir.path().join("work")),
        ));
        let (_, out, _) = ask(&env, "install --all --profile work,twin --dry-run --json");
        let steps = json(&out)["plan"].as_array().unwrap().clone();
        // One place a skill, however many profiles share it.
        assert_eq!(steps.len(), 2);
        for step in &steps {
            assert_eq!(step["profiles"], serde_json::json!(["work", "twin"]));
        }
    }

    #[test]
    fn a_profile_there_is_none_of_is_refused() {
        let Setup { env, .. } = setup();
        let (code, _, err) = ask(&env, "install --all --profile nope");
        assert_eq!(code, 2);
        assert!(err.contains("nope"), "{err}");
    }

    #[test]
    fn update_never_adds_a_skill() {
        let Setup { dir, env } = setup();
        let (code, out, _) = ask(&env, "update --all --json");
        assert_eq!(code, 0);
        assert_eq!(json(&out)["plan"], serde_json::json!([]));
        assert!(!dir.path().join("work/skills").exists());
    }

    #[test]
    fn a_copy_the_user_changed_is_kept_unless_forced_and_then_backed_up() {
        let Setup { dir, env } = setup();
        let skill = dir.path().join("work/skills/start-session");
        std::fs::create_dir_all(&skill).unwrap();
        let mine = "---\nname: start-session\n---\nmine";
        std::fs::write(skill.join("SKILL.md"), mine).unwrap();
        let (code, out, _) = ask(&env, "update --all --profile work");
        assert_eq!(code, 0);
        assert!(
            out.contains("Left alone") && out.contains("You've changed this copy"),
            "{out}"
        );
        assert_eq!(
            std::fs::read_to_string(skill.join("SKILL.md")).unwrap(),
            mine
        );
        assert!(!dir.path().join("backups").exists());

        let (code, out, _) = ask(&env, "update --all --profile work --force --json");
        assert_eq!(code, 0);
        let backup = PathBuf::from(json(&out)["results"][0]["backup"].as_str().unwrap());
        assert!(backup.starts_with(dir.path().join("backups")));
        assert_eq!(
            std::fs::read_to_string(backup.join("SKILL.md")).unwrap(),
            mine
        );
        assert!(
            std::fs::read_to_string(skill.join("SKILL.md"))
                .unwrap()
                .contains("tsumugi skills get")
        );
    }

    #[test]
    fn a_folder_that_failed_or_changed_ends_the_command_with_1() {
        let step = Step {
            skill: "s".into(),
            folder: "/f".into(),
            standing: Standing::Absent,
            action: Action::Write,
            reason: None,
        };
        let outcome = |status| Outcome {
            step: step.clone(),
            status,
            backup: None,
            error: None,
        };
        assert_eq!(
            exit_code(&[outcome(Status::Applied), outcome(Status::Left)]),
            0
        );
        assert_eq!(
            exit_code(&[outcome(Status::Applied), outcome(Status::Failed)]),
            1
        );
        assert_eq!(exit_code(&[outcome(Status::Changed)]), 1);
    }

    #[test]
    fn what_is_asked_wrongly_is_refused_with_the_usage() {
        let Setup { env, .. } = setup();
        for args in [
            "",
            "frob",
            "list --skill x",
            "install --bogus",
            "install --profile",
            "get a b",
        ] {
            let (code, out, err) = ask(&env, args);
            assert_eq!((code, out.as_str()), (2, ""), "{args}");
            assert!(err.contains("usage: tsumugi skills"), "{args}: {err}");
        }
    }
}
