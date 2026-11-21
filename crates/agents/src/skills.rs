//! The skills tsumugi ships, and how they are put into an agent's skills
//! folder: each as a short stub that tells the agent to read its guide from
//! the tsumugi that runs, so the guide follows the binary and the copy on disk
//! rarely changes.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::AgentContext;

/// One skill this build ships.
#[derive(Debug, Clone)]
pub struct Shipped {
    pub name: String,
    pub description: String,
    /// The `SKILL.md` an agent finds in its skills folder.
    pub stub: String,
    /// The guide `tsumugi skills get` prints, still naming `@TSUMUGI@`.
    guide: String,
    /// sha256 of every `SKILL.md` an earlier build wrote for this skill.
    pub released: Vec<String>,
}

/// What a skill's folder holds, as seen from the skill shipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Standing {
    Absent,
    /// The stub this build ships, and nothing else.
    Current,
    /// A stub an earlier build shipped, and nothing else.
    Released,
    /// The skill, with something changed in it or added to it.
    Edited,
    /// Not a folder of this skill: another skill's, a file or a link.
    Other,
}

/// What a plan does at one skill folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    Write,
    Keep,
    Update,
    /// Put the stub in the place of a copy that is not tsumugi's, after
    /// moving that copy to a backup.
    Replace,
    Leave,
}

/// One skill folder in a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Step {
    pub skill: String,
    pub folder: PathBuf,
    pub standing: Standing,
    pub action: Action,
    /// Why a copy is left alone.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Applied,
    Unchanged,
    Left,
    /// The folder is not as it was planned, so nothing was written.
    Changed,
    Failed,
}

/// What came of one step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Outcome {
    #[serde(flatten)]
    pub step: Step,
    pub status: Status,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

const START_SESSION: &str = include_str!("../plugin/skills/start-session/SKILL.md");

/// sha256 of every stub an earlier or this build wrote, by skill. A copy on
/// disk is tsumugi's untouched one when it is only a `SKILL.md` whose digest
/// is here. Changing a stub adds its digest; none is ever removed.
const RELEASED: &[(&str, &str)] = &[(
    "start-session",
    "11abf0141e399ede1b4c8bb9300bdff33d4576af170bd56ba029a2fd72a24f97",
)];

/// The file an agent reads a skill from, in the skill's folder.
const FILE: &str = "SKILL.md";

/// The sha256 of `bytes` as text, as lines end with `\n` however they were
/// written, so a copy checked out with another line ending still matches.
pub fn digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let text = String::from_utf8_lossy(bytes).replace("\r\n", "\n");
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The value of `key` in the front matter of `markdown`.
fn front_matter<'a>(markdown: &'a str, key: &str) -> Option<&'a str> {
    let rest = markdown.strip_prefix("---\n")?;
    let (head, _) = rest.split_once("\n---")?;
    head.lines()
        .find_map(|line| line.strip_prefix(key)?.strip_prefix(':'))
        .map(|value| value.trim().trim_matches(['"', '\'']))
}

/// The skill's text with its front matter taken off.
fn body(markdown: &str) -> &str {
    markdown
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .map_or(markdown, |(_, body)| body.trim_start())
}

/// The skills this build ships, each as a stub.
pub fn shipped() -> Vec<Shipped> {
    [("start-session", START_SESSION)]
        .into_iter()
        .map(|(name, text)| {
            let description = front_matter(text, "description")
                .expect("a skill describes itself")
                .to_string();
            let stub = format!(
                "---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n\n\
                 This discovery stub loads the version-matched guide from the tsumugi \
                 executable used for this session.\n\n\
                 ## Load the guide before running tsumugi commands\n\n\
                 ```text\ntsumugi skills get {name}\n```\n\n\
                 Follow what it prints, not commands remembered from elsewhere: the \
                 guide is written by the tsumugi that runs. When `tsumugi` is not on \
                 `PATH`, run the executable that started this session.\n"
            );
            Shipped {
                name: name.to_string(),
                description,
                released: RELEASED
                    .iter()
                    .filter(|(skill, _)| *skill == name)
                    .map(|(_, digest)| digest.to_string())
                    .collect(),
                stub,
                guide: body(text).to_string(),
            }
        })
        .collect()
}

