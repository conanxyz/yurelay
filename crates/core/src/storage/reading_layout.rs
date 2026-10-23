//! Independent reading preferences: changing layout never rewrites service settings.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct ReadingLayout {
    pub source_height: Option<u32>,
    pub translation_height: Option<u32>,
}
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadingPanel {
    Source,
    Translation,
}
fn validate(height: Option<u32>) -> Result<(), String> {
    if height.is_some_and(|h| !(80..=4000).contains(&h)) {
        return Err("阅读区高度须介于 80 和 4000 之间。".into());
    }
    Ok(())
}
fn read(connection: &Connection) -> Result<ReadingLayout, String> {
    let value: Option<String> = connection
        .query_row(
            "SELECT value FROM preferences WHERE key='reading_layout'",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(db_error)?;
    let layout: ReadingLayout = value
        .map(|v| serde_json::from_str(&v).map_err(|_| "无法读取阅读区尺寸设置。".to_string()))
        .transpose()?
        .unwrap_or_default();
    validate(layout.source_height)?;
    validate(layout.translation_height)?;
    Ok(layout)
}
impl Storage {
    pub async fn reading_layout(&self) -> Result<ReadingLayout, String> {
        self.call(|db| read(&db.connection)).await
    }
    pub async fn save_reading_height(
        &self,
        panel: ReadingPanel,
        height: Option<u32>,
    ) -> Result<(), String> {
        validate(height)?;
        self.call(move |db| {
            let mut layout = read(&db.connection)?;
            match panel {
                ReadingPanel::Source => layout.source_height = height,
                ReadingPanel::Translation => layout.translation_height = height,
            }
            let value = serde_json::to_string(&layout).map_err(db_error)?;
            db.connection.execute("INSERT INTO preferences(key,value) VALUES('reading_layout',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[value]).map_err(db_error)?;
            Ok(())
        }).await
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn independently_saved_heights_survive_restart_and_settings_save() {
        let dir = tempfile::tempdir().unwrap();
        let db = Storage::open(dir.path().to_path_buf(), system_clock())
            .await
            .unwrap();
        assert_eq!(db.reading_layout().await.unwrap(), ReadingLayout::default());
        let (a, b) = tokio::join!(
            db.save_reading_height(ReadingPanel::Source, Some(280)),
            db.save_reading_height(ReadingPanel::Translation, Some(420))
        );
        a.unwrap();
        b.unwrap();
        db.save_settings(Settings::default(), vec![]).await.unwrap();
        drop(db);
        let db = Storage::open(dir.path().to_path_buf(), system_clock())
            .await
            .unwrap();
        assert_eq!(
            db.reading_layout().await.unwrap(),
            ReadingLayout {
                source_height: Some(280),
                translation_height: Some(420)
            }
        );
        db.save_reading_height(ReadingPanel::Source, None)
            .await
            .unwrap();
        assert_eq!(
            db.reading_layout().await.unwrap(),
            ReadingLayout {
                source_height: None,
                translation_height: Some(420)
            }
        );
        assert!(
            db.save_reading_height(ReadingPanel::Translation, Some(79))
                .await
                .is_err()
        );
        assert!(
            db.save_reading_height(ReadingPanel::Translation, Some(4001))
                .await
                .is_err()
        );
        assert_eq!(
            db.reading_layout().await.unwrap().translation_height,
            Some(420)
        );
        assert_eq!(db.settings().await.unwrap(), Settings::default());
    }
    #[tokio::test]
    async fn failed_layout_write_is_reported_without_changing_preferences() {
        let dir = tempfile::tempdir().unwrap();
        let db = Storage::open(dir.path().to_path_buf(), system_clock())
            .await
            .unwrap();
        db.save_reading_height(ReadingPanel::Source, Some(200))
            .await
            .unwrap();
        db.reject_writes_for_test().await;
        assert!(
            db.save_reading_height(ReadingPanel::Source, Some(300))
                .await
                .is_err()
        );
        assert_eq!(db.reading_layout().await.unwrap().source_height, Some(200));
    }
}
