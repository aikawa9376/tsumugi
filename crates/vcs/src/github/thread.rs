//! A pull request or an issue read whole, with the images it shows.

use std::path::Path;

use base64::Engine;
use serde::Deserialize;
use tsumugi_model::github::{
    Check, Did, FileChange, Image, Item, Label, Listing, Outcome, Post, Pull, REACTIONS, Reaction,
    ReviewThread, Thread,
};

use super::{reason, remote};
use crate::{Error, run};

/// The most bytes of images a thread carries, short enough of the largest
/// frame the work host sends once in base64 to leave room for the text.
const CARRIED_BYTES: usize = 8 * 1024 * 1024;

/// The pull request or the issue `number` of the repository at `root`, with
/// its comments and reviews, and the images they show as far as
/// [`CARRIED_BYTES`] holds them. An image of a private repository is served
/// only to its login, at an address GitHub signs for a few minutes, so the
/// host fetches it here rather than leave the window an address it cannot
/// read.
pub async fn thread(root: &Path, listing: Listing, number: u64) -> Result<Thread, String> {
    let remote = remote(root)
        .await
        .ok_or("the repository has no remote on GitHub")?;
    let slug = &remote.slug;
    let issue = api(root, &format!("repos/{slug}/issues/{number}"), None).await?;
    let comments = api(
        root,
        &format!("repos/{slug}/issues/{number}/comments?per_page=100"),
        Some(".[]"),
    )
    .await?;
    let (pull, reviews) = match listing {
        Listing::Pulls => (
            Some(api(root, &format!("repos/{slug}/pulls/{number}"), None).await?),
            api(
                root,
                &format!("repos/{slug}/pulls/{number}/reviews?per_page=100"),
                Some(".[]"),
            )
            .await?,
        ),
        Listing::Issues => (None, String::new()),
    };
    let (mut thread, mut texts) = read(&issue, pull.as_deref(), &comments, &reviews)?;
    if let Some(sha) = pull.as_deref().and_then(head_of) {
        // What cannot be read of them leaves them unlisted, not the thread
        // unread.
        let files_path = format!("repos/{slug}/pulls/{number}/files?per_page=100");
        let runs_path = format!("repos/{slug}/commits/{sha}/check-runs?per_page=100");
        let statuses_path = format!("repos/{slug}/commits/{sha}/status");
        let (files_read, runs, statuses) = tokio::join!(
            api(root, &files_path, Some(".[]")),
            api(root, &runs_path, Some(".check_runs[]")),
            api(root, &statuses_path, Some(".statuses[]")),
        );
        thread.files = files(&files_read.unwrap_or_default());
        thread.checks = checks(&runs.unwrap_or_default(), &statuses.unwrap_or_default());
        // What only GraphQL tells is left out when it cannot be read: the
        // comments on lines and the viewed marks are not drawn, and writing
        // to them is refused for want of their ids.
        if let Some((owner, name)) = slug.split_once('/')
            && let Ok(out) = graphql(root, owner, name, number).await
            && let Ok(graph) = graph(&out)
        {
            merge(&mut thread, &mut texts, graph);
        }
    }
    let mut wanted: Vec<(String, String)> = Vec::new();
    for (written, fetched) in texts.iter().flat_map(images) {
        if !wanted.iter().any(|(w, _)| *w == written) {
            wanted.push((written, fetched));
        }
    }
    // Each read fetches into a folder of its own, gone once it is read.
    static READS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let read_no = READS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("tsumugi-github-{}-{read_no}", std::process::id()));
    let fetched = fetch(&dir, wanted).await;
    thread.images = carried(fetched, CARRIED_BYTES);
    let _ = std::fs::remove_dir_all(&dir);
    Ok(thread)
}

/// What GitHub's API answers at `path`, the body of each text rendered as
/// HTML beside its Markdown; with `items`, what that jq expression picks of
/// every page, one JSON value after another.
async fn api(root: &Path, path: &str, items: Option<&str>) -> Result<String, String> {
    let mut args = vec![
        "api",
        "-H",
        "Accept: application/vnd.github.full+json",
        path,
    ];
    if let Some(items) = items {
        args.extend(["--paginate", "--jq", items]);
    }
    match run("gh", &args, Some(root)).await {
        Ok(out) => Ok(out),
        Err(Error::Failed { stderr, .. }) => Err(reason(&stderr)),
        Err(e) => Err(e.to_string()),
    }
}

/// What the GraphQL query for a [`Graph`] answers for the pull request
/// `number` of `owner/name`.
async fn graphql(root: &Path, owner: &str, name: &str, number: u64) -> Result<String, String> {
    let query = format!("query={GRAPH_QUERY}");
    let (owner, name, number) = (
        format!("owner={owner}"),
        format!("name={name}"),
        format!("number={number}"),
    );
    let args = [
        "api", "graphql", "-f", &query, "-F", &owner, "-F", &name, "-F", &number,
    ];
    match run("gh", &args, Some(root)).await {
        Ok(out) => Ok(out),
        Err(Error::Failed { stderr, .. }) => Err(reason(&stderr)),
        Err(e) => Err(e.to_string()),
    }
}

