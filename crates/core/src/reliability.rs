//! Safe service verification and an explicitly authorized, bounded dispatch policy.
use crate::{
    config::{Profile, Provider},
    provider::{self, Error},
};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, time::Duration};
use tokio::time::Instant;

pub const VERIFICATION_TTL_MS: i64 = 7 * 86_400_000;
pub const MAX_ATTEMPTS: usize = 3;
pub const OPERATION_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FailoverSettings {
    pub enabled: bool,
    pub profile_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
    Cancelled,
    Configuration,
    Connection,
    Timeout,
    RateLimit,
    Server,
    Quota,
    Authentication,
    Protocol,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailureSummary {
    pub class: FailureClass,
    pub status: Option<u16>,
    pub provider_code: Option<String>,
    pub retry_after_seconds: Option<u64>,
}
impl FailureSummary {
    pub fn from_error(error: &Error) -> Self {
        let (class, status, code, retry) = match error {
            Error::Cancelled => (FailureClass::Cancelled, None, None, None),
            Error::Connection => (FailureClass::Connection, None, None, None),
            Error::Timeout | Error::FirstContentTimeout | Error::IdleTimeout => {
                (FailureClass::Timeout, None, None, None)
            }
            Error::Configuration(_) | Error::TlsOrConfiguration => {
                (FailureClass::Configuration, None, None, None)
            }
            Error::Http(s) => (http_class(*s), Some(*s), None, None),
            Error::Service {
                status,
                code,
                retry_after_seconds,
                quota,
            } => (
                if *quota {
                    FailureClass::Quota
                } else {
                    http_class(*status)
                },
                Some(*status),
                code.clone(),
                *retry_after_seconds,
            ),
            Error::Zhipu {
                status,
                code,
                retry_after_seconds,
                ..
            } => (
                match code {
                    1113 | 1308 | 1310 => FailureClass::Quota,
                    1302 => FailureClass::RateLimit,
                    1305 => FailureClass::Server,
                    1000 | 1001 | 1003 | 1005 | 1220 | 1311 | 1315 => FailureClass::Authentication,
                    _ => FailureClass::Configuration,
                },
                Some(*status),
                Some(code.to_string()),
                *retry_after_seconds,
            ),
            _ => (FailureClass::Protocol, None, None, None),
        };
        Self {
            class,
            status,
            provider_code: code,
            retry_after_seconds: retry,
        }
    }
    pub fn retryable(&self) -> bool {
        matches!(
            self.class,
            FailureClass::Connection
                | FailureClass::Timeout
                | FailureClass::RateLimit
                | FailureClass::Server
                | FailureClass::Quota
        )
    }
    pub fn invalidates_verification(&self) -> bool {
        matches!(
            self.class,
            FailureClass::Authentication | FailureClass::Quota
        )
    }
}
fn http_class(status: u16) -> FailureClass {
    match status {
        401 | 403 => FailureClass::Authentication,
        429 => FailureClass::RateLimit,
        500..=599 => FailureClass::Server,
        _ => FailureClass::Configuration,
    }
}
pub fn permits_fallback(
    error: &Error,
    has_primary: bool,
    cancelled: bool,
    sensitive: bool,
) -> bool {
    !has_primary && !cancelled && !sensitive && FailureSummary::from_error(error).retryable()
}
pub fn supports_generation(profile: &Profile) -> bool {
    !matches!(
        profile.provider,
        Provider::SystemDictionary | Provider::Youdao
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceVerification {
    pub profile_id: String,
    pub configuration_fingerprint: String,
    pub profile_revision: u64,
    pub credential_generation: String,
    pub tested_at: i64,
    pub success: bool,
    pub tested_capability: String,
}
impl ServiceVerification {
    pub fn new(profile: &Profile, generation: &str, now: i64, success: bool) -> Self {
        Self {
            profile_id: profile.id.clone(),
            configuration_fingerprint: configuration_fingerprint(profile, generation),
            profile_revision: profile.revision,
            credential_generation: generation.into(),
            tested_at: now,
            success,
            tested_capability: "translation_transport".into(),
        }
    }
    pub fn matches(&self, profile: &Profile, generation: &str) -> bool {
        self.profile_id == profile.id
            && self.profile_revision == profile.revision
            && self.credential_generation == generation
            && self.configuration_fingerprint == configuration_fingerprint(profile, generation)
            && self.tested_capability == "translation_transport"
    }
    pub fn eligible(&self, profile: &Profile, generation: &str, now: i64) -> bool {
        self.success
            && self.matches(profile, generation)
            && now >= self.tested_at
            && now < self.tested_at.saturating_add(VERIFICATION_TTL_MS)
    }
}
pub fn configuration_fingerprint(profile: &Profile, generation: &str) -> String {
    use sha2::{Digest, Sha256};
    // Contains configuration and a random opaque generation only. Never hash a credential.
    let bytes = serde_json::to_vec(&(profile, generation, provider::PROMPT_VERSION))
        .expect("serializable profile");
    crate::hex::lower(&Sha256::digest(bytes))
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationState {
    Untested,
    Verified,
    Expired,
    Changed,
    Failed,
}
#[derive(Debug, Clone, Serialize)]
pub struct VerificationStatus {
    pub profile_id: String,
    pub state: VerificationState,
    pub tested_at: Option<i64>,
    pub expires_at: Option<i64>,
    pub tested_capability: &'static str,
}
impl VerificationStatus {
    pub fn for_profile(
        profile: &Profile,
        generation: &str,
        receipt: Option<&ServiceVerification>,
        now: i64,
    ) -> Self {
        let state = match receipt {
            None => VerificationState::Untested,
            Some(r) if !r.matches(profile, generation) => VerificationState::Changed,
            Some(r) if !r.success => VerificationState::Failed,
            Some(r) if !r.eligible(profile, generation, now) => VerificationState::Expired,
            Some(_) => VerificationState::Verified,
        };
        Self {
            profile_id: profile.id.clone(),
            state,
            tested_at: receipt.map(|r| r.tested_at),
            expires_at: receipt.map(|r| r.tested_at.saturating_add(VERIFICATION_TTL_MS)),
            tested_capability: "translation_transport",
        }
    }
}

pub struct DispatchBudget {
    pub deadline: Instant,
    attempted: HashSet<String>,
}
impl Default for DispatchBudget {
    fn default() -> Self {
        Self {
            deadline: Instant::now() + OPERATION_TIMEOUT,
            attempted: HashSet::new(),
        }
    }
}
impl DispatchBudget {
    pub fn begin(&mut self, profile_id: &str) -> bool {
        Instant::now() < self.deadline
            && self.attempted.len() < MAX_ATTEMPTS
            && self.attempted.insert(profile_id.into())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn receipt_binds_config_generation_and_expiry_without_credentials() {
        let mut p = Profile::default();
        let r = ServiceVerification::new(&p, "random-generation", 10, true);
        assert!(r.eligible(&p, "random-generation", 11));
        assert!(!r.eligible(&p, "random-generation", 9));
        assert!(!r.eligible(&p, "different", 11));
        assert!(!r.eligible(&p, "random-generation", 10 + VERIFICATION_TTL_MS));
        p.model = "changed".into();
        assert!(!r.eligible(&p, "random-generation", 11));
    }
    #[test]
    fn only_transient_precontent_failures_can_fallback() {
        for error in [
            Error::Connection,
            Error::Timeout,
            Error::Http(429),
            Error::Http(503),
        ] {
            assert!(permits_fallback(&error, false, false, false));
            assert!(!permits_fallback(&error, true, false, false));
            assert!(!permits_fallback(&error, false, true, false));
            assert!(!permits_fallback(&error, false, false, true));
        }
        for error in [
            Error::Cancelled,
            Error::Http(401),
            Error::Http(403),
            Error::Http(400),
            Error::TlsOrConfiguration,
            Error::Truncated,
            Error::Empty,
            Error::LengthLimit,
        ] {
            assert!(!permits_fallback(&error, false, false, false));
        }
    }
    #[tokio::test]
    async fn budget_never_repeats_service_or_restarts_deadline() {
        let mut b = DispatchBudget::default();
        let deadline = b.deadline;
        assert!(b.begin("a"));
        assert!(!b.begin("a"));
        assert!(b.begin("b"));
        assert!(b.begin("c"));
        assert!(!b.begin("d"));
        assert_eq!(b.deadline, deadline);
        b.deadline = Instant::now();
        assert!(!b.begin("e"));
    }
}
