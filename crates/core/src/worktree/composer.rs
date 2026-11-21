//! The dialog that asks for a new worktree: its name, where it starts, and
//! the branch it works on.

use std::path::PathBuf;

use tsumugi_model::{EnvironmentId, Key, NewWorktree, StartChoice, StartPoint};

use crate::actions::Action;
use crate::chrome::status_line;
use crate::cursor;
use crate::effect::Effect;
use crate::overlay::{Holder, action_over};
use crate::picker::fuzzy::{Query, best};
use crate::state::ViewState;
use crate::tree::nav::Item;

/// The most places the dialog lists at once.
const ROWS: usize = 200;

/// A field of the dialog.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Field {
    #[default]
    Name,
    Start,
    Branch,
}

impl Field {
    fn step(self, back: bool) -> Self {
        const ALL: [Field; 3] = [Field::Name, Field::Start, Field::Branch];
        let at = ALL.iter().position(|f| *f == self).unwrap_or(0);
        ALL[cursor::wrap(at, if back { -1 } else { 1 }, ALL.len())]
    }
}

/// Where a worktree may start, as the repository's host listed them.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum Starts {
    /// Not listed yet; the checkout's own commit can be chosen meanwhile.
    #[default]
    Reading,
    Read(Vec<StartChoice>),
    /// The host could not list them, for this reason.
    Failed(String),
}

/// A new worktree being asked for.
#[derive(Debug, Clone, PartialEq)]
pub struct Composer {
    pub environment: EnvironmentId,
    /// The repository's main checkout, which the worktree is made of.
    pub repository: PathBuf,
    /// The workspace's name, for the dialog's title.
    pub project: String,
    pub field: Field,
    pub name: String,
    /// What narrows the places to start from, and what is typed as one that
    /// is not listed, such as a commit.
    pub query: String,
    pub cursor: usize,
    /// The branch it works on, new, when not named after its directory.
    pub branch: String,
    pub starts: Starts,
}

impl Composer {
    fn new(environment: EnvironmentId, repository: PathBuf, project: String) -> Self {
        Self {
            environment,
            repository,
            project,
            field: Field::Name,
            name: String::new(),
            query: String::new(),
            cursor: 0,
            branch: String::new(),
            starts: Starts::Reading,
        }
    }

    /// The places to start from that what is typed narrows to, best first.
    /// What is typed that no row names stands last as a revision of its own,
    /// so a commit or a tag can be given.
    pub fn rows(&self) -> Vec<StartChoice> {
        let mut listed = match &self.starts {
            Starts::Read(listed) => listed.clone(),
            Starts::Reading | Starts::Failed(_) => Vec::new(),
        };
        // The checkout's own commit can always be chosen.
        if listed.is_empty() {
            listed.push(StartChoice {
                point: StartPoint::Head,
                subject: String::new(),
            });
        }
        let query = self.query.trim();
        let mut rows: Vec<StartChoice> = if query.is_empty() {
            listed.truncate(ROWS);
            listed
        } else {
            let matching = Query::new(query);
            best(
                listed.into_iter().filter_map(|c| {
                    let name = c.point.name();
                    matching
                        .rank_named(&format!("{name} {}", c.subject), &name)
                        .map(|rank| (rank, c))
                }),
                ROWS,
            )
        };
        if !query.is_empty() && !rows.iter().any(|c| c.point.name() == query) {
            rows.push(StartChoice {
                point: StartPoint::Rev { rev: query.into() },
                subject: String::new(),
            });
        }
        rows
    }

    /// The place the cursor is on.
    pub fn chosen(&self) -> StartPoint {
        self.rows()
            .into_iter()
            .nth(self.cursor)
            .map_or(StartPoint::Head, |c| c.point)
    }

    /// The worktree asked for, or why none is.
    pub fn request(&self) -> Result<NewWorktree, String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err("Name the worktree first".into());
        }
        let branch = self.branch.trim();
        Ok(NewWorktree {
            name: name.into(),
            start: self.chosen(),
            branch: (!branch.is_empty()).then(|| branch.into()),
        })
    }
}

