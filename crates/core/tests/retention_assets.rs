use std::{collections::BTreeMap, sync::Arc};
use yurelay_core::{
    config::Retention,
    engine::{Credentials, Engine},
    source::SourceRequest,
    tasks::{SaveState, Source, Task},
};

struct FakeCredentials;
impl Credentials for FakeCredentials {
    fn exists(&self, _: &str) -> Result<bool, String> {
        Ok(false)
    }
    fn read(&self, _: &str) -> Result<Option<String>, String> {
        Ok(None)
    }
    fn write(&self, _: &str, _: Option<&str>) -> Result<(), String> {
        Ok(())
    }
}

fn synthetic_png() -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    bytes.extend(2u32.to_be_bytes());
    bytes.extend(2u32.to_be_bytes());
    bytes
}

async fn open(path: &std::path::Path) -> Arc<Engine> {
    Engine::open(path.into(), Arc::new(FakeCredentials), Arc::new(|| 100))
        .await
        .unwrap()
}

async fn append_image(engine: &Arc<Engine>, task: &Task) -> Task {
    engine
        .source_append(
            SourceRequest {
                task_id: task.id.clone(),
                expected_revision: task.source_revision,
            },
            Source::Screenshot,
            "Synthetic appended screenshot.".into(),
            Some(synthetic_png()),
            None,
        )
        .await
        .unwrap()
        .unwrap()
}

async fn disable_history_and_check(engine: &Arc<Engine>, task: &Task, path: &std::path::Path) {
    let assets = engine.source_assets(&task.id).await.unwrap();
    assert_eq!(assets.len(), 2);
    let mut settings = engine.configuration().await.settings;
    settings.retention = Retention::Disabled;
    engine
        .save_configuration(settings, BTreeMap::new())
        .await
        .unwrap();
    let retained = engine.book.get(&task.id).expect("current draft survives");
    assert_eq!(retained.query_text(), task.query_text());
    assert_eq!(retained.source_segments, task.source_segments);
    assert_eq!(retained.save_state, SaveState::MemoryOnly);
    for asset in assets {
        assert_eq!(
            engine
                .source_asset(&task.id, &asset.id)
                .await
                .unwrap()
                .expect("each screenshot survives in the session")
                .as_ref(),
            &synthetic_png()
        );
    }
    assert!(
        engine
            .storage
            .task(task.id.clone())
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(std::fs::read_dir(path.join("images")).unwrap().count(), 0);
    engine.clear().await.unwrap();
    assert!(engine.book.get(&task.id).is_none());
    assert!(engine.source_assets(&task.id).await.is_err());
}

#[tokio::test]
async fn disabling_history_keeps_selection_and_appended_images_in_session() {
    let root = tempfile::tempdir().unwrap();
    let engine = open(root.path()).await;
    let handle = engine.begin(Source::Selection).await.unwrap();
    engine.accept_text(&handle, "Synthetic original selection.".into(), 1);
    let task = engine.book.get(&handle.id).unwrap();
    let task = append_image(&engine, &task).await;
    let task = append_image(&engine, &task).await;
    disable_history_and_check(&engine, &task, root.path()).await;
    engine.shutdown().await;
}

#[tokio::test]
async fn disabling_history_loads_legacy_and_appended_images_after_restart() {
    let root = tempfile::tempdir().unwrap();
    let engine = open(root.path()).await;
    let handle = engine.begin(Source::Screenshot).await.unwrap();
    engine.accept_text(&handle, "Synthetic original screenshot.".into(), 1);
    engine.accept_image(&handle.id, synthetic_png());
    engine.retry_save(&handle.id).await.unwrap();
    let task = engine.book.get(&handle.id).unwrap();
    let task = append_image(&engine, &task).await;
    engine.shutdown().await;
    drop(engine);

    let engine = open(root.path()).await;
    let task = engine.load_into_book(&task.id).await.unwrap();
    disable_history_and_check(&engine, &task, root.path()).await;
    engine.shutdown().await;
}