/// `graph` added to `thread`: the pull request's id, the files marked
/// viewed, and the comments on lines among the rest said, oldest first.
fn merge(thread: &mut Thread, texts: &mut Vec<Text>, graph: Graph) {
    thread.pull_id = graph.id;
    for file in &mut thread.files {
        file.viewed = graph.viewed.contains(&file.path);
    }
    for (posts, text) in graph.threads {
        thread.said.extend(posts);
        texts.extend(text);
    }
    thread.said.sort_by(|a, b| a.at.cmp(&b.at));
}

/// A text written on GitHub: its Markdown and, where GitHub gave it, the
/// HTML it renders to, whose images are addresses that can be fetched.
#[derive(Debug, Clone, Default, PartialEq)]
struct Text {
    markdown: String,
    html: String,
}

#[derive(Deserialize)]
struct User {
    login: String,
}

fn login(user: &Option<User>) -> String {
    user.as_ref()
        .map_or_else(|| "ghost".to_string(), |u| u.login.clone())
}

/// A pull request or an issue from what GitHub's API answered: the issue,
/// the pull request when it is one, and the comments and reviews, each a
/// JSON value after another; with every text it holds, the body first, for
/// the images they show.
fn read(
    issue: &str,
    pull: Option<&str>,
    comments: &str,
    reviews: &str,
) -> Result<(Thread, Vec<Text>), String> {
    #[derive(Deserialize)]
    struct Titled {
        title: String,
    }
    #[derive(Deserialize)]
    struct Issue {
        number: u64,
        title: String,
        html_url: String,
        user: Option<User>,
        body: Option<String>,
        body_html: Option<String>,
        #[serde(default)]
        state: String,
        #[serde(default)]
        created_at: String,
        #[serde(default)]
        updated_at: String,
        #[serde(default)]
        labels: Vec<Label>,
        #[serde(default)]
        assignees: Vec<User>,
        milestone: Option<Titled>,
        node_id: Option<String>,
        reactions: Option<serde_json::Map<String, serde_json::Value>>,
    }
    #[derive(Deserialize)]
    struct Ref {
        #[serde(rename = "ref")]
        name: String,
    }
    #[derive(Deserialize)]
    struct PullRead {
        #[serde(default)]
        draft: bool,
        #[serde(default)]
        merged: bool,
        head: Ref,
        base: Ref,
        #[serde(default)]
        additions: u64,
        #[serde(default)]
        deletions: u64,
        #[serde(default)]
        changed_files: u64,
        #[serde(default)]
        requested_reviewers: Vec<User>,
    }
    #[derive(Deserialize)]
    struct Written {
        user: Option<User>,
        body: Option<String>,
        body_html: Option<String>,
        #[serde(default)]
        state: String,
        created_at: Option<String>,
        submitted_at: Option<String>,
        node_id: Option<String>,
        reactions: Option<serde_json::Map<String, serde_json::Value>>,
    }
    let unread = |e: serde_json::Error| format!("GitHub answered what cannot be read: {e}");
    let text = |markdown: Option<String>, html: Option<String>| Text {
        markdown: markdown.unwrap_or_default().trim().replace("\r\n", "\n"),
        html: html.unwrap_or_default(),
    };
    let each = |out: &str| -> Result<Vec<Written>, String> {
        serde_json::Deserializer::from_str(out)
            .into_iter::<Written>()
            .collect::<Result<_, _>>()
            .map_err(unread)
    };
    let issue: Issue = serde_json::from_str(issue).map_err(unread)?;
    let pull: Option<PullRead> = pull.map(serde_json::from_str).transpose().map_err(unread)?;
    let state = match &pull {
        Some(p) if p.merged => "merged".to_string(),
        Some(p) if p.draft && issue.state == "open" => "draft".to_string(),
        _ => issue.state.to_lowercase(),
    };
    let comments = each(comments)?.into_iter().map(|c| {
        let at = c.created_at.clone().unwrap_or_default();
        (
            login(&c.user),
            Did::Commented,
            at,
            text(c.body, c.body_html),
            c.node_id.unwrap_or_default(),
            counted(c.reactions.as_ref()),
        )
    });
    let reviews = each(reviews)?.into_iter().filter_map(|r| {
        let did = match r.state.as_str() {
            "APPROVED" => Did::Approved,
            "CHANGES_REQUESTED" => Did::ChangesRequested,
            "DISMISSED" => Did::Dismissed,
            "PENDING" => return None,
            _ if r.body.as_deref().unwrap_or_default().trim().is_empty() => return None,
            _ => Did::Reviewed,
        };
        let at = r.submitted_at.clone().unwrap_or_default();
        Some((
            login(&r.user),
            did,
            at,
            text(r.body, r.body_html),
            r.node_id.unwrap_or_default(),
            counted(r.reactions.as_ref()),
        ))
    });
    let mut said: Vec<_> = comments.chain(reviews).collect();
    said.sort_by(|a, b| a.2.cmp(&b.2));
    let body = text(issue.body, issue.body_html);
    let body_id = issue.node_id.unwrap_or_default();
    let body_reactions = counted(issue.reactions.as_ref());
    let author = login(&issue.user);
    let texts = std::iter::once(body.clone())
        .chain(said.iter().map(|s| s.3.clone()))
        .collect();
    let thread = Thread {
        item: Item {
            number: issue.number,
            title: issue.title,
            url: issue.html_url,
            author: author.clone(),
            draft: pull.as_ref().is_some_and(|p| p.draft),
            created_at: issue.created_at.clone(),
            updated_at: issue.updated_at,
            labels: issue.labels,
            assignees: issue.assignees.into_iter().map(|a| a.login).collect(),
            milestone: issue.milestone.map(|m| m.title),
            pull: pull.map(|p| Pull {
                head: p.head.name,
                base: p.base.name,
                additions: p.additions,
                deletions: p.deletions,
                changed_files: p.changed_files,
                requested: p.requested_reviewers.into_iter().map(|r| r.login).collect(),
                ..Default::default()
            }),
        },
        state,
        body: Post {
            author,
            did: Did::Opened,
            at: issue.created_at,
            body: body.markdown,
            id: body_id,
            reactions: body_reactions,
            thread: None,
        },
        said: said
            .into_iter()
            .map(|(author, did, at, text, id, reactions)| Post {
                author,
                did,
                at,
                body: text.markdown,
                id,
                reactions,
                thread: None,
            })
            .collect(),
        images: Vec::new(),
        files: Vec::new(),
        checks: Vec::new(),
        pull_id: String::new(),
    };
    Ok((thread, texts))
}

