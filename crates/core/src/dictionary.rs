//! Capability checks for the local word/phrase dictionary, independent of vocabulary extraction.
use crate::config::{Profile, Provider};
use serde::Serialize;

pub const SENTENCE_MESSAGE: &str = "系统词典用于查询单词或短语；整句或段落请使用翻译服务。";
pub const TOO_LONG_MESSAGE: &str = "查询内容过长，请缩短为单词或短语，或使用翻译服务。";
pub const NOT_FOUND_MESSAGE: &str = "系统词典未找到匹配词条。";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryIssue {
    TooLong,
    Sentence,
}

impl QueryIssue {
    pub fn message(self) -> &'static str {
        match self {
            Self::TooLong => TOO_LONG_MESSAGE,
            Self::Sentence => SENTENCE_MESSAGE,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Guidance {
    pub issue: Option<QueryIssue>,
    pub profile_ids: Vec<String>,
}

/// Conservative shape check: short idioms and abbreviations remain lookup candidates.
/// A short sentence can still reach the dictionary; a miss is never a diagnosis of its configuration.
pub fn query_issue(text: &str) -> Option<QueryIssue> {
    let text = text.trim();
    if text.chars().count() > 200 {
        return Some(QueryIssue::TooLong);
    }
    let words = text.split_whitespace().count();
    let unspaced = text
        .chars()
        .filter(|c| {
            matches!(*c,
        '\u{3400}'..='\u{9fff}' | '\u{3040}'..='\u{30ff}' | '\u{ac00}'..='\u{d7af}')
        })
        .count();
    let sentence_end = text
        .trim_end_matches(['\"', '\'', '”', '’', ')', ']', '）', '】'])
        .ends_with(['.', '?', '!', '。', '？', '！']);
    let paragraph = text.lines().any(|line| line.trim().is_empty());
    if paragraph || ((words > 3 || unspaced > 8) && sentence_end) {
        Some(QueryIssue::Sentence)
    } else {
        None
    }
}

pub fn configured_translator(profile: &Profile, has_key: bool) -> bool {
    profile.provider != Provider::SystemDictionary
        && profile.validate().is_ok()
        && (has_key || profile.provider == Provider::Custom)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictionary_accepts_phrases_abbreviations_and_unspaced_terms() {
        for text in [
            "hello",
            "as a matter of fact",
            "at the end of the day",
            "all work and no play makes Jack a dull boy",
            "U.S.",
            "C++",
            "std::vector",
            "中华人民共和国",
            "画蛇添足",
            "人工知能",
            "気候変動に関する政府間パネル",
            "  take\n off  ",
        ] {
            assert_eq!(query_issue(text), None, "{text}");
        }
    }

    #[test]
    fn dictionary_identifies_obvious_sentences_without_using_vocabulary_limits() {
        for text in [
            "A local dictionary explains individual words but cannot translate this complete example sentence.",
            "The meeting starts tomorrow.",
            "First paragraph\n\nSecond paragraph",
            "今天天气很好，我们一起出去走走吧。",
        ] {
            assert_eq!(query_issue(text), Some(QueryIssue::Sentence));
        }
        assert_eq!(query_issue(&"a".repeat(201)), Some(QueryIssue::TooLong));
        assert_eq!(query_issue(""), None);
    }

    #[test]
    fn translation_choices_require_configuration_and_provider_credentials() {
        let mut profile = Profile {
            endpoint: "https://fixture.example/v1".into(),
            model: "fixture-model".into(),
            ..Profile::default()
        };
        assert!(configured_translator(&profile, false));
        profile.model.clear();
        assert!(!configured_translator(&profile, true));
        profile.model = "fixture-model".into();
        profile.provider = Provider::Openai;
        assert!(!configured_translator(&profile, false));
        assert!(configured_translator(&profile, true));
        profile.provider = Provider::SystemDictionary;
        assert!(!configured_translator(&profile, true));
        profile.provider = Provider::Youdao;
        assert!(!configured_translator(&profile, true));
        profile.app_id = "fixture-app".into();
        assert!(configured_translator(&profile, true));
    }
}
