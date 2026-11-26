//! What a tool call did, read from its name and input: the kind of work it
//! was, the words its line says, the edit it made, the plan it wrote.

use serde_json::Value;
use tsumugi_model::conversation::Tool;

/// The kind of work a call did, whichever agent named its tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Command,
    Read,
    Edit,
    Search,
    List,
    Web,
    Agent,
    Plan,
    Other,
}

impl Kind {
    pub fn of(name: &str) -> Kind {
        let name = name.to_ascii_lowercase();
        match name.as_str() {
            "bash" | "shell" | "exec_command" | "local_shell" | "bashoutput" | "killshell"
            | "run_command" | "terminal" => Kind::Command,
            "read" | "read_file" | "view" | "notebookread" | "view_image" => Kind::Read,
            "edit" | "multiedit" | "write" | "notebookedit" | "apply_patch" | "patch"
            | "write_file" | "edit_file" | "str_replace" => Kind::Edit,
            "grep" | "search" | "rg" | "codebase_search" | "grep_search" => Kind::Search,
            "glob" | "ls" | "list" | "list_dir" | "list_files" | "find" => Kind::List,
            "webfetch" | "websearch" | "web_search" | "web_fetch" | "fetch" => Kind::Web,
            "task" | "agent" | "spawn_agent" => Kind::Agent,
            "todowrite" | "update_plan" | "todo_write" => Kind::Plan,
            _ => Kind::Other,
        }
    }

    /// The icon its line is drawn with, as Tabler names it.
    pub fn icon(self) -> tsumugi_core::icons::Icon {
        use tsumugi_core::icons::Icon;
        match self {
            Kind::Command => Icon::Terminal2,
            Kind::Read => Icon::File,
            Kind::Edit => Icon::FilePencil,
            Kind::Search => Icon::Search,
            Kind::List => Icon::FolderSearch,
            Kind::Web => Icon::World,
            Kind::Agent => Icon::UsersGroup,
            Kind::Plan => Icon::ListTree,
            Kind::Other => Icon::Dots,
        }
    }

    /// The verb a call of this kind is told with, as it runs, once it is
    /// done, and once it failed.
    fn verbs(self) -> [&'static str; 3] {
        match self {
            Kind::Command => ["Running", "Ran", "Tried to run"],
            Kind::Read => ["Reading", "Read", "Tried to read"],
            Kind::Edit => ["Editing", "Edited", "Tried to edit"],
            Kind::Search => ["Searching", "Searched", "Tried to search"],
            Kind::List => ["Listing", "Listed", "Tried to list"],
            Kind::Web => ["Fetching", "Fetched", "Tried to fetch"],
            Kind::Agent => ["Running agent", "Agent", "Agent failed"],
            Kind::Plan => [
                "Updating the plan",
                "Updated the plan",
                "Tried to update the plan",
            ],
            Kind::Other => ["Using", "Used", "Tried to use"],
        }
    }

    /// How a run's heading counts calls of this kind, once and many times.
    fn counted(self) -> (&'static str, &'static str) {
        match self {
            Kind::Command => ("ran 1 command", "ran {} commands"),
            Kind::Read => ("read 1 file", "read {} files"),
            Kind::Edit => ("edited 1 file", "edited {} files"),
            Kind::Search => ("searched 1 time", "searched {} times"),
            Kind::List => ("listed 1 directory", "listed {} directories"),
            Kind::Web => ("fetched 1 page", "fetched {} pages"),
            Kind::Agent => ("ran 1 agent", "ran {} agents"),
            Kind::Plan => ("updated the plan", "updated the plan {} times"),
            Kind::Other => ("used 1 tool", "used {} tools"),
        }
    }
}

/// Where a call stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Running,
    Done,
    Failed,
}

impl Phase {
    /// `live`: the call is in the turn still being worked, where a call with
    /// no result yet is still running; elsewhere it is not waited on.
    pub fn of(tool: &Tool, live: bool) -> Phase {
        match (&tool.output, tool.failed) {
            (_, true) => Phase::Failed,
            (None, false) if live => Phase::Running,
            _ => Phase::Done,
        }
    }

    pub fn class(self) -> &'static str {
        match self {
            Phase::Running => "running",
            Phase::Done => "done",
            Phase::Failed => "failed",
        }
    }
}

/// What a call's line says: its verb and what it was done to, with the
/// whole of that for its title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub verb: String,
    pub target: String,
    pub title: String,
}

