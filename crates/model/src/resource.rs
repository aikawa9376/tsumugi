//! A path or an address the user keeps in a project for its agents and for
//! themselves. Only the reference is kept, never what it points at.
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The longest a target, a label or a note may be.
pub const MAX_TARGET: usize = 2048;
pub const MAX_LABEL: usize = 256;
pub const MAX_NOTE: usize = 4096;

/// What a resource points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reach {
    /// A web address, or any other `scheme://` address.
    Address,
    /// A file or a folder on the work machine, by its absolute path.
    Path,
}

impl Reach {
    /// What `target` points at, told by its form alone.
    pub fn of(target: &str) -> Reach {
        match target.split_once("://") {
            Some((scheme, _))
                if !scheme.is_empty()
                    && scheme
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-')) =>
            {
                Reach::Address
            }
            _ => Reach::Path,
        }
    }
}

/// A resource to keep.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewResource {
    pub id: String,
    /// The project it belongs to: the root of its workspace.
    pub project: PathBuf,
    /// An address, or the absolute path of a file or folder.
    pub target: String,
    pub label: Option<String>,
    pub note: Option<String>,
}

/// A resource the project keeps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resource {
    pub id: String,
    pub project: PathBuf,
    pub target: String,
    pub label: Option<String>,
    pub note: Option<String>,
}

impl Resource {
    pub fn reach(&self) -> Reach {
        Reach::of(&self.target)
    }

    /// What the resource is called: its label, else the last part of its
    /// path, else its address without the scheme.
    pub fn title(&self) -> String {
        if let Some(label) = self.label.as_deref().filter(|l| !l.trim().is_empty()) {
            return label.trim().to_owned();
        }
        match self.reach() {
            Reach::Path => {
                let trimmed = self.target.trim_end_matches('/');
                trimmed
                    .rsplit('/')
                    .next()
                    .filter(|name| !name.is_empty())
                    .unwrap_or(trimmed)
                    .to_owned()
            }
            Reach::Address => self
                .target
                .split_once("://")
                .map_or(self.target.as_str(), |(_, rest)| rest)
                .trim_end_matches('/')
                .to_owned(),
        }
    }

    /// Whether every word of `query` appears, in any case, in the resource's
    /// label, target or note. An empty query matches every resource.
    pub fn matches(&self, query: &str) -> bool {
        let haystack = format!(
            "{}\n{}\n{}",
            self.label.as_deref().unwrap_or_default(),
            self.target,
            self.note.as_deref().unwrap_or_default()
        )
        .to_lowercase();
        query
            .split_whitespace()
            .all(|word| haystack.contains(&word.to_lowercase()))
    }
}

/// What is wrong with a resource to keep, if anything.
pub fn refuse(new: &NewResource) -> Option<&'static str> {
    let target = new.target.trim();
    if target.is_empty() || new.target.len() > MAX_TARGET {
        return Some("A resource needs a path or an address of at most 2048 bytes");
    }
    if target.chars().any(char::is_whitespace) {
        return Some("A path or an address holds no spaces; a label can say more");
    }
    if Reach::of(target) == Reach::Path && !target.starts_with('/') && !has_drive(target) {
        return Some("A path must be absolute");
    }
    if new.label.as_deref().is_some_and(|l| l.len() > MAX_LABEL) {
        return Some("A label is at most 256 bytes");
    }
    if new.note.as_deref().is_some_and(|n| n.len() > MAX_NOTE) {
        return Some("A note is at most 4096 bytes");
    }
    if !new.project.is_absolute() {
        return Some("A project is an absolute directory");
    }
    None
}

/// Whether `path` begins with a Windows drive, as `C:\` or `C:/`.
fn has_drive(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() > 2
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resource(target: &str, label: Option<&str>, note: Option<&str>) -> Resource {
        Resource {
            id: "r".into(),
            project: "/p".into(),
            target: target.into(),
            label: label.map(Into::into),
            note: note.map(Into::into),
        }
    }

    fn new(target: &str) -> NewResource {
        NewResource {
            id: "r".into(),
            project: "/p".into(),
            target: target.into(),
            label: None,
            note: None,
        }
    }

    #[test]
    fn a_target_with_a_scheme_is_an_address_and_any_other_is_a_path() {
        assert_eq!(Reach::of("https://example.com/a"), Reach::Address);
        assert_eq!(Reach::of("file:///etc/hosts"), Reach::Address);
        assert_eq!(Reach::of("/etc/hosts"), Reach::Path);
        assert_eq!(Reach::of("C:/work/a.md"), Reach::Path);
        assert_eq!(Reach::of("../a://b"), Reach::Path);
    }

    #[test]
    fn a_resource_is_called_by_its_label_before_anything_derived() {
        assert_eq!(
            resource("/etc/nginx/nginx.conf", Some(" Web config "), None).title(),
            "Web config"
        );
        assert_eq!(
            resource("/etc/nginx/nginx.conf", Some("  "), None).title(),
            "nginx.conf"
        );
    }

    #[test]
    fn a_path_is_called_by_its_last_part_and_an_address_by_what_follows_its_scheme() {
        assert_eq!(resource("/srv/docs/", None, None).title(), "docs");
        assert_eq!(resource("/", None, None).title(), "");
        assert_eq!(
            resource("https://example.com/guide/", None, None).title(),
            "example.com/guide"
        );
    }

    #[test]
    fn a_query_matches_when_each_of_its_words_is_somewhere_in_the_resource() {
        let r = resource(
            "/etc/nginx/nginx.conf",
            Some("Web server"),
            Some("Proxy settings"),
        );
        assert!(r.matches(""));
        assert!(r.matches("CONF"));
        assert!(r.matches("web proxy"));
        assert!(r.matches("nginx.conf server"));
        assert!(!r.matches("conf apache"));
    }

    #[test]
    fn a_resource_is_kept_only_with_a_target_that_holds_together() {
        assert_eq!(refuse(&new("/etc/hosts")), None);
        assert_eq!(refuse(&new("https://example.com/a?b=c")), None);
        assert_eq!(refuse(&new("C:\\work\\a.md")), None);
        assert!(refuse(&new("")).is_some());
        assert!(refuse(&new("  ")).is_some());
        assert!(refuse(&new("relative/path")).is_some());
        assert!(refuse(&new("/a b")).is_some());
        assert!(refuse(&new(&format!("/{}", "a".repeat(MAX_TARGET)))).is_some());
    }

    #[test]
    fn a_resource_is_kept_only_in_an_absolute_project_with_a_short_label_and_note() {
        let mut kept = new("/etc/hosts");
        kept.project = "p".into();
        assert!(refuse(&kept).is_some());
        let mut kept = new("/etc/hosts");
        kept.label = Some("l".repeat(MAX_LABEL + 1));
        assert!(refuse(&kept).is_some());
        let mut kept = new("/etc/hosts");
        kept.note = Some("n".repeat(MAX_NOTE + 1));
        assert!(refuse(&kept).is_some());
    }
}