/// The reactions a post has, from the counts GitHub's API rolls them up
/// into, those most given first.
fn counted(rolled: Option<&serde_json::Map<String, serde_json::Value>>) -> Vec<Reaction> {
    let mut out: Vec<Reaction> = REACTIONS
        .iter()
        .filter_map(|content| {
            let count = rolled?.get(*content)?.as_u64()?;
            (count > 0).then(|| Reaction {
                content: (*content).into(),
                count,
            })
        })
        .collect();
    out.sort_by_key(|r| std::cmp::Reverse(r.count));
    out
}

/// What GitHub's GraphQL API tells of a pull request that its REST API does
/// not: its id, the files the login marked viewed, and the threads of
/// comments on lines.
#[derive(Debug, Clone, Default, PartialEq)]
struct Graph {
    id: String,
    viewed: Vec<String>,
    /// Each thread as the posts of its comments, with the text each is
    /// drawn from.
    threads: Vec<(Vec<Post>, Vec<Text>)>,
}

/// The query for a [`Graph`].
const GRAPH_QUERY: &str = "query($owner:String!,$name:String!,$number:Int!){repository(owner:$owner,name:$name){pullRequest(number:$number){id files(first:100){nodes{path viewerViewedState}} reviewThreads(first:100){nodes{id isResolved path line comments(first:50){nodes{id body bodyHTML createdAt author{login} reactionGroups{content reactors{totalCount}}}}}}}}}";

/// A [`Graph`] from what the query answered.
fn graph(out: &str) -> Result<Graph, String> {
    #[derive(Deserialize)]
    struct Answer {
        data: Option<Data>,
    }
    #[derive(Deserialize)]
    struct Data {
        repository: Option<Repository>,
    }
    #[derive(Deserialize)]
    struct Repository {
        #[serde(rename = "pullRequest")]
        pull: Option<PullGraph>,
    }
    #[derive(Deserialize)]
    struct Nodes<T> {
        #[serde(default = "Vec::new")]
        nodes: Vec<T>,
    }
    #[derive(Deserialize)]
    struct PullGraph {
        id: String,
        files: Nodes<File>,
        #[serde(rename = "reviewThreads")]
        threads: Nodes<Threaded>,
    }
    #[derive(Deserialize)]
    struct File {
        path: String,
        #[serde(rename = "viewerViewedState")]
        state: String,
    }
    #[derive(Deserialize)]
    struct Threaded {
        id: String,
        #[serde(rename = "isResolved")]
        resolved: bool,
        path: String,
        line: Option<u64>,
        comments: Nodes<Comment>,
    }
    #[derive(Deserialize)]
    struct Comment {
        id: String,
        #[serde(default)]
        body: String,
        #[serde(rename = "bodyHTML", default)]
        html: String,
        #[serde(rename = "createdAt", default)]
        at: String,
        author: Option<User>,
        #[serde(rename = "reactionGroups", default)]
        reactions: Vec<Group>,
    }
    #[derive(Deserialize)]
    struct Group {
        content: String,
        reactors: Total,
    }
    #[derive(Deserialize)]
    struct Total {
        #[serde(rename = "totalCount")]
        count: u64,
    }
    let answer: Answer = serde_json::from_str(out)
        .map_err(|e| format!("GitHub answered what cannot be read: {e}"))?;
    let pull = answer
        .data
        .and_then(|d| d.repository)
        .and_then(|r| r.pull)
        .ok_or("GitHub does not know the pull request")?;
    let graphql_name = |content: &str| match content {
        "THUMBS_UP" => Some("+1"),
        "THUMBS_DOWN" => Some("-1"),
        "LAUGH" => Some("laugh"),
        "CONFUSED" => Some("confused"),
        "HEART" => Some("heart"),
        "HOORAY" => Some("hooray"),
        "ROCKET" => Some("rocket"),
        "EYES" => Some("eyes"),
        _ => None,
    };
    let threads = pull
        .threads
        .nodes
        .into_iter()
        .map(|t| {
            let on = ReviewThread {
                id: t.id,
                resolved: t.resolved,
                path: t.path,
                line: t.line,
            };
            let mut posts = Vec::new();
            let mut texts = Vec::new();
            for c in t.comments.nodes {
                let mut reactions: Vec<Reaction> = c
                    .reactions
                    .iter()
                    .filter(|g| g.reactors.count > 0)
                    .filter_map(|g| {
                        Some(Reaction {
                            content: graphql_name(&g.content)?.into(),
                            count: g.reactors.count,
                        })
                    })
                    .collect();
                reactions.sort_by_key(|r| std::cmp::Reverse(r.count));
                let markdown = c.body.trim().replace("\r\n", "\n");
                posts.push(Post {
                    author: login(&c.author),
                    did: Did::Commented,
                    at: c.at,
                    body: markdown.clone(),
                    id: c.id,
                    reactions,
                    thread: Some(on.clone()),
                });
                texts.push(Text {
                    markdown,
                    html: c.html,
                });
            }
            (posts, texts)
        })
        .collect();
    Ok(Graph {
        id: pull.id,
        viewed: pull
            .files
            .nodes
            .into_iter()
            .filter(|f| f.state == "VIEWED")
            .map(|f| f.path)
            .collect(),
        threads,
    })
}

