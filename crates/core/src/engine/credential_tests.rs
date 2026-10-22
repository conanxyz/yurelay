use super::*;

#[derive(Default)]
struct CountingCredentials {
    values: Mutex<BTreeMap<String, String>>,
    reads: Mutex<Vec<String>>,
    denied: Mutex<HashSet<String>>,
    authorizations: Mutex<Vec<String>>,
    denied_authorizations: Mutex<HashSet<String>>,
    failed_writes: Mutex<HashSet<String>>,
    gate: Mutex<Option<Arc<ReadGate>>>,
}

#[derive(Default)]
struct ReadGate {
    started: Notify,
    released: Mutex<bool>,
    wake: std::sync::Condvar,
}
impl ReadGate {
    fn release(&self) {
        *self.released.lock().unwrap() = true;
        self.wake.notify_all();
    }
}

impl Credentials for CountingCredentials {
    fn exists(&self, id: &str) -> Result<bool, String> {
        Ok(self.values.lock().unwrap().contains_key(id))
    }
    fn read(&self, id: &str) -> Result<Option<String>, String> {
        self.reads.lock().unwrap().push(id.into());
        let gate = self.gate.lock().unwrap().clone();
        if let Some(gate) = gate {
            gate.started.notify_one();
            let _released = gate
                .wake
                .wait_while(gate.released.lock().unwrap(), |released| !*released)
                .unwrap();
        }
        if self.denied.lock().unwrap().contains(id) {
            return Err("Synthetic denied credential".into());
        }
        Ok(self.values.lock().unwrap().get(id).cloned())
    }

    fn write(&self, id: &str, value: Option<&str>) -> Result<(), String> {
        if self.failed_writes.lock().unwrap().contains(id) {
            return Err("Synthetic failed credential write".into());
        }
        let mut values = self.values.lock().unwrap();
        if let Some(value) = value {
            values.insert(id.into(), value.into());
        } else {
            values.remove(id);
        }
        Ok(())
    }

    fn read_for_update(&self, id: &str) -> Result<Option<String>, String> {
        self.authorizations.lock().unwrap().push(id.into());
        if self.denied_authorizations.lock().unwrap().contains(id) {
            return Err("Synthetic cancelled authorization".into());
        }
        self.denied.lock().unwrap().remove(id);
        self.read(id)
    }
}

async fn configured_engine() -> (tempfile::TempDir, Arc<CountingCredentials>, Arc<Engine>) {
    configured_engine_with_active(false).await
}

async fn configured_engine_with_active(
    dictionary: bool,
) -> (tempfile::TempDir, Arc<CountingCredentials>, Arc<Engine>) {
    let directory = tempfile::tempdir().unwrap();
    let clock: Clock = Arc::new(|| 100);
    let storage = Storage::open(directory.path().into(), clock.clone())
        .await
        .unwrap();
    let mut settings = storage.settings().await.unwrap();
    settings.profiles = ["a", "b", "c"]
        .into_iter()
        .map(|id| Profile {
            id: id.into(),
            endpoint: "http://localhost:9/v1".into(),
            model: "synthetic-model".into(),
            ..Profile::default()
        })
        .collect();
    settings.active_profile = "a".into();
    if dictionary {
        settings.profiles[0].provider = crate::config::Provider::SystemDictionary;
        settings.profiles[0].endpoint.clear();
        settings.profiles[0].model.clear();
    }
    storage.save_settings(settings, vec![]).await.unwrap();
    drop(storage);
    let credentials = Arc::new(CountingCredentials::default());
    for id in ["a", "b", "c"] {
        credentials
            .write(id, Some("synthetic-fixture-key"))
            .unwrap();
    }
    let engine = Engine::open(directory.path().into(), credentials.clone(), clock)
        .await
        .unwrap();
    (directory, credentials, engine)
}

