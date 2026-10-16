use super::*;
use crate::vocabulary::{Candidate, Context, VocabularyItem};
use std::collections::HashSet;

impl Storage {
    pub async fn vocabulary(&self) -> Result<Vec<VocabularyItem>, String> {
        self.call(|db| {
            let mut stmt = db
                .connection
                .prepare("SELECT data FROM vocabulary ORDER BY updated_at DESC")
                .map_err(db_error)?;
            let rows = stmt
                .query_map([], |r| r.get::<_, String>(0))
                .map_err(db_error)?;
            rows.map(|row| {
                let mut item: VocabularyItem =
                    serde_json::from_str(&row.map_err(db_error)?).map_err(db_error)?;
                item.normalize_display();
                Ok(item)
            })
            .collect()
        })
        .await
    }
    pub async fn vocabulary_lookup(
        &self,
        words: Vec<String>,
    ) -> Result<Vec<VocabularyItem>, String> {
        if words.iter().any(|w| w.chars().count() > 128) {
            return Err("查询的生词过长。".into());
        }
        self.call(move |db| {
            let mut result = vec![];
            let mut seen = HashSet::new();
            // Reuse a single indexed lookup regardless of list length, without
            // building an IN clause that can exceed SQLite's parameter limit.
            let mut statement = db
                .connection
                .prepare("SELECT data FROM vocabulary WHERE word=?1")
                .map_err(db_error)?;
            for word in words {
                let key = crate::vocabulary::word_key(&word);
                if key.is_empty() || !seen.insert(key.clone()) {
                    continue;
                }
                let data: Option<String> = statement
                    .query_row([key], |r| r.get(0))
                    .optional()
                    .map_err(db_error)?;
                if let Some(data) = data {
                    let mut item: VocabularyItem = serde_json::from_str(&data).map_err(db_error)?;
                    item.normalize_display();
                    result.push(item);
                }
            }
            Ok(result)
        })
        .await
    }
    pub async fn collect_word(&self, mut c: Candidate) -> Result<VocabularyItem, String> {
        c.validate()?;
        self.call(move |db| {
            let now = (db.clock)();
            let old: Option<String> = db
                .connection
                .query_row(
                    "SELECT data FROM vocabulary WHERE word=?1",
                    [crate::vocabulary::word_key(&c.word)],
                    |r| r.get(0),
                )
                .optional()
                .map_err(db_error)?;
            let is_new = old.is_none();
            let mut item = match old {
                Some(data) => serde_json::from_str::<VocabularyItem>(&data).map_err(db_error)?,
                None => VocabularyItem {
                    id: uuid::Uuid::new_v4().to_string(),
                    word: c.word.clone(),
                    phonetic: c.phonetic.clone(),
                    pos: c.pos.clone(),
                    dict_meaning: c.dict_meaning.clone(),
                    note: String::new(),
                    difficulty: c.difficulty.clone(),
                    status: "learning".into(),
                    review_count: 0,
                    contexts: vec![],
                    created_at: now,
                    updated_at: now,
                },
            };
            if crate::vocabulary::technical_word(&c.word, &c.difficulty) {
                item.word = term_case(&item.word, &c.word);
                item.phonetic.clear();
                if c.difficulty.eq_ignore_ascii_case("Term") {
                    item.difficulty = "Term".into();
                }
            }
            item.updated_at = now;
            item.status = "learning".into();
            if item.phonetic.is_empty() {
                item.phonetic = c.phonetic.clone()
            }
            if item.pos.is_empty() {
                item.pos = c.pos.clone()
            }
            if item.dict_meaning.is_empty() {
                item.dict_meaning = c.dict_meaning.clone()
            }
            if item.difficulty.is_empty() {
                item.difficulty = c.difficulty.clone()
            }
            let normalized = crate::vocabulary::context::normalize_whitespace(&c.context_sentence);
            let existing = item.contexts.iter_mut().find(|v| {
                crate::vocabulary::context::normalize_whitespace(&v.context_sentence) == normalized
            });
            let new_context = !normalized.is_empty() && existing.is_none();
            if let Some(context) = existing {
                if context.surface.trim().is_empty() {
                    context.surface = c.surface.clone();
                }
                if context.context_meaning.trim().is_empty() {
                    context.context_meaning = c.context_meaning.clone();
                }
                if context.sentence_trans.trim().is_empty() {
                    context.sentence_trans = c.sentence_trans.clone();
                }
            } else if new_context {
                item.contexts.insert(
                    0,
                    Context {
                        id: uuid::Uuid::new_v4().to_string(),
                        surface: c.surface,
                        context_meaning: c.context_meaning,
                        context_sentence: c.context_sentence,
                        sentence_trans: c.sentence_trans,
                        created_at: now,
                    },
                );
            }
            if is_new || new_context {
                item.review_count = item.review_count.saturating_add(1);
            }
            db.write_word(&item)?;
            Ok(item)
        })
        .await
    }
    pub async fn update_word(
        &self,
        id: String,
        note: Option<String>,
        status: Option<String>,
        remove_context: Option<String>,
    ) -> Result<VocabularyItem, String> {
        if note.as_ref().is_some_and(|s| s.len() > 16000)
            || status
                .as_ref()
                .is_some_and(|s| !matches!(s.as_str(), "learning" | "mastered"))
        {
            return Err("生词状态或笔记无效。".into());
        }
        self.call(move |db| {
            let data: String = db
                .connection
                .query_row("SELECT data FROM vocabulary WHERE id=?1", [&id], |r| {
                    r.get(0)
                })
                .map_err(|_| "生词已删除。".to_string())?;
            let mut item: VocabularyItem = serde_json::from_str(&data).map_err(db_error)?;
            if let Some(note) = note {
                item.note = note
            }
            if let Some(status) = status {
                item.status = status
            }
            if let Some(id) = remove_context {
                item.contexts.retain(|c| c.id != id)
            }
            item.updated_at = (db.clock)();
            db.write_word(&item)?;
            Ok(item)
        })
        .await
    }
    pub async fn remove_word(&self, id: String) -> Result<(), String> {
        self.call(move |db| {
            db.connection
                .execute("DELETE FROM vocabulary WHERE id=?1", [id])
                .map_err(db_error)?;
            Ok(())
        })
        .await
    }
}
impl Database {
    fn write_word(&self, item: &VocabularyItem) -> Result<(), String> {
        self.connection.execute("INSERT INTO vocabulary(id,word,updated_at,data) VALUES(?1,?2,?3,?4) ON CONFLICT(word) DO UPDATE SET updated_at=excluded.updated_at,data=excluded.data",params![item.id,crate::vocabulary::word_key(&item.word),item.updated_at,serde_json::to_string(item).map_err(db_error)?]).map_err(db_error)?;
        Ok(())
    }
}

