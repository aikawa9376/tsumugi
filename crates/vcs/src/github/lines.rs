//! The review threads of the pull request a worktree's branch opened, pinned
//! to the lines GitHub quoted, for the branch's diff to draw.

use std::path::Path;

use serde::Deserialize;
use tsumugi_model::github::{LineRemark, LineThread};
use tsumugi_model::review::{Anchor, Side};

use super::{reason, remote};
use crate::branch::head;
use crate::{Error, Head, run};

const QUERY: &str = "query($owner:String!,$name:String!,$branch:String!){repository(owner:$owner,name:$name){pullRequests(headRefName:$branch,states:OPEN,first:10){nodes{headRepositoryOwner{login} reviewThreads(first:100){nodes{isResolved isOutdated path diffSide line startLine originalLine originalStartLine comments(first:50){nodes{url body author{login} diffHunk}}}}}}}}";

/// The review threads on lines of the open pull request of the branch the
/// worktree at `root` has checked out; none where it has none, or where the
/// repository is not on GitHub.
pub async fn line_threads(root: &Path) -> Result<Vec<LineThread>, String> {
    let Some(remote) = remote(root).await else {
        return Ok(Vec::new());
    };
    let Ok(Head::Branch(branch)) = head(root).await else {
        return Ok(Vec::new());
    };
    let Some((owner, name)) = remote.slug.split_once('/') else {
        return Ok(Vec::new());
    };
    let query = format!("query={QUERY}");
    let (owner_field, name_field, branch_field) = (
        format!("owner={owner}"),
        format!("name={name}"),
        format!("branch={branch}"),
    );
    let args = [
        "api",
        "graphql",
        "-f",
        &query,
        "-F",
        &owner_field,
        "-F",
        &name_field,
        "-f",
        &branch_field,
    ];
    match run("gh", &args, Some(root)).await {
        Ok(out) => read(&out, owner),
        Err(Error::Failed { stderr, .. }) => Err(reason(&stderr)),
        Err(e) => Err(e.to_string()),
    }
}

#[derive(Deserialize)]
struct Answer {
    data: Data,
}

#[derive(Deserialize)]
struct Data {
    repository: Option<Repository>,
}

#[derive(Deserialize)]
struct Repository {
    #[serde(rename = "pullRequests")]
    pulls: Nodes<Pull>,
}

#[derive(Deserialize)]
struct Nodes<T> {
    nodes: Vec<T>,
}

#[derive(Deserialize)]
struct Login {
    login: String,
}