#[tokio::test]
async fn concurrent_captures_share_one_deferred_credential_read() {
    let (_directory, credentials, engine) = configured_engine().await;
    let results = futures_util::future::join_all(
        (0..8).map(|_| engine.begin_with_profile(Source::Manual, Some("b"))),
    )
    .await;
    for result in results {
        assert_eq!(result.unwrap().key.as_str(), "synthetic-fixture-key");
    }
    assert_eq!(*credentials.reads.lock().unwrap(), ["a", "b"]);
}

#[tokio::test]
async fn cancelled_caller_rejoins_the_pending_read_without_another_prompt() {
    let (_directory, credentials, engine) = configured_engine().await;
    let gate = Arc::new(ReadGate::default());
    *credentials.gate.lock().unwrap() = Some(gate.clone());
    let first_engine = engine.clone();
    let first = tokio::spawn(async move {
        first_engine
            .begin_with_profile(Source::Manual, Some("b"))
            .await
    });
    let started = tokio::time::timeout(Duration::from_secs(2), gate.started.notified()).await;
    if started.is_err() {
        gate.release();
        panic!("credential read did not start");
    }
    first.abort();
    assert!(matches!(first.await, Err(error) if error.is_cancelled()));
    gate.release();
    let second = engine
        .begin_with_profile(Source::Manual, Some("b"))
        .await
        .unwrap();
    assert_eq!(second.key.as_str(), "synthetic-fixture-key");
    assert_eq!(*credentials.reads.lock().unwrap(), ["a", "b"]);
}

#[tokio::test]
async fn denied_and_missing_credentials_are_not_retried_on_each_capture() {
    let (_directory, credentials, engine) = configured_engine().await;
    credentials.denied.lock().unwrap().insert("b".into());
    credentials.values.lock().unwrap().remove("c");
    for _ in 0..3 {
        let denied = engine
            .begin_with_profile(Source::Manual, Some("b"))
            .await
            .unwrap();
        assert!(denied.credential_error.is_some());
        let missing = engine
            .begin_with_profile(Source::Manual, Some("c"))
            .await
            .unwrap();
        assert!(missing.key.is_empty());
    }
    assert_eq!(*credentials.reads.lock().unwrap(), ["a", "b", "c"]);
    assert_eq!(engine.configuration().await.has_keys, ["a", "b"]);
    assert!(credentials.authorizations.lock().unwrap().is_empty());
}

#[tokio::test]
async fn only_explicit_key_save_can_authorize_and_then_refreshes_the_cache() {
    let (directory, credentials, engine) = configured_engine().await;
    engine.shutdown().await;
    drop(engine);
    credentials
        .denied
        .lock()
        .unwrap()
        .extend(["a".into(), "b".into(), "c".into()]);
    credentials.reads.lock().unwrap().clear();
    let engine = Engine::open(
        directory.path().into(),
        credentials.clone(),
        Arc::new(|| 100),
    )
    .await
    .unwrap();
    assert!(
        engine
            .configuration()
            .await
            .credential_errors
            .contains_key("a")
    );
    assert!(credentials.authorizations.lock().unwrap().is_empty());
    let mut settings = engine.configuration().await.settings;
    settings.auto_clean = !settings.auto_clean;
    engine
        .save_configuration(settings.clone(), BTreeMap::new())
        .await
        .unwrap();
    let denied = engine.begin(Source::Manual).await.unwrap();
    assert!(denied.credential_error.is_some());
    assert!(credentials.authorizations.lock().unwrap().is_empty());
    engine
        .save_configuration(
            settings,
            BTreeMap::from([("a".into(), Some("synthetic-new-key".into()))]),
        )
        .await
        .unwrap();
    let ready = engine.begin(Source::Manual).await.unwrap();
    assert_eq!(ready.key.as_str(), "synthetic-new-key");
    assert!(ready.credential_error.is_none());
    assert_eq!(*credentials.authorizations.lock().unwrap(), ["a"]);
    assert_eq!(*credentials.reads.lock().unwrap(), ["a", "a"]);
    assert_eq!(
        credentials.values.lock().unwrap()["b"],
        "synthetic-fixture-key"
    );
}

