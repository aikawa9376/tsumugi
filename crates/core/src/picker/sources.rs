//! What the picker lists beside sessions and directories: the worktrees the
//! tree lists, the resources projects keep, the files of the project shown
//! and the actions Tsumugi runs. Each source ranks its own rows by the fuzzy
//! match and cuts them at the cap it is given.

use std::path::{Path, PathBuf};

use tsumugi_model::{EnvironmentId, Resource, Tree};

use super::Choice;
use super::fuzzy::{Query, best};
use crate::actions::{ACTIONS, Action};
use crate::host::Hosts;
use crate::tree::{self, Place};

/// A worktree to go to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeOffer {
    pub environment: EnvironmentId,
    pub path: PathBuf,
    /// Its branch, else the name of its directory.
    pub name: String,
    /// The workspace it belongs to.
    pub place: String,
}

/// A resource to open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceOffer {
    pub environment: EnvironmentId,
    pub resource: Resource,
}

/// A file of a worktree to open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileOffer {
    pub environment: EnvironmentId,
    pub worktree: PathBuf,
    /// Its path from the root of the worktree.
    pub path: String,
}

impl FileOffer {
    /// The file's name, and the directory it is in from the worktree's root.
    pub fn parts(&self) -> (&str, &str) {
        match self.path.rsplit_once('/') {
            Some((dir, name)) => (name, dir),
            None => (&self.path, ""),
        }
    }

    pub fn absolute(&self) -> PathBuf {
        self.worktree.join(&self.path)
    }
}

/// A worktree to make, as typed, in the project of the worktree shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewWorktreeOffer {
    pub environment: EnvironmentId,
    /// A worktree of the project it is made in.
    pub from: PathBuf,
    /// The project's name.
    pub project: String,
    pub name: String,
}

/// A branch of the repository of the project shown, to go to or switch to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchOffer {
    pub environment: EnvironmentId,
    /// The worktree shown, which a switch changes and whose repository it is
    /// of.
    pub worktree: PathBuf,
    pub point: tsumugi_model::StartPoint,
    /// Its last commit's subject.
    pub subject: String,
    /// The worktree it is checked out in, if any.
    pub checked_out: Option<PathBuf>,
}

impl BranchOffer {
    /// The name of the local branch it is, or is checked out as.
    pub fn local(&self) -> String {
        match &self.point {
            tsumugi_model::StartPoint::Remote { branch, .. } => branch.clone(),
            other => other.name(),
        }
    }
}

/// An action to run, as the keys and menus name it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionOffer {
    /// The name the file of bindings calls it.
    pub name: &'static str,
    /// What the menu bar, a menu or the keys help calls it.
    pub label: String,
}

impl ActionOffer {
    /// The action itself.
    pub fn action(&self) -> Option<Action> {
        self.name.parse().ok()
    }
}

/// Every worktree of every environment `hosts` holds, in the tree's order
/// with nothing typed, else the best `cap` for what is typed against
/// `workspace/worktree`.
pub fn worktrees(hosts: &Hosts, focus: Option<&Place>, query: &str, cap: usize) -> Vec<Choice> {
    let typed = query.trim();
    let query = Query::new(query);
    let offers = hosts.each().flat_map(|(environment, host)| {
        let tree: &Tree = tree::listed(host.work.as_ref());
        tree.workspaces.iter().flat_map(move |ws| {
            ws.worktrees.iter().map(move |w| WorktreeOffer {
                environment: environment.clone(),
                path: w.worktree.path.clone(),
                name: w.worktree.branch.clone().unwrap_or_else(|| {
                    w.worktree
                        .path
                        .file_name()
                        .map(|f| f.to_string_lossy().into_owned())
                        .unwrap_or_default()
                }),
                place: ws.name.clone(),
            })
        })
    });
    if query.is_empty() {
        return offers.take(cap).map(Choice::Worktree).collect();
    }
    let offers: Vec<WorktreeOffer> = offers.collect();
    let named = offers.iter().any(|o| o.name.eq_ignore_ascii_case(typed));
    let mut rows = best(
        offers.into_iter().filter_map(|o| {
            let text = format!("{}/{}", o.place, o.name);
            query
                .rank_named(&text, &o.name)
                .map(|rank| (rank, Choice::Worktree(o)))
        }),
        cap,
    );
    // What matches no worktree's name is a worktree to make, last so that
    // choosing the first row still goes to one that exists.
    if !named
        && let Some(focus) = focus
        && let Some(ws) = hosts
            .work(&focus.environment)
            .and_then(|w| w.tree.tree().workspace_of(&focus.worktree))
    {
        rows.push(Choice::NewWorktree(NewWorktreeOffer {
            environment: focus.environment.clone(),
            from: focus.worktree.clone(),
            project: ws.name.clone(),
            name: typed.into(),
        }));
    }
    rows
}

