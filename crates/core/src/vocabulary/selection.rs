//! A selected source word is a preview until the user explicitly collects it.
//!
//! Dictionary text is local, and a task may contribute context only when its
//! source and completed attempt still match. No sentence-position alignment is
//! inferred from the source and target translations.
use super::{Candidate, context, word_key};
use crate::{
    config::Provider,
    tasks::{Status, Task},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct Selection {
    source: String,
    pub surface: String,
    pub sentence: String,
}

#[derive(Debug, Clone, Default)]
pub struct DictionaryEntry {
    pub lemma: String,
    pub definition: String,
    pub phonetic: String,
    pub pos: String,
}

/// UTF-16 offsets into the completed task's translation, matching browser text.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TranslationHighlight {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Preview {
    pub candidate: Candidate,
    pub translation_highlights: Vec<TranslationHighlight>,
}

impl Selection {
    /// Validate before sending selected text to a platform dictionary or UI.
    pub fn new(source: &str, start: usize, end: usize) -> Result<Self, String> {
        if source.chars().count() > crate::MAX_INPUT_CHARACTERS {
            return Err("原文过长。".into());
        }
        let (surface, sentence) =
            context::selected_sentence(source, start, end).ok_or("请在原文中选择单词或短语。")?;
        if surface.chars().count() > 128
            || surface.split_whitespace().count() > 3
            || sentence.len() > 16000
            || !surface.chars().any(char::is_alphanumeric)
        {
            return Err("请在原文中选择单词或短语。".into());
        }
        if crate::sensitive::detect(&sentence).is_some() {
            return Err("原句包含疑似敏感内容，未查询。".into());
        }
        Ok(Self {
            source: source.into(),
            surface,
            sentence,
        })
    }

    pub fn preview(
        &self,
        dictionary: DictionaryEntry,
        task: Option<(&Task, u32)>,
    ) -> Result<Preview, String> {
        let task = self.matching_task(task);
        let lemma = if dictionary.lemma.trim().is_empty() {
            self.surface.clone()
        } else {
            dictionary.lemma
        };
        let existing = task.and_then(|task| {
            task.vocabulary.iter().find(|candidate| {
                context::normalize_whitespace(&candidate.context_sentence)
                    == context::normalize_whitespace(&self.sentence)
                    && (word_key(&candidate.word) == word_key(&lemma)
                        || word_key(&candidate.surface) == word_key(&self.surface))
            })
        });
        let mut candidate = existing.cloned().unwrap_or_default();
        candidate.word = lemma;
        candidate.surface = self.surface.clone();
        candidate.context_sentence = self.sentence.clone();
        candidate.sentence_trans = task
            .map(|task| task.sentence_translation(task.attempt, &self.source, &self.sentence))
            .unwrap_or_default();
        let local_definition = dictionary.definition;
        if !local_definition.trim().is_empty() {
            candidate.dict_meaning = local_definition.chars().take(1800).collect();
        }
        if !dictionary.phonetic.trim().is_empty() {
            candidate.phonetic = dictionary.phonetic;
        }
        if !dictionary.pos.trim().is_empty() {
            candidate.pos = dictionary.pos;
        }
        candidate.validate()?;
        let mut translation_highlights = self.highlights_for_meaning(
            &candidate.context_meaning,
            task.map(|task| (task, task.attempt)),
        );
        if translation_highlights.is_empty() && self.has_unique_surface() {
            translation_highlights = task
                .map(|task| {
                    dictionary_highlight(
                        &task.translation,
                        if local_definition.trim().is_empty() {
                            &candidate.dict_meaning
                        } else {
                            &local_definition
                        },
                    )
                })
                .unwrap_or_default();
        }
        Ok(Preview {
            candidate,
            translation_highlights,
        })
    }

    /// Revalidate a later context meaning against the same completed source and
    /// attempt. The returned range always refers to the original translation;
    /// Markdown rendering must map it to text nodes before displaying a mark.
    pub fn highlights_for_meaning(
        &self,
        meaning: &str,
        task: Option<(&Task, u32)>,
    ) -> Vec<TranslationHighlight> {
        if crate::sensitive::detect(meaning).is_some() || !self.has_unique_surface() {
            return vec![];
        }
        self.matching_task(task)
            .map(|task| unique_highlight(&task.translation, meaning_pieces(meaning, false)))
            .unwrap_or_default()
    }