/// The guide of `skill`, naming `program` as the tsumugi to run.
pub fn guide(skill: &Shipped, program: &Path) -> String {
    skill
        .guide
        .replace("@TSUMUGI@", &crate::shell_word(program))
}

/// The folder in which `context`'s agent finds the user's skills, in its
/// state directory: the login's own, else where the agent's variable or its
/// usual place in `home` puts it.
pub fn skills_folder(context: &AgentContext, home: &Path) -> PathBuf {
    use tsumugi_model::Agent;
    let state = |variable: &str, usual: PathBuf| {
        context
            .home
            .clone()
            .or_else(|| context.variable(variable))
            .unwrap_or(usual)
    };
    match context.agent() {
        Agent::Claude => state("CLAUDE_CONFIG_DIR", home.join(".claude")),
        Agent::Codex => state("CODEX_HOME", home.join(".codex")),
        Agent::Pi => state("PI_CODING_AGENT_DIR", home.join(".pi").join("agent")),
        Agent::Cursor => state("CURSOR_CONFIG_DIR", home.join(".cursor")),
        Agent::OpenCode => context
            .home
            .as_ref()
            .map(|dir| dir.join("config"))
            .or_else(|| context.variable("XDG_CONFIG_HOME"))
            .unwrap_or_else(|| home.join(".config"))
            .join("opencode"),
    }
    .join("skills")
}

/// What `folder`, the folder of `skill`, holds.
pub fn standing(skill: &Shipped, folder: &Path) -> Standing {
    let kind = match std::fs::symlink_metadata(folder) {
        Ok(kind) => kind,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Standing::Absent,
        Err(_) => return Standing::Other,
    };
    if !kind.is_dir() {
        return Standing::Other;
    }
    let file = folder.join(FILE);
    let Ok(bytes) = std::fs::read(&file) else {
        return Standing::Other;
    };
    let alone = std::fs::read_dir(folder)
        .is_ok_and(|entries| entries.flatten().all(|e| e.file_name() == FILE));
    let own = std::fs::symlink_metadata(&file).is_ok_and(|m| m.is_file());
    let text = String::from_utf8_lossy(&bytes);
    if alone && own {
        let found = digest(&bytes);
        if found == digest(skill.stub.as_bytes()) {
            return Standing::Current;
        }
        if skill.released.contains(&found) {
            return Standing::Released;
        }
    }
    if front_matter(&text, "name") == Some(skill.name.as_str()) {
        Standing::Edited
    } else {
        Standing::Other
    }
}

fn step(skill: &Shipped, folder: &Path, installed_only: bool, force: bool) -> Option<Step> {
    let folder = folder.join(&skill.name);
    let standing = standing(skill, &folder);
    let (action, reason) = match standing {
        Standing::Absent if installed_only => return None,
        Standing::Absent => (Action::Write, None),
        Standing::Current => (Action::Keep, None),
        Standing::Released => (Action::Update, None),
        Standing::Edited | Standing::Other if force => (Action::Replace, None),
        Standing::Edited => (Action::Leave, Some("You've changed this copy")),
        Standing::Other => (
            Action::Leave,
            Some("A different skill already uses this name"),
        ),
    };
    Some(Step {
        skill: skill.name.clone(),
        folder,
        standing,
        action,
        reason,
    })
}

/// What to do at `folder`, a skills folder, for each of `skills`: nothing is
/// written. With `installed_only`, a skill that is not there is not added.
/// With `force`, a copy that is not tsumugi's untouched one is replaced.
pub fn plan(skills: &[Shipped], folder: &Path, installed_only: bool, force: bool) -> Vec<Step> {
    skills
        .iter()
        .filter_map(|skill| step(skill, folder, installed_only, force))
        .collect()
}

