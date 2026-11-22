//! Named browser profiles: each keeps website data of its own, so that a page
//! can be signed in as another user than the pages beside it.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tsumugi_core::content::BrowserId;
use tsumugi_model::EnvironmentId;

/// What the profile every page starts in is called.
pub(super) const DEFAULT: &str = "Default";

/// How long a profile's name can be.
const LONGEST: usize = 40;

thread_local! {
    /// The profile each page was moved to; a page left out is in the default
    /// one. A page outlives the window that shows it, so its profile does too.
    static CHOSEN: RefCell<HashMap<BrowserId, String>> = RefCell::new(HashMap::new());
    /// The names of the profiles made, as the file kept them when it was last
    /// read or written.
    static MADE: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
}

/// The profile page `id` is in, unless it is the default one.
pub(super) fn chosen(id: BrowserId) -> Option<String> {
    CHOSEN.with(|chosen| chosen.borrow().get(&id).cloned())
}

/// Put page `id` in `profile`, the default one for none.
pub(super) fn choose(id: BrowserId, profile: Option<String>) {
    CHOSEN.with(|chosen| {
        let mut chosen = chosen.borrow_mut();
        match profile {
            Some(profile) => chosen.insert(id, profile),
            None => chosen.remove(&id),
        };
    });
}

/// Put page `id` in the profile page `from` is in, as a page opened from it
/// goes on as the same user.
pub(super) fn inherit(from: BrowserId, id: BrowserId) {
    choose(id, chosen(from));
}

/// The name a profile asked for goes by: `typed` without the space around
/// it, if nothing else is called alike. Case does not tell two names apart.
pub(super) fn named(typed: &str, existing: &[String]) -> Result<String, &'static str> {
    let name = typed.trim();
    if name.is_empty() {
        return Err("Name the profile");
    }
    if name.chars().count() > LONGEST {
        return Err("Use a shorter name");
    }
    if name.chars().any(char::is_control) {
        return Err("A name cannot hold control characters");
    }
    let alike = |other: &str| other.to_lowercase() == name.to_lowercase();
    if alike(DEFAULT) || existing.iter().any(|other| alike(other)) {
        return Err("A profile is called that already");
    }
    Ok(name.to_string())
}

/// The SHA-256 that names the website data of `profile` in `environment`:
/// the default profile's is the environment's own, as it was before profiles
/// were, so what a user signed in to stays signed in. A name is length-
/// prefixed so that no environment and name read as another pair.
pub(super) fn digest(environment: &EnvironmentId, profile: Option<&str>) -> [u8; 32] {
    let mut hash = Sha256::new();
    match profile {
        None => hash.update(environment.to_string().as_bytes()),
        Some(profile) => {
            let environment = environment.to_string();
            hash.update(b"profile:");
            hash.update(environment.len().to_string().as_bytes());
            hash.update(b":");
            hash.update(environment.as_bytes());
            hash.update(profile.as_bytes());
        }
    }
    hash.finalize().into()
}

/// The folder under `root` that holds the website data of `profile` in
/// `environment`.
pub(super) fn directory(
    root: &Path,
    environment: &EnvironmentId,
    profile: Option<&str>,
) -> PathBuf {
    root.join(
        digest(environment, profile)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>(),
    )
}

/// The profiles named in the file at `path`; a missing or unreadable file
/// names none.
fn read(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Vec<String>>(&text).ok())
        .unwrap_or_default()
}

fn write(path: &Path, names: &[String]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(names)?)
}

fn file() -> PathBuf {
    crate::paths::data_dir().join("tsumugi/browser-profiles.json")
}

/// The profiles made, in the order they were made.
pub(super) fn made() -> Vec<String> {
    MADE.with(|made| {
        made.borrow_mut()
            .get_or_insert_with(|| read(&file()))
            .clone()
    })
}

/// Make a profile called `typed`, and keep it.
pub(super) fn make(typed: &str) -> Result<String, String> {
    make_in(&file(), typed)
}

fn make_in(path: &Path, typed: &str) -> Result<String, String> {
    // Another window of this run may have made one since this one read.
    let mut names = read(path);
    let name = named(typed, &names).map_err(str::to_string)?;
    names.push(name.clone());
    write(path, &names).map_err(|e| e.to_string())?;
    MADE.with(|made| *made.borrow_mut() = Some(names));
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(name: &str) -> EnvironmentId {
        EnvironmentId::new(name)
    }

    #[test]
    fn a_name_is_trimmed_and_may_not_repeat_another_whatever_its_case() {
        let existing = vec!["Staging".to_string()];
        assert_eq!(named("  Admin ", &existing).as_deref(), Ok("Admin"));
        for typed in ["staging", "STAGING ", "default", "Default"] {
            assert!(named(typed, &existing).is_err(), "{typed}");
        }
    }

    #[test]
    fn a_name_is_not_empty_long_or_made_of_control_characters() {
        for typed in ["", "   ", "a\nb", &"x".repeat(41)] {
            assert!(named(typed, &[]).is_err(), "{typed:?}");
        }
        assert!(named(&"x".repeat(40), &[]).is_ok());
    }

    #[test]
    fn the_default_profile_keeps_the_data_an_environment_had_before_profiles() {
        let root = Path::new("/profiles");
        assert_eq!(
            directory(root, &env("a"), None),
            root.join(
                Sha256::digest(b"a")
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            )
        );
    }

    #[test]
    fn each_profile_of_each_environment_keeps_data_of_its_own() {
        let root = Path::new("/profiles");
        let places = [
            directory(root, &env("a"), None),
            directory(root, &env("a"), Some("Admin")),
            directory(root, &env("a"), Some("Guest")),
            directory(root, &env("b"), Some("Admin")),
            // Neither the name nor the environment can shift the other.
            directory(root, &env("a1"), Some("Admin")),
            directory(root, &env("a"), Some("1Admin")),
            directory(root, &env("../a"), Some("../Admin")),
        ];
        for (n, place) in places.iter().enumerate() {
            assert_eq!(place.parent(), Some(root));
            assert!(places.iter().skip(n + 1).all(|other| other != place), "{n}");
        }
    }

    #[test]
    fn a_page_goes_to_a_profile_and_back_and_one_opened_from_it_goes_on_in_it() {
        let (page, from_it) = (BrowserId::new(), BrowserId::new());
        assert_eq!(chosen(page), None);

        choose(page, Some("Admin".into()));
        inherit(page, from_it);

        assert_eq!(chosen(page).as_deref(), Some("Admin"));
        assert_eq!(chosen(from_it).as_deref(), Some("Admin"));
        choose(page, None);
        assert_eq!(chosen(page), None);
    }

    #[test]
    fn the_profiles_made_are_kept_in_order_and_read_again() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/profiles.json");
        assert_eq!(read(&path), Vec::<String>::new());

        assert_eq!(make_in(&path, "Admin").as_deref(), Ok("Admin"));
        assert_eq!(make_in(&path, " Guest ").as_deref(), Ok("Guest"));
        assert!(make_in(&path, "admin").is_err());

        assert_eq!(read(&path), ["Admin", "Guest"]);
    }

    #[test]
    fn a_file_that_is_not_a_list_of_names_names_no_profile() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profiles.json");
        std::fs::write(&path, "{\"not\": \"a list\"}").unwrap();
        assert_eq!(read(&path), Vec::<String>::new());
    }
}
