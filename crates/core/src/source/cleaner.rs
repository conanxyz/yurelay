//! Deliberately conservative, deterministic and reversible text cleaning.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanMode {
    Pdf,
    Comments,
}
#[derive(Debug, Clone, Serialize)]
pub struct CleanResult {
    pub text: String,
    pub changes: usize,
    pub uncertain: usize,
    pub rules: Vec<String>,
}
pub fn clean(text: &str, mode: CleanMode) -> CleanResult {
    let mut result = CleanResult {
        text: text.into(),
        changes: 0,
        uncertain: 0,
        rules: vec![],
    };
    match mode {
        CleanMode::Pdf => {
            let mut lines: Vec<String> = Vec::new();
            let mut fenced = false;
            for line in text.split('\n') {
                let trimmed = line.trim();
                if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                    fenced = !fenced;
                    lines.push(line.into());
                    continue;
                }
                if fenced {
                    lines.push(line.into());
                    continue;
                }
                let original_line = line;
                let line = line.replace('\u{00ad}', "");
                result.changes += usize::from(line != original_line);
                if let Some(previous) = lines.last_mut() {
                    let prev = previous.trim_end();
                    let prose = |s: &str| {
                        s.len() >= 45
                            && !s.contains(['|', '=', '$', '{', '}', ';'])
                            && !s.trim_start().starts_with(['#', '>', '-', '*', '+'])
                            && !s.starts_with(char::is_whitespace)
                            && !s.chars().next().is_some_and(|c| c.is_ascii_digit())
                    };
                    if prev.ends_with('-') && line.starts_with(char::is_alphabetic) {
                        result.uncertain += 1;
                    }
                    if prose(prev)
                        && prose(&line)
                        && !prev.ends_with(['.', '!', '?', ':', '-', '。', '！', '？'])
                        && line.starts_with(|c: char| c.is_ascii_lowercase())
                    {
                        previous.truncate(prev.len());
                        previous.push(' ');
                        previous.push_str(line.trim());
                        result.changes += 1;
                        continue;
                    }
                }
                lines.push(line);
            }
            result.text = lines.join("\n");
            result.rules.push("conservative_pdf".into());
        }
        CleanMode::Comments => {
            let lines: Vec<_> = text.lines().collect();
            let nonempty: Vec<_> = lines
                .iter()
                .map(|v| v.trim())
                .filter(|v| !v.is_empty())
                .collect();
            if nonempty.is_empty() {
                return result;
            }
            let prefix = ["///", "//", "--", "#"].into_iter().find(|p| {
                nonempty.iter().all(|l| l.starts_with(p))
                    && !nonempty.iter().any(|l| l.starts_with("#!"))
            });
            if let Some(prefix) = prefix {
                result.text = lines
                    .iter()
                    .map(|line| {
                        line.trim_start()
                            .strip_prefix(prefix)
                            .unwrap_or(line)
                            .strip_prefix(' ')
                            .unwrap_or_else(|| {
                                line.trim_start().strip_prefix(prefix).unwrap_or(line)
                            })
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                result.changes = nonempty.len();
            } else if text.trim().starts_with("/*") && text.trim().ends_with("*/") {
                let inner = text
                    .trim()
                    .strip_prefix("/*")
                    .unwrap()
                    .strip_suffix("*/")
                    .unwrap();
                if !inner.contains("/*") && !inner.contains("*/") {
                    result.text = inner
                        .lines()
                        .map(|line| {
                            line.trim_start()
                                .strip_prefix('*')
                                .unwrap_or(line)
                                .trim_start()
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                        .trim_matches('\n')
                        .into();
                    result.changes = nonempty.len();
                }
            }
            if result.changes > 0 {
                result.rules.push("explicit_comment_shell".into());
            }
        }
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pdf_preserves_structure_and_unknown_hyphens() {
        for text in [
            "# Heading\n- item\n- other",
            "```rust\nlet value = 2;\n```",
            "| A | B |\n| 1 | 2 |",
            "real-\ntime",
            "地址\n下一行",
            "price = -2\nvalue",
            "短诗\n下一句",
        ] {
            assert_eq!(clean(text, CleanMode::Pdf).text, text);
        }
        let text = "This paragraph contains enough ordinary prose to be a wrapped\ncontinuation with enough ordinary words to be confidently joined.";
        let result = clean(text, CleanMode::Pdf);
        assert!(!result.text.contains('\n'));
        assert_eq!(clean(&result.text, CleanMode::Pdf).text, result.text);
    }
    #[test]
    fn comments_only_strip_consistent_explicit_shell() {
        assert_eq!(
            clean("// hello\n// https://example.test", CleanMode::Comments).text,
            "hello\nhttps://example.test"
        );
        for text in [
            "#!/bin/sh\n# text",
            "let url = \"https://example.test\"; // note",
            "SELECT a -- note\nFROM test",
            "* item\nplain",
        ] {
            assert_eq!(clean(text, CleanMode::Comments).text, text);
        }
    }
}
