use serde::{Deserialize, Serialize};
use std::{collections::HashSet, sync::LazyLock};
pub mod context;
pub mod selection;

/// Space-delimited languages use words; unspaced CJK input uses a conservative
/// three-character short-query boundary rather than treating whole paragraphs as one word.
pub fn short_query(input: &str) -> bool {
    let input = input.trim();
    if input.is_empty()
        || input.contains(['\n', '\r', '?', '!', '。', '！', '？', '\\', '_'])
        || input.ends_with('.')
    {
        return false;
    }
    let words = input.split_whitespace().count();
    let cjk=input.chars().filter(|c|matches!(*c,'\u{3400}'..='\u{9fff}'|'\u{3040}'..='\u{30ff}'|'\u{ac00}'..='\u{d7af}')).count();
    words <= 3 && cjk <= 3
}

pub fn extraction_enabled(enabled: bool, text: &str) -> bool {
    enabled
        && text.split_whitespace().take(151).count() <= 150
        && text.chars().take(801).count() <= 800
}

pub const MARKER: &str = "---VOCAB_JSON---";
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Candidate {
    pub word: String,
    pub surface: String,
    pub phonetic: String,
    pub pos: String,
    pub dict_meaning: String,
    pub context_meaning: String,
    pub context_sentence: String,
    pub sentence_trans: String,
    pub difficulty: String,
}
pub fn word_key(word: &str) -> String {
    word.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}
