use super::*;
use crate::reliability::ServiceVerification;
use std::collections::BTreeMap;

pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS credential_generations(profile_id TEXT PRIMARY KEY,generation TEXT NOT NULL);CREATE TABLE IF NOT EXISTS service_verifications(profile_id TEXT PRIMARY KEY,record_json TEXT NOT NULL);").map_err(db_error)
}
/// Called in the same transaction that commits settings. A random generation is
/// deliberately unrelated to the Keychain value and is safe to persist.
pub(super) fn sync_configuration(
    connection: &Connection,
    old: &Settings,
    new: &Settings,
) -> Result<(), String> {
    for profile in &new.profiles {
        let prior = old.profiles.iter().find(|p| p.id == profile.id);
        if prior.is_none_or(|p| p.revision != profile.revision) {
            connection.execute("INSERT INTO credential_generations(profile_id,generation) VALUES(?1,?2) ON CONFLICT(profile_id) DO UPDATE SET generation=excluded.generation",params![profile.id,uuid::Uuid::new_v4().to_string()]).map_err(db_error)?;
        } else {
            connection.execute("INSERT OR IGNORE INTO credential_generations(profile_id,generation) VALUES(?1,?2)",params![profile.id,uuid::Uuid::new_v4().to_string()]).map_err(db_error)?;
        }
    }
    for profile in &old.profiles {
        if !new.profiles.iter().any(|p| p.id == profile.id) {
            connection
                .execute(
                    "DELETE FROM credential_generations WHERE profile_id=?1",
                    [&profile.id],
                )
                .map_err(db_error)?;
            connection
                .execute(
                    "DELETE FROM service_verifications WHERE profile_id=?1",
                    [&profile.id],
                )
                .map_err(db_error)?;
        }
    }
    Ok(())
}
impl Storage {
    pub async fn service_verification_data(
        &self,
    ) -> Result<
        (
            BTreeMap<String, String>,
            BTreeMap<String, ServiceVerification>,
        ),
        String,
    > {
        self.call(|db| {
            let mut generations = BTreeMap::new();
            let mut query = db
                .connection
                .prepare("SELECT profile_id,generation FROM credential_generations")
                .map_err(db_error)?;
            for row in query
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                .map_err(db_error)?
            {
                let (id, generation) = row.map_err(db_error)?;
                generations.insert(id, generation);
            }
            let mut receipts = BTreeMap::new();
            let mut query = db
                .connection
                .prepare("SELECT record_json FROM service_verifications")
                .map_err(db_error)?;
            for row in query
                .query_map([], |r| r.get::<_, String>(0))
                .map_err(db_error)?
            {
                if let Ok(receipt) =
                    serde_json::from_str::<ServiceVerification>(&row.map_err(db_error)?)
                {
                    receipts.insert(receipt.profile_id.clone(), receipt);
                }
            }
            Ok((generations, receipts))
        })
        .await
    }
    pub async fn save_service_verification(
        &self,
        receipt: ServiceVerification,
    ) -> Result<(), String> {
        self.call(move|db| {
            let current:Option<String>=db.connection.query_row("SELECT generation FROM credential_generations WHERE profile_id=?1",[&receipt.profile_id],|r|r.get(0)).optional().map_err(db_error)?;
            if current.as_deref()!=Some(&receipt.credential_generation) || !db.settings.profiles.iter().any(|p| receipt.matches(p,&receipt.credential_generation)) { return Ok(()); }
            db.connection.execute("INSERT INTO service_verifications(profile_id,record_json) VALUES(?1,?2) ON CONFLICT(profile_id) DO UPDATE SET record_json=excluded.record_json",params![receipt.profile_id,serde_json::to_string(&receipt).map_err(db_error)?]).map_err(db_error)?;Ok(())
        }).await
    }
}