#[tokio::test]
async fn cancelled_save_authorization_preserves_the_previous_key_and_settings() {
    let (_directory, credentials, engine) = configured_engine().await;
    credentials.denied.lock().unwrap().insert("b".into());
    credentials
        .denied_authorizations
        .lock()
        .unwrap()
        .insert("b".into());
    let mut settings = engine.configuration().await.settings;
    let previous = settings.auto_clean;
    settings.auto_clean = !previous;
    let result = engine
        .save_configuration(
            settings,
            BTreeMap::from([("b".into(), Some("synthetic-new-key".into()))]),
        )
        .await;
    assert!(result.is_err());
    assert_eq!(engine.configuration().await.settings.auto_clean, previous);
    assert_eq!(
        credentials.values.lock().unwrap()["b"],
        "synthetic-fixture-key"
    );
    assert_eq!(*credentials.authorizations.lock().unwrap(), ["b"]);
    let denied = engine
        .begin_with_profile(Source::Manual, Some("b"))
        .await
        .unwrap();
    assert!(denied.credential_error.is_some());
    assert_eq!(*credentials.authorizations.lock().unwrap(), ["b"]);
}

#[tokio::test]
async fn backup_enumeration_does_not_unlock_unused_services() {
    let (_directory, credentials, engine) = configured_engine().await;
    let mut settings = engine.configuration().await.settings;
    settings.failover.enabled = true;
    let initial = settings.profiles[0].clone();
    engine
        .save_configuration(settings, BTreeMap::new())
        .await
        .unwrap();
    let mut dispatch = engine.dispatch_profiles(&initial, false, false).await;
    assert_eq!(dispatch.len(), 3);
    assert_eq!(*credentials.reads.lock().unwrap(), ["a"]);
    assert!(engine.prepare_backup(&mut dispatch[1], false).await);
    assert!(engine.prepare_backup(&mut dispatch[1], false).await);
    assert_eq!(dispatch[1].key.as_str(), "synthetic-fixture-key");
    assert_eq!(*credentials.reads.lock().unwrap(), ["a", "b"]);
}

#[tokio::test]
async fn key_replacement_updates_the_cache_and_deletion_removes_deferred_keys() {
    let (_directory, credentials, engine) = configured_engine().await;
    let mut settings = engine.configuration().await.settings;
    settings.profiles.retain(|p| p.id != "c");
    engine
        .save_configuration(
            settings,
            BTreeMap::from([("b".into(), Some("synthetic-new-key".into()))]),
        )
        .await
        .unwrap();
    let handle = engine
        .begin_with_profile(Source::Manual, Some("b"))
        .await
        .unwrap();
    assert_eq!(handle.key.as_str(), "synthetic-new-key");
    assert!(!credentials.values.lock().unwrap().contains_key("c"));
    assert_eq!(engine.configuration().await.has_keys, ["a", "b"]);
    assert_eq!(*credentials.reads.lock().unwrap(), ["a", "b", "c"]);
}

#[tokio::test]
async fn failed_update_restores_the_previous_deferred_secret() {
    let (_directory, credentials, engine) = configured_engine().await;
    credentials.failed_writes.lock().unwrap().insert("c".into());
    let settings = engine.configuration().await.settings;
    let result = engine
        .save_configuration(
            settings,
            BTreeMap::from([
                ("b".into(), Some("synthetic-new-key".into())),
                ("c".into(), Some("synthetic-other-key".into())),
            ]),
        )
        .await;
    assert!(result.is_err());
    assert_eq!(
        credentials.values.lock().unwrap()["b"],
        "synthetic-fixture-key"
    );
    assert_eq!(*credentials.reads.lock().unwrap(), ["a", "b", "c"]);
    let handle = engine
        .begin_with_profile(Source::Manual, Some("b"))
        .await
        .unwrap();
    assert!(handle.credential_error.is_some());
}

#[tokio::test]
async fn system_dictionary_startup_does_not_read_any_secret() {
    let (_directory, credentials, engine) = configured_engine_with_active(true).await;
    let _handle = engine.begin(Source::Manual).await.unwrap();
    assert!(credentials.reads.lock().unwrap().is_empty());
}