pub fn technical_word(word: &str, difficulty: &str) -> bool {
    difficulty.eq_ignore_ascii_case("term")
        || word.split_whitespace().any(|part| {
            part.chars().any(|c| c.is_ascii_digit() || c == '_')
                || part.chars().skip(1).any(|c| c.is_ascii_uppercase())
        })
}
fn term_display(word: &str, surface: &str) -> String {
    let word = word.split_whitespace().collect::<Vec<_>>().join(" ");
    if word == word.to_lowercase() && word_key(&word) == word_key(surface) {
        surface.split_whitespace().collect::<Vec<_>>().join(" ")
    } else {
        word
    }
}
impl Candidate {
    pub fn validate(&mut self) -> Result<(), String> {
        let technical =
            technical_word(&self.word, &self.difficulty) || technical_word(&self.surface, "");
        self.word = if technical {
            term_display(&self.word, &self.surface)
        } else {
            word_key(&self.word)
        };
        if technical {
            self.phonetic.clear();
        }
        if self.word.is_empty()
            || self.word.chars().count() > 128
            || self.surface.chars().count() > 256
            || self.phonetic.len() > 256
            || self.pos.len() > 64
            || self.dict_meaning.len() > 8000
            || self.context_meaning.len() > 8000
            || self.context_sentence.len() > 16000
            || self.sentence_trans.len() > 16000
            || self.difficulty.len() > 32
        {
            return Err("生词字段为空或过长。".into());
        }
        if crate::sensitive::detect(&serde_json::to_string(self).unwrap_or_default()).is_some() {
            return Err("生词包含疑似敏感内容，未保存。".into());
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Context {
    pub id: String,
    pub surface: String,
    pub context_meaning: String,
    pub context_sentence: String,
    pub sentence_trans: String,
    pub created_at: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VocabularyItem {
    pub id: String,
    pub word: String,
    pub phonetic: String,
    pub pos: String,
    pub dict_meaning: String,
    pub note: String,
    pub difficulty: String,
    pub status: String,
    pub review_count: u64,
    pub contexts: Vec<Context>,
    pub created_at: i64,
    pub updated_at: i64,
}
impl VocabularyItem {
    pub fn normalize_display(&mut self) {
        if technical_word(&self.word, &self.difficulty) {
            if let Some(context) = self
                .contexts
                .iter()
                .find(|c| word_key(&c.surface) == word_key(&self.word))
            {
                self.word = term_display(&self.word, &context.surface);
            }
            self.phonetic.clear();
        }
    }
}
// Accept the old short meaning fields for existing custom prompts. Sentence
// copies from any wire format are ignored: source context is always local.
#[derive(Default, Deserialize)]
#[serde(default)]
struct WireCandidate {
    word: String,
    surface: String,
    phonetic: String,
    pos: String,
    meaning: Option<String>,
    level: Option<String>,
    context_meaning: Option<String>,
    dict_meaning: Option<String>,
    difficulty: Option<String>,
}
fn without_trailing_commas(raw: &str) -> String {
    let mut output = String::with_capacity(raw.len());
    let mut quoted = false;
    let mut escaped = false;
    for (at, c) in raw.char_indices() {
        if quoted {
            output.push(c);
            if escaped {
                escaped = false
            } else if c == '\\' {
                escaped = true
            } else if c == '"' {
                quoted = false
            }
            continue;
        }
        if c == '"' {
            quoted = true
        }
        if c == ',' && raw[at + 1..].trim_start().starts_with([']', '}']) {
            continue;
        }
        output.push(c);
    }
    output
}

// Some providers render the sentinel as a standalone Markdown label instead of
// the requested delimiter. Only accept those aliases at a line boundary and
// after an array begins; ordinary mentions of VOCAB_JSON remain visible.
static MARKER_LABELS: LazyLock<Vec<String>> = LazyLock::new(|| {
    let mut labels = Vec::new();
    for separator in ["", "---\n", "---\r\n"] {
        for heading in ["", "# ", "## ", "### ", "#### ", "##### ", "###### "] {
            for wrapper in ["", "**", "__", "`"] {
                for marker in [MARKER, "VOCAB_JSON"] {
                    for suffix in ["", ":", "："] {
                        labels.push(format!(
                            "{separator}{heading}{wrapper}{marker}{wrapper}{suffix}"
                        ));
                    }
                }
            }
        }
    }
    labels
});

// (array start, could still become a metadata boundary).
fn alternate_boundary(text: &str) -> (Option<usize>, bool) {
    let trimmed = text.trim_start_matches([' ', '\t']);
    if text.len() - trimmed.len() > 3 {
        return (None, false);
    }
    let mut possible = false;
    for label in MARKER_LABELS.iter() {
        if label.starts_with(trimmed) {
            possible = true;
        }
        if let Some(rest) = trimmed.strip_prefix(label) {
            if !rest.is_empty()
                && !rest.starts_with(char::is_whitespace)
                && !label.ends_with([':', '：'])
            {
                continue;
            }
            let mut body = rest.trim_start();
            // Fenced JSON is another common presentation of the same payload.
            if body.len() < 7 && "```json".starts_with(body) {
                possible = true;
                continue;
            }
            if let Some(rest) = body.strip_prefix("```json") {
                body = rest.trim_start();
            } else if let Some(rest) = body.strip_prefix("```") {
                body = rest.trim_start();
            }
            if body.starts_with('[') {
                return (Some(text.len() - body.len()), false);
            }
            if body.is_empty() {
                possible = true;
            }
        }
    }
    // Bound withholding even if a model produces unending whitespace.
    (None, possible && text.len() <= 128)
}

/// Withholds only possible delimiter prefixes; metadata stays out of display.
#[derive(Default)]
pub struct StreamParser {
    pending: String,
    json: String,
    metadata: bool,
    overflow: bool,
    mid_line: bool,
}
impl StreamParser {
    pub fn has_metadata(&self) -> bool {
        self.metadata
    }
    pub fn push(&mut self, text: &str) -> String {
        if self.metadata {
            if !self.overflow && self.json.len() + text.len() <= 64 * 1024 {
                self.json.push_str(text)
            } else {
                self.json.clear();
                self.overflow = true;
            }
            return String::new();
        }
        self.pending.push_str(text);
        let mut boundary = self.pending.find(MARKER).map(|at| {
            let line = self.pending[..at].rfind('\n').map_or(0, |n| n + 1);
            let prefix = &self.pending[line..at];
            let start = if (line > 0 || !self.mid_line)
                && prefix.len() <= 16
                && prefix
                    .chars()
                    .all(|c| matches!(c, ' ' | '\t' | '#' | '*' | '_' | '`'))
            {
                line
            } else {
                at
            };
            (start, at + MARKER.len())
        });
        let mut withheld = None;
        let starts = std::iter::once(0)
            .filter(|_| !self.mid_line)
            .chain(self.pending.match_indices('\n').map(|(at, _)| at + 1));
        for start in starts {
            let (array, possible) = alternate_boundary(&self.pending[start..]);
            if let Some(array) = array {
                if boundary.is_none_or(|(at, _)| start < at) {
                    boundary = Some((start, start + array));
                }
            } else if possible {
                withheld.get_or_insert(start);
            }
        }
        if let Some((at, after)) = boundary {
            let visible = self.pending[..at].to_string();
            self.json = self.pending[after..].to_string();
            if self.json.len() > 64 * 1024 {
                self.json.clear();
                self.overflow = true;
            }
            self.pending.clear();
            self.metadata = true;
            return visible;
        }
        let keep = (1..MARKER.len())
            .rev()
            .find(|&n| self.pending.ends_with(&MARKER[..n]))
            .unwrap_or(0);
        let cut = withheld
            .unwrap_or(self.pending.len())
            .min(self.pending.len() - keep);
        let visible = self.pending[..cut].to_string();
        if !visible.is_empty() {
            self.mid_line = !visible.ends_with('\n');
        }
        self.pending.drain(..cut);
        visible
    }
    pub fn finish(
        &mut self,
        source: &str,
        translation: &str,
    ) -> (String, Vec<Candidate>, Option<String>) {
        let tail = std::mem::take(&mut self.pending);
        if !self.metadata {
            return (tail, vec![], None);
        }
        if self.overflow {
            return (tail, vec![], Some("生词数据过大，译文已保留。".into()));
        }
        let raw = self
            .json
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();
        let array = raw
            .find('[')
            .zip(raw.rfind(']'))
            .and_then(|(a, b)| (a <= b).then_some(&raw[a..=b]));
        let parsed = array.and_then(|s| {
            serde_json::from_str::<Vec<serde_json::Value>>(s)
                .or_else(|_| serde_json::from_str(&without_trailing_commas(s)))
                .ok()
        });
        let Some(values) = parsed else {
            return (tail, vec![], Some("生词数据格式无效，译文已保留。".into()));
        };
        let mut seen = HashSet::new();
        let mut rejected = false;
        let candidates = values
            .into_iter()
            .filter_map(|value| {
                let result = (|| {
                    let wire: WireCandidate = serde_json::from_value(value).ok()?;
                    let (sentence, sentence_trans, surface) =
                        context::bind_original(source, &wire.surface, translation)?;
                    let meaning = wire
                        .meaning
                        .or(wire.context_meaning.filter(|s| !s.is_empty()))
                        .or(wire.dict_meaning)
                        .unwrap_or_default();
                    if meaning.trim().is_empty() {
                        return None;
                    }
                    let mut candidate = Candidate {
                        word: wire.word,
                        surface,
                        phonetic: wire.phonetic,
                        pos: wire.pos,
                        context_meaning: meaning,
                        dict_meaning: String::new(),
                        context_sentence: sentence,
                        sentence_trans,
                        difficulty: wire.level.or(wire.difficulty).unwrap_or_default(),
                    };
                    if candidate.validate().is_err() {
                        return None;
                    }
                    if !seen.insert(word_key(&candidate.word)) {
                        return None;
                    }
                    Some(candidate)
                })();
                if result.is_none() {
                    rejected = true
                }
                result
            })
            .take(5)
            .collect();
        let notice =
            rejected.then(|| "部分生词无法可靠定位或格式无效，已忽略；译文已保留。".into());
        (tail, candidates, notice)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn provider_formatted_markers_hide_metadata_at_every_chunk_boundary() {
        let source = "An offline provider is available.";
        let json = r#"[{"word":"offline","surface":"offline","meaning":"离线的","level":"B2"}]"#;
        for label in [
            "---\nVOCAB_JSON\n",
            "VOCAB_JSON\n",
            "## VOCAB_JSON\n",
            "**VOCAB_JSON**:\n",
            "`VOCAB_JSON`\n```json\n",
            "---\r\nVOCAB_JSON\r\n",
            "**---VOCAB_JSON---**\n",
        ] {
            let output = format!("提供离线服务。\n\n{label}{json}");
            for split in output.char_indices().map(|(at, _)| at) {
                let mut parser = StreamParser::default();
                let mut visible = parser.push(&output[..split]);
                visible += &parser.push(&output[split..]);
                assert!(parser.has_metadata(), "{label:?} split={split}");
                let (tail, items, notice) = parser.finish(source, &visible);
                visible += &tail;
                assert_eq!(visible, "提供离线服务。\n\n", "{label:?} split={split}");
                assert_eq!(items.len(), 1);
                assert!(notice.is_none());
            }
            let mut parser = StreamParser::default();
            let visible = output
                .chars()
                .map(|c| parser.push(&c.to_string()))
                .collect::<String>();
            assert_eq!(visible, "提供离线服务。\n\n", "{label:?}");
            assert_eq!(parser.finish(source, &visible).1.len(), 1);
        }
    }

    #[test]
    fn marker_mentions_in_prose_and_unconfirmed_labels_are_not_removed() {
        for output in [
            "Use VOCAB_JSON in code.",
            "VOCAB_JSON is a label.\n[]",
            "VOCAB_JSON[0]",
            "## VOCAB_JSON\nThis explains the format.",
            "---\nA normal section follows.",
            "VOCAB_JSON",
            "前缀VOCAB_JSON\n[]",
        ] {
            let mut parser = StreamParser::default();
            let mut visible = output
                .chars()
                .map(|c| parser.push(&c.to_string()))
                .collect::<String>();
            let (tail, items, _) = parser.finish("source", &visible);
            visible += &tail;
            assert_eq!(visible, output);
            assert!(items.is_empty());
        }
    }

    #[test]
    fn malformed_alternate_metadata_is_hidden_and_reports_a_notice() {
        let mut parser = StreamParser::default();
        assert_eq!(parser.push("译文\nVOCAB_JSON\n[not valid json]"), "译文\n");
        let (tail, items, notice) = parser.finish("source", "译文");
        assert!(tail.is_empty() && items.is_empty() && notice.is_some());
    }

    #[test]
    fn terms_keep_display_case_and_never_keep_fabricated_phonetics() {
        for (word, surface, expected) in [
            ("oauth2", "OAuth2", "OAuth2"),
            ("SameSite", "SameSite", "SameSite"),
            ("idp", "IdP", "IdP"),
        ] {
            let mut candidate = Candidate {
                word: word.into(),
                surface: surface.into(),
                phonetic: "/invented/".into(),
                difficulty: "Term".into(),
                ..Default::default()
            };
            candidate.validate().unwrap();
            assert_eq!(candidate.word, expected);
            assert!(candidate.phonetic.is_empty());
        }
        let mut manual = Candidate {
            word: "idp".into(),
            surface: "IdP".into(),
            phonetic: "/fake/".into(),
            ..Default::default()
        };
        manual.validate().unwrap();
        assert_eq!(manual.word, "IdP");
        assert!(manual.phonetic.is_empty());
        let mut phrase = Candidate {
            word: "Take Off".into(),
            surface: "Take Off".into(),
            phonetic: "/test/".into(),
            ..Default::default()
        };
        phrase.validate().unwrap();
        assert_eq!(phrase.word, "take off");
        assert_eq!(phrase.phonetic, "/test/");
        let mut ordinary = Candidate {
            word: "Playground".into(),
            surface: "playground".into(),
            phonetic: "/ˈpleɪɡraʊnd/".into(),
            difficulty: "B2".into(),
            ..Default::default()
        };
        ordinary.validate().unwrap();
        assert_eq!(ordinary.word, "playground");
        assert!(!ordinary.phonetic.is_empty());
    }
    #[test]
    fn metadata_case_variants_deduplicate_by_key() {
        let mut parser = StreamParser::default();
        parser.push(r#"OAuth2。---VOCAB_JSON---[{"word":"OAuth2","surface":"OAuth2","phonetic":"/fake/","meaning":"授权协议","level":"Term"},{"word":"oauth2","surface":"OAuth2","meaning":"授权协议","level":"Term"}]"#);
        let (_, items, _) = parser.finish("OAuth2 is a protocol.", "OAuth2 是一种协议。");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].word, "OAuth2");
        assert!(items[0].phonetic.is_empty());
    }
    #[test]
    fn every_split_and_unicode_boundaries_hide_json() {
        let data = r#"[{"word":"resilient","surface":"resilient","context_sentence":"Stay resilient.","context_meaning":"坚韧"}]"#;
        let output = format!("保持坚韧。\n{MARKER}\n{data}");
        for split in output.char_indices().map(|(i, _)| i) {
            let mut p = StreamParser::default();
            let mut visible = p.push(&output[..split]);
            visible += &p.push(&output[split..]);
            let (tail, c, err) = p.finish("Stay resilient.", "保持坚韧。");
            visible += &tail;
            assert_eq!(visible, "保持坚韧。\n");
            assert_eq!(c.len(), 1);
            assert!(err.is_none());
        }
    }
    #[test]
    fn malformed_or_fabricated_metadata_keeps_translation() {
        let mut p = StreamParser::default();
        assert_eq!(p.push("ok---VOCAB_JSON---{broken"), "ok");
        assert!(p.finish("source", "text").2.is_some());
        let mut p = StreamParser::default();
        assert_eq!(p.push("text---VOC"), "text");
        assert_eq!(p.finish("source", "text").0, "---VOC");
    }
    #[test]
    fn compact_json_binds_quoted_source_locally_at_every_chunk_boundary() {
        let source = r#"Dr. Lee said "Stay resilient near C:\work\notes.""#;
        let json = r#"[{"word":"resilient","surface":"resilient","phonetic":"/rɪˈzɪliənt/","pos":"adj.","meaning":"坚韧","level":"B2"}]"#;
        let output = format!("李博士说：保持坚韧。{MARKER}{json}");
        for split in output.char_indices().map(|(i, _)| i) {
            let mut parser = StreamParser::default();
            let mut visible = parser.push(&output[..split]);
            visible += &parser.push(&output[split..]);
            let (_, items, notice) = parser.finish(source, &visible);
            assert!(notice.is_none());
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].context_sentence, source);
            assert_eq!(items[0].context_meaning, "坚韧");
            assert_eq!(items[0].difficulty, "B2");
            assert!(items[0].dict_meaning.is_empty());
            assert_eq!(items[0].sentence_trans, "李博士说：保持坚韧。");
            assert!(!visible.contains("VOCAB"));
        }
    }
    #[test]
    fn trailing_commas_are_repaired_without_touching_quoted_strings() {
        let mut parser = StreamParser::default();
        parser.push(r#"translation---VOCAB_JSON---```json
[{"word":"resilient","surface":"resilient","meaning":"literal ,] and \"quoted\" text","level":"B2",},]
```"#);
        let (_, items, notice) = parser.finish("Stay resilient.", "保持坚韧。");
        assert!(notice.is_none());
        assert_eq!(items.len(), 1);
        assert!(items[0].context_meaning.contains(",]"));
    }
    #[test]
    fn rejects_overflow_ambiguous_and_partial_word_metadata_without_affecting_translation() {
        let mut parser = StreamParser::default();
        assert_eq!(parser.push("kept---VOCAB_JSON---"), "kept");
        parser.push(&" ".repeat(65537));
        parser.push("[]");
        assert!(parser.finish("source", "").2.is_some());
        for source in [
            "The bank closed. Visit the bank tomorrow.",
            "A riverbank can flood.",
        ] {
            let mut parser = StreamParser::default();
            assert_eq!(
                parser.push(
                    r#"译文---VOCAB_JSON---[{"word":"bank","surface":"bank","meaning":"银行"}]"#
                ),
                "译文"
            );
            let (_, items, notice) = parser.finish(source, "译文");
            assert!(items.is_empty());
            assert!(notice.is_some());
        }
    }
    #[test]
    fn modes_are_selected_locally_and_questions_are_not_dictionary_queries() {
        for word in ["resilient", "took off", "state of art", "你好"] {
            assert!(short_query(word));
        }
        for sentence in [
            "How are you?",
            "Do this now!",
            "This is a long sentence",
            "这是完整的中文句子。",
            "stage_envelope_may_include_waiting",
        ] {
            assert!(!short_query(sentence));
        }
    }
}
