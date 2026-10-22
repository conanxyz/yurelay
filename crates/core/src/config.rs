use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EndpointMode {
    #[default]
    Base,
    Exact,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    #[default]
    Custom,
    Deepseek,
    Minimax,
    MinimaxChina,
    Openai,
    Gemini,
    Anthropic,
    Qwen,
    Zhipu,
    Siliconflow,
    Openrouter,
    SystemDictionary,
    Youdao,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Profile {
    pub id: String,
    pub provider: Provider,
    pub name: String,
    pub endpoint: String,
    pub endpoint_mode: EndpointMode,
    pub model: String,
    pub app_id: String,
    pub custom_prompt: String,
    pub custom_reply_prompt: String,
    pub streaming: bool,
    pub temperature: f32,
    pub revision: u64,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            id: "default".into(),
            provider: Provider::Custom,
            name: "自定义模型".into(),
            endpoint: String::new(),
            endpoint_mode: EndpointMode::Base,
            model: String::new(),
            app_id: String::new(),
            custom_prompt: String::new(),
            custom_reply_prompt: String::new(),
            streaming: true,
            temperature: 0.2,
            revision: 0,
        }
    }
}

impl Profile {
    pub fn request_url(&self) -> Result<Url, String> {
        let endpoint = match self.provider {
            Provider::Custom => self.endpoint.trim(),
            Provider::Deepseek => "https://api.deepseek.com/v1",
            Provider::Minimax => "https://api.minimax.io/v1",
            Provider::MinimaxChina => "https://api.minimax.cn/v1",
            Provider::Openai => "https://api.openai.com/v1",
            Provider::Gemini => "https://generativelanguage.googleapis.com/v1beta/openai",
            Provider::Anthropic => "https://api.anthropic.com/v1",
            Provider::Qwen => "https://dashscope.aliyuncs.com/compatible-mode/v1",
            Provider::Zhipu => "https://open.bigmodel.cn/api/paas/v4",
            Provider::Siliconflow => "https://api.siliconflow.cn/v1",
            Provider::Openrouter => "https://openrouter.ai/api/v1",
            Provider::Youdao => "https://openapi.youdao.com/api",
            Provider::SystemDictionary => return Err("系统词典无需网络服务地址。".into()),
        };
        let mut url = Url::parse(endpoint).map_err(|_| "请输入有效的服务地址。")?;
        let is_loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
        if url.scheme() != "https" && !(url.scheme() == "http" && is_loopback) {
            return Err("请使用 HTTPS；仅本机回环地址支持 HTTP。".into());
        }
        if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
            return Err("服务地址不能包含用户名、密码或片段；请在 API Key 字段设置密钥。".into());
        }
        if self.provider == Provider::Youdao {
            return Ok(url);
        }
        if self.provider == Provider::Anthropic {
            url.set_path("/v1/messages");
            return Ok(url);
        }
        if self.provider != Provider::Custom || self.endpoint_mode == EndpointMode::Base {
            let base = url.path().trim_end_matches('/');
            if base.ends_with("/chat/completions") {
                return Err("这看起来是完整请求地址，请选择“完整地址”。".into());
            }
            let path = if base.is_empty() {
                "/v1/chat/completions".into()
            } else {
                format!("{base}/chat/completions")
            };
            url.set_path(&path);
        }
        Ok(url)
    }

    pub fn models_url(&self) -> Result<Url, String> {
        if self.provider == Provider::Anthropic {
            return Url::parse("https://api.anthropic.com/v1/models")
                .map_err(|_| "无效的模型列表地址。".into());
        }
        if matches!(self.provider, Provider::SystemDictionary | Provider::Youdao) {
            return Err("此服务不使用模型列表。".into());
        }
        let mut url = self.request_url()?;
        let prefix = url
            .path()
            .strip_suffix("/chat/completions")
            .ok_or("此完整地址无法推导模型列表地址，请手动填写模型名称。")?;
        let path = format!("{prefix}/models");
        url.set_path(&path);
        Ok(url)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.provider == Provider::SystemDictionary {
            return Ok(());
        }
        self.request_url()?;
        if self.provider == Provider::Youdao {
            return if self.app_id.trim().is_empty() {
                Err("请填写有道应用 ID。".into())
            } else {
                Ok(())
            };
        }
        if self.model.trim().is_empty() {
            return Err("请填写或选择模型。".into());
        }
        if !self.temperature.is_finite() || !(0.0..=2.0).contains(&self.temperature) {
            return Err("温度必须介于 0 和 2 之间。".into());
        }
        if self.provider == Provider::Zhipu && self.temperature > 1.0 {
            return Err("智谱温度必须介于 0 和 1 之间。".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "mode", content = "days", rename_all = "snake_case")]
pub enum Retention {
    Disabled,
    Days(u32),
    Forever,
}

impl Default for Retention {
    fn default() -> Self {
        Self::Days(7)
    }
}

impl Retention {
    pub fn expires_at(self, created_at: i64) -> Option<i64> {
        match self {
            Self::Days(days) => Some(created_at.saturating_add(i64::from(days) * 86_400_000)),
            _ => None,
        }
    }

    pub fn validate(self) -> Result<(), String> {
        if let Self::Days(days) = self
            && !(1..=36_500).contains(&days)
        {
            return Err("保存天数必须介于 1 和 36500 之间。".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub profiles: Vec<Profile>,
    pub active_profile: String,
    pub source_language: String,
    pub target_language: String,
    pub ui_language: String,
    pub theme: String,
    pub ui_font_size: u16,
    pub skip_same_language: bool,
    pub sensitive_guard: bool,
    pub auto_vocabulary: bool,
    pub retention: Retention,
    pub selection_shortcut: String,
    pub screenshot_shortcut: String,
    pub auto_hide: bool,
    pub font_size: u16,
    pub max_concurrency: u8,
    pub append_selection_shortcut: Option<String>,
    pub append_screenshot_shortcut: Option<String>,
    pub auto_clean: bool,
    pub encounter_highlight: bool,
    pub preserve_structure: bool,
    pub translation_prompt: String,
    pub reply_prompt: String,
    pub failover: crate::reliability::FailoverSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            profiles: vec![Profile::default()],
            active_profile: "default".into(),
            source_language: String::new(),
            target_language: "zh-Hans".into(),
            ui_language: "follow_primary".into(),
            theme: "system".into(),
            ui_font_size: 14,
            skip_same_language: true,
            sensitive_guard: true,
            auto_vocabulary: true,
            retention: Retention::default(),
            selection_shortcut: "Alt+D".into(),
            screenshot_shortcut: "Alt+S".into(),
            auto_hide: false,
            font_size: 16,
            max_concurrency: 2,
            append_selection_shortcut: None,
            append_screenshot_shortcut: None,
            auto_clean: true,
            encounter_highlight: true,
            preserve_structure: false,
            translation_prompt: String::new(),
            reply_prompt: String::new(),
            failover: Default::default(),
        }
    }
}

pub fn language_family(value: &str) -> &str {
    match value {
        "简体中文" | "繁體中文" | "中文" | "Chinese" | "zh-Hans" | "zh-Hant" | "zh-CN"
        | "zh-TW" => "zh",
        "English" | "英语" | "en-US" | "en-GB" => "en",
        other => other.split('-').next().unwrap_or(other),
    }
}

/// Script variants remain distinct: simplified/traditional conversion still
/// needs translation. Unknown or ambiguous language tags never skip a request.
pub fn same_translation_language(source: &str, target: &str) -> bool {
    fn canonical(value: &str) -> String {
        match value.to_ascii_lowercase().replace('_', "-").as_str() {
            "zh-cn" | "zh-sg" | "zh-hans" => "zh-hans".into(),
            "zh-tw" | "zh-hk" | "zh-mo" | "zh-hant" => "zh-hant".into(),
            other => other.into(),
        }
    }
    let source = canonical(source);
    !source.is_empty() && source != "und" && source == canonical(target)
}

impl Settings {
    /// Capture resolved prompt preferences with the request. Empty service
    /// overrides inherit the global setting; empty global values use built-ins.
    pub fn resolved_profile(&self, profile: &Profile) -> Profile {
        let mut profile = profile.clone();
        if profile.custom_prompt.trim().is_empty() {
            profile.custom_prompt = self.translation_prompt.clone();
        }
        if profile.custom_reply_prompt.trim().is_empty() {
            profile.custom_reply_prompt = self.reply_prompt.clone();
        }
        profile
    }

    pub fn ui_locale(&self) -> &str {
        match self.ui_language.as_str() {
            "zh" => "zh",
            "en" => "en",
            _ if language_family(&self.target_language) == "zh" => "zh",
            _ => "en",
        }
    }

    pub fn active(&self) -> Result<&Profile, String> {
        self.profiles
            .iter()
            .find(|p| p.id == self.active_profile)
            .ok_or("请先选择翻译服务。".into())
    }

    pub fn validate(&self) -> Result<(), String> {
        self.retention.validate()?;
        if !matches!(self.ui_language.as_str(), "follow_primary" | "zh" | "en")
            || !matches!(self.theme.as_str(), "system" | "light" | "dark")
            || !(12..=18).contains(&self.ui_font_size)
        {
            return Err("界面语言、主题或字号无效。".into());
        }
        if !(1..=2).contains(&self.max_concurrency) {
            return Err("同时翻译任务数须为 1 或 2。".into());
        }
        self.active()?;
        if self.profiles.is_empty() || self.profiles.len() > 20 {
            return Err("服务数量须介于 1 和 20 之间。".into());
        }
        if self.target_language.trim().is_empty() || self.target_language.len() > 120 {
            return Err("请选择有效的目标语言。".into());
        }
        if !self.source_language.is_empty()
            && (self.source_language.len() > 32
                || !self
                    .source_language
                    .chars()
                    .all(|c| c.is_ascii_alphabetic() || c == '-'))
        {
            return Err("请选择有效的原文语言。".into());
        }
        if !(12..=28).contains(&self.font_size) {
            return Err("字号须介于 12 和 28 之间。".into());
        }
        if self.translation_prompt.len() > 32000 || self.reply_prompt.len() > 32000 {
            return Err("全局提示词过长。".into());
        }
        let mut ids = std::collections::HashSet::new();
        for p in &self.profiles {
            if p.custom_prompt.len() > 32000 || p.custom_reply_prompt.len() > 32000 {
                return Err("自定义提示词过长。".into());
            }
            if p.name.len() > 240
                || p.endpoint.len() > 8192
                || p.model.len() > 256
                || p.app_id.len() > 256
            {
                return Err("服务名称、地址或模型名称过长。".into());
            }
            if p.id.is_empty()
                || p.id.len() > 64
                || !p.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
                || !ids.insert(&p.id)
            {
                return Err("服务标识无效或重复。".into());
            }
            // A new, not-yet-configured profile is allowed to be saved.
            if !p.endpoint.trim().is_empty() {
                p.request_url()?;
            }
        }
        // profile_ids remains readable for older settings files, but automatic
        // backup now uses every configured compatible service in saved order.
        for shortcut in [
            &self.append_selection_shortcut,
            &self.append_screenshot_shortcut,
        ]
        .into_iter()
        .flatten()
        {
            if shortcut.trim().is_empty() || shortcut.len() > 128 {
                return Err("追加快捷键无效。".into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_prompts_preserve_legacy_customization_and_explicit_cleanup_preference() {
        let old:Settings=serde_json::from_str(r#"{"auto_clean":false,"profiles":[{"custom_prompt":"Legacy style"}],"failover":{"enabled":true,"profile_ids":["removed","removed"]}}"#).unwrap();
        assert!(!old.auto_clean);
        assert!(old.validate().is_ok());
        assert_eq!(
            old.resolved_profile(&old.profiles[0]).custom_prompt,
            "Legacy style"
        );
        let mut settings = Settings {
            translation_prompt: "Global translation".into(),
            reply_prompt: "Global reply".into(),
            ..Settings::default()
        };
        assert_eq!(
            settings
                .resolved_profile(&settings.profiles[0])
                .custom_prompt,
            "Global translation"
        );
        assert_eq!(
            settings
                .resolved_profile(&settings.profiles[0])
                .custom_reply_prompt,
            "Global reply"
        );
        settings.profiles[0].custom_reply_prompt = "Local reply".into();
        assert_eq!(
            settings
                .resolved_profile(&settings.profiles[0])
                .custom_reply_prompt,
            "Local reply"
        );
        assert!(settings.profiles[0].custom_prompt.is_empty());
    }

    #[test]
    fn local_dictionary_and_youdao_have_distinct_configuration_contracts() {
        let p = Profile {
            provider: Provider::SystemDictionary,
            ..Profile::default()
        };
        assert!(p.validate().is_ok());
        assert!(p.models_url().is_err());
        let p = Profile {
            provider: Provider::Youdao,
            app_id: "fixture-id".into(),
            ..Profile::default()
        };
        assert!(p.validate().is_ok());
        assert!(p.models_url().is_err());
        let old: Profile = serde_json::from_str(
            r#"{"provider":"custom","endpoint":"https://example.test/v1","model":"old"}"#,
        )
        .unwrap();
        assert!(old.app_id.is_empty());
    }

    #[test]
    fn endpoint_modes_do_not_duplicate_paths() {
        let mut p = Profile {
            endpoint: "https://example.test/v1/".into(),
            ..Profile::default()
        };
        assert_eq!(
            p.request_url().unwrap().as_str(),
            "https://example.test/v1/chat/completions"
        );
        assert_eq!(
            p.models_url().unwrap().as_str(),
            "https://example.test/v1/models"
        );
        p.endpoint = "https://example.test/v1/chat/completions".into();
        assert!(p.request_url().is_err());
        p.endpoint_mode = EndpointMode::Exact;
        assert_eq!(p.request_url().unwrap().as_str(), p.endpoint);
    }

    #[test]
    fn insecure_remote_or_embedded_credentials_are_rejected() {
        for endpoint in [
            "http://example.test/v1",
            "https://user:secret@example.test/v1",
            "https://example.test/v1#secret",
            "file:///etc/passwd",
        ] {
            assert!(
                Profile {
                    endpoint: endpoint.into(),
                    ..Profile::default()
                }
                .request_url()
                .is_err()
            );
        }
        for endpoint in [
            "http://127.0.0.1:5555/v1",
            "http://localhost:5555/v1",
            "http://[::1]:5555/v1",
        ] {
            assert!(
                Profile {
                    endpoint: endpoint.into(),
                    ..Profile::default()
                }
                .request_url()
                .is_ok()
            );
        }
    }

    #[test]
    fn expiry_is_based_on_creation_and_safe_for_large_values() {
        assert_eq!(Retention::Days(1).expires_at(100), Some(86_400_100));
        assert_eq!(Retention::Days(1).expires_at(i64::MAX), Some(i64::MAX));
        assert_eq!(Retention::Forever.expires_at(100), None);
        assert!(Retention::Days(0).validate().is_err());
    }
    #[test]
    fn builtin_providers_use_official_urls_and_legacy_settings_still_load() {
        for (provider, expected) in [
            (
                Provider::Deepseek,
                "https://api.deepseek.com/v1/chat/completions",
            ),
            (
                Provider::Minimax,
                "https://api.minimax.io/v1/chat/completions",
            ),
            (
                Provider::MinimaxChina,
                "https://api.minimax.cn/v1/chat/completions",
            ),
        ] {
            let profile = Profile {
                provider,
                endpoint: "https://wrong.test".into(),
                endpoint_mode: EndpointMode::Exact,
                ..Profile::default()
            };
            assert_eq!(profile.request_url().unwrap().as_str(), expected);
        }
        let settings: Settings = serde_json::from_str(r#"{"target_language":"简体中文"}"#).unwrap();
        assert!(settings.skip_same_language);
        assert!(!settings.failover.enabled);
        assert!(settings.failover.profile_ids.is_empty());
        assert!(settings.append_selection_shortcut.is_none());
        assert!(settings.append_screenshot_shortcut.is_none());
        assert!(settings.auto_clean);
        assert!(settings.translation_prompt.is_empty());
        assert!(settings.reply_prompt.is_empty());
        assert!(settings.encounter_highlight);
        assert!(!settings.preserve_structure);
        assert_eq!(settings.ui_locale(), "zh");
        assert_eq!(settings.profiles[0].provider, Provider::Custom);
        let en = Settings {
            target_language: "en".into(),
            ..settings.clone()
        };
        assert_eq!(en.ui_locale(), "en");
        let independent = Settings {
            ui_language: "zh".into(),
            ..en
        };
        assert_eq!(independent.ui_locale(), "zh");
    }
}

/// Preferences that may be saved immediately without changing service drafts,
/// credentials, shortcuts, or destructive history-retention options.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplayPreferences {
    pub target_language: String,
    pub ui_language: String,
    pub theme: String,
    pub ui_font_size: u16,
    pub font_size: u16,
    pub auto_hide: bool,
    pub max_concurrency: u8,
}
impl DisplayPreferences {
    pub fn apply(self, settings: &mut Settings) {
        settings.target_language = self.target_language;
        settings.ui_language = self.ui_language;
        settings.theme = self.theme;
        settings.ui_font_size = self.ui_font_size;
        settings.font_size = self.font_size;
        settings.auto_hide = self.auto_hide;
        settings.max_concurrency = self.max_concurrency;
    }
}
