//! Local source boundaries. Never infer translation alignment from sentence counts.
use std::ops::Range;
fn trimmed_range(text: &str, range: Range<usize>) -> Option<Range<usize>> {
    let part = &text[range.clone()];
    let left = part.len() - part.trim_start().len();
    let end = part.trim_end().len();
    (left < end).then_some(range.start + left..range.start + end)
}
fn closing(c: char) -> bool {
    matches!(
        c,
        '"' | '\'' | '”' | '’' | '»' | ')' | ']' | '}' | '」' | '』'
    )
}
fn abbreviation(text: &str, dot: usize) -> bool {
    let token = text[..dot]
        .split_whitespace()
        .next_back()
        .unwrap_or("")
        .trim_matches(|c: char| !c.is_alphabetic() && c != '.');
    matches!(
        token.to_ascii_lowercase().as_str(),
        "mr" | "mrs"
            | "ms"
            | "dr"
            | "prof"
            | "sr"
            | "jr"
            | "st"
            | "vs"
            | "etc"
            | "e.g"
            | "i.e"
            | "fig"
            | "no"
    ) || (token.chars().count() == 1 && token.chars().all(|c| c.is_ascii_uppercase()))
        || (token.contains('.')
            && token
                .split('.')
                .all(|part| part.len() == 1 && part.chars().all(|c| c.is_ascii_alphabetic())))
}
pub fn sentences(text: &str) -> Vec<Range<usize>> {
    let mut ranges = vec![];
    let mut start = 0;
    let mut iter = text.char_indices().peekable();
    while let Some((at, c)) = iter.next() {
        let newline = matches!(c, '\n' | '\r');
        if !newline && !matches!(c, '.' | '!' | '?' | '。' | '！' | '？') {
            continue;
        }
        let next = text[at + c.len_utf8()..].chars().next();
        if c == '.'
            && (abbreviation(text, at) || next.is_some_and(|n| n.is_alphanumeric() || n == '.'))
        {
            continue;
        }
        let mut end = at + c.len_utf8();
        if !newline {
            while let Some(&(pos, n)) = iter.peek() {
                if closing(n) || matches!(n, '!' | '?' | '。' | '！' | '？') {
                    end = pos + n.len_utf8();
                    iter.next();
                } else {
                    break;
                }
            }
            if c.is_ascii()
                && text[end..]
                    .chars()
                    .next()
                    .is_some_and(|n| !n.is_whitespace())
            {
                continue;
            }
        }
        if let Some(range) = trimmed_range(text, start..end) {
            ranges.push(range)
        }
        start = end;
    }
    if let Some(range) = trimmed_range(text, start..text.len()) {
        ranges.push(range)
    }
    ranges
}
fn word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}
fn boundary_match(sentence: &str, at: usize, surface: &str) -> bool {
    let before = sentence[..at].chars().next_back();
    let after = sentence[at + surface.len()..].chars().next();
    !(surface.starts_with(|c: char| c.is_ascii_alphanumeric()) && before.is_some_and(word_char)
        || surface.ends_with(|c: char| c.is_ascii_alphanumeric()) && after.is_some_and(word_char))
}
pub fn normalize_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
struct Normalized {
    text: String,
    offsets: Vec<(usize, usize)>,
}
fn normalize_mapped(text: &str) -> Normalized {
    let mut result = Normalized {
        text: String::new(),
        offsets: vec![],
    };
    let mut space = false;
    for (at, c) in text.char_indices() {
        let end = at + c.len_utf8();
        if c.is_whitespace() {
            if space {
                if let Some(last) = result.offsets.last_mut() {
                    last.1 = end;
                }
            } else {
                result.text.push(' ');
                result.offsets.push((at, end));
            }
            space = true;
        } else {
            space = false;
            for lower in c.to_lowercase() {
                result.text.push(lower);
                result
                    .offsets
                    .extend(std::iter::repeat_n((at, end), lower.len_utf8()));
            }
        }
    }
    result
}
/// Match normalized text but return original slices, including OCR line breaks.
/// A phrase can span soft lines; distinct sentence occurrences remain ambiguous.
pub fn bind_original(
    source: &str,
    surface: &str,
    translation: &str,
) -> Option<(String, String, String)> {
    let needle = normalize_mapped(surface.trim()).text;
    if needle.is_empty() {
        return None;
    }
    let normalized = normalize_mapped(source);
    let ranges = sentences(source);
    let mut chosen: Option<(Range<usize>, Range<usize>)> = None;
    for (at, _) in normalized.text.match_indices(&needle) {
        if !boundary_match(&normalized.text, at, &needle) {
            continue;
        }
        let original = normalized.offsets[at].0..normalized.offsets[at + needle.len() - 1].1;
        let first = ranges
            .iter()
            .find(|r| r.start <= original.start && original.start < r.end)?;
        let last = ranges
            .iter()
            .find(|r| r.start < original.end && original.end <= r.end)?;
        let sentence = first.start..last.end;
        if let Some((prior, _)) = &chosen {
            if prior != &sentence {
                return None;
            }
        } else {
            chosen = Some((sentence, original));
        }
    }
    let (sentence, original) = chosen?;
    let text = source[sentence].to_owned();
    let translated = single_sentence_translation(source, &text, translation);
    Some((text, translated, source[original].to_owned()))
}
pub fn bind(source: &str, surface: &str, translation: &str) -> Option<(String, String)> {
    bind_original(source, surface, translation)
        .map(|(sentence, translated, _)| (sentence, translated))
}
pub fn single_sentence_translation(source: &str, sentence: &str, translation: &str) -> String {
    if !super::short_query(source) && normalize_whitespace(source) == normalize_whitespace(sentence)
    {
        translation.trim().into()
    } else {
        String::new()
    }
}
/// Textarea selection offsets are UTF-16, while Rust slices use byte offsets.
pub fn selected_sentence(source: &str, start: usize, end: usize) -> Option<(String, String)> {
    fn byte_offset(text: &str, offset: usize) -> Option<usize> {
        let mut units = 0;
        for (at, c) in text.char_indices() {
            if units == offset {
                return Some(at);
            }
            units += c.len_utf16();
            if units > offset {
                return None;
            }
        }
        (units == offset).then_some(text.len())
    }
    let start = byte_offset(source, start)?;
    let end = byte_offset(source, end)?;
    if start >= end {
        return None;
    }
    let selected = trimmed_range(source, start..end)?;
    let word = &source[selected.clone()];
    let range = sentences(source)
        .into_iter()
        .find(|r| r.start <= selected.start && selected.end <= r.end)?;
    Some((word.into(), source[range].into()))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn punctuation_paths_quotes_and_abbreviations_remain_exact() {
        let source = "Dr. Lee paid 3.14 dollars. He said \"Use C:\\work\\resilient.txt!\" Then left. 下一句！";
        let pieces: Vec<_> = sentences(source)
            .iter()
            .map(|r| &source[r.clone()])
            .collect();
        assert_eq!(
            pieces,
            vec![
                "Dr. Lee paid 3.14 dollars.",
                "He said \"Use C:\\work\\resilient.txt!\"",
                "Then left.",
                "下一句！"
            ]
        );
    }
    #[test]
    fn no_ordinal_pairing_or_partial_word_matches() {
        assert!(bind("Concatenate these strings.", "cat", "").is_none());
        assert!(bind("The bank closed. Visit the bank tomorrow.", "bank", "").is_none());
        let c = bind("Stay resilient. Rest now.", "resilient", "先休息。再坚持。").unwrap();
        assert_eq!(c.0, "Stay resilient.");
        assert!(c.1.is_empty());
        let c = bind(
            "The resilient team adapted quickly.",
            "resilient",
            "坚韧的团队很快适应了。",
        )
        .unwrap();
        assert_eq!(c.1, "坚韧的团队很快适应了。");
        assert!(
            bind("resilient", "resilient", "IPA: /.../ adjective")
                .unwrap()
                .1
                .is_empty()
        );
    }
    #[test]
    fn manual_selection_resolves_utf16() {
        let source = "😀 Take off now. They took off later.";
        let at = source.find("took off").unwrap();
        let start = source[..at].encode_utf16().count();
        assert_eq!(
            selected_sentence(source, start, start + 8),
            Some(("took off".into(), "They took off later.".into()))
        );
        assert!(selected_sentence(source, 1, 2).is_none());
    }
    #[test]
    fn ocr_phrase_with_case_and_whitespace_changes_binds_local_original() {
        let source = "They Took\n  OFF before dawn.";
        let bound = bind(source, "took off", "他们在黎明前起飞。");
        assert!(bound.is_some());
        assert_eq!(bound.unwrap().0, source);
        assert_eq!(
            bind_original(source, "took off", "").unwrap().2,
            "Took\n  OFF"
        );
    }
}
