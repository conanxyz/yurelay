//! First-use progress is independent of settings and live capability checks.
use crate::{
    config::{Profile, Provider},
    engine::Configuration,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Progress {
    New,
    Started,
    Skipped,
    Completed,
    Legacy,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Experience {
    pub version: u8,
    pub setup: Progress,
    pub step: u8,
    pub tutorial: Progress,
    pub lesson: u8,
}
impl Experience {
    pub fn initial(existing: bool) -> Self {
        Self {
            version: 1,
            setup: if existing {
                Progress::Legacy
            } else {
                Progress::New
            },
            step: 0,
            tutorial: Progress::New,
            lesson: 0,
        }
    }
    pub fn apply(&mut self, action: Action) -> Result<(), String> {
        match action {
            Action::Step { step } if step <= 2 => {
                self.step = step;
                if self.setup == Progress::New {
                    self.setup = Progress::Started;
                }
            }
            Action::Lesson { lesson } if lesson <= 3 => {
                self.lesson = lesson;
                if self.tutorial == Progress::New {
                    self.tutorial = Progress::Started;
                }
            }
            Action::Skip => self.setup = Progress::Skipped,
            Action::Finish => self.setup = Progress::Completed,
            Action::FinishTutorial => self.tutorial = Progress::Completed,
            Action::SkipTutorial => self.tutorial = Progress::Skipped,
            _ => return Err("引导步骤无效。".into()),
        }
        Ok(())
    }
}
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    Step { step: u8 },
    Lesson { lesson: u8 },
    Skip,
    Finish,
    FinishTutorial,
    SkipTutorial,
}
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ServiceState {
    NeedsConfiguration,
    NeedsKey,
    NeedsAuthorization,
    Configured,
}
#[derive(Debug, Serialize)]
pub struct Readiness {
    pub profile_id: String,
    pub state: ServiceState,
    pub dictionary: bool,
    pub translation: bool,
    pub generation: bool,
}
pub fn readiness(configuration: &Configuration) -> Vec<Readiness> {
    configuration
        .settings
        .profiles
        .iter()
        .map(|p| service_readiness(p, configuration))
        .collect()
}
fn service_readiness(p: &Profile, c: &Configuration) -> Readiness {
    let state = if p.validate().is_err() {
        ServiceState::NeedsConfiguration
    } else if p.provider == Provider::SystemDictionary {
        ServiceState::Configured
    } else if c.credential_errors.contains_key(&p.id) {
        ServiceState::NeedsAuthorization
    } else if p.provider != Provider::Custom && !c.has_keys.contains(&p.id) {
        ServiceState::NeedsKey
    } else {
        ServiceState::Configured
    };
    Readiness {
        profile_id: p.id.clone(),
        state,
        dictionary: p.provider == Provider::SystemDictionary,
        translation: p.provider != Provider::SystemDictionary,
        generation: crate::reliability::supports_generation(p),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Settings;
    #[test]
    fn progress_is_separate_from_configuration_and_replay_does_not_restart_setup() {
        let mut e = Experience::initial(false);
        e.apply(Action::Step { step: 1 }).unwrap();
        assert_eq!(e.setup, Progress::Started);
        e.apply(Action::Skip).unwrap();
        e.apply(Action::Lesson { lesson: 2 }).unwrap();
        assert_eq!(e.setup, Progress::Skipped);
        e.apply(Action::FinishTutorial).unwrap();
        e.apply(Action::Finish).unwrap();
        e.apply(Action::Step { step: 0 }).unwrap();
        assert_eq!(e.setup, Progress::Completed);
        assert!(e.apply(Action::Step { step: 3 }).is_err());
        assert!(e.apply(Action::Lesson { lesson: 4 }).is_err());
        assert_eq!(Experience::initial(true).setup, Progress::Legacy);
    }
    #[test]
    fn readiness_distinguishes_configuration_credentials_and_capabilities() {
        let mut c = Configuration {
            settings: Settings::default(),
            has_keys: vec![],
            credential_errors: Default::default(),
        };
        assert_eq!(readiness(&c)[0].state, ServiceState::NeedsConfiguration);
        c.settings.profiles[0].provider = Provider::SystemDictionary;
        let r = readiness(&c).remove(0);
        assert_eq!(r.state, ServiceState::Configured);
        assert!(r.dictionary && !r.translation && !r.generation);
        c.settings.profiles[0].provider = Provider::Openai;
        c.settings.profiles[0].model = "synthetic-model".into();
        assert_eq!(readiness(&c)[0].state, ServiceState::NeedsKey);
        c.has_keys.push("default".into());
        assert_eq!(readiness(&c)[0].state, ServiceState::Configured);
        c.credential_errors
            .insert("default".into(), "Synthetic denied access".into());
        assert_eq!(readiness(&c)[0].state, ServiceState::NeedsAuthorization);
        c.credential_errors.clear();
        c.settings.profiles[0].provider = Provider::Custom;
        c.settings.profiles[0].endpoint = "http://localhost:1234/v1".into();
        c.has_keys.clear();
        assert_eq!(readiness(&c)[0].state, ServiceState::Configured);
        c.settings.profiles[0].provider = Provider::Youdao;
        c.settings.profiles[0].app_id = "synthetic-app".into();
        assert!(!readiness(&c)[0].generation);
    }
}
