use super::*;
use crate::onboarding::{Action, Experience};

pub(super) fn initialize(connection: &Connection) -> Result<(), String> {
    let existing: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM preferences WHERE key='settings') OR EXISTS(SELECT 1 FROM records)", [], |r| r.get(0)).map_err(db_error)?;
    let value = serde_json::to_string(&Experience::initial(existing)).map_err(db_error)?;
    connection
        .execute(
            "INSERT OR IGNORE INTO preferences(key,value) VALUES('experience',?1)",
            [value],
        )
        .map_err(db_error)?;
    Ok(())
}
fn read(connection: &Connection) -> Result<Experience, String> {
    let value: String = connection
        .query_row(
            "SELECT value FROM preferences WHERE key='experience'",
            [],
            |r| r.get(0),
        )
        .map_err(db_error)?;
    serde_json::from_str(&value).map_err(|_| "无法读取使用引导进度。".into())
}
impl Storage {
    pub async fn experience(&self) -> Result<Experience, String> {
        self.call(|db| read(&db.connection)).await
    }
    pub async fn update_experience(&self, action: Action) -> Result<Experience, String> {
        self.call(move |db| {
            let mut progress = read(&db.connection)?;
            progress.apply(action)?;
            let value = serde_json::to_string(&progress).map_err(db_error)?;
            db.connection
                .execute(
                    "UPDATE preferences SET value=?1 WHERE key='experience'",
                    [value],
                )
                .map_err(db_error)?;
            Ok(progress)
        })
        .await
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::onboarding::Progress;
    #[test]
    fn initialization_preserves_upgrade_settings_and_progress() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE preferences(key TEXT PRIMARY KEY,value TEXT NOT NULL);CREATE TABLE records(id TEXT);").unwrap();
        initialize(&db).unwrap();
        assert_eq!(read(&db).unwrap().setup, Progress::New);
        db.execute(
            "UPDATE preferences SET value=?1 WHERE key='experience'",
            [serde_json::to_string(&Experience {
                step: 1,
                setup: Progress::Started,
                ..Experience::initial(false)
            })
            .unwrap()],
        )
        .unwrap();
        initialize(&db).unwrap();
        assert_eq!(read(&db).unwrap().step, 1);
        db.execute_batch(
            "DELETE FROM preferences;INSERT INTO preferences VALUES('settings','unchanged');",
        )
        .unwrap();
        initialize(&db).unwrap();
        assert_eq!(read(&db).unwrap().setup, Progress::Legacy);
        let settings: String = db
            .query_row(
                "SELECT value FROM preferences WHERE key='settings'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(settings, "unchanged");
        db.execute_batch("DELETE FROM preferences;INSERT INTO records VALUES('existing');")
            .unwrap();
        initialize(&db).unwrap();
        assert_eq!(read(&db).unwrap().setup, Progress::Legacy);
    }
    #[tokio::test]
    async fn progress_survives_reopen_without_changing_settings() {
        let dir = tempfile::tempdir().unwrap();
        let db = Storage::open(dir.path().to_path_buf(), system_clock())
            .await
            .unwrap();
        let settings = db.settings().await.unwrap();
        db.update_experience(Action::Step { step: 1 })
            .await
            .unwrap();
        db.update_experience(Action::SkipTutorial).await.unwrap();
        assert!(
            db.update_experience(Action::Step { step: 9 })
                .await
                .is_err()
        );
        drop(db);
        let reopened = Storage::open(dir.path().to_path_buf(), system_clock())
            .await
            .unwrap();
        let e = reopened.experience().await.unwrap();
        assert_eq!(e.step, 1);
        assert_eq!(e.tutorial, Progress::Skipped);
        assert_eq!(reopened.settings().await.unwrap(), settings);
    }
}