/// The commit a pull request GitHub's API answered ends at.
fn head_of(pull: &str) -> Option<String> {
    #[derive(Deserialize)]
    struct Head {
        sha: String,
    }
    #[derive(Deserialize)]
    struct Pull {
        head: Head,
    }
    serde_json::from_str::<Pull>(pull).ok().map(|p| p.head.sha)
}

/// The files a pull request changes, as GitHub's API lists them, one JSON
/// value after another.
fn files(out: &str) -> Vec<FileChange> {
    #[derive(Deserialize)]
    struct Listed {
        filename: String,
        #[serde(default)]
        status: String,
        previous_filename: Option<String>,
        #[serde(default)]
        additions: u64,
        #[serde(default)]
        deletions: u64,
        patch: Option<String>,
    }
    serde_json::Deserializer::from_str(out)
        .into_iter::<Listed>()
        .map_while(Result::ok)
        .map(|f| FileChange {
            path: f.filename,
            status: f.status,
            previous: f.previous_filename,
            additions: f.additions,
            deletions: f.deletions,
            patch: f.patch,
            viewed: false,
        })
        .collect()
}

/// The checks on a pull request's head: its check runs and the statuses
/// set on it, as GitHub's API lists them, each one JSON value after
/// another; those that failed first, then those still running, then the
/// rest, each by name.
fn checks(runs: &str, statuses: &str) -> Vec<Check> {
    #[derive(Deserialize)]
    struct App {
        name: String,
    }
    #[derive(Deserialize)]
    struct Output {
        title: Option<String>,
    }
    #[derive(Deserialize)]
    struct Run {
        name: String,
        #[serde(default)]
        status: String,
        conclusion: Option<String>,
        html_url: Option<String>,
        details_url: Option<String>,
        app: Option<App>,
        output: Option<Output>,
    }
    #[derive(Deserialize)]
    struct Status {
        context: String,
        #[serde(default)]
        state: String,
        target_url: Option<String>,
        description: Option<String>,
    }
    let ran = serde_json::Deserializer::from_str(runs)
        .into_iter::<Run>()
        .map_while(Result::ok)
        .map(|r| Check {
            outcome: match (r.status.as_str(), r.conclusion.as_deref()) {
                ("completed", Some("success" | "neutral")) => Outcome::Passed,
                ("completed", Some("skipped")) => Outcome::Skipped,
                ("completed", _) => Outcome::Failed,
                _ => Outcome::Pending,
            },
            name: r.name,
            by: r.app.map(|a| a.name).unwrap_or_default(),
            link: r.html_url.or(r.details_url).unwrap_or_default(),
            said: r.output.and_then(|o| o.title).unwrap_or_default(),
        });
    let set = serde_json::Deserializer::from_str(statuses)
        .into_iter::<Status>()
        .map_while(Result::ok)
        .map(|s| Check {
            outcome: match s.state.as_str() {
                "success" => Outcome::Passed,
                "pending" => Outcome::Pending,
                _ => Outcome::Failed,
            },
            name: s.context,
            by: String::new(),
            link: s.target_url.unwrap_or_default(),
            said: s.description.unwrap_or_default(),
        });
    let mut all: Vec<Check> = ran.chain(set).collect();
    all.sort_by(|a, b| a.outcome.cmp(&b.outcome).then_with(|| a.name.cmp(&b.name)));
    all
}