    fn matching_task<'a>(&self, task: Option<(&'a Task, u32)>) -> Option<&'a Task> {
        task.filter(|(task, attempt)| {
            task.attempt == *attempt
                && task.status == Status::Completed
                && task.query_text() == self.source
                && task.profile.provider != Provider::SystemDictionary
        })
        .map(|(task, _)| task)
    }

    fn has_unique_surface(&self) -> bool {
        matching_occurrences(&word_key(&self.source), &word_key(&self.surface)).len() == 1
    }
}

fn dictionary_highlight(translation: &str, definition: &str) -> Vec<TranslationHighlight> {
    unique_highlight(translation, meaning_pieces(definition, true))
}

fn without_pos_prefix(text: &str) -> &str {
    let text = text.trim();
    let unwrapped = text.trim_start_matches(['(', '（', '[']);
    let lower = unwrapped.to_ascii_lowercase();
    for prefix in [
        "adjective ",
        "adverb ",
        "noun ",
        "verb ",
        "preposition ",
        "adj.",
        "adv.",
        "n.",
        "v.",
        "vt.",
        "vi.",
        "prep.",
        "conj.",
        "pron.",
        "det.",
        "interj.",
        "aux.",
        "num.",
    ] {
        if lower.starts_with(prefix) {
            return unwrapped[prefix.len()..]
                .trim_start_matches([')', '）', ']', ':', '：'])
                .trim();
        }
    }
    text
}

fn meaning_pieces(text: &str, dictionary: bool) -> Vec<&str> {
    text.split([';', '；', '、', '，', ',', '\n', '\r', '|'])
        .filter_map(|piece| {
            let piece = piece.trim();
            let piece = if dictionary {
                piece
                    .trim_start_matches(|c: char| c.is_ascii_digit() || c.is_whitespace())
                    .trim_start_matches(['.', ')', '）', '、'])
                    .trim()
            } else {
                piece
            };
            let piece = without_pos_prefix(piece);
            if dictionary && !(2..=16).contains(&piece.chars().count()) {
                return None;
            }
            // Native dictionaries also include examples, headwords and prose.
            // Only an entire short Han sense between explicit separators is a
            // target-language dictionary hint; never extract fragments of prose.
            if dictionary && !piece.chars().all(|c| matches!(c, '\u{3400}'..='\u{9fff}')) {
                return None;
            }
            Some(piece)
        })
        .collect()
}

fn unique_highlight(translation: &str, meanings: Vec<&str>) -> Vec<TranslationHighlight> {
    let mut found = vec![];
    for meaning in meanings {
        if matching_occurrences(translation, meaning).len() > 1 {
            return vec![];
        }
        let ranges = exact_meaning_highlight(translation, meaning);
        for range in ranges {
            if !found.contains(&range) {
                found.push(range);
            }
        }
    }
    if found.len() == 1 { found } else { vec![] }
}

/// A short, unique lexical occurrence is evidence; an entire sentence, a
/// repeated meaning, or a dictionary paragraph is not an alignment.
fn exact_meaning_highlight(translation: &str, meaning: &str) -> Vec<TranslationHighlight> {
    let meaning = meaning.trim();
    if meaning.chars().count() < 2
        || meaning.chars().count() > 40
        || meaning.contains(['\n', '\r', '.', '!', '?', '。', '！', '？', ';', '；'])
    {
        return vec![];
    }
    let occurrences = matching_occurrences(translation, meaning);
    let [at] = occurrences.as_slice() else {
        return vec![];
    };
    if context::sentences(translation).iter().any(|range| {
        translation[range.clone()]
            .trim_matches(|c: char| c.is_whitespace() || ".!?。！？\"'”’»)]}」』".contains(c))
            == meaning
    }) {
        return vec![];
    }
    vec![TranslationHighlight {
        start: translation[..*at].encode_utf16().count(),
        end: translation[..*at + meaning.len()].encode_utf16().count(),
    }]
}

