//! Local dictionary pronunciation and installed macOS speech voices.
//!
//! Dictionary Services returns plain text, without reliable accent metadata. We
//! expose one explicitly delimited pronunciation and never invent a second one.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct DictionaryDetails {
    pub definition: String,
    pub phonetic: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SpeechVoice {
    pub locale: String,
    pub available: bool,
}

pub fn lookup_dictionary_details(text: &str) -> Result<DictionaryDetails, String> {
    let definition = crate::lookup_dictionary(text)?;
    let phonetic = dictionary_pronunciation(&definition);
    Ok(DictionaryDetails {
        definition,
        phonetic,
    })
}

fn dictionary_pronunciation(definition: &str) -> Option<String> {
    // Oxford dictionaries put the headword's pronunciation between the first
    // two vertical bars. Restrict extraction to that header so examples and
    // derivative entries cannot supply an unrelated word's pronunciation.
    let (heading, rest) = definition.split_once('|')?;
    if heading.chars().count() > 192 || heading.chars().any(|c| matches!(c, '\n' | ':' | '!' | '?'))
    {
        return None;
    }
    let (pronunciation, _) = rest.split_once('|')?;
    let pronunciation = pronunciation.trim();
    if pronunciation.is_empty()
        || pronunciation.chars().count() > 96
        || pronunciation.chars().any(|c| {
            !(c.is_alphabetic()
                || matches!(c, ' ' | ',' | ';' | '.' | '-' | '(' | ')' | '/' | 'ˈ' | 'ˌ')
                || ('\u{0300}'..='\u{036f}').contains(&c))
        })
    {
        return None;
    }
    Some(pronunciation.to_owned())
}

#[derive(Debug)]
pub(super) struct InstalledVoice {
    pub(super) name: String,
    pub(super) locale: String,
}

fn parse_installed_voices(output: &str) -> Vec<InstalledVoice> {
    output
        .lines()
        .filter_map(|line| {
            let (description, _) = line.split_once('#')?;
            let (name, locale) = description.trim().rsplit_once(char::is_whitespace)?;
            let name = name.trim();
            let locale = locale.trim().replace('_', "-");
            if !locale.contains('-') || !locale.chars().all(|c| c.is_ascii_alphabetic() || c == '-')
            {
                return None;
            }
            if name.is_empty() || name.chars().any(char::is_control) {
                return None;
            }
            Some(InstalledVoice {
                name: name.to_owned(),
                locale: locale.to_owned(),
            })
        })
        .collect()
}

pub(super) fn preferred_voice<'a>(
    voices: &'a [InstalledVoice],
    locale: &str,
) -> Option<&'a InstalledVoice> {
    let preferences: &[&str] = match locale {
        "en-GB" => &["Daniel", "Serena", "Oliver", "Stephanie", "Eddy"],
        "en-US" => &["Samantha", "Alex", "Ava", "Allison", "Eddy"],
        _ => return None,
    };
    voices
        .iter()
        .filter(|voice| voice.locale == locale)
        .min_by_key(|voice| {
            preferences
                .iter()
                .position(|name| {
                    voice.name == *name
                        || voice
                            .name
                            .strip_prefix(name)
                            .is_some_and(|suffix| suffix.starts_with(" ("))
                })
                .unwrap_or(preferences.len())
        })
}

