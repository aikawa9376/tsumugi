//! Structural diff output is display text, never a unified patch.

use crate::{Change, DIFF_BYTES, DiffKind, DiffLine, Error, git, run_command_head};
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

pub async fn file_diff(
    path: &Path,
    rev: Option<&str>,
    change: &Change,
    lines: usize,
) -> Result<Vec<DiffLine>, Error> {
    file_diff_with(path, rev, change, lines, "difft").await
}

pub(crate) async fn file_diff_with(
    path: &Path,
    rev: Option<&str>,
    change: &Change,
    lines: usize,
    program: &str,
) -> Result<Vec<DiffLine>, Error> {
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        structural(path, rev, change, lines, program),
    )
    .await;
    if let Ok(Some(text)) = output {
        return Ok(text
            .lines()
            .take(lines)
            .map(|text| DiffLine {
                kind: DiffKind::Difft,
                text: text.into(),
            })
            .collect());
    }
    git::file_diff(path, rev, change, lines).await
}

async fn structural(
    path: &Path,
    rev: Option<&str>,
    change: &Change,
    lines: usize,
    program: &str,
) -> Option<String> {
    if let Some(id) = rev {
        let parents = crate::run_head(
            "git",
            &[
                "-C",
                path.to_str()?,
                "rev-list",
                "--parents",
                "--max-count=1",
                id,
                "--",
            ],
            None,
            1,
            4096,
        )
        .await
        .ok()?;
        if parents.split_whitespace().count() > 2 {
            // A pair of snapshots cannot represent Git's combined merge diff.
            return None;
        }
    }
    let added = change.status == "??" || change.status.starts_with('A');
    let old = if added {
        Vec::new()
    } else {
        let name = change.from.as_deref().unwrap_or(&change.path);
        let base = rev.map_or_else(|| "HEAD".into(), |id| format!("{id}^"));
        blob(path, &format!("{base}:{name}")).await?
    };
    let new = match rev {
        Some(_) if change.status == "D" => Vec::new(),
        Some(id) => blob(path, &format!("{id}:{}", change.path)).await?,
        None => {
            let file = path.join(&change.path);
            match std::fs::symlink_metadata(&file) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
                Ok(meta) if meta.file_type().is_symlink() => {
                    let target = std::fs::read_link(&file).ok()?;
                    #[cfg(unix)]
                    {
                        use std::os::unix::ffi::OsStringExt;
                        target.into_os_string().into_vec()
                    }
                    #[cfg(not(unix))]
                    {
                        target.to_string_lossy().as_bytes().to_vec()
                    }
                }
                Ok(meta) if meta.is_file() && meta.len() <= DIFF_BYTES as u64 => {
                    use std::io::Read;
                    let mut bytes = Vec::new();
                    std::fs::File::open(file)
                        .ok()?
                        .take(DIFF_BYTES as u64 + 1)
                        .read_to_end(&mut bytes)
                        .ok()?;
                    bytes
                }
                _ => return None,
            }
        }
    };
    if old.len() > DIFF_BYTES || new.len() > DIFF_BYTES {
        return None;
    }
    if old == new {
        // A rename or mode change still matters when its bytes are unchanged.
        return None;
    }
    let temp = tempfile::tempdir().ok()?;
    let left = temp.path().join("old");
    let right = temp.path().join("new");
    std::fs::write(&left, &old).ok()?;
    std::fs::write(&right, &new).ok()?;
    // Git's positional format supplies the display path while both inputs
    // stay private snapshots. No configured Git diff driver is executed.
    let mut command = Command::new(program);
    command
        .args(OPTIONS)
        .arg("--")
        .arg(change.shown())
        .arg(&left)
        .args(["0000000", if added { "000000" } else { "100644" }])
        .arg(&right)
        .args([
            "0000000",
            if change.status.trim() == "D" {
                "000000"
            } else {
                "100644"
            },
        ])
        .env_remove("DFT_EXIT_CODE")
        .env_remove("DFT_CHECK_ONLY");
    run_command_head(command, "difft", lines, DIFF_BYTES)
        .await
        .ok()
}

async fn blob(path: &Path, object: &str) -> Option<Vec<u8>> {
    use tokio::io::AsyncReadExt;
    let mut child = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["cat-file", "blob", object])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .ok()?;
    let mut bytes = Vec::new();
    child
        .stdout
        .take()?
        .take(DIFF_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .await
        .ok()?;
    if bytes.len() > DIFF_BYTES {
        let _ = child.kill().await;
        return None;
    }
    child.wait().await.ok()?.success().then_some(bytes)
}

const OPTIONS: &[&str] = &[
    "--color=always",
    "--display=inline",
    "--width=100",
    "--byte-limit=1048576",
    "--graph-limit=1000000",
];