fn matching_occurrences(translation: &str, meaning: &str) -> Vec<usize> {
    let meaning = meaning.trim();
    if meaning.is_empty() {
        return vec![];
    }
    translation
        .match_indices(meaning)
        .filter(|(start, _)| {
            let before = translation[..*start].chars().next_back();
            let after = translation[*start + meaning.len()..].chars().next();
            !(meaning.starts_with(|c: char| c.is_ascii_alphanumeric())
                && before.is_some_and(|c| c.is_alphanumeric() || c == '_')
                || meaning.ends_with(|c: char| c.is_ascii_alphanumeric())
                    && after.is_some_and(|c| c.is_alphanumeric() || c == '_'))
        })
        .map(|(start, _)| start)
        .take(2)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{Profile, Retention},
        tasks::{Source, TaskBook},
    };

    fn completed(source: &str, translation: &str) -> Task {
        let book = TaskBook::new(Retention::Disabled, 10);
        let task = book
            .create(Source::Manual, Profile::default(), "zh-Hans".into(), 1)
            .unwrap();
        book.recognized(&task.id, source.into(), 0, 2).unwrap();
        book.start(&task.id, 1, 0, 3);
        book.append(&task.id, 1, translation, 1, 4).unwrap();
        book.finish(&task.id, 1, 2, 5);
        book.get(&task.id).unwrap()
    }

    fn selection(source: &str, word: &str) -> Selection {
        let start = source.find(word).unwrap();
        Selection::new(
            source,
            source[..start].encode_utf16().count(),
            source[..start + word.len()].encode_utf16().count(),
        )
        .unwrap()
    }

    #[test]
    fn selection_preview_reuses_only_the_matching_local_candidate_without_mutating_task() {
        let source = "The resilient team prevailed.";
        let mut task = completed(source, "😀 坚韧的团队获胜了。");
        task.vocabulary.push(Candidate {
            word: "resilient".into(),
            surface: "resilient".into(),
            context_meaning: "坚韧的".into(),
            context_sentence: source.into(),
            ..Default::default()
        });
        let preview = selection(source, "resilient")
            .preview(
                DictionaryEntry {
                    lemma: "resilient".into(),
                    definition: "坚韧的；能复原的".into(),
                    phonetic: "/rɪˈzɪliənt/".into(),
                    pos: "adj.".into(),
                },
                Some((&task, 1)),
            )
            .unwrap();
        assert_eq!(preview.candidate.context_meaning, "坚韧的");
        assert_eq!(preview.candidate.dict_meaning, "坚韧的；能复原的");
        assert_eq!(preview.candidate.phonetic, "/rɪˈzɪliənt/");
        assert_eq!(preview.candidate.sentence_trans, task.translation);
        assert_eq!(
            preview.translation_highlights,
            vec![TranslationHighlight { start: 3, end: 6 }]
        );
        assert_eq!(task.vocabulary.len(), 1);
        assert!(task.vocabulary[0].dict_meaning.is_empty());
    }

    #[test]
    fn edited_source_stale_attempt_and_incomplete_task_do_not_reuse_translation() {
        let source = "The resilient team prevailed.";
        let mut task = completed(source, "坚韧的团队获胜了。");
        task.vocabulary.push(Candidate {
            word: "resilient".into(),
            surface: "resilient".into(),
            context_meaning: "坚韧的".into(),
            context_sentence: source.into(),
            ..Default::default()
        });
        let chosen = selection(source, "resilient");
        let edited = selection("The resilient team left.", "resilient");
        let mut incomplete = task.clone();
        incomplete.status = Status::Translating;
        let mut dictionary = task.clone();
        dictionary.profile.provider = Provider::SystemDictionary;
        for (selection, candidate_task, attempt) in [
            (&chosen, &task, 2),
            (&edited, &task, 1),
            (&chosen, &incomplete, 1),
            (&chosen, &dictionary, 1),
        ] {
            let preview = selection
                .preview(DictionaryEntry::default(), Some((candidate_task, attempt)))
                .unwrap();
            assert!(preview.candidate.context_meaning.is_empty());
            assert!(preview.candidate.sentence_trans.is_empty());
            assert!(preview.translation_highlights.is_empty());
            assert!(
                selection
                    .highlights_for_meaning("坚韧的", Some((candidate_task, attempt)))
                    .is_empty()
            );
        }
    }

    #[test]
    fn same_word_in_another_source_sentence_does_not_supply_context_meaning() {
        let source = "The bank closed. They reached the bank.";
        let mut task = completed(source, "银行关门了。他们到达了河岸。");
        task.vocabulary.push(Candidate {
            word: "bank".into(),
            surface: "bank".into(),
            context_meaning: "银行".into(),
            context_sentence: "The bank closed.".into(),
            ..Default::default()
        });
        let at = source.rfind("bank").unwrap();
        let chosen = Selection::new(source, at, at + 4).unwrap();
        let preview = chosen
            .preview(DictionaryEntry::default(), Some((&task, 1)))
            .unwrap();
        assert_eq!(preview.candidate.context_sentence, "They reached the bank.");
        assert!(preview.candidate.context_meaning.is_empty());
        assert!(preview.candidate.sentence_trans.is_empty());
        assert!(preview.translation_highlights.is_empty());
    }

    #[test]
    fn exact_alignment_rejects_ambiguity_sentence_guesses_and_partial_words() {
        assert!(exact_meaning_highlight("银行旁有另一家银行。", "银行").is_empty());
        assert!(exact_meaning_highlight("显著减少。", "明显").is_empty());
        assert!(exact_meaning_highlight("坚韧的团队获胜了。", "坚韧的团队获胜了。").is_empty());
        assert!(exact_meaning_highlight("The catapult moved.", "cat").is_empty());
        assert!(exact_meaning_highlight("They are very important.", "very").len() == 1);
        assert!(exact_meaning_highlight("银行", "银行").is_empty());
        assert!(exact_meaning_highlight("银行关门。他们回家了。", "银行关门").is_empty());
        assert!(exact_meaning_highlight("😀 银行还开着。", "银行").len() == 1);
    }

    #[test]
    fn dictionary_alignment_requires_one_exact_short_sense_and_rejects_prose() {
        assert_eq!(
            dictionary_highlight("明显减少了中断。", "adv. 显著；明显"),
            vec![TranslationHighlight { start: 0, end: 2 }]
        );
        assert!(dictionary_highlight("明显而显著的变化。", "显著；明显").is_empty());
        assert!(dictionary_highlight("银行和银行旁边是河岸。", "银行；河岸").is_empty());
        assert!(dictionary_highlight("明显减少了中断。", "明显地；显著地").is_empty());
        assert!(dictionary_highlight("中断显著减少了。", "例如中断显著减少了。").is_empty());
        assert!(dictionary_highlight("中断显著减少了。", "noun a word that is 显著").is_empty());
        assert!(dictionary_highlight("The very important thing.", "very important").is_empty());
        assert_eq!(
            dictionary_highlight("明显减少了中断。", "1. adv. 明显；明显"),
            vec![TranslationHighlight { start: 0, end: 2 }]
        );
    }

    #[test]
    fn local_sense_can_align_without_copying_dictionary_meaning_into_context() {
        let source = "The disruption decreased significantly.";
        let task = completed(source, "中断显著减少了。");
        let preview = selection(source, "significantly")
            .preview(
                DictionaryEntry {
                    lemma: "significantly".into(),
                    definition: "显著；明显".into(),
                    ..Default::default()
                },
                Some((&task, 1)),
            )
            .unwrap();
        assert!(preview.candidate.context_meaning.is_empty());
        assert_eq!(
            preview.translation_highlights,
            vec![TranslationHighlight { start: 2, end: 4 }]
        );
    }

    #[test]
    fn repeated_source_surface_is_not_assumed_to_share_a_target_meaning() {
        let source = "The bank is next to the bank.";
        let task = completed(source, "银行在河岸旁边。");
        let preview = selection(source, "bank")
            .preview(
                DictionaryEntry {
                    lemma: "bank".into(),
                    definition: "银行；岸边".into(),
                    ..Default::default()
                },
                Some((&task, 1)),
            )
            .unwrap();
        assert!(preview.translation_highlights.is_empty());
    }

    #[test]
    fn lexical_context_alignment_strips_only_explicit_pos_prefixes() {
        let source = "The disruption decreased significantly.";
        let task = completed(source, "中断显著减少了。");
        let chosen = selection(source, "significantly");
        assert_eq!(
            chosen.highlights_for_meaning("adv. 显著", Some((&task, 1))),
            vec![TranslationHighlight { start: 2, end: 4 }]
        );
        assert!(
            chosen
                .highlights_for_meaning("显著地", Some((&task, 1)))
                .is_empty()
        );
        assert!(
            chosen
                .highlights_for_meaning("完全不同", Some((&task, 1)))
                .is_empty()
        );
    }

    #[test]
    fn selection_validation_guards_sensitive_and_invalid_utf16_before_dictionary_lookup() {
        assert!(Selection::new("😀 resilient", 1, 2).is_err());
        assert!(Selection::new("resilient", 0, 50).is_err());
        assert!(Selection::new("....", 0, 4).is_err());
        let source = format!("The token sk-{} is resilient.", "x".repeat(40));
        assert!(selection_result(&source, "resilient").is_err());
        assert!(Selection::new("one two three four", 0, 18).is_err());
    }

    fn selection_result(source: &str, word: &str) -> Result<Selection, String> {
        let at = source.find(word).unwrap();
        Selection::new(source, at, at + word.len())
    }
}