#[cfg(target_os = "macos")]
pub(super) fn installed_voices() -> Result<Vec<InstalledVoice>, String> {
    let output = std::process::Command::new("/usr/bin/say")
        .args(["-v", "?"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .map_err(|_| "无法读取系统朗读声音。")?;
    if !output.status.success() {
        return Err("无法读取系统朗读声音。".into());
    }
    let output = String::from_utf8(output.stdout).map_err(|_| "系统朗读声音信息无效。")?;
    Ok(parse_installed_voices(&output))
}

/// Report accents that can actually be spoken with an installed system voice.
pub fn speech_voices() -> Vec<SpeechVoice> {
    #[cfg(target_os = "macos")]
    let voices = installed_voices().unwrap_or_default();
    #[cfg(not(target_os = "macos"))]
    let voices = Vec::new();
    ["en-GB", "en-US"]
        .map(|locale| SpeechVoice {
            locale: locale.to_owned(),
            available: preferred_voice(&voices, locale).is_some(),
        })
        .to_vec()
}

/// Speak through stdin: neither a word nor a system voice name is shell code.
/// A requested accent must exist; never silently substitute another accent.
/// Without an accent, retain the user's system voice and normal speech rate.
pub fn speak_word(word: &str, locale: Option<&str>, slow: bool) -> Result<(), String> {
    // `say` understands embedded [[speech commands]] even on stdin. Vocabulary
    // playback accepts words/phrases, never those instructions or control text.
    if word.trim().is_empty()
        || word.chars().count() > 128
        || word
            .chars()
            .any(|c| !(c.is_alphanumeric() || matches!(c, ' ' | '-' | '\'' | '’' | '.')))
    {
        return Err("朗读内容无效。".into());
    }
    if locale.is_some_and(|locale| !matches!(locale, "en-US" | "en-GB")) {
        return Err("不支持所选朗读口音。".into());
    }
    #[cfg(target_os = "macos")]
    {
        use std::{
            io::Write,
            process::{Command, Stdio},
        };
        let voice_name = if let Some(locale) = locale {
            let voices = installed_voices()?;
            Some(
                preferred_voice(&voices, locale)
                    .ok_or("所选口音的系统声音尚未安装，请在 macOS 辅助功能的朗读内容中添加声音。")?
                    .name
                    .clone(),
            )
        } else {
            None
        };
        let mut child = Command::new("/usr/bin/say")
            .args(speech_arguments(voice_name.as_deref(), slow))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "无法启动系统朗读。")?;
        let write_result = child
            .stdin
            .take()
            .ok_or("无法启动系统朗读。")
            .and_then(|mut input| {
                input
                    .write_all(word.as_bytes())
                    .map_err(|_| "系统朗读失败。")
            });
        if let Err(error) = write_result {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error.into());
        }
        if child.wait().map_err(|_| "系统朗读失败。")?.success() {
            Ok(())
        } else {
            Err("系统朗读失败。".into())
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = slow;
        Err("此平台暂不支持系统朗读。".into())
    }
}

#[cfg(any(target_os = "macos", test))]
fn speech_arguments(voice_name: Option<&str>, slow: bool) -> Vec<String> {
    let mut arguments = Vec::new();
    if let Some(name) = voice_name {
        arguments.extend(["-v".to_owned(), name.to_owned()]);
    }
    if slow || voice_name.is_some() {
        arguments.extend(["-r".to_owned(), if slow { "110" } else { "175" }.to_owned()]);
    }
    arguments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_only_the_headword_pronunciation_from_local_dictionary_text() {
        let definition = "disruptive dis·rup·tive | disˈrəptiv | adjective causing change. DERIVATIVES disruptively | disˈrəptəvlē | adverb";
        assert_eq!(
            dictionary_pronunciation(definition).as_deref(),
            Some("disˈrəptiv")
        );
        assert_eq!(
            dictionary_pronunciation("hello hel·lo | həˈlō, heˈlō | exclamation a greeting")
                .as_deref(),
            Some("həˈlō, heˈlō")
        );
        assert_eq!(
            dictionary_pronunciation("compute com·pute | kəmˈpyo͞ot | verb calculate").as_deref(),
            Some("kəmˈpyo͞ot")
        );
    }

    #[test]
    fn missing_or_ambiguous_dictionary_pronunciation_is_not_invented() {
        for definition in [
            "word noun a definition without pronunciation",
            "word | missing closing delimiter",
            "word | | noun empty pronunciation",
            "word noun: an example | with a pipe | between examples",
            "word\nDERIVATIVES alternate | ˈôltərnət | adjective",
            "word | <script>untrusted</script> | noun",
        ] {
            assert_eq!(dictionary_pronunciation(definition), None);
        }
    }

    #[test]
    fn selects_installed_voices_with_spaces_without_substituting_accents() {
        let voices = parse_installed_voices(
            "Albert en_US # Hello\nDaniel en_GB # Hello\nSamantha (English (US)) en_US # Hello\nSamantha (English (US)) en_US # duplicate\nAlice it_IT # Ciao\nBroken data\n",
        );
        assert_eq!(voices.len(), 5);
        assert_eq!(
            preferred_voice(&voices, "en-US").map(|voice| voice.name.as_str()),
            Some("Samantha (English (US))")
        );
        assert_eq!(
            preferred_voice(&voices, "en-GB").map(|voice| voice.name.as_str()),
            Some("Daniel")
        );
        assert!(preferred_voice(&voices, "fr-FR").is_none());
        let us_only = parse_installed_voices("Samantha en_US # Hello");
        assert!(preferred_voice(&us_only, "en-GB").is_none());
    }

    #[test]
    fn validates_speech_boundary_before_launching_any_native_process() {
        for word in [
            "",
            "   ",
            "hello\nworld",
            "hello\0",
            "hello [[slnc 999999]]",
        ] {
            assert_eq!(speak_word(word, None, false), Err("朗读内容无效。".into()));
        }
        assert!(speak_word(&"a".repeat(129), None, false).is_err());
        assert_eq!(
            speak_word("hello", Some("arbitrary -v option"), false),
            Err("不支持所选朗读口音。".into())
        );
    }

    #[test]
    fn unspecified_accent_preserves_system_voice_and_normal_rate() {
        assert!(speech_arguments(None, false).is_empty());
        assert_eq!(speech_arguments(None, true), ["-r", "110"]);
        assert_eq!(
            speech_arguments(Some("Daniel"), false),
            ["-v", "Daniel", "-r", "175"]
        );
        assert_eq!(
            speech_arguments(Some("Samantha (English (US))"), true),
            ["-v", "Samantha (English (US))", "-r", "110"]
        );
    }
}
