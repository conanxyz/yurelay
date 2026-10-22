//! Local vocabulary matching never changes collection/review counts.
use crate::vocabulary::{VocabularyItem, word_key};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenRange {
    pub start_utf16: usize,
    pub end_utf16: usize,
    pub surface: String,
    pub lemma: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct Encounter {
    pub start_utf16: usize,
    pub end_utf16: usize,
    pub item_id: String,
    pub word: String,
    pub surface: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct Encounters {
    pub task_id: String,
    pub source_revision: u64,
    pub matches: Vec<Encounter>,
}
pub fn token_ranges(text: &str, mut lemma: impl FnMut(&str) -> String) -> Vec<TokenRange> {
    let mut out = vec![];
    let mut start = None;
    let mut utf16 = 0;
    let mut start_utf16 = 0;
    for (byte, c) in text.char_indices() {
        if c.is_ascii_alphanumeric() || (matches!(c, '\'' | '’' | '-') && start.is_some()) {
            if start.is_none() {
                start = Some(byte);
                start_utf16 = utf16;
            }
        } else if let Some(at) = start.take() {
            let surface = &text[at..byte];
            out.push(TokenRange {
                start_utf16,
                end_utf16: utf16,
                surface: surface.into(),
                lemma: lemma(surface),
            });
        }
        utf16 += c.len_utf16();
    }
    if let Some(at) = start {
        let surface = &text[at..];
        out.push(TokenRange {
            start_utf16,
            end_utf16: utf16,
            surface: surface.into(),
            lemma: lemma(surface),
        });
    }
    out
}
/// Indexed lookup keys avoid loading every notebook entry and its contexts.
pub fn lookup_keys(text: &str, tokens: &[TokenRange]) -> Vec<String> {
    let utf16: Vec<_> = text.encode_utf16().collect();
    let mut keys = std::collections::BTreeSet::new();
    for at in 0..tokens.len() {
        for count in 1..=3.min(tokens.len() - at) {
            let span = &tokens[at..at + count];
            if span.windows(2).any(|pair| {
                pair[0].end_utf16 > pair[1].start_utf16
                    || pair[1].start_utf16 > utf16.len()
                    || String::from_utf16_lossy(&utf16[pair[0].end_utf16..pair[1].start_utf16])
                        .chars()
                        .any(|c| !c.is_whitespace())
            }) {
                break;
            }
            for key in [
                span.iter()
                    .map(|t| word_key(&t.surface))
                    .collect::<Vec<_>>()
                    .join(" "),
                span.iter()
                    .map(|t| word_key(&t.lemma))
                    .collect::<Vec<_>>()
                    .join(" "),
            ] {
                if !key.is_empty() && key.chars().count() <= 128 {
                    keys.insert(key);
                }
            }
        }
    }
    keys.into_iter().collect()
}
pub fn matches(text: &str, tokens: &[TokenRange], items: &[VocabularyItem]) -> Vec<Encounter> {
    let index: HashMap<_, _> = items
        .iter()
        .filter(|v| v.status == "learning")
        .map(|v| (word_key(&v.word), v))
        .collect();
    let utf16: Vec<_> = text.encode_utf16().collect();
    let mut result = vec![];
    let mut at = 0;
    while at < tokens.len() {
        let mut matched = None;
        for count in (1..=3.min(tokens.len() - at)).rev() {
            let span = &tokens[at..at + count];
            if span.windows(2).any(|w| {
                w[0].end_utf16 > w[1].start_utf16
                    || w[1].start_utf16 > utf16.len()
                    || String::from_utf16_lossy(&utf16[w[0].end_utf16..w[1].start_utf16])
                        .chars()
                        .any(|c| !c.is_whitespace())
            }) {
                continue;
            }
            let key = span
                .iter()
                .map(|t| word_key(&t.surface))
                .collect::<Vec<_>>()
                .join(" ");
            let lemma = span
                .iter()
                .map(|t| word_key(&t.lemma))
                .collect::<Vec<_>>()
                .join(" ");
            if let Some(item) = index.get(&key).or_else(|| index.get(&lemma)) {
                matched = Some((count, *item));
                break;
            }
        }
        if let Some((count, item)) = matched {
            let first = &tokens[at];
            let last = &tokens[at + count - 1];
            if first.start_utf16 < last.end_utf16 && last.end_utf16 <= utf16.len() {
                result.push(Encounter {
                    start_utf16: first.start_utf16,
                    end_utf16: last.end_utf16,
                    item_id: item.id.clone(),
                    word: item.word.clone(),
                    surface: String::from_utf16_lossy(&utf16[first.start_utf16..last.end_utf16]),
                });
            }
            at += count;
        } else {
            at += 1;
        }
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    fn item(word: &str, status: &str) -> VocabularyItem {
        VocabularyItem {
            id: word.into(),
            word: word.into(),
            status: status.into(),
            phonetic: String::new(),
            pos: String::new(),
            dict_meaning: String::new(),
            note: String::new(),
            difficulty: String::new(),
            review_count: 2,
            contexts: vec![],
            created_at: 1,
            updated_at: 1,
        }
    }
    #[test]
    fn inflections_utf16_phrases_and_mastered() {
        let text = "😀 running with API, take care. run";
        let tokens = token_ranges(text, |s| {
            if s == "running" {
                "run".into()
            } else {
                s.to_lowercase()
            }
        });
        let items = [
            item("run", "learning"),
            item("API", "mastered"),
            item("take care", "learning"),
        ];
        let result = matches(text, &tokens, &items);
        assert_eq!(result.len(), 3);
        assert_eq!((result[0].start_utf16, result[0].end_utf16), (3, 10));
        assert_eq!(result[1].word, "take care");
        assert_eq!(items[0].review_count, 2);
    }
    #[test]
    fn phrase_does_not_cross_punctuation() {
        let text = "take, care";
        assert!(
            matches(
                text,
                &token_ranges(text, |s| s.into()),
                &[item("take care", "learning")]
            )
            .is_empty()
        );
    }
    #[test]
    fn large_dictionary_and_near_limit_source() {
        let items = (0..10000)
            .map(|i| item(&format!("word{i}"), "learning"))
            .collect::<Vec<_>>();
        let text = (0..5500)
            .map(|i| format!("word{} ", i % 2000))
            .collect::<String>();
        let now = std::time::Instant::now();
        let tokens = token_ranges(&text, |s| s.into());
        assert_eq!(matches(&text, &tokens, &items).len(), 5500);
        eprintln!(
            "local encounter benchmark: {} chars, {} tokens, 10000 entries, {:?}",
            text.len(),
            tokens.len(),
            now.elapsed()
        );
    }
}