/// Each image of `fetched`, in the order the posts show them, as far as
/// `most` bytes hold.
fn carried(fetched: Vec<(String, std::path::PathBuf, &'static str)>, most: usize) -> Vec<Image> {
    let mut bytes = 0;
    let mut images = Vec::new();
    for (address, file, mime) in fetched {
        let Ok(read) = std::fs::read(&file) else {
            continue;
        };
        if bytes + read.len() > most {
            continue;
        }
        bytes += read.len();
        images.push(Image {
            address,
            mime: mime.to_string(),
            data: base64::engine::general_purpose::STANDARD.encode(read),
        });
    }
    images
}

/// The images `text` shows, each by the address its Markdown names and the
/// one to fetch it at. GitHub renders an image of a private repository at
/// an address signed for a few minutes, which its HTML holds: the HTML's
/// images stand for the Markdown's in their order when they are as many,
/// and otherwise only by the address an image says it was written as.
fn images(text: &Text) -> Vec<(String, String)> {
    let written: Vec<String> = written_images(&text.markdown)
        .into_iter()
        .filter(|w| w.starts_with("https://") || w.starts_with("http://"))
        .collect();
    let rendered = rendered_images(&text.html);
    if written.len() == rendered.len() {
        return written
            .into_iter()
            .zip(rendered.into_iter().map(|(src, _)| src))
            .collect();
    }
    written
        .into_iter()
        .map(|w| {
            let fetched = rendered
                .iter()
                .find(|(_, canonical)| canonical.as_deref() == Some(w.as_str()))
                .map_or_else(|| w.clone(), |(src, _)| src.clone());
            (w, fetched)
        })
        .collect()
}

/// The addresses of the images Markdown shows, as `![…](…)` or as an
/// `<img>` tag, in the order written.
fn written_images(markdown: &str) -> Vec<String> {
    let mut found: Vec<(usize, String)> = Vec::new();
    for (at, _) in markdown.match_indices("![") {
        let rest = &markdown[at..];
        let Some(open) = rest.find("](") else {
            continue;
        };
        let target = rest[open + 2..].trim_start();
        let address = match target.strip_prefix('<') {
            Some(inside) => inside.split('>').next(),
            None => target.split([')', ' ', '\n', '\t']).next(),
        };
        if let Some(address) = address.filter(|a| !a.is_empty()) {
            found.push((at, address.to_string()));
        }
    }
    for (at, tag) in tags(markdown, "img") {
        if let Some(src) = attribute(tag, "src") {
            found.push((at, src));
        }
    }
    found.sort_by_key(|(at, _)| *at);
    found.into_iter().map(|(_, a)| a).collect()
}

/// The images HTML shows, but emoji: each by its `src` and the address it
/// says it was written as, when it says.
fn rendered_images(html: &str) -> Vec<(String, Option<String>)> {
    tags(html, "img")
        .into_iter()
        .filter(|(_, tag)| !attribute(tag, "class").is_some_and(|c| c.contains("emoji")))
        .filter_map(|(_, tag)| Some((attribute(tag, "src")?, attribute(tag, "data-canonical-src"))))
        .collect()
}

/// Every `<name …>` tag of `text`, where it starts and what it holds up to
/// its `>`.
fn tags<'a>(text: &'a str, name: &str) -> Vec<(usize, &'a str)> {
    let open = format!("<{name}");
    text.match_indices(&open)
        .filter_map(|(at, _)| {
            let rest = &text[at + open.len()..];
            rest.starts_with(|c: char| c.is_whitespace())
                .then(|| rest.find('>').map(|end| (at, &rest[..end])))
                .flatten()
        })
        .collect()
}

/// The value of the attribute `name` of a tag's insides, quoted or not,
/// its character references read.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let mut rest = tag;
    loop {
        let at = rest.find(name)?;
        let before = rest[..at].chars().next_back();
        let after = rest[at + name.len()..].trim_start();
        rest = &rest[at + name.len()..];
        if !before.is_some_and(char::is_whitespace) {
            continue;
        }
        let Some(value) = after.strip_prefix('=') else {
            continue;
        };
        let value = value.trim_start();
        let raw = match value.chars().next() {
            Some(q @ ('"' | '\'')) => value[1..].split(q).next()?,
            _ => value
                .split(|c: char| c.is_whitespace() || c == '/')
                .next()?,
        };
        return Some(
            raw.replace("&quot;", "\"")
                .replace("&#39;", "'")
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&amp;", "&"),
        );
    }
}

/// The most bytes an image fetched may hold.
const MOST_BYTES: u64 = 32 * 1024 * 1024;

/// Each image of `images` fetched into `dir`, in their order: the address it
/// was written with, the file it was fetched to and the type it was served
/// as. An image that cannot be fetched, or is served as no image, is left
/// out.
async fn fetch(
    dir: &Path,
    images: Vec<(String, String)>,
) -> Vec<(String, std::path::PathBuf, &'static str)> {
    if images.is_empty() || std::fs::create_dir_all(dir).is_err() {
        return Vec::new();
    }
    let mut fetching = tokio::task::JoinSet::new();
    for (n, (written, address)) in images.into_iter().enumerate() {
        let file = dir.join(n.to_string());
        fetching.spawn(async move {
            let most = MOST_BYTES.to_string();
            let file_arg = file.to_string_lossy().into_owned();
            let args = [
                "-sSfL",
                "--max-time",
                "30",
                "--max-filesize",
                &most,
                "-o",
                &file_arg,
                "-w",
                "%{content_type}",
                &address,
            ];
            let kind = run("curl", &args, None).await.ok()?;
            Some((n, written, file, mime(&kind)?))
        });
    }
    let mut fetched = Vec::new();
    while let Some(done) = fetching.join_next().await {
        if let Ok(Some(one)) = done {
            fetched.push(one);
        }
    }
    fetched.sort_by_key(|(n, ..)| *n);
    fetched
        .into_iter()
        .map(|(_, written, file, mime)| (written, file, mime))
        .collect()
}

