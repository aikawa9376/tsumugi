//! Which of the picker's sources a query searches: the scope it is stepped
//! to, unless the query starts with the prefix of another.

use crate::settings::{PickerScope, PickerSource};

/// What the picker searches for what is typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Aim<'a> {
    /// The scope it is in, counted from 0.
    pub scope: usize,
    pub sources: &'a [PickerSource],
    /// What is typed, the prefix that entered the scope taken off.
    pub query: &'a str,
}

/// What a picker with no scopes searches: sessions and directories.
const GO: &[PickerSource] = &[PickerSource::Sessions, PickerSource::Directories];

/// Directories alone, for what adds to the tree.
pub fn directories_only(query: &str) -> Aim<'_> {
    Aim {
        scope: 0,
        sources: &[PickerSource::Directories],
        query,
    }
}

/// The scope `query` is searched in: that whose prefix it starts with, the
/// longest of those that do, else the scope stepped to.
pub fn resolve<'a>(scopes: &'a [PickerScope], stepped: usize, query: &'a str) -> Aim<'a> {
    let entered = scopes
        .iter()
        .enumerate()
        .filter_map(|(at, scope)| Some((at, scope, entered(&scope.prefix, query)?)))
        .max_by_key(|(_, scope, _)| scope.prefix.len());
    if let Some((scope, entered, rest)) = entered {
        return Aim {
            scope,
            sources: &entered.sources,
            query: rest,
        };
    }
    match scopes.get(stepped).or(scopes.first()) {
        Some(scope) => Aim {
            scope: stepped.min(scopes.len() - 1),
            sources: &scope.sources,
            query,
        },
        None => Aim {
            scope: 0,
            sources: GO,
            query,
        },
    }
}

/// What follows `prefix` at the start of `query`, when it starts with it. A
/// prefix ending in a letter or digit is a word of its own: it ends where
/// the query does or at a space.
fn entered<'a>(prefix: &str, query: &'a str) -> Option<&'a str> {
    if prefix.is_empty() {
        return None;
    }
    let rest = query.strip_prefix(prefix)?;
    let word = prefix.chars().last().is_some_and(char::is_alphanumeric);
    if word && !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    Some(rest.trim_start())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(prefix: &str, sources: &[PickerSource]) -> PickerScope {
        PickerScope {
            name: prefix.into(),
            prefix: prefix.into(),
            sources: sources.to_vec(),
        }
    }

    #[test]
    fn a_prefix_typed_first_enters_its_scope_and_is_taken_off_the_query() {
        let scopes = [
            scope("", &[PickerSource::Sessions]),
            scope(">", &[PickerSource::Actions]),
        ];
        let aim = resolve(&scopes, 0, ">  new tab");
        assert_eq!(aim.scope, 1);
        assert_eq!(aim.sources, [PickerSource::Actions]);
        assert_eq!(aim.query, "new tab");
        assert_eq!(resolve(&scopes, 0, "new > tab").scope, 0);
    }

    #[test]
    fn a_prefix_of_letters_is_a_word_of_its_own() {
        let scopes = [
            scope("", &[PickerSource::Sessions]),
            scope("d", &[PickerSource::Resources]),
        ];
        assert_eq!(resolve(&scopes, 0, "design").scope, 0);
        assert_eq!(resolve(&scopes, 0, "d design").scope, 1);
        assert_eq!(resolve(&scopes, 0, "d").scope, 1);
    }

    #[test]
    fn the_longest_prefix_wins() {
        let scopes = [
            scope(":", &[PickerSource::Worktrees]),
            scope("::", &[PickerSource::Files]),
        ];
        assert_eq!(resolve(&scopes, 0, "::a").scope, 1);
        assert_eq!(resolve(&scopes, 0, ":a").scope, 0);
    }

    #[test]
    fn with_no_prefix_typed_the_scope_stepped_to_is_searched() {
        let scopes = [
            scope("", &[PickerSource::Sessions]),
            scope("", &[PickerSource::Files]),
        ];
        let aim = resolve(&scopes, 1, "main");
        assert_eq!(
            (aim.scope, aim.sources, aim.query),
            (1, &[PickerSource::Files][..], "main")
        );
        assert_eq!(resolve(&[], 0, "x").sources, GO);
    }
}
