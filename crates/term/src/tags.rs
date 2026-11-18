//! Values kept in a session's tags, `key=value`, written across as many
//! tags as a long one takes: a tag holds only so many bytes, and a path or
//! a name can hold more.

use felis_protocol::messages::MAX_TAG_BYTES;

/// The tags that hold `value` under `key`.
pub fn tagged(key: &str, value: &str) -> Vec<String> {
    let whole = format!("{key}={value}");
    if whole.len() <= MAX_TAG_BYTES {
        return vec![whole];
    }
    let mut out = Vec::new();
    let mut rest = value;
    while !rest.is_empty() {
        let head = format!("{key}.{}=", out.len());
        let room = MAX_TAG_BYTES - head.len();
        let mut end = room.min(rest.len());
        while !rest.is_char_boundary(end) {
            end -= 1;
        }
        out.push(format!("{head}{}", &rest[..end]));
        rest = &rest[end..];
    }
    out
}

/// The value held under `key`, whole or in parts.
pub fn value(tags: &[String], key: &str) -> Option<String> {
    value_at(tags, key).or_else(|| earlier_key(key).and_then(|old| value_at(tags, &old)))
}

fn earlier_key(key: &str) -> Option<String> {
    // Pre-release Tsumugi builds wrote the inherited namespace into live
    // terminals. Reading it keeps their programs and unsaved editors visible.
    let suffix = key.strip_prefix("tsumugi")?;
    (suffix.starts_with('.') || suffix.starts_with(':')).then(|| format!("adroit{suffix}"))
}

fn value_at(tags: &[String], key: &str) -> Option<String> {
    let whole = format!("{key}=");
    if let Some(v) = tags.iter().find_map(|t| t.strip_prefix(&whole)) {
        return Some(v.to_string());
    }
    let mut out = String::new();
    for n in 0.. {
        let head = format!("{key}.{n}=");
        match tags.iter().find_map(|t| t.strip_prefix(&head)) {
            Some(part) => out.push_str(part),
            None if n == 0 => return None,
            None => break,
        }
    }
    Some(out)
}

/// The tags among `tags` that hold something under `key`.
pub fn holding(tags: &[String], key: &str) -> Vec<String> {
    let whole = format!("{key}=");
    let part = format!("{key}.");
    let earlier = earlier_key(key).map(|old| (format!("{old}="), format!("{old}.")));
    tags.iter()
        .filter(|t| {
            t.starts_with(&whole)
                || t.starts_with(&part)
                || earlier
                    .as_ref()
                    .is_some_and(|(whole, part)| t.starts_with(whole) || t.starts_with(part))
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pre_release_terminal_tags_remain_readable_and_are_replaced_together() {
        let mut tags = tagged("adroit.name", &"old terminal ".repeat(20));
        assert_eq!(
            value(&tags, "tsumugi.name"),
            Some("old terminal ".repeat(20))
        );
        assert_eq!(holding(&tags, "tsumugi.name"), tags);
        tags.extend(tagged("tsumugi.name", "renamed"));
        assert_eq!(value(&tags, "tsumugi.name").as_deref(), Some("renamed"));
        assert_eq!(holding(&tags, "tsumugi.name"), tags);
        assert_eq!(value(&tags, "other.name"), None);
    }

    #[test]
    fn a_pre_release_startup_lease_can_still_be_released() {
        let tags = vec!["adroit:startup-lease=/tmp/existing.sock".into()];
        assert_eq!(
            value(&tags, "tsumugi:startup-lease").as_deref(),
            Some("/tmp/existing.sock")
        );
        assert_eq!(tagged("tsumugi.kind", "shell"), ["tsumugi.kind=shell"]);
    }

    #[test]
    fn a_short_value_is_one_tag() {
        assert_eq!(tagged("tsumugi.kind", "shell"), vec!["tsumugi.kind=shell"]);
        assert_eq!(
            value(&tagged("tsumugi.kind", "shell"), "tsumugi.kind").as_deref(),
            Some("shell")
        );
    }

    #[test]
    fn a_long_value_is_spread_over_tags_and_read_back_whole() {
        let long = "日本語のとても長い名前".repeat(10);
        let tags = tagged("tsumugi.name", &long);
        assert!(tags.len() > 1);
        assert!(tags.iter().all(|t| t.len() <= MAX_TAG_BYTES));
        assert_eq!(value(&tags, "tsumugi.name"), Some(long));
    }

    #[test]
    fn a_key_holds_nothing_when_no_tag_names_it() {
        let tags = vec!["tsumugi.kindly=x".to_string()];
        assert_eq!(value(&tags, "tsumugi.kind"), None);
        assert!(holding(&tags, "tsumugi.kind").is_empty());
    }

    #[test]
    fn the_tags_holding_a_key_are_found_to_be_replaced() {
        let mut tags = tagged("tsumugi.name", &"x".repeat(200));
        tags.push("tsumugi.kind=shell".into());
        assert_eq!(holding(&tags, "tsumugi.name").len(), 2);
    }
}