pub fn line(tool: &Tool, phase: Phase) -> Line {
    let kind = Kind::of(&tool.name);
    let input = parsed(tool);
    let [running, done, failed] = kind.verbs();
    let verb = match phase {
        Phase::Running => running,
        Phase::Done => done,
        Phase::Failed => failed,
    };
    let path = input.as_ref().and_then(file_path);
    let (target, title) = match kind {
        Kind::Read | Kind::Edit if path.is_some() => {
            let path = path.unwrap_or_default();
            (file_name(&path).to_string(), path)
        }
        Kind::Edit if patch_files(&tool.input).len() == 1 => {
            let path = patch_files(&tool.input).remove(0);
            (file_name(&path).to_string(), path)
        }
        Kind::Web => {
            let url = tool.summary.clone();
            let bare = url
                .split_once("://")
                .map_or(url.as_str(), |(_, rest)| rest)
                .to_string();
            (bare, url)
        }
        Kind::Other => (
            readable_name(&tool.name),
            format!("{}: {}", tool.name, tool.summary),
        ),
        _ => (tool.summary.clone(), tool.summary.clone()),
    };
    // A tool named for itself says nothing more by its summary.
    let target = if kind != Kind::Other && target == tool.name {
        String::new()
    } else {
        target
    };
    Line {
        verb: verb.into(),
        target,
        title,
    }
}

/// A tool's name as words: an MCP tool's server and tool, without the
/// prefix it is called by.
fn readable_name(name: &str) -> String {
    match name.strip_prefix("mcp__") {
        Some(rest) => rest.replacen("__", " · ", 1),
        None => name.to_string(),
    }
}

fn parsed(tool: &Tool) -> Option<Value> {
    serde_json::from_str::<Value>(&tool.input)
        .ok()
        .filter(Value::is_object)
}

fn file_path(input: &Value) -> Option<String> {
    ["file_path", "path", "notebook_path", "filePath"]
        .iter()
        .find_map(|k| input.get(*k).and_then(Value::as_str))
        .filter(|p| !p.is_empty())
        .map(str::to_string)
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\'])
        .find(|p| !p.is_empty())
        .unwrap_or(path)
}

/// The files a patch in Codex's own format touches.
fn patch_files(input: &str) -> Vec<String> {
    let text = patch_text(input);
    text.lines()
        .filter_map(|l| {
            ["*** Update File: ", "*** Add File: ", "*** Delete File: "]
                .iter()
                .find_map(|p| l.strip_prefix(p))
        })
        .map(|p| p.trim().to_string())
        .collect()
}

/// A patch given as a string, or as the string the input's one field holds.
fn patch_text(input: &str) -> String {
    match serde_json::from_str::<Value>(input) {
        Ok(Value::Object(map)) => map
            .values()
            .find_map(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        Ok(Value::String(s)) => s,
        _ => input.to_string(),
    }
}

/// A line of an edit, as a diff shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Same(String),
    Removed(String),
    Added(String),
    /// Where an edit's hunks part, or a patch names its file.
    Gap(String),
}

/// The edit a call made, line by line, when its input tells it.
pub fn edit(tool: &Tool) -> Option<Vec<Change>> {
    if Kind::of(&tool.name) != Kind::Edit {
        return None;
    }
    let Some(input) = parsed(tool) else {
        let text = patch_text(&tool.input);
        return text.contains("*** Begin Patch").then(|| patch(&text));
    };
    if let Some(text) = input
        .get("patch")
        .or_else(|| input.get("input"))
        .and_then(Value::as_str)
        .filter(|t| t.contains("*** Begin Patch"))
    {
        return Some(patch(text));
    }
    if let Some(edits) = input.get("edits").and_then(Value::as_array) {
        let mut out = Vec::new();
        for (i, e) in edits.iter().enumerate() {
            if i > 0 {
                out.push(Change::Gap(String::new()));
            }
            out.extend(replaced(
                e.get("old_string").and_then(Value::as_str).unwrap_or(""),
                e.get("new_string").and_then(Value::as_str).unwrap_or(""),
            ));
        }
        return Some(out);
    }
    if let Some(new) = input.get("new_string").and_then(Value::as_str) {
        let old = input
            .get("old_string")
            .and_then(Value::as_str)
            .unwrap_or("");
        return Some(replaced(old, new));
    }
    input
        .get("content")
        .and_then(Value::as_str)
        .map(|c| c.lines().map(|l| Change::Added(l.into())).collect())
}

