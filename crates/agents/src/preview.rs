//! What a conversation's file says, read for a person to look at before
//! choosing it. Whatever the file holds, only its two ends are read, so a
//! transcript of any size, or one that is not a transcript, costs no more
//! than a small one.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use serde_json::Value;
use tsumugi_model::{Preview, Speaker, Turn};

/// How far into a file the first prompt is looked for, in bytes.
const HEAD: u64 = 512 * 1024;
/// How much of a file's end the latest turns are looked for in, in bytes.
const TAIL: u64 = 256 * 1024;
/// The most characters the first prompt keeps.
const FIRST_CHARS: usize = 20_000;
/// The most characters a latest turn keeps.
const TURN_CHARS: usize = 1_500;
/// The most latest turns kept.
const TURNS: usize = 6;

/// The preview of the file at `path`, whose lines are JSON records that
/// `turn` tells the turns from; none when the file cannot be read.
pub(crate) fn read(path: &Path, turn: impl Fn(&Value) -> Option<Turn>) -> Option<Preview> {
    let mut file = File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let mut first = None;
    let mut turns = Vec::new();
    for line in BufReader::new((&file).take(HEAD))
        .split(b'\n')
        .map_while(Result::ok)
    {
        let Some(said) = record(&line, &turn) else {
            continue;
        };
        if first.is_none() && said.speaker == Speaker::User {
            first = Some(cut(&said.text, FIRST_CHARS));
        }
        turns.push(said);
    }
    if len > HEAD {
        turns.clear();
        let from = len.saturating_sub(TAIL);
        file.seek(SeekFrom::Start(from)).ok()?;
        let mut tail = Vec::new();
        file.take(TAIL).read_to_end(&mut tail).ok()?;
        // What is read from the middle of a file starts in the middle of a
        // line.
        turns.extend(
            tail.split(|b| *b == b'\n')
                .skip(1)
                .filter_map(|line| record(line, &turn)),
        );
    }
    let skip = turns.len().saturating_sub(TURNS);
    Some(Preview {
        first: first.unwrap_or_default(),
        recent: turns
            .into_iter()
            .skip(skip)
            .map(|t| Turn {
                text: cut(&t.text, TURN_CHARS),
                ..t
            })
            .collect(),
    })
}

fn record(line: &[u8], turn: &impl Fn(&Value) -> Option<Turn>) -> Option<Turn> {
    turn(&serde_json::from_slice::<Value>(line).ok()?)
}

/// `text` cut to `chars` characters, marked when it was.
fn cut(text: &str, chars: usize) -> String {
    let text = text.trim();
    match text.char_indices().nth(chars) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_string(),
    }
}

/// The text of the parts of a message's `content` that are `kind`, or all of
/// it when it is a plain string.
pub(crate) fn text_of(content: &Value, kind: &str) -> Option<String> {
    let text = match content {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter(|p| p["type"] == kind)
            .filter_map(|p| p["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => return None,
    };
    (!text.trim().is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn turn(v: &Value) -> Option<Turn> {
        let speaker = match v["role"].as_str()? {
            "user" => Speaker::User,
            "agent" => Speaker::Agent,
            _ => return None,
        };
        Some(Turn {
            speaker,
            text: v["text"].as_str()?.into(),
        })
    }

    fn line(role: &str, text: &str) -> String {
        format!("{}\n", json!({"role": role, "text": text}))
    }

    fn file(content: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("c.jsonl");
        std::fs::write(&path, content).unwrap();
        (dir, path)
    }

    #[test]
    fn the_first_prompt_is_kept_whole_and_the_latest_turns_after_it() {
        let long = "word ".repeat(2_000);
        let (_dir, path) = file(&format!(
            "{}{}{}",
            line("user", &long),
            line("agent", "done"),
            line("user", "and then")
        ));
        let got = read(&path, turn).unwrap();
        assert_eq!(got.first, long.trim());
        let said: Vec<&str> = got.recent.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(said.len(), 3);
        assert_eq!(&said[1..], ["done", "and then"]);
        // A turn cut short says so.
        assert!(said[0].ends_with('…') && said[0].chars().count() <= TURN_CHARS + 1);
    }

    #[test]
    fn lines_that_are_not_turns_are_passed_over() {
        let (_dir, path) = file(&format!(
            "not json\n{{\"role\":\"tool\",\"text\":\"x\"}}\n\n{}\u{0}\u{1}\n{}",
            line("user", "hello").trim_end(),
            line("agent", "hi")
        ));
        let got = read(&path, turn).unwrap();
        assert_eq!(got.first, "");
        assert_eq!(got.recent.len(), 1);
    }

    #[test]
    fn only_the_newest_turns_are_kept() {
        let text: String = (0..20).map(|i| line("user", &format!("m{i}"))).collect();
        let (_dir, path) = file(&text);
        let got = read(&path, turn).unwrap();
        let said: Vec<&str> = got.recent.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(said, ["m14", "m15", "m16", "m17", "m18", "m19"]);
        assert_eq!(got.first, "m0");
    }

    #[test]
    fn a_file_of_many_megabytes_is_read_at_its_two_ends_only() {
        let mut text = line("user", "the beginning");
        // One line far longer than anything read, and many short ones.
        text.push_str(&format!("{}\n", "x".repeat(8 * 1024 * 1024)));
        for _ in 0..20_000 {
            text.push_str(&line("agent", &"filler ".repeat(40)));
        }
        text.push_str(&line("user", "the end"));
        let (_dir, path) = file(&text);
        let got = read(&path, turn).unwrap();
        assert_eq!(got.first, "the beginning");
        assert_eq!(got.recent.last().map(|t| t.text.as_str()), Some("the end"));
        assert!(got.recent.len() <= TURNS);
    }

    #[test]
    fn a_file_that_holds_no_records_has_an_empty_preview() {
        let (_dir, path) = file(&"\u{0}garbage".repeat(10));
        assert_eq!(read(&path, turn), Some(Preview::default()));
        assert_eq!(read(Path::new("/no/such/file"), turn), None);
    }
}