/// The type of an image as it was served, or none when what was served is
/// no image a page shows.
fn mime(content_type: &str) -> Option<&'static str> {
    let kind = content_type.split(';').next()?.trim().to_ascii_lowercase();
    Some(match kind.as_str() {
        "image/png" => "image/png",
        "image/jpeg" | "image/jpg" => "image/jpeg",
        "image/gif" => "image/gif",
        "image/webp" => "image/webp",
        "image/svg+xml" => "image/svg+xml",
        "image/avif" => "image/avif",
        "image/bmp" => "image/bmp",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PULL_ISSUE: &str = r#"{
        "number": 7, "title": "Fix the parser", "html_url": "https://github.com/o/r/pull/7",
        "user": {"login": "alice"}, "body": "It broke on tabs.\r\n", "body_html": "<p>It broke on tabs.</p>",
        "state": "open", "created_at": "2026-11-01T09:00:00Z", "updated_at": "2026-11-05T00:00:00Z",
        "labels": [{"name": "bug", "color": "d73a4a"}], "assignees": [{"login": "you"}],
        "milestone": {"title": "v1"}
    }"#;
    const PULL: &str = r#"{"draft": false, "merged": false,
        "head": {"ref": "fix-parser", "repo": {"full_name": "o/r"}}, "base": {"ref": "main"},
        "additions": 10, "deletions": 2, "changed_files": 3,
        "requested_reviewers": [{"login": "rev"}]}"#;

    fn text(markdown: &str, html: &str) -> Text {
        Text {
            markdown: markdown.into(),
            html: html.into(),
        }
    }

    #[test]
    fn a_pull_request_reads_with_its_body_and_what_was_said_on_it_oldest_first() {
        let comments =
            r#"{"user": {"login": "bob"}, "body": "Later.", "created_at": "2026-11-03T00:00:00Z"}"#;
        let reviews = concat!(
            r#"{"user": {"login": "carol"}, "body": "", "state": "APPROVED", "submitted_at": "2026-11-04T00:00:00Z"}"#,
            "\n",
            r#"{"user": {"login": "dave"}, "body": "", "state": "COMMENTED", "submitted_at": "2026-11-02T00:00:00Z"}"#,
            "\n",
            r#"{"user": {"login": "erin"}, "body": "Looks fine.", "state": "COMMENTED", "submitted_at": "2026-11-02T00:00:00Z"}"#,
            "\n",
            r#"{"user": {"login": "fay"}, "body": "draft", "state": "PENDING", "submitted_at": null}"#,
        );
        let (thread, texts) = read(PULL_ISSUE, Some(PULL), comments, reviews).unwrap();
        assert_eq!(
            thread.body,
            Post {
                author: "alice".into(),
                did: Did::Opened,
                at: "2026-11-01T09:00:00Z".into(),
                body: "It broke on tabs.".into(),
                ..Default::default()
            }
        );
        let said: Vec<_> = thread
            .said
            .iter()
            .map(|p| (p.author.as_str(), p.did, p.body.as_str()))
            .collect();
        assert_eq!(
            said,
            [
                ("erin", Did::Reviewed, "Looks fine."),
                ("bob", Did::Commented, "Later."),
                ("carol", Did::Approved, ""),
            ]
        );
        assert_eq!(texts.len(), 4);
        assert_eq!(thread.state, "open");
        let item = &thread.item;
        assert_eq!(item.labels[0].name, "bug");
        assert_eq!(item.assignees, ["you"]);
        assert_eq!(item.milestone.as_deref(), Some("v1"));
        let pull = item.pull.as_ref().unwrap();
        assert_eq!(
            (pull.head.as_str(), pull.base.as_str()),
            ("fix-parser", "main")
        );
        assert_eq!(
            (pull.additions, pull.deletions, pull.changed_files),
            (10, 2, 3)
        );
        assert_eq!(pull.requested, ["rev"]);
    }

    #[test]
    fn what_is_said_carries_the_id_a_reaction_to_it_is_asked_by_and_its_counts() {
        let issue = PULL_ISSUE.replace(
            r#""state": "open""#,
            r#""node_id": "I_1", "reactions": {"url": "u", "total_count": 3, "+1": 2, "-1": 0, "eyes": 1}, "state": "open""#,
        );
        let comments = r#"{"node_id": "IC_9", "user": {"login": "bob"}, "body": "Later.", "created_at": "2026-11-03T00:00:00Z", "reactions": {"heart": 4, "+1": 1, "rocket": 0}}"#;
        let (thread, _) = read(&issue, None, comments, "").unwrap();
        assert_eq!(thread.body.id, "I_1");
        let reacted = |post: &Post| -> Vec<(String, u64)> {
            post.reactions
                .iter()
                .map(|r| (r.content.clone(), r.count))
                .collect()
        };
        assert_eq!(
            reacted(&thread.body),
            [("+1".to_string(), 2), ("eyes".to_string(), 1)]
        );
        assert_eq!(thread.said[0].id, "IC_9");
        assert_eq!(
            reacted(&thread.said[0]),
            [("heart".to_string(), 4), ("+1".to_string(), 1)]
        );
    }

    const GRAPH: &str = r#"{"data": {"repository": {"pullRequest": {
        "id": "PR_1",
        "files": {"nodes": [
            {"path": "src/a.rs", "viewerViewedState": "VIEWED"},
            {"path": "src/b.rs", "viewerViewedState": "UNVIEWED"}
        ]},
        "reviewThreads": {"nodes": [{
            "id": "PRRT_1", "isResolved": true, "path": "src/a.rs", "line": 12,
            "comments": {"nodes": [
                {"id": "PRRC_1", "body": "Why?\r\n", "bodyHTML": "<p>Why?</p>",
                 "createdAt": "2026-11-02T12:00:00Z", "author": {"login": "erin"},
                 "reactionGroups": [{"content": "EYES", "reactors": {"totalCount": 2}},
                                    {"content": "HEART", "reactors": {"totalCount": 0}}]},
                {"id": "PRRC_2", "body": "Because.", "bodyHTML": "",
                 "createdAt": "2026-11-04T12:00:00Z", "author": null, "reactionGroups": []}
            ]}
        }]}
    }}}}"#;

    #[test]
    fn comments_on_lines_join_the_conversation_with_their_thread_and_files_their_viewed_marks() {
        let comments =
            r#"{"user": {"login": "bob"}, "body": "Later.", "created_at": "2026-11-03T00:00:00Z"}"#;
        let (mut thread, mut texts) = read(PULL_ISSUE, Some(PULL), comments, "").unwrap();
        thread.files = vec![
            FileChange {
                path: "src/a.rs".into(),
                ..Default::default()
            },
            FileChange {
                path: "src/b.rs".into(),
                ..Default::default()
            },
        ];
        merge(&mut thread, &mut texts, graph(GRAPH).unwrap());
        assert_eq!(thread.pull_id, "PR_1");
        assert_eq!(
            thread.files.iter().map(|f| f.viewed).collect::<Vec<_>>(),
            [true, false]
        );
        let said: Vec<_> = thread
            .said
            .iter()
            .map(|p| (p.author.as_str(), p.body.as_str()))
            .collect();
        assert_eq!(
            said,
            [("erin", "Why?"), ("bob", "Later."), ("ghost", "Because.")]
        );
        let first = &thread.said[0];
        assert_eq!(first.id, "PRRC_1");
        assert_eq!(
            first.thread,
            Some(ReviewThread {
                id: "PRRT_1".into(),
                resolved: true,
                path: "src/a.rs".into(),
                line: Some(12),
            })
        );
        assert_eq!(first.reactions.len(), 1);
        assert_eq!(first.reactions[0].content, "eyes");
        assert_eq!(thread.said[1].thread, None);
        assert_eq!(
            texts.len(),
            4,
            "the body, bob's comment and the two on lines"
        );
    }

    #[test]
    fn a_graphql_answer_without_the_pull_request_is_refused() {
        assert!(graph(r#"{"data": {"repository": {"pullRequest": null}}}"#).is_err());
        assert!(graph("nope").is_err());
    }

    #[test]
    fn an_issue_with_no_body_and_no_author_reads_as_one_by_a_ghost_with_nothing_said() {
        let issue = r#"{
            "number": 3, "title": "Crash", "html_url": "https://github.com/o/r/issues/3",
            "user": null, "body": null, "state": "open", "created_at": "2026-11-01T09:00:00Z"
        }"#;
        let (thread, _) = read(issue, None, "", "").unwrap();
        assert_eq!(thread.body.author, "ghost");
        assert_eq!(thread.body.body, "");
        assert_eq!(thread.item.pull, None);
    }

    #[test]
    fn a_pull_request_stands_as_a_draft_while_open_and_as_merged_once_merged() {
        let stands = |pull: &str| read(PULL_ISSUE, Some(pull), "", "").unwrap().0.state;
        assert_eq!(
            stands(&PULL.replace(r#""draft": false"#, r#""draft": true"#)),
            "draft"
        );
        assert_eq!(
            stands(&PULL.replace(r#""merged": false"#, r#""merged": true"#)),
            "merged"
        );
    }

    #[test]
    fn images_are_carried_in_order_as_far_as_the_bytes_allowed_hold_them() {
        let dir = tempfile::tempdir().unwrap();
        let file = |name: &str, n: usize| {
            let path = dir.path().join(name);
            std::fs::write(&path, vec![1u8; n]).unwrap();
            path
        };
        let fetched = vec![
            ("https://x/a".to_string(), file("a", 6), "image/png"),
            ("https://x/b".to_string(), file("b", 6), "image/png"),
            ("https://x/c".to_string(), file("c", 3), "image/gif"),
        ];
        let carried = carried(fetched, 10);
        let addresses: Vec<_> = carried.iter().map(|i| i.address.as_str()).collect();
        assert_eq!(addresses, ["https://x/a", "https://x/c"]);
        assert_eq!(carried[1].mime, "image/gif");
        assert_eq!(carried[1].data, "AQEB");
    }

    #[test]
    fn a_pull_request_s_files_read_with_their_diffs_and_where_a_renamed_one_was() {
        let out = concat!(
            r#"{"filename": "src/a.rs", "status": "modified", "additions": 2, "deletions": 1, "patch": "@@ -1 +1,2 @@\n-a\n+b\n+c"}"#,
            "\n",
            r#"{"filename": "img.png", "status": "added", "additions": 0, "deletions": 0}"#,
            "\n",
            r#"{"filename": "new.rs", "status": "renamed", "previous_filename": "old.rs", "additions": 0, "deletions": 0}"#,
        );
        let read = files(out);
        assert_eq!(read.len(), 3);
        assert_eq!(read[0].patch.as_deref(), Some("@@ -1 +1,2 @@\n-a\n+b\n+c"));
        assert_eq!((read[0].additions, read[0].deletions), (2, 1));
        assert_eq!(read[1].patch, None);
        assert_eq!(read[2].previous.as_deref(), Some("old.rs"));
    }

    #[test]
    fn checks_list_those_that_failed_then_those_running_then_the_rest() {
        let runs = concat!(
            r#"{"name": "lint", "status": "completed", "conclusion": "failure", "html_url": "https://x/1", "app": {"name": "GitHub Actions"}, "output": {"title": "2 errors"}}"#,
            "\n",
            r#"{"name": "build", "status": "completed", "conclusion": "success", "html_url": "https://x/2"}"#,
            "\n",
            r#"{"name": "docs", "status": "completed", "conclusion": "skipped"}"#,
            "\n",
            r#"{"name": "test", "status": "in_progress", "conclusion": null}"#,
        );
        let statuses = r#"{"context": "ci/old", "state": "error", "target_url": "https://y", "description": "broke"}"#;
        let read = checks(runs, statuses);
        let named: Vec<_> = read.iter().map(|c| (c.name.as_str(), c.outcome)).collect();
        assert_eq!(
            named,
            [
                ("ci/old", Outcome::Failed),
                ("lint", Outcome::Failed),
                ("test", Outcome::Pending),
                ("build", Outcome::Passed),
                ("docs", Outcome::Skipped),
            ]
        );
        assert_eq!(read[1].by, "GitHub Actions");
        assert_eq!(read[1].said, "2 errors");
        assert_eq!(read[1].link, "https://x/1");
        assert_eq!(read[0].said, "broke");
    }

    #[test]
    fn a_pull_request_ends_at_its_head() {
        assert_eq!(
            head_of(r#"{"head": {"sha": "abc"}}"#).as_deref(),
            Some("abc")
        );
        assert_eq!(head_of("{}"), None);
    }

    #[test]
    fn an_image_is_known_by_the_type_it_was_served_as() {
        assert_eq!(mime("image/png"), Some("image/png"));
        assert_eq!(mime("image/jpg; charset=binary"), Some("image/jpeg"));
        assert_eq!(mime("text/html; charset=utf-8"), None);
    }

    #[test]
    fn the_images_markdown_shows_are_found_in_the_order_written_however_written() {
        let markdown = concat!(
            "![a](https://x/a.png \"title\") then <img width=\"10\" src=\"https://x/b.png\">\n",
            "![c](<https://x/c d.png>) and ![d](d.png) <imgx src=\"no\">"
        );
        assert_eq!(
            written_images(markdown),
            [
                "https://x/a.png",
                "https://x/b.png",
                "https://x/c d.png",
                "d.png"
            ]
        );
    }

    #[test]
    fn a_private_image_is_fetched_at_the_signed_address_its_html_holds_in_its_place() {
        let shown = text(
            "![a](https://github.com/user-attachments/assets/a)\n![b](https://example.com/b.png)",
            concat!(
                r#"<p><a href="x"><img src="https://private-user-images.githubusercontent.com/1.png?jwt=t&amp;s=1" alt="a"></a>"#,
                r#"<img src="https://camo.githubusercontent.com/2" data-canonical-src="https://example.com/b.png"></p>"#,
            ),
        );
        assert_eq!(
            images(&shown),
            [
                (
                    "https://github.com/user-attachments/assets/a".to_string(),
                    "https://private-user-images.githubusercontent.com/1.png?jwt=t&s=1".to_string()
                ),
                (
                    "https://example.com/b.png".to_string(),
                    "https://camo.githubusercontent.com/2".to_string()
                ),
            ]
        );
    }

    #[test]
    fn images_the_html_does_not_match_one_for_one_are_fetched_by_what_they_say_or_as_written() {
        let shown = text(
            "`![no](https://x/code.png)` ![b](https://example.com/b.png) ![c](https://x/c.png)",
            concat!(
                r#"<img class="emoji" src="https://x/smile.png">"#,
                r#"<img src="https://camo/2" data-canonical-src="https://example.com/b.png">"#,
            ),
        );
        assert_eq!(
            images(&shown),
            [
                (
                    "https://x/code.png".to_string(),
                    "https://x/code.png".to_string()
                ),
                (
                    "https://example.com/b.png".to_string(),
                    "https://camo/2".to_string()
                ),
                ("https://x/c.png".to_string(), "https://x/c.png".to_string()),
            ]
        );
    }
}
