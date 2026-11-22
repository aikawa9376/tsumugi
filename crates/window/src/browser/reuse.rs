//! A page already at the address a link asks for is shown again, not opened a
//! second time.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;

use tsumugi_core::content::BrowserId;
use tsumugi_core::tree::nav::Page as Listed;
use tsumugi_model::EnvironmentId;

thread_local! {
    /// The address each page is at, as its environment writes it.
    static ADDRESSES: RefCell<HashMap<BrowserId, String>> = RefCell::new(HashMap::new());
}

/// The page `id` is at `address`.
pub(super) fn at(id: BrowserId, address: String) {
    ADDRESSES.with(|addresses| addresses.borrow_mut().insert(id, address));
}

/// The page `id` is gone.
pub(super) fn forget(id: BrowserId) {
    ADDRESSES.with(|addresses| addresses.borrow_mut().remove(&id));
}

/// The page of `environment` under `worktree`, among `listed`, that is at
/// `wanted`, or is on its way there. A link comes from no page, so it is
/// met only by a page in the default profile: another is another user.
pub(super) fn existing(
    listed: &[(EnvironmentId, Listed)],
    environment: &EnvironmentId,
    worktree: &Path,
    wanted: &str,
) -> Option<BrowserId> {
    let candidates = listed
        .iter()
        .filter(|(env, page)| env == environment && page.worktree == worktree)
        .filter(|(_, page)| super::profiles::chosen(page.id).is_none())
        .map(|(_, page)| {
            let address = super::START
                .with(|start| start.borrow().get(&page.id).cloned())
                .or_else(|| ADDRESSES.with(|addresses| addresses.borrow().get(&page.id).cloned()));
            (page.id, address)
        })
        .collect::<Vec<_>>();
    find(candidates, wanted)
}

/// The first of `pages`, by identity, whose address is `wanted`.
fn find(
    pages: impl IntoIterator<Item = (BrowserId, Option<String>)>,
    wanted: &str,
) -> Option<BrowserId> {
    pages
        .into_iter()
        .filter(|(_, address)| address.as_deref().is_some_and(|a| same_address(a, wanted)))
        .map(|(id, _)| id)
        .min()
}

/// Whether two web addresses are the same destination: the scheme and the
/// host are not case-sensitive, and a bare host is its root path. The rest,
/// fragment included, has to match.
fn same_address(a: &str, b: &str) -> bool {
    fn normal(url: &str) -> Option<String> {
        let (scheme, rest) = url.split_once("://")?;
        let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        let (authority, after) = rest.split_at(end);
        let after = if after.is_empty() || after.starts_with(['?', '#']) {
            format!("/{after}")
        } else {
            after.to_string()
        };
        Some(format!(
            "{}://{}{after}",
            scheme.to_ascii_lowercase(),
            authority.to_ascii_lowercase()
        ))
    }
    matches!((normal(a), normal(b)), (Some(a), Some(b)) if a == b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pages(n: &[Option<&str>]) -> Vec<(BrowserId, Option<String>)> {
        n.iter()
            .map(|a| (BrowserId::new(), a.map(str::to_string)))
            .collect()
    }

    #[test]
    fn the_same_destination_is_found_however_the_page_writes_it() {
        for (a, b) in [
            ("https://example.com", "https://example.com/"),
            ("HTTPS://Example.COM/a", "https://example.com/a"),
            ("http://localhost:3000?q=1", "http://localhost:3000/?q=1"),
            ("https://example.com/a#b", "https://example.com/a#b"),
        ] {
            assert!(same_address(a, b), "{a} {b}");
        }
    }

    #[test]
    fn another_path_query_fragment_or_port_is_another_destination() {
        for (a, b) in [
            ("https://example.com/a", "https://example.com/b"),
            ("https://example.com/A", "https://example.com/a"),
            ("https://example.com/?q=1", "https://example.com/?q=2"),
            ("https://example.com/#a", "https://example.com/#b"),
            ("http://localhost:3000/", "http://localhost:3001/"),
            ("http://example.com/", "https://example.com/"),
            ("about:blank", "about:blank"),
        ] {
            assert!(!same_address(a, b), "{a} {b}");
        }
    }

    #[test]
    fn the_page_at_the_address_is_chosen_and_the_first_of_two_wins() {
        let listed = pages(&[
            Some("https://other.example/"),
            Some("https://example.com/"),
            Some("https://example.com"),
            None,
        ]);
        let first = listed[1].0;
        let mut shuffled = listed.clone();
        shuffled.reverse();
        assert_eq!(find(shuffled, "https://example.com/"), Some(first));
        assert_eq!(find(listed, "https://none.example/"), None);
    }

    #[test]
    fn a_page_is_found_only_under_its_own_worktree_of_its_environment() {
        let env = EnvironmentId::new("a");
        let page = |worktree: &str| Listed {
            worktree: worktree.into(),
            id: BrowserId::new(),
        };
        let (here, there) = (page("/w"), page("/v"));
        let listed = vec![
            (env.clone(), here.clone()),
            (env.clone(), there.clone()),
            (EnvironmentId::new("b"), page("/w")),
        ];
        at(here.id, "https://example.com/".into());
        at(there.id, "https://example.com/".into());
        assert_eq!(
            existing(&listed, &env, Path::new("/w"), "https://example.com/"),
            Some(here.id)
        );
        assert_eq!(
            existing(&listed, &env, Path::new("/x"), "https://example.com/"),
            None
        );
        crate::browser::profiles::choose(here.id, Some("Admin".into()));
        assert_eq!(
            existing(&listed, &env, Path::new("/w"), "https://example.com/"),
            None,
            "a page in another profile is another user's"
        );
        crate::browser::profiles::choose(here.id, None);
        forget(here.id);
        assert_eq!(
            existing(&listed, &env, Path::new("/w"), "https://example.com/"),
            None
        );
    }
}
