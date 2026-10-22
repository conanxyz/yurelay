//! The history list carries previews; full documents are fetched by id on demand.
use super::*;
use crate::tasks::Source;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistorySummary {
    pub id: String,
    pub revision: u64,
    pub source: Source,
    pub source_kinds: Vec<Source>,
    pub status: Status,
    pub original_text: String,
    pub created_at: i64,
}
impl Storage {
    pub async fn search_history(
        &self,
        query: String,
        source: String,
        offset: usize,
    ) -> Result<Vec<HistorySummary>, String> {
        if query.len() > 512
            || !matches!(source.as_str(), "" | "selection" | "screenshot" | "manual")
        {
            return Err("历史筛选条件无效。".into());
        }
        self.call(move|db| {
            let mut statement=db.connection.prepare(
                "SELECT id,revision,status,created_at,json_extract(data,'$.source'),
                 substr(COALESCE(json_extract(data,'$.original_text'),''),1,240),
                 (SELECT json_group_array(DISTINCT json_extract(value,'$.kind')) FROM json_each(data,'$.source_segments'))
                 FROM records WHERE (expires_at IS NULL OR expires_at>?1)
                 AND (?2='' OR json_extract(data,'$.source')=?2 OR EXISTS(SELECT 1 FROM json_each(data,'$.source_segments') WHERE json_extract(value,'$.kind')=?2))
                 AND (?3='' OR instr(lower(json_extract(data,'$.original_text')),lower(?3))>0 OR instr(lower(json_extract(data,'$.translation')),lower(?3))>0)
                 ORDER BY created_at DESC,id DESC LIMIT 30 OFFSET ?4"
            ).map_err(db_error)?;
            let rows=statement.query_map(params![(db.clock)(),source,query,offset],|row|Ok((row.get::<_,String>(0)?,row.get::<_,u64>(1)?,row.get::<_,String>(2)?,row.get::<_,i64>(3)?,row.get::<_,String>(4)?,row.get::<_,String>(5)?,row.get::<_,String>(6)?))).map_err(db_error)?;
            rows.map(|row|{let (id,revision,status,created_at,source,original_text,kinds)=row.map_err(db_error)?;let source:Source=serde_json::from_value(serde_json::Value::String(source)).map_err(db_error)?;let mut source_kinds:Vec<Source>=serde_json::from_str(&kinds).map_err(db_error)?;if source_kinds.is_empty(){source_kinds.push(source);}Ok(HistorySummary{id,revision,source,source_kinds,status:serde_json::from_value(serde_json::Value::String(status)).map_err(db_error)?,original_text,created_at})}).collect()
        }).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::Profile,
        source::SourceSegment,
        tasks::{Source, TaskBook},
    };
    #[tokio::test]
    async fn summary_paging_search_filters_and_delete_preserve_full_details() {
        let root = tempfile::tempdir().unwrap();
        let storage = Storage::open(root.path().into(), Arc::new(|| 1000))
            .await
            .unwrap();
        let book = TaskBook::new(Retention::Forever, 12);
        let task = book
            .create(Source::Selection, Profile::default(), "zh-Hans".into(), 0)
            .unwrap();
        book.recognized(&task.id, "😀".repeat(300) + " source-tail", 0, 1)
            .unwrap();
        let mut task = book.ready(&task.id, Some("en".into()), 2).unwrap();
        task.translation = "Full translation searchable needle".into();
        task.status = Status::Completed;
        task.source_segments.push(SourceSegment {
            id: uuid::Uuid::new_v4().to_string(),
            kind: Source::Screenshot,
            original_text: "Synthetic screenshot text".into(),
            captured_at: 2,
            asset_id: None,
            ocr_layout: None,
        });
        let original = task.original_text.clone();
        let first_id = task.id.clone();
        storage.call(move|db|{let tx=db.connection.transaction().map_err(db_error)?;for index in 0..32 {let mut item=task.clone();if index>0 {item.id=uuid::Uuid::new_v4().to_string();}tx.execute("INSERT INTO records(id,created_at,expires_at,revision,status,data) VALUES(?1,?2,NULL,1,'completed',?3)",params![item.id,index,serde_json::to_string(&item).map_err(db_error)?]).map_err(db_error)?;}tx.commit().map_err(db_error)?;Ok(())}).await.unwrap();
        let first = storage
            .search_history(String::new(), String::new(), 0)
            .await
            .unwrap();
        let second = storage
            .search_history(String::new(), String::new(), 30)
            .await
            .unwrap();
        assert_eq!(first.len(), 30);
        assert_eq!(second.len(), 2);
        assert!(first.iter().all(|a| second.iter().all(|b| a.id != b.id)));
        assert_eq!(first[0].original_text.chars().count(), 240);
        assert!(first[0].source_kinds.contains(&Source::Screenshot));
        for query in ["source-tail", "searchable needle"] {
            assert_eq!(
                storage
                    .search_history(query.into(), "screenshot".into(), 0)
                    .await
                    .unwrap()
                    .len(),
                30
            );
        }
        assert!(
            storage
                .search_history(String::new(), "manual".into(), 0)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            storage
                .task(first_id.clone())
                .await
                .unwrap()
                .unwrap()
                .original_text,
            original
        );
        let id = first[0].id.clone();
        storage.remove(vec![id.clone()]).await.unwrap();
        assert!(
            !storage
                .search_history(String::new(), String::new(), 0)
                .await
                .unwrap()
                .iter()
                .any(|item| item.id == id)
        );
        let id = first_id.clone();
        storage
            .call(move |db| {
                db.connection
                    .execute("UPDATE records SET expires_at=999 WHERE id=?1", [id])
                    .map_err(db_error)?;
                Ok(())
            })
            .await
            .unwrap();
        assert_eq!(
            storage
                .search_history(String::new(), String::new(), 30)
                .await
                .unwrap()
                .len(),
            0
        );
        assert!(storage.task(first_id).await.unwrap().is_none());
    }
}