/// The branches of the repository of `focus`, the worktree shown, as the
/// host listed them, newest first with nothing typed, else the best `cap`
/// for what is typed against the name and the last commit's subject. A
/// remote's branch is left out where a local branch has its name, as both
/// come to the same.
pub fn branches(hosts: &Hosts, focus: Option<&Place>, query: &str, cap: usize) -> Vec<Choice> {
    use tsumugi_model::StartPoint;
    let Some(focus) = focus else {
        return Vec::new();
    };
    let Some(listed) = hosts
        .work(&focus.environment)
        .and_then(|w| w.pick.branches.get(&focus.worktree))
    else {
        return Vec::new();
    };
    let local = |name: &str| {
        listed
            .iter()
            .any(|c| matches!(&c.point, StartPoint::Local { branch } if branch == name))
    };
    let offers = listed
        .iter()
        .filter(|c| match &c.point {
            StartPoint::Local { .. } => true,
            StartPoint::Remote { branch, .. } => !local(branch),
            StartPoint::Head | StartPoint::Rev { .. } => false,
        })
        .map(|c| BranchOffer {
            environment: focus.environment.clone(),
            worktree: focus.worktree.clone(),
            point: c.point.clone(),
            subject: c.subject.clone(),
            checked_out: c.checked_out.clone(),
        });
    let query = Query::new(query);
    if query.is_empty() {
        return offers.take(cap).map(Choice::Branch).collect();
    }
    best(
        offers.filter_map(|o| {
            let name = o.point.name();
            query
                .rank_named(&format!("{name} {}", o.subject), &name)
                .map(|rank| (rank, Choice::Branch(o)))
        }),
        cap,
    )
}

/// The resources the projects of `hosts` keep, those of the project of
/// `focus` before the others, each group ranked among its own.
pub fn resources(hosts: &Hosts, focus: Option<&Place>, query: &str, cap: usize) -> Vec<Choice> {
    let query = Query::new(query);
    let mut here = Vec::new();
    let mut elsewhere = Vec::new();
    for (environment, host) in hosts.each() {
        let Some(work) = host.work.as_ref() else {
            continue;
        };
        let project = focus
            .filter(|p| p.environment == *environment)
            .and_then(|p| work.tree.tree().workspace_of(&p.worktree))
            .map(|ws| ws.root.clone());
        for resource in &work.tree.resources {
            let offer = ResourceOffer {
                environment: environment.clone(),
                resource: resource.clone(),
            };
            let group = match Some(&resource.project) == project.as_ref() {
                true => &mut here,
                false => &mut elsewhere,
            };
            group.push(offer);
        }
    }
    let ranked = |offers: Vec<ResourceOffer>| {
        if query.is_empty() {
            return offers.into_iter().map(Choice::Resource).collect();
        }
        best(
            offers.into_iter().filter_map(|o| {
                let title = o.resource.title();
                let text = format!(
                    "{title} {} {}",
                    o.resource.target,
                    o.resource.note.as_deref().unwrap_or_default()
                );
                query
                    .rank_named(&text, &title)
                    .map(|rank| (rank, Choice::Resource(o)))
            }),
            usize::MAX,
        )
    };
    ranked(here)
        .into_iter()
        .chain(ranked(elsewhere))
        .take(cap)
        .collect()
}

/// The files of `focus`, the worktree shown, as the host listed them: the
/// best `cap` for what is typed, in the order listed with nothing typed.
pub fn files(hosts: &Hosts, focus: Option<&Place>, query: &str, cap: usize) -> Vec<Choice> {
    let Some(focus) = focus else {
        return Vec::new();
    };
    let Some(listed) = hosts
        .work(&focus.environment)
        .and_then(|w| w.pick.files.get(&focus.worktree))
    else {
        return Vec::new();
    };
    let query = Query::new(query);
    let offer = |path: &String| FileOffer {
        environment: focus.environment.clone(),
        worktree: focus.worktree.clone(),
        path: path.clone(),
    };
    if query.is_empty() {
        return listed
            .iter()
            .take(cap)
            .map(|p| Choice::File(offer(p)))
            .collect();
    }
    best(
        listed.iter().filter_map(|p| {
            query
                .rank_path(Path::new(p))
                .map(|rank| (rank, Choice::File(offer(p))))
        }),
        cap,
    )
}

/// What an action is called where it is listed: the menu bar's name, else
/// the keys help's, else a right-click menu's, else the name bindings use.
fn label(spec: &crate::actions::ActionSpec) -> String {
    spec.bar
        .or(spec.says)
        .or(spec.row.map(|r| r.label))
        .unwrap_or(spec.name)
        .to_string()
}

/// The actions a binding can name alone, in the order of the key reference,
/// or the best `cap` for what is typed against their label and name.
pub fn actions(query: &str, cap: usize) -> Vec<Choice> {
    let query = Query::new(query);
    let named: Vec<&crate::actions::ActionSpec> = ACTIONS
        .iter()
        .filter(|s| s.action != Action::None && s.action.number().is_none())
        .collect();
    let offer = |s: &crate::actions::ActionSpec| ActionOffer {
        name: s.name,
        label: label(s),
    };
    if query.is_empty() {
        return named
            .into_iter()
            .take(cap)
            .map(|s| Choice::Action(offer(s)))
            .collect();
    }
    best(
        named.into_iter().filter_map(|s| {
            let o = offer(s);
            let text = format!("{} {}", o.label, o.name);
            query
                .rank_named(&text, &o.label)
                .map(|rank| (rank, Choice::Action(o)))
        }),
        cap,
    )
}