/// Do `steps`, each only while its folder is as it was planned, moving a
/// copy replaced into `backups` first.
pub fn apply(skills: &[Shipped], steps: &[Step], backups: &Path) -> Vec<Outcome> {
    steps
        .iter()
        .map(|step| {
            let skill = skills.iter().find(|s| s.name == step.skill);
            let (status, backup, error) = match skill {
                Some(skill) => carry_out(skill, step, backups),
                None => (Status::Failed, None, Some("no such skill".to_string())),
            };
            Outcome {
                step: step.clone(),
                status,
                backup,
                error,
            }
        })
        .collect()
}

type Done = (Status, Option<PathBuf>, Option<String>);

fn carry_out(skill: &Shipped, step: &Step, backups: &Path) -> Done {
    match step.action {
        Action::Keep => return (Status::Unchanged, None, None),
        Action::Leave => return (Status::Left, None, None),
        _ => {}
    }
    if standing(skill, &step.folder) != step.standing {
        return (Status::Changed, None, None);
    }
    let result = (|| {
        let backup = match step.action {
            Action::Replace => Some(move_aside(&step.folder, &skill.name, backups)?),
            _ => None,
        };
        std::fs::create_dir_all(&step.folder)?;
        tsumugi_sys::fs::replace(step.folder.join(FILE), &skill.stub, 0o644)?;
        Ok::<_, std::io::Error>(backup)
    })();
    match result {
        Ok(backup) => (Status::Applied, backup, None),
        Err(e) => (Status::Failed, None, Some(e.to_string())),
    }
}