fn term_case(existing: &str, incoming: &str) -> String {
    if incoming != incoming.to_lowercase() || existing == existing.to_lowercase() {
        incoming.to_owned()
    } else {
        existing.to_owned()
    }
}
fn cell(value: &str) -> String {
    // Spreadsheet formula injection, including leading control/whitespace.
    let safe = if value.trim_start().starts_with(['=', '+', '-', '@']) {
        format!("'{value}")
    } else {
        value.to_string()
    };
    format!("\"{}\"", safe.replace('"', "\"\""))
}
fn plain(value: &str) -> String {
    value.replace(['\t', '\r', '\n'], " ")
}
fn html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
pub fn export(items: &[VocabularyItem], format: &str) -> Result<String, String> {
    match format {
        "csv" => {
            let mut out = String::from(
                "\u{feff}word,phonetic,pos,meaning,status,frequency,note,surface,context_meaning,context_sentence,sentence_translation\r\n",
            );
            for item in items {
                let contexts: Vec<Option<&Context>> = if item.contexts.is_empty() {
                    vec![None]
                } else {
                    item.contexts.iter().map(Some).collect()
                };
                for c in contexts {
                    out += &[
                        item.word.clone(),
                        item.phonetic.clone(),
                        item.pos.clone(),
                        item.dict_meaning.clone(),
                        item.status.clone(),
                        item.review_count.to_string(),
                        item.note.clone(),
                        c.map(|c| c.surface.clone()).unwrap_or_default(),
                        c.map(|c| c.context_meaning.clone()).unwrap_or_default(),
                        c.map(|c| c.context_sentence.clone()).unwrap_or_default(),
                        c.map(|c| c.sentence_trans.clone()).unwrap_or_default(),
                    ]
                    .iter()
                    .map(|s| cell(s))
                    .collect::<Vec<_>>()
                    .join(",");
                    out += "\r\n";
                }
            }
            Ok(out)
        }
        "markdown" => Ok(items
            .iter()
            .map(|i| {
                format!(
                    "## {}\n\n{} · {} · {}\n\n{}\n\n笔记：{}\n\n{}\n",
                    plain(&html(&i.word)),
                    html(&i.phonetic),
                    html(&i.pos),
                    html(&i.status),
                    html(&i.dict_meaning),
                    html(&i.note),
                    i.contexts
                        .iter()
                        .map(|c| format!(
                            "> {}\n\n{}\n\n{}\n",
                            plain(&html(&c.context_sentence)),
                            html(&c.context_meaning),
                            html(&c.sentence_trans)
                        ))
                        .collect::<String>()
                )
            })
            .collect()),
        "anki" => Ok(items
            .iter()
            .map(|i| {
                format!(
                    "{}\t{}\t{}\n",
                    plain(&i.word),
                    plain(&format!("{} {} {}", i.phonetic, i.pos, i.dict_meaning)),
                    plain(
                        &i.contexts
                            .iter()
                            .map(|c| format!("{} — {}", c.context_sentence, c.sentence_trans))
                            .collect::<Vec<_>>()
                            .join(" | ")
                    )
                )
            })
            .collect()),
        _ => Err("不支持的导出格式。".into()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn lookup_handles_unbounded_counts_and_returns_unique_matches_in_request_order() {
        let root = tempfile::tempdir().unwrap();
        let storage = Storage::open(root.path().into(), system_clock())
            .await
            .unwrap();
        let mut stored = vec![];
        for index in 0..12 {
            stored.push(
                storage
                    .collect_word(Candidate {
                        word: format!("fixture{index}"),
                        dict_meaning: "Synthetic definition".into(),
                        ..Default::default()
                    })
                    .await
                    .unwrap(),
            );
        }
        let mut words: Vec<_> = (0..1100).map(|i| format!("unknown{i}")).collect();
        words.extend(
            stored
                .iter()
                .rev()
                .map(|item| format!("  {}  ", item.word.to_uppercase())),
        );
        words.extend(stored.iter().map(|item| item.word.clone()));
        words.extend(["   ".into(), "x' OR 1=1 --".into()]);
        let found = storage.vocabulary_lookup(words).await.unwrap();
        assert_eq!(
            found.iter().map(|item| &item.id).collect::<Vec<_>>(),
            stored.iter().rev().map(|item| &item.id).collect::<Vec<_>>()
        );
        assert!(storage.vocabulary_lookup(vec![]).await.unwrap().is_empty());
        assert_eq!(storage.vocabulary().await.unwrap().len(), 12);
        assert!(
            storage
                .vocabulary_lookup(vec!["x".repeat(129)])
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn legacy_lowercase_terms_merge_without_losing_notes_or_context_counts() {
        let root = tempfile::tempdir().unwrap();
        let storage = Storage::open(root.path().into(), system_clock())
            .await
            .unwrap();
        let legacy = storage
            .collect_word(Candidate {
                word: "oauth2".into(),
                surface: "oauth2".into(),
                context_sentence: "Use oauth2.".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        storage
            .update_word(
                legacy.id.clone(),
                Some("Keep this note".into()),
                Some("mastered".into()),
                None,
            )
            .await
            .unwrap();
        let next = storage
            .collect_word(Candidate {
                word: "OAuth2".into(),
                surface: "OAuth2".into(),
                context_sentence: "OAuth2 protects access.".into(),
                difficulty: "Term".into(),
                phonetic: "/fake/".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(next.id, legacy.id);
        assert_eq!(next.word, "OAuth2");
        assert_eq!(next.review_count, 2);
        assert_eq!(next.contexts.len(), 2);
        assert_eq!(next.note, "Keep this note");
        assert!(next.phonetic.is_empty());
        let again = storage
            .collect_word(Candidate {
                word: "oauth2".into(),
                surface: "OAuth2".into(),
                context_sentence: "OAuth2 protects access.".into(),
                difficulty: "Term".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(again.review_count, 2);
        assert_eq!(again.word, "OAuth2");
        assert_eq!(storage.vocabulary().await.unwrap().len(), 1);
        assert_eq!(
            storage
                .vocabulary_lookup(vec!["OAUTH2".into()])
                .await
                .unwrap()[0]
                .id,
            legacy.id
        );
        assert!(
            export(&storage.vocabulary().await.unwrap(), "csv")
                .unwrap()
                .contains("OAuth2")
        );
        storage.flush().await.unwrap();
        drop(storage);
        let reopened = Storage::open(root.path().into(), system_clock())
            .await
            .unwrap();
        assert_eq!(reopened.vocabulary().await.unwrap()[0].word, "OAuth2");
    }
    #[tokio::test]
    async fn merge_contexts_reopen_update_and_export() {
        let temp = tempfile::tempdir().unwrap();
        let db = Storage::open(temp.path().into(), system_clock())
            .await
            .unwrap();
        let c = Candidate {
            word: " Resilient ".into(),
            surface: "resilient".into(),
            context_sentence: "Stay resilient.".into(),
            dict_meaning: "坚韧".into(),
            ..Default::default()
        };
        let a = db.collect_word(c.clone()).await.unwrap();
        let b = db.collect_word(c.clone()).await.unwrap();
        assert_eq!(a.id, b.id);
        assert_eq!(b.review_count, 1);
        assert_eq!(b.contexts.len(), 1);
        let c = Candidate {
            context_sentence: "A resilient team.".into(),
            ..c
        };
        let c = db.collect_word(c).await.unwrap();
        assert_eq!(c.contexts.len(), 2);
        db.update_word(
            c.id.clone(),
            Some("=formula".into()),
            Some("mastered".into()),
            None,
        )
        .await
        .unwrap();
        db.flush().await.unwrap();
        drop(db);
        let db = Storage::open(temp.path().into(), system_clock())
            .await
            .unwrap();
        let items = db.vocabulary().await.unwrap();
        assert_eq!(items[0].status, "mastered");
        assert_eq!(items[0].review_count, 2);
        assert!(export(&items, "csv").unwrap().contains("'=formula"));
        assert!(export(&items, "anki").unwrap().contains('\t'));
        db.update_word(c.id.clone(), None, None, Some(c.contexts[0].id.clone()))
            .await
            .unwrap();
        assert_eq!(db.vocabulary().await.unwrap()[0].contexts.len(), 1);
        db.remove_word(c.id).await.unwrap();
        assert!(db.vocabulary().await.unwrap().is_empty());
    }
    #[tokio::test]
    async fn repeated_context_is_idempotent_and_recollecting_reactivates_mastered_word() {
        let root = tempfile::tempdir().unwrap();
        let storage = Storage::open(root.path().into(), system_clock())
            .await
            .unwrap();
        let c = Candidate {
            word: "resilient".into(),
            surface: "resilient".into(),
            context_sentence: "A resilient\n team.".into(),
            ..Default::default()
        };
        let first = storage.collect_word(c.clone()).await.unwrap();
        storage
            .update_word(
                first.id.clone(),
                Some("Keep my note".into()),
                Some("mastered".into()),
                None,
            )
            .await
            .unwrap();
        let next = storage
            .collect_word(Candidate {
                context_sentence: " A resilient   team. ".into(),
                phonetic: "/test/".into(),
                context_meaning: "坚韧".into(),
                sentence_trans: "坚韧的团队。".into(),
                ..c
            })
            .await
            .unwrap();
        assert_eq!(next.review_count, 1);
        assert_eq!(next.contexts.len(), 1);
        assert_eq!(next.status, "learning");
        assert_eq!(next.note, "Keep my note");
        assert_eq!(next.contexts[0].context_meaning, "坚韧");
        assert_eq!(next.contexts[0].sentence_trans, "坚韧的团队。");
        assert_eq!(
            storage
                .vocabulary_lookup(vec![" RESILIENT ".into()])
                .await
                .unwrap()[0]
                .id,
            next.id
        );
        assert!(
            storage
                .vocabulary_lookup(vec!["unknown".into()])
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            storage
                .vocabulary_lookup(vec!["word".into(); 6])
                .await
                .unwrap()
                .is_empty()
        );
    }
}
