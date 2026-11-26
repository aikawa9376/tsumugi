//! A conversation read in order: the user's words in bubbles, the replies as
//! prose, and the work between them folded into one line a run.

mod tools;

use tsumugi_core::icons::Icon;
use tsumugi_model::conversation::{Message, Role, Tool};

use self::tools::{Change, Kind, Phase, Question, Step};
use super::{escape, render_post};

pub fn conversation_update_script(id: &str, message: &str) -> String {
    let script = include_str!("../../assets/js/conversation-update.js");
    format!("{{const id = {id:?}; const message = {message}; {script}}}")
}

/// What the conversation is drawn as, one after another.
enum Part<'a> {
    Said(usize, &'a Message),
    Plan(usize, &'a Tool, Vec<(Step, String)>),
    Asked(usize, &'a Tool, Vec<Question>),
    /// The thinking and the calls between two things said.
    Work(Vec<(usize, &'a Message)>),
}

fn parts(messages: &[Message]) -> Vec<Part<'_>> {
    let mut out: Vec<Part> = Vec::new();
    for (i, m) in messages.iter().enumerate() {
        if let Some((tool, steps)) = m.tool.as_ref().and_then(|t| Some((t, tools::plan(t)?))) {
            out.push(Part::Plan(i, tool, steps));
        } else if let Some((tool, asked)) = m
            .tool
            .as_ref()
            .and_then(|t| Some((t, tools::questions(t)?)))
        {
            out.push(Part::Asked(i, tool, asked));
        } else if m.tool.is_some() || m.thinking {
            match out.last_mut() {
                Some(Part::Work(work)) => work.push((i, m)),
                _ => out.push(Part::Work(vec![(i, m)])),
            }
        } else if !m.text.trim().is_empty() {
            out.push(Part::Said(i, m));
        }
    }
    out
}

/// The conversation as the page draws it. `working`: the session is still
/// at its turn, whose last calls may still run and whose time is not yet
/// known.
pub fn conversation_fragment(messages: &[Message], working: bool) -> String {
    if messages.is_empty() {
        return "<p class=\"conversation-empty\">No conversation yet.</p>".into();
    }
    let parts = parts(messages);
    let last = parts.len().saturating_sub(1);
    let mut out = String::new();
    // When the turn under way began and when it last said or did anything.
    let mut turn: Option<(Option<u64>, Option<u64>)> = None;
    for (n, part) in parts.iter().enumerate() {
        let live = n == last && working;
        if let Part::Said(_, m) = part
            && m.role == Role::User
        {
            out.push_str(&turn_end(turn.take()));
            turn = Some((m.at_ms, None));
        } else if let Some((_, end)) = &mut turn {
            *end = part_at(messages, part).or(*end);
        }
        match part {
            Part::Said(i, m) => out.push_str(&said(m, *i)),
            Part::Plan(i, tool, steps) => out.push_str(&plan(tool, steps, *i)),
            Part::Asked(i, tool, asked) => out.push_str(&question(tool, asked, *i, live)),
            Part::Work(work) => out.push_str(&run(work, live)),
        }
    }
    if !working {
        out.push_str(&turn_end(turn));
    }
    out
}

/// When the latest of a part's messages was recorded.
fn part_at(messages: &[Message], part: &Part) -> Option<u64> {
    match part {
        Part::Said(i, _) | Part::Plan(i, ..) | Part::Asked(i, ..) => messages[*i].at_ms,
        Part::Work(work) => work.iter().filter_map(|(_, m)| m.at_ms).max(),
    }
}

/// The line that ends a turn with how long it worked, when both its ends
/// were recorded.
fn turn_end(turn: Option<(Option<u64>, Option<u64>)>) -> String {
    match turn {
        Some((Some(start), Some(end))) if end > start => format!(
            "<div class=\"conversation-turn-end\">{}<span>Worked for {}</span></div>",
            icon(Icon::TimelineEvent),
            tools::took(end - start)
        ),
        _ => String::new(),
    }
}

/// `icon` as the page draws it, at the size of the text beside it.
fn icon(icon: Icon) -> String {
    let svg = icon.svg();
    let svg = svg.find("<svg").map_or(svg, |at| &svg[at..]);
    format!("<span class=\"conversation-icon\" aria-hidden=\"true\">{svg}</span>")
}

/// A message's foot: when it was said, filled in by the page in the
/// reader's time, and the button that copies it as it was written.
fn foot(m: &Message) -> String {
    let at = m
        .at_ms
        .map(|ms| format!("<time data-at=\"{ms}\"></time>"))
        .unwrap_or_default();
    format!(
        "<footer>{at}<button type=\"button\" data-conversation-copy=\"{}\" title=\"Copy\">{}Copy</button></footer>",
        escape(&m.text),
        icon(Icon::Copy),
    )
}

fn said(m: &Message, index: usize) -> String {
    match m.role {
        Role::User => format!(
            "<section class=\"conversation-message user\" data-message=\"{index}\">\
            <div class=\"conversation-bubble markdown-body\">{}</div>{}</section>",
            render_post(&m.text),
            foot(m)
        ),
        Role::Assistant => format!(
            "<section class=\"conversation-message assistant\" data-message=\"{index}\">\
            <div class=\"conversation-text markdown-body\">{}</div>{}</section>",
            render_post(&m.text),
            foot(m)
        ),
    }
}

/// The work between two things said: one line for a single thing done,
/// else one folded line telling all of it, which opens to a line each.
fn run(work: &[(usize, &Message)], live: bool) -> String {
    let lines = lines(work, live);
    let calls: Vec<&Tool> = work.iter().filter_map(|(_, m)| m.tool.as_ref()).collect();
    let thought = work.iter().any(|(_, m)| m.thinking);
    if lines.len() == 1 {
        return lines.into_iter().next().unwrap_or_default();
    }
    let phases: Vec<Phase> = calls.iter().map(|t| Phase::of(t, live)).collect();
    let running = calls
        .iter()
        .rev()
        .find(|t| Phase::of(t, live) == Phase::Running);
    let (state, heading) = match running {
        Some(tool) => {
            let line = tools::line(tool, Phase::Running);
            (
                "running",
                format!("{} {}", line.verb, line.target).trim().to_string(),
            )
        }
        None => {
            let mut heading = tools::sentence(&calls);
            if thought {
                heading = if heading.is_empty() {
                    "Thought".into()
                } else {
                    let mut chars = heading.chars();
                    let first = chars.next().map(|c| c.to_lowercase().to_string());
                    format!("Thought, {}{}", first.unwrap_or_default(), chars.as_str())
                };
            }
            let state = if phases.contains(&Phase::Failed) {
                "failed"
            } else {
                "done"
            };
            (state, heading)
        }
    };
    let (added, removed) = calls
        .iter()
        .filter_map(|t| tools::edit(t))
        .map(|c| tools::counts(&c))
        .fold((0, 0), |(a, r), (x, y)| (a + x, r + y));
    let counts = counts(added, removed);
    let failed = phases.iter().filter(|p| **p == Phase::Failed).count();
    let failed = if failed > 0 && running.is_none() {
        format!("<span class=\"conversation-run-failed\">{failed} failed</span>")
    } else {
        String::new()
    };
    let mark = match state {
        "running" => icon(Icon::Loader),
        "failed" => icon(Icon::AlertTriangle),
        _ => icon(Icon::Check),
    };
    format!(
        "<details class=\"conversation-run {state}\" data-progress=\"run-{}\">\
        <summary>{mark}<span class=\"conversation-run-heading\">{}</span>{counts}{failed}{}</summary>\
        <div class=\"conversation-run-lines\">{}</div></details>",
        work[0].0,
        escape(&heading),
        icon(Icon::ChevronRight),
        lines.concat(),
    )
}

/// A line for each thing the work did: thinking that follows on, as one.
fn lines(work: &[(usize, &Message)], live: bool) -> Vec<String> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < work.len() {
        let (index, m) = work[at];
        if let Some(tool) = &m.tool {
            out.push(call(tool, index, live));
            at += 1;
            continue;
        }
        let end = work[at..]
            .iter()
            .position(|(_, m)| !m.thinking || m.tool.is_some())
            .map_or(work.len(), |n| at + n);
        let text: Vec<&str> = work[at..end].iter().map(|(_, m)| m.text.as_str()).collect();
        out.push(format!(
            "<details class=\"conversation-thought\" data-progress=\"thought-{index}\">\
            <summary>{}<span class=\"conversation-tool-verb\">Thought</span>{}</summary>\
            <div class=\"conversation-thought-text markdown-body\">{}</div></details>",
            icon(Icon::Message),
            icon(Icon::ChevronRight),
            render_post(&text.join("\n\n")),
        ));
        at = end;
    }
    out
}

/// A tool call as one line telling what it did, which opens to what it was
/// given, the edit it made, and what it returned.
fn call(tool: &Tool, index: usize, live: bool) -> String {
    let phase = Phase::of(tool, live);
    let line = tools::line(tool, phase);
    let kind = Kind::of(&tool.name);
    let edit = tools::edit(tool);
    let counts = edit
        .as_deref()
        .map(tools::counts)
        .map(|(a, r)| counts(a, r))
        .unwrap_or_default();
    let mark = match phase {
        Phase::Running => icon(Icon::Loader),
        Phase::Failed => icon(Icon::X),
        Phase::Done => icon(kind.icon()),
    };
    let given = match (&edit, tools::command(tool)) {
        (Some(changes), _) => diff(changes),
        // The line shows a command it holds whole; one cut short opens to it.
        (None, Some(command)) if command.trim() == line.target => String::new(),
        (None, Some(command)) => format!(
            "<pre class=\"conversation-command\"><code>{}</code></pre>",
            escape(&command)
        ),
        (None, None) if tools::said_by_its_line(tool) => String::new(),
        (None, None) => part("Input", &tool.input),
    };
    let output = part(
        if phase == Phase::Failed {
            "Error"
        } else {
            "Output"
        },
        tool.output.as_deref().unwrap_or_default(),
    );
    format!(
        "<details class=\"conversation-tool {}\" data-progress=\"tool-{index}\">\
        <summary title=\"{}\">{mark}<span class=\"conversation-tool-verb\">{}</span>\
        <span class=\"conversation-tool-target\">{}</span>{counts}{}</summary>\
        <div class=\"conversation-tool-body\">{given}{output}</div></details>",
        phase.class(),
        escape(&line.title),
        escape(&line.verb),
        escape(&line.target),
        icon(Icon::ChevronRight),
    )
}

/// The lines an edit added and removed, when it changed any.
fn counts(added: usize, removed: usize) -> String {
    if added + removed == 0 {
        return String::new();
    }
    format!(
        "<span class=\"conversation-counts\"><span class=\"added\">+{added}</span><span class=\"removed\">−{removed}</span></span>"
    )
}

fn part(label: &str, text: &str) -> String {
    if text.trim().is_empty() {
        return String::new();
    }
    format!(
        "<div class=\"conversation-tool-part\"><b>{label}</b><pre>{}</pre></div>",
        escape(text)
    )
}

fn diff(changes: &[Change]) -> String {
    let rows: String = changes
        .iter()
        .map(|c| {
            let (class, mark, text) = match c {
                Change::Same(t) => ("same", " ", t.as_str()),
                Change::Removed(t) => ("removed", "−", t.as_str()),
                Change::Added(t) => ("added", "+", t.as_str()),
                Change::Gap(t) => ("gap", "", t.as_str()),
            };
            format!(
                "<div class=\"{class}\"><span class=\"mark\">{mark}</span><span>{}</span></div>",
                escape(text)
            )
        })
        .collect();
    format!("<div class=\"conversation-diff\">{rows}</div>")
}

/// Questions the assistant asked, as a card of numbered options: while the
/// session waits on them, a number pressed picks one there; once answered,
/// the card keeps the answers.
fn question(tool: &Tool, asked: &[Question], index: usize, live: bool) -> String {
    let answers = tool
        .output
        .as_deref()
        .map(tools::answers)
        .unwrap_or_default();
    let (state, foot) = match (&tool.output, tool.failed) {
        (_, true) => ("dismissed", "Not answered".to_string()),
        (Some(_), false) => ("answered", "Answered".to_string()),
        (None, false) if live => {
            let most = asked.first().map_or(0, |q| q.options.len()).min(9);
            (
                "waiting",
                if most > 1 {
                    format!("Press 1–{most} to answer, or answer in the console")
                } else {
                    "Answer in the console".to_string()
                },
            )
        }
        (None, false) => ("dismissed", "Not answered".to_string()),
    };
    let cards: String = asked
        .iter()
        .map(|q| {
            let answer = answers
                .iter()
                .find(|(text, _)| *text == q.text)
                .map(|(_, a)| a.as_str());
            let picked = |label: &str| {
                answer.is_some_and(|a| a == label || a.split(", ").any(|part| part == label))
            };
            let options: String = q
                .options
                .iter()
                .enumerate()
                .map(|(n, (label, about))| {
                    let about = if about.is_empty() {
                        String::new()
                    } else {
                        format!("<span class=\"conversation-option-about\">{}</span>", escape(about))
                    };
                    format!(
                        "<li class=\"{}\"><span class=\"conversation-option-number\">{}</span>\
                        <span class=\"conversation-option-words\"><span class=\"conversation-option-label\">{}</span>{about}</span>{}</li>",
                        if picked(label) { "picked" } else { "" },
                        n + 1,
                        escape(label),
                        if picked(label) { icon(Icon::Check) } else { String::new() },
                    )
                })
                .collect();
            // An answer that is none of the options was written by the user.
            let own = answer
                .filter(|a| !q.options.iter().any(|(l, _)| picked(l)) && !a.is_empty())
                .map(|a| format!("<p class=\"conversation-question-own\">{}</p>", escape(a)))
                .unwrap_or_default();
            let header = if q.header.is_empty() {
                String::new()
            } else {
                format!("<span class=\"conversation-question-header\">{}</span>", escape(&q.header))
            };
            format!(
                "<div class=\"conversation-question-one\">{header}<p class=\"conversation-question-text\">{}</p><ol>{options}</ol>{own}</div>",
                escape(&q.text)
            )
        })
        .collect();
    format!(
        "<section class=\"conversation-question {state}\" data-message=\"{index}\">\
        <header>{}<span>{}</span></header>{cards}<footer>{}</footer></section>",
        icon(Icon::MessageQuestion),
        if asked.len() == 1 {
            "Question"
        } else {
            "Questions"
        },
        escape(&foot),
    )
}

/// A plan the assistant wrote, as the checklist it is.
fn plan(tool: &Tool, steps: &[(Step, String)], index: usize) -> String {
    let done = steps.iter().filter(|(s, _)| *s == Step::Done).count();
    let items: String = steps
        .iter()
        .map(|(step, text)| {
            let (class, mark) = match step {
                Step::Done => ("done", icon(Icon::Check)),
                Step::Doing => ("doing", icon(Icon::ArrowRight)),
                Step::Pending => ("pending", icon(Icon::Circle)),
            };
            format!(
                "<li class=\"{class}\">{mark}<span>{}</span></li>",
                escape(text)
            )
        })
        .collect();
    format!(
        "<section class=\"conversation-plan{}\" data-message=\"{index}\">\
        <header>{}<span>Plan</span><span class=\"conversation-plan-count\">{done} of {}</span></header>\
        <ul>{items}</ul></section>",
        if tool.failed { " failed" } else { "" },
        icon(Icon::ListTree),
        steps.len(),
    )
}

#[cfg(test)]
mod tests;