/// Move what stands at `path` into a folder of its own in `backups`, which
/// is made when missing; the folder, named for the skill and the moment.
fn move_aside(path: &Path, name: &str, backups: &Path) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(backups)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let mut kept = backups.join(format!("{name}-{now}"));
    for n in 1.. {
        if std::fs::symlink_metadata(&kept).is_err() {
            break;
        }
        kept = backups.join(format!("{name}-{now}-{n}"));
    }
    if std::fs::rename(path, &kept).is_err() {
        // Another file system: copy, and only then take the original away.
        copy_tree(path, &kept)?;
        if std::fs::symlink_metadata(path)?.is_dir() {
            std::fs::remove_dir_all(path)?;
        } else {
            std::fs::remove_file(path)?;
        }
    }
    Ok(kept)
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    if std::fs::metadata(from)?.is_dir() {
        std::fs::create_dir_all(to)?;
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            copy_tree(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(from, to).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(name: &str, stub: &str, released: &[&str]) -> Shipped {
        Shipped {
            name: name.into(),
            description: "d".into(),
            stub: stub.into(),
            guide: String::new(),
            released: released.iter().map(|s| digest(s)).collect(),
        }
    }

    fn digest(text: &str) -> String {
        super::digest(text.as_bytes())
    }

    fn start_session() -> Shipped {
        shipped()
            .into_iter()
            .find(|s| s.name == "start-session")
            .unwrap()
    }

    #[test]
    fn start_session_is_shipped_as_a_stub_that_loads_its_guide_from_tsumugi() {
        let skill = start_session();
        assert!(
            skill
                .stub
                .starts_with("---\nname: start-session\ndescription: ")
        );
        assert!(skill.stub.contains(&skill.description));
        assert!(skill.stub.contains("tsumugi skills get start-session"));
        assert!(
            skill.stub.len() < 1200,
            "a stub stays short: {}",
            skill.stub.len()
        );
        assert!(!skill.stub.contains("@TSUMUGI@"));
    }

    #[test]
    fn the_guide_is_the_skill_s_text_naming_the_tsumugi_that_prints_it() {
        let text = guide(&start_session(), Path::new("/My Apps/tsumugi"));
        assert!(text.starts_with("# Work beside other sessions in Tsumugi"));
        assert!(text.contains("'/My Apps/tsumugi' session start"));
        assert!(!text.contains("@TSUMUGI@"));
        assert!(!text.contains("name: start-session"));
    }

    #[test]
    fn every_stub_this_build_ships_is_among_the_released_ones() {
        // Changing a stub means adding the digest of the old one to the list
        // that says which copies on disk are still tsumugi's untouched ones.
        for skill in shipped() {
            assert!(
                skill.released.contains(&digest(&skill.stub)),
                "{} stub digest {} is not in released",
                skill.name,
                digest(&skill.stub)
            );
        }
    }

    #[test]
    fn a_missing_skill_is_written_and_then_kept() {
        let dir = tempfile::tempdir().unwrap();
        let skills = [named("alpha", "stub", &["stub"])];
        let steps = plan(&skills, dir.path(), false, false);
        assert_eq!(steps[0].action, Action::Write);
        assert_eq!(steps[0].standing, Standing::Absent);
        let done = apply(&skills, &steps, &dir.path().join("backups"));
        assert_eq!(done[0].status, Status::Applied);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("alpha/SKILL.md")).unwrap(),
            "stub"
        );
        let again = plan(&skills, dir.path(), false, false);
        assert_eq!(
            (again[0].action, again[0].standing),
            (Action::Keep, Standing::Current)
        );
        let done = apply(&skills, &again, &dir.path().join("backups"));
        assert_eq!(done[0].status, Status::Unchanged);
    }

    #[test]
    fn a_plan_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let skills = [named("alpha", "stub", &["stub"])];
        let folder = dir.path().join("skills");
        plan(&skills, &folder, false, false);
        assert!(!folder.exists());
    }

    #[test]
    fn an_untouched_copy_of_an_earlier_stub_is_updated_without_a_backup() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("alpha")).unwrap();
        std::fs::write(dir.path().join("alpha/SKILL.md"), "old").unwrap();
        let skills = [named("alpha", "new", &["old", "new"])];
        let steps = plan(&skills, dir.path(), true, false);
        assert_eq!(
            (steps[0].action, steps[0].standing),
            (Action::Update, Standing::Released)
        );
        let done = apply(&skills, &steps, &dir.path().join("backups"));
        assert_eq!(done[0].status, Status::Applied);
        assert_eq!(done[0].backup, None);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("alpha/SKILL.md")).unwrap(),
            "new"
        );
        assert!(!dir.path().join("backups").exists());
    }

    #[test]
    fn a_copy_the_user_edited_is_left_alone_and_a_forced_one_is_backed_up_first() {
        let dir = tempfile::tempdir().unwrap();
        let alpha = dir.path().join("alpha");
        std::fs::create_dir_all(&alpha).unwrap();
        let mine = "---\nname: alpha\n---\nmy own words";
        std::fs::write(alpha.join("SKILL.md"), mine).unwrap();
        let skills = [named("alpha", "new", &["new"])];
        let backups = dir.path().join("backups");
        let steps = plan(&skills, dir.path(), false, false);
        assert_eq!(
            (steps[0].action, steps[0].standing),
            (Action::Leave, Standing::Edited)
        );
        assert_eq!(steps[0].reason, Some("You've changed this copy"));
        let done = apply(&skills, &steps, &backups);
        assert_eq!(done[0].status, Status::Left);
        assert_eq!(
            std::fs::read_to_string(alpha.join("SKILL.md")).unwrap(),
            mine
        );

        let steps = plan(&skills, dir.path(), false, true);
        assert_eq!(steps[0].action, Action::Replace);
        let done = apply(&skills, &steps, &backups);
        assert_eq!(done[0].status, Status::Applied);
        let kept = done[0].backup.clone().unwrap();
        assert!(kept.starts_with(&backups));
        assert_eq!(
            std::fs::read_to_string(kept.join("SKILL.md")).unwrap(),
            mine
        );
        assert_eq!(
            std::fs::read_to_string(alpha.join("SKILL.md")).unwrap(),
            "new"
        );
    }

    #[test]
    fn a_file_added_to_the_folder_makes_it_the_user_s() {
        let dir = tempfile::tempdir().unwrap();
        let alpha = dir.path().join("alpha");
        std::fs::create_dir_all(&alpha).unwrap();
        std::fs::write(alpha.join("SKILL.md"), "old").unwrap();
        std::fs::write(alpha.join("notes.md"), "mine").unwrap();
        let skills = [named("alpha", "new", &["old", "new"])];
        assert_eq!(standing(&skills[0], &alpha), Standing::Other);
        std::fs::write(alpha.join("SKILL.md"), "---\nname: alpha\n---\n").unwrap();
        assert_eq!(standing(&skills[0], &alpha), Standing::Edited);
    }

    #[test]
    fn another_skill_under_the_name_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let alpha = dir.path().join("alpha");
        std::fs::create_dir_all(&alpha).unwrap();
        std::fs::write(alpha.join("SKILL.md"), "---\nname: other\n---\n").unwrap();
        let skills = [named("alpha", "new", &["new"])];
        let steps = plan(&skills, dir.path(), false, false);
        assert_eq!(
            (steps[0].action, steps[0].standing),
            (Action::Leave, Standing::Other)
        );
        assert_eq!(
            steps[0].reason,
            Some("A different skill already uses this name")
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_link_is_never_written_through() {
        let dir = tempfile::tempdir().unwrap();
        let elsewhere = dir.path().join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::os::unix::fs::symlink(&elsewhere, dir.path().join("alpha")).unwrap();
        let skills = [named("alpha", "new", &["new"])];
        let steps = plan(&skills, dir.path(), false, false);
        assert_eq!(steps[0].standing, Standing::Other);
        apply(&skills, &steps, &dir.path().join("backups"));
        assert!(!elsewhere.join("SKILL.md").exists());
    }

    #[test]
    fn an_update_never_adds_a_skill() {
        let dir = tempfile::tempdir().unwrap();
        let skills = [named("alpha", "new", &["new"])];
        assert!(plan(&skills, dir.path(), true, false).is_empty());
    }

    #[test]
    fn a_folder_changed_after_it_was_planned_is_not_written() {
        let dir = tempfile::tempdir().unwrap();
        let alpha = dir.path().join("alpha");
        std::fs::create_dir_all(&alpha).unwrap();
        std::fs::write(alpha.join("SKILL.md"), "old").unwrap();
        let skills = [named("alpha", "new", &["old", "new"])];
        let steps = plan(&skills, dir.path(), true, false);
        std::fs::write(
            alpha.join("SKILL.md"),
            "---\nname: alpha\n---\nedited meanwhile",
        )
        .unwrap();
        let done = apply(&skills, &steps, &dir.path().join("backups"));
        assert_eq!(done[0].status, Status::Changed);
        assert_eq!(
            std::fs::read_to_string(alpha.join("SKILL.md")).unwrap(),
            "---\nname: alpha\n---\nedited meanwhile"
        );
    }

    #[test]
    fn skills_are_looked_for_where_each_agent_reads_them() {
        use tsumugi_model::Agent;
        let home = Path::new("/h");
        let own = |agent, dir: &str| {
            let mut profile = tsumugi_model::profile::AgentProfile::builtin(agent);
            profile.home = Some(dir.into());
            AgentContext::new(&profile, "")
        };
        for (agent, expected) in [
            (Agent::Claude, "/w/skills"),
            (Agent::Codex, "/w/skills"),
            (Agent::Pi, "/w/skills"),
            (Agent::Cursor, "/w/skills"),
            (Agent::OpenCode, "/w/config/opencode/skills"),
        ] {
            assert_eq!(
                skills_folder(&own(agent, "/w"), home),
                Path::new(expected),
                "{agent:?}"
            );
        }
    }
}