#[tokio::test]
async fn connection_and_model_queries_load_only_the_selected_saved_key() {
    let (_directory, credentials, engine) = configured_engine().await;
    let mut profile = engine.configuration().await.settings.profiles[1].clone();
    profile.endpoint = "invalid-url".into();
    assert!(engine.models(profile.clone(), None).await.is_err());
    assert!(engine.test_connection(profile, None).await.is_err());
    let mut other = engine.configuration().await.settings.profiles[2].clone();
    other.endpoint = "invalid-url".into();
    assert!(
        engine
            .test_connection(other, Some("synthetic-override".into()))
            .await
            .is_err()
    );
    assert_eq!(*credentials.reads.lock().unwrap(), ["a", "b"]);
}

#[tokio::test]
async fn startup_reads_only_active_credential_and_configuration_does_not_unlock_others() {
    let (_directory, credentials, engine) = configured_engine().await;
    assert_eq!(*credentials.reads.lock().unwrap(), ["a"]);
    for _ in 0..5 {
        assert_eq!(engine.configuration().await.has_keys, ["a", "b", "c"]);
        engine.service_verifications().await.unwrap();
        engine.dictionary_guidance("word").await;
    }
    assert_eq!(*credentials.reads.lock().unwrap(), ["a"]);
}

#[tokio::test]
async fn translation_before_capture_ready_does_not_read_another_service_key() {
    let (_directory, credentials, engine) = configured_engine().await;
    for source in [Source::Selection, Source::Screenshot] {
        let handle = engine.begin(source).await.unwrap();
        assert!(
            engine
                .retry_languages_with_profile(
                    &handle.id,
                    Some("Partial synthetic source".into()),
                    None,
                    None,
                    Some("b")
                )
                .await
                .is_err()
        );
    }
    assert_eq!(*credentials.reads.lock().unwrap(), ["a"]);
}

#[tokio::test]
async fn saved_key_can_be_authorized_without_reentering_or_writing_it() {
    let (_directory, credentials, engine) = configured_engine().await;
    credentials.denied.lock().unwrap().insert("b".into());
    let before = credentials.values.lock().unwrap().clone();
    let settings = serde_json::to_value(engine.configuration().await.settings).unwrap();
    assert!(
        engine
            .credential_authorization_required(Some("b"))
            .await
            .unwrap()
    );
    assert!(credentials.authorizations.lock().unwrap().is_empty());
    let (first, second) = tokio::join!(
        engine.authorize_saved_credential("b"),
        engine.authorize_saved_credential("b")
    );
    first.unwrap();
    second.unwrap();
    assert!(
        !engine
            .credential_authorization_required(Some("b"))
            .await
            .unwrap()
    );
    assert_eq!(*credentials.authorizations.lock().unwrap(), ["b"]);
    assert_eq!(*credentials.values.lock().unwrap(), before);
    assert_eq!(
        serde_json::to_value(engine.configuration().await.settings).unwrap(),
        settings
    );
    let ready = engine
        .begin_with_profile(Source::Manual, Some("b"))
        .await
        .unwrap();
    assert!(ready.credential_error.is_none());
    assert_eq!(ready.key.as_str(), "synthetic-fixture-key");
}

#[tokio::test]
async fn cancelled_saved_key_authorization_never_retries_automatically_or_changes_the_key() {
    let (_directory, credentials, engine) = configured_engine().await;
    credentials.denied.lock().unwrap().insert("b".into());
    credentials
        .denied_authorizations
        .lock()
        .unwrap()
        .insert("b".into());
    assert!(engine.authorize_saved_credential("b").await.is_err());
    for _ in 0..3 {
        assert!(
            engine
                .credential_authorization_required(Some("b"))
                .await
                .unwrap()
        );
    }
    assert!(
        engine
            .authorize_saved_credential("removed-profile")
            .await
            .is_err()
    );
    assert_eq!(*credentials.authorizations.lock().unwrap(), ["b"]);
    assert_eq!(
        credentials.values.lock().unwrap()["b"],
        "synthetic-fixture-key"
    );
}