#[derive(Deserialize)]
struct Pull {
    #[serde(rename = "headRepositoryOwner")]
    owner: Option<Login>,
    #[serde(rename = "reviewThreads")]
    threads: Nodes<Thread>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Thread {
    is_resolved: bool,
    is_outdated: bool,
    path: String,
    diff_side: String,
    line: Option<u32>,
    start_line: Option<u32>,
    original_line: Option<u32>,
    original_start_line: Option<u32>,
    comments: Nodes<Remark>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Remark {
    url: String,
    body: String,
    author: Option<Login>,
    diff_hunk: String,
}

/// The threads of the first pull request in `out` whose head is in the
/// repository of `owner`: a fork's branch of the same name is another's
/// pull request. A thread on a whole file, with no lines, is left out.
fn read(out: &str, owner: &str) -> Result<Vec<LineThread>, String> {
    let answer: Answer = serde_json::from_str(out).map_err(|e| e.to_string())?;
    let Some(pull) = answer.data.repository.and_then(|r| {
        r.pulls.nodes.into_iter().find(|p| {
            p.owner
                .as_ref()
                .is_some_and(|o| o.login.eq_ignore_ascii_case(owner))
        })
    }) else {
        return Ok(Vec::new());
    };
    Ok(pull.threads.nodes.into_iter().filter_map(thread).collect())
}

fn thread(t: Thread) -> Option<LineThread> {
    // An outdated thread has lines only where it was first made.
    let end = t.line.or(t.original_line)?;
    let start = if t.line.is_some() {
        t.start_line
    } else {
        t.original_start_line
    }
    .unwrap_or(end)
    .min(end);
    let side = if t.diff_side == "LEFT" {
        Side::Old
    } else {
        Side::New
    };
    let first = t.comments.nodes.first()?;
    let quoted = quoted(&first.diff_hunk, side, (end - start + 1) as usize);
    Some(LineThread {
        url: first.url.clone(),
        anchor: Anchor {
            path: t.path,
            side,
            start,
            end,
        },
        quoted,
        resolved: t.is_resolved,
        outdated: t.is_outdated,
        remarks: t
            .comments
            .nodes
            .into_iter()
            .map(|c| LineRemark {
                author: c.author.map(|a| a.login).unwrap_or_else(|| "ghost".into()),
                body: c.body,
            })
            .collect(),
    })
}

/// The last `lines` lines of `side` in the hunk GitHub quotes a thread with,
/// which ends at the line the thread is on.
fn quoted(hunk: &str, side: Side, lines: usize) -> Vec<String> {
    let skipped = match side {
        Side::Old => '+',
        Side::New => '-',
    };
    let of_side: Vec<String> = hunk
        .lines()
        .filter(|l| !l.starts_with("@@") && !l.starts_with('\\'))
        .filter(|l| !l.starts_with(skipped))
        .map(|l| l.get(1..).unwrap_or_default().to_string())
        .collect();
    of_side[of_side.len().saturating_sub(lines)..].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn answer(pulls: serde_json::Value) -> String {
        json!({"data": {"repository": {"pullRequests": {"nodes": pulls}}}}).to_string()
    }

    fn pull(owner: &str, threads: serde_json::Value) -> serde_json::Value {
        json!({"headRepositoryOwner": {"login": owner}, "reviewThreads": {"nodes": threads}})
    }

    fn remark(author: &str, body: &str, hunk: &str) -> serde_json::Value {
        json!({"url": format!("https://github.com/o/r/pull/1#{body}"), "body": body, "author": {"login": author}, "diffHunk": hunk})
    }

    const HUNK: &str = "@@ -1,4 +1,4 @@\n fn a() {\n-    old();\n+    new();\n+    more();";

    #[test]
    fn a_thread_quotes_the_lines_of_its_side_that_end_its_hunk() {
        let out = answer(json!([pull(
            "o",
            json!([{
                "isResolved": false, "isOutdated": false, "path": "src/a.rs", "diffSide": "RIGHT",
                "line": 3, "startLine": 2, "originalLine": 3, "originalStartLine": 2,
                "comments": {"nodes": [remark("alice", "why?", HUNK), remark("bob", "because", "")]}
            }])
        )]));
        let threads = read(&out, "o").unwrap();
        assert_eq!(threads.len(), 1);
        let t = &threads[0];
        assert_eq!(
            t.anchor,
            Anchor {
                path: "src/a.rs".into(),
                side: Side::New,
                start: 2,
                end: 3
            }
        );
        assert_eq!(t.quoted, vec!["    new();", "    more();"]);
        assert_eq!(t.url, "https://github.com/o/r/pull/1#why?");
        assert_eq!(
            t.remarks
                .iter()
                .map(|r| r.author.as_str())
                .collect::<Vec<_>>(),
            ["alice", "bob"]
        );
    }

    #[test]
    fn a_thread_on_the_old_side_quotes_the_removed_and_unchanged_lines() {
        assert_eq!(quoted(HUNK, Side::Old, 2), vec!["fn a() {", "    old();"]);
    }

    #[test]
    fn an_outdated_thread_keeps_the_lines_it_was_first_made_on() {
        let out = answer(json!([pull(
            "o",
            json!([{
                "isResolved": true, "isOutdated": true, "path": "a", "diffSide": "LEFT",
                "line": null, "startLine": null, "originalLine": 2, "originalStartLine": null,
                "comments": {"nodes": [remark("alice", "x", HUNK)]}
            }])
        )]));
        let t = &read(&out, "o").unwrap()[0];
        assert!(t.outdated && t.resolved);
        assert_eq!(
            (t.anchor.side, t.anchor.start, t.anchor.end),
            (Side::Old, 2, 2)
        );
    }

    #[test]
    fn the_threads_of_a_branch_s_pull_request_are_read_from_its_head_s_owner_only() {
        let thread = json!([{
            "isResolved": false, "isOutdated": false, "path": "a", "diffSide": "RIGHT",
            "line": 1, "startLine": null, "originalLine": 1, "originalStartLine": null,
            "comments": {"nodes": [remark("alice", "x", "@@ -0,0 +1 @@\n+a")]}
        }]);
        let out = answer(json!([pull("fork", thread.clone()), pull("O", json!([]))]));
        assert!(read(&out, "o").unwrap().is_empty());
        let out = answer(json!([pull("fork", json!([])), pull("o", thread)]));
        assert_eq!(read(&out, "o").unwrap().len(), 1);
    }

    #[test]
    fn a_thread_on_a_whole_file_is_left_out() {
        let out = answer(json!([pull(
            "o",
            json!([{
                "isResolved": false, "isOutdated": false, "path": "a", "diffSide": "RIGHT",
                "line": null, "startLine": null, "originalLine": null, "originalStartLine": null,
                "comments": {"nodes": [remark("alice", "x", "")]}
            }])
        )]));
        assert!(read(&out, "o").unwrap().is_empty());
    }
}
