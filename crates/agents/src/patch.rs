//! The patches Codex's `apply_patch` tool writes files with.

/// The tool Codex writes every file with, as its hooks name it.
pub const CODEX_TOOL: &str = "apply_patch";

/// The files `patch` writes: every file it adds, updates or deletes, and
/// every one it moves a file to, in the order it names them.
pub fn files(patch: &str) -> Vec<String> {
    const HEADS: [&str; 4] = [
        "*** Add File: ",
        "*** Update File: ",
        "*** Delete File: ",
        "*** Move to: ",
    ];
    patch
        .lines()
        .filter_map(|line| HEADS.iter().find_map(|head| line.strip_prefix(head)))
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_patch_names_every_file_it_adds_updates_deletes_or_moves_to() {
        let patch = "*** Begin Patch\n\
            *** Update File: src/a.rs\n\
            *** Move to: src/メモ.rs\n\
            @@ fn a()\n\
            -*** Add File: not a heading\n\
            +x\n\
            *** Add File: b c.rs\r\n\
            +y\n\
            *** Delete File: old.rs\n\
            *** End Patch\n";
        assert_eq!(
            files(patch),
            ["src/a.rs", "src/メモ.rs", "b c.rs", "old.rs"]
        );
        assert!(files("*** Begin Patch\n*** End Patch").is_empty());
    }
}
