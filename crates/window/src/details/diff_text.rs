//! Only SGR styling reaches the page; diff text remains escaped Dioxus text.

pub(super) fn spans(line: &str) -> Vec<(String, String)> {
    let mut output = Vec::new();
    let mut text = String::new();
    let (mut color, mut bold, mut italic, mut underline) = (String::new(), false, false, false);
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            if !c.is_control() || c == '\t' {
                text.push(c);
            }
            continue;
        }
        let mut style = color.clone();
        if bold {
            style.push_str("font-weight:bold;");
        }
        if italic {
            style.push_str("font-style:italic;");
        }
        if underline {
            style.push_str("text-decoration:underline;");
        }
        if !text.is_empty() {
            output.push((style, std::mem::take(&mut text)));
        }
        match chars.next() {
            Some('[') => {
                let mut parameters = String::new();
                let mut sgr = false;
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        sgr = c == 'm';
                        break;
                    }
                    parameters.push(c);
                }
                if !sgr {
                    continue;
                }
                let codes: Vec<u16> = parameters
                    .split(';')
                    .map(|s| s.parse().unwrap_or(0))
                    .collect();
                let mut codes = codes.iter();
                while let Some(&code) = codes.next() {
                    match code {
                        0 => {
                            color.clear();
                            bold = false;
                            italic = false;
                            underline = false;
                        }
                        1 => bold = true,
                        3 => italic = true,
                        4 => underline = true,
                        22 => bold = false,
                        23 => italic = false,
                        24 => underline = false,
                        30..=37 => color = format!("color:var(--ansi-{});", code - 30),
                        90..=97 => color = format!("color:var(--ansi-{});", code - 90 + 8),
                        39 => color.clear(),
                        38 => match codes.next() {
                            Some(2) => {
                                if let (Some(r), Some(g), Some(b)) =
                                    (codes.next(), codes.next(), codes.next())
                                {
                                    color = format!(
                                        "color:rgb({},{},{});",
                                        r.min(&255),
                                        g.min(&255),
                                        b.min(&255)
                                    );
                                }
                            }
                            Some(5) => {
                                if let Some(&index) = codes.next() {
                                    color = if index < 16 {
                                        format!("color:var(--ansi-{index});")
                                    } else if index < 232 {
                                        let n = index - 16;
                                        let level = |v| if v == 0 { 0 } else { 55 + 40 * v };
                                        format!(
                                            "color:rgb({},{},{});",
                                            level(n / 36),
                                            level(n / 6 % 6),
                                            level(n % 6)
                                        )
                                    } else {
                                        let n = 8 + 10 * (index.min(255) - 232);
                                        format!("color:rgb({n},{n},{n});")
                                    };
                                }
                            }
                            _ => {}
                        },
                        _ => {}
                    }
                }
            }
            Some(']') => {
                while let Some(c) = chars.next() {
                    if c == '\x07' || (c == '\x1b' && chars.next() == Some('\\')) {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    let mut style = color;
    if bold {
        style.push_str("font-weight:bold;");
    }
    if italic {
        style.push_str("font-style:italic;");
    }
    if underline {
        style.push_str("text-decoration:underline;");
    }
    if !text.is_empty() {
        output.push((style, text));
    }
    output
}

#[cfg(test)]
mod tests {
    #[test]
    fn structural_colors_are_styled_without_interpreting_html_or_terminal_controls() {
        let spans = super::spans(
            "\x1b[31;1m<removed>\x1b[0m plain\x1b[38;2;12;34;56m new\x1b[0m\x1b]0;title\x07",
        );
        assert_eq!(spans[0].1, "<removed>");
        assert!(spans[0].0.contains("var(--ansi-1)"));
        assert!(spans[0].0.contains("font-weight:bold"));
        assert_eq!(spans[1], (String::new(), " plain".into()));
        assert!(spans[2].0.contains("rgb(12,34,56)"));
        assert_eq!(
            spans.iter().map(|s| s.1.as_str()).collect::<String>(),
            "<removed> plain new"
        );
    }
}