/// `old` replaced by `new`: the lines both begin and end with kept, those
/// between removed and added.
fn replaced(old: &str, new: &str) -> Vec<Change> {
    let old: Vec<&str> = old.lines().collect();
    let new: Vec<&str> = new.lines().collect();
    let head = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let tail = old[head..]
        .iter()
        .rev()
        .zip(new[head..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let mut out: Vec<Change> = old[..head]
        .iter()
        .map(|l| Change::Same((*l).into()))
        .collect();
    out.extend(
        old[head..old.len() - tail]
            .iter()
            .map(|l| Change::Removed((*l).into())),
    );
    out.extend(
        new[head..new.len() - tail]
            .iter()
            .map(|l| Change::Added((*l).into())),
    );
    out.extend(
        old[old.len() - tail..]
            .iter()
            .map(|l| Change::Same((*l).into())),
    );
    out
}

/// A patch in Codex's own format, or a unified diff, by its lines' marks.
fn patch(text: &str) -> Vec<Change> {
    text.lines()
        .filter(|l| {
            !matches!(
                l.trim(),
                "*** Begin Patch" | "*** End Patch" | "*** End of File"
            )
        })
        .map(|l| {
            if let Some(file) = ["*** Update File: ", "*** Add File: ", "*** Delete File: "]
                .iter()
                .find_map(|p| l.strip_prefix(p))
            {
                Change::Gap(file.trim().into())
            } else if l.starts_with("@@") {
                Change::Gap(String::new())
            } else if let Some(r) = l.strip_prefix('+') {
                Change::Added(r.into())
            } else if let Some(r) = l.strip_prefix('-') {
                Change::Removed(r.into())
            } else {
                Change::Same(l.strip_prefix(' ').unwrap_or(l).into())
            }
        })
        .collect()
}

/// The lines an edit adds and removes.
pub fn counts(changes: &[Change]) -> (usize, usize) {
    changes.iter().fold((0, 0), |(a, r), c| match c {
        Change::Added(_) => (a + 1, r),
        Change::Removed(_) => (a, r + 1),
        _ => (a, r),
    })
}

/// Where a step of a plan stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Pending,
    Doing,
    Done,
}

/// The plan a call wrote, step by step, when it wrote one.
pub fn plan(tool: &Tool) -> Option<Vec<(Step, String)>> {
    if Kind::of(&tool.name) != Kind::Plan {
        return None;
    }
    let input = parsed(tool)?;
    let steps = input
        .get("todos")
        .or_else(|| input.get("plan"))
        .and_then(Value::as_array)?;
    Some(
        steps
            .iter()
            .filter_map(|s| {
                let text = ["content", "step", "text", "title"]
                    .iter()
                    .find_map(|k| s.get(*k).and_then(Value::as_str))?;
                let step = match s.get("status").and_then(Value::as_str) {
                    Some("completed" | "done") => Step::Done,
                    Some("in_progress" | "in-progress" | "active") => Step::Doing,
                    _ => Step::Pending,
                };
                Some((step, text.to_string()))
            })
            .collect(),
    )
}

/// Whether a call's line says all it was given: an input of one field,
/// which the line names.
pub fn said_by_its_line(tool: &Tool) -> bool {
    parsed(tool).is_some_and(|input| input.as_object().is_some_and(|m| m.len() <= 1))
}

/// The command a call ran, whole, when its input names one.
pub fn command(tool: &Tool) -> Option<String> {
    if Kind::of(&tool.name) != Kind::Command {
        return None;
    }
    let input = parsed(tool)?;
    match input.get("command").or_else(|| input.get("cmd"))? {
        Value::String(s) => Some(s.clone()),
        Value::Array(parts) => Some(
            parts
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" "),
        ),
        _ => None,
    }
}

/// A run's heading: what its calls did, by kind in the order first done.
pub fn sentence(tools: &[&Tool]) -> String {
    let mut counts: Vec<(Kind, usize)> = Vec::new();
    for tool in tools {
        let kind = Kind::of(&tool.name);
        match counts.iter_mut().find(|(k, _)| *k == kind) {
            Some((_, n)) => *n += 1,
            None => counts.push((kind, 1)),
        }
    }
    let clauses: Vec<String> = counts
        .iter()
        .map(|(kind, n)| {
            let (one, many) = kind.counted();
            if *n == 1 {
                one.to_string()
            } else {
                many.replace("{}", &n.to_string())
            }
        })
        .collect();
    let joined = match clauses.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [a, b] => format!("{a} and {b}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    };
    let mut chars = joined.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}