/// Open the dialog from the tree's row `row`: for a new worktree of the
/// repository of the workspace it belongs to, whose places to start from
/// the host is asked for. A row outside git, or outside any workspace, opens
/// nothing and says so.
pub(crate) fn open(state: &mut ViewState, row: &Item) -> Vec<Effect> {
    let (environment, root) = match row.own() {
        Item::Workspace(environment, root) | Item::Across(environment, root, _) => {
            (environment.clone(), root.clone())
        }
        Item::Worktree(environment, path)
        | Item::Heading(environment, path, _)
        | Item::File {
            environment,
            worktree: path,
            ..
        } => (environment.clone(), path.clone()),
        _ => return Vec::new(),
    };
    let tree = crate::tree::listed(state.hosts.work(&environment));
    let Some(ws) = tree.workspace_of(&root) else {
        return Vec::new();
    };
    if !ws.git {
        return status_line::notice(
            &mut state.chrome.status,
            format!(
                "{} is not in a Git repository, so it has no worktrees",
                ws.name
            ),
        );
    }
    let repository = ws
        .worktrees
        .iter()
        .find(|w| w.worktree.is_main())
        .or(ws.worktrees.first())
        .map_or_else(|| ws.root.clone(), |w| w.worktree.path.clone());
    let project = ws.name.clone();
    state.overlays.composer = Some(Composer::new(
        environment.clone(),
        repository.clone(),
        project,
    ));
    vec![Effect::WorktreeStarts {
        environment,
        repository,
    }]
}

/// The places to start from the host listed for the dialog, or why it
/// could not.
pub(crate) fn read_starts(
    state: &mut ViewState,
    environment: &EnvironmentId,
    repository: &std::path::Path,
    result: Result<Vec<StartChoice>, String>,
) -> Vec<Effect> {
    let Some(composer) = state
        .overlays
        .composer
        .as_mut()
        .filter(|c| c.environment == *environment && c.repository == repository)
    else {
        return Vec::new();
    };
    composer.starts = match result {
        Ok(listed) => Starts::Read(listed),
        Err(why) => Starts::Failed(why),
    };
    composer.cursor = 0;
    Vec::new()
}

/// `text` typed into `field`, which then has the keys.
pub(crate) fn type_into(state: &mut ViewState, field: Field, text: String) -> Vec<Effect> {
    let Some(composer) = state.overlays.composer.as_mut() else {
        return Vec::new();
    };
    composer.field = field;
    match field {
        Field::Name => composer.name = text,
        Field::Branch => composer.branch = text,
        Field::Start => {
            if composer.query != text {
                composer.query = text;
                composer.cursor = 0;
            }
        }
    }
    Vec::new()
}

/// `field` has the keys.
pub(crate) fn focus(state: &mut ViewState, field: Field) -> Vec<Effect> {
    if let Some(composer) = state.overlays.composer.as_mut() {
        composer.field = field;
    }
    Vec::new()
}

/// The place to start from in row `at` of the list chosen, the field of the
/// start having the keys.
pub(crate) fn cursor_at(state: &mut ViewState, at: usize) -> Vec<Effect> {
    if let Some(composer) = state.overlays.composer.as_mut()
        && at < composer.rows().len()
    {
        composer.cursor = at;
        composer.field = Field::Start;
    }
    Vec::new()
}

/// A key the dialog's fields do not type with: `Tab` goes to the next field,
/// the keys that move through a list move through the places to start from,
/// the key that chooses makes the worktree, and the key that closes puts
/// the dialog away.
pub(crate) fn composer_key(state: &mut ViewState, key: &Key) -> Vec<Effect> {
    if key.key == "Tab" && !key.ctrl && !key.alt && !key.meta {
        if let Some(composer) = state.overlays.composer.as_mut() {
            composer.field = composer.field.step(key.shift);
        }
        return Vec::new();
    }
    let action = action_over(&state.chrome.keymap, Holder::Composer, key);
    let Some(composer) = state.overlays.composer.as_mut() else {
        return Vec::new();
    };
    match action {
        Some(Action::OverlayClose) => {
            state.overlays.composer = None;
            Vec::new()
        }
        Some(Action::OverlayDown) => {
            let len = composer.rows().len();
            composer.cursor = cursor::wrap(composer.cursor, 1, len);
            Vec::new()
        }
        Some(Action::OverlayUp) => {
            let len = composer.rows().len();
            composer.cursor = cursor::wrap(composer.cursor, -1, len);
            Vec::new()
        }
        Some(Action::OverlayChoose) => match composer.request() {
            Ok(new) => {
                let composer = state.overlays.composer.take().expect("it was open");
                super::pending::begin(state, composer, new)
            }
            Err(why) => {
                composer.field = Field::Name;
                status_line::notice(&mut state.chrome.status, why)
            }
        },
        _ => Vec::new(),
    }
}
