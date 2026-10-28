//! One owned local speech process. Stopping a stale request never stops its successor.
use std::sync::{LazyLock, Mutex};

#[derive(Default)]
pub struct SpeechPlayer {
    playback: Mutex<PlaybackState>,
}
#[derive(Default)]
struct PlaybackState {
    requested: Option<String>,
    child: Option<std::process::Child>,
}
pub static SPEECH: LazyLock<SpeechPlayer> = LazyLock::new(SpeechPlayer::default);

impl SpeechPlayer {
    /// Reserve synchronously before the desktop queues voice discovery. A hide
    /// or stop can then cancel even a request whose worker has not started.
    pub fn reserve(&self, id: &str) -> Result<(), String> {
        let mut active = self.playback.lock().map_err(|_| "系统朗读失败。")?;
        stop_child(&mut active);
        active.requested = Some(id.into());
        Ok(())
    }
    pub fn start(&self, id: &str, text: String, language: Option<&str>) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        {
            use std::{
                io::Write,
                process::{Command, Stdio},
            };
            let detected = language
                .map(str::to_owned)
                .or_else(|| crate::detect_language(&text));
            let voices = super::pronunciation::installed_voices()?;
            let voice = match detected.as_deref().filter(|s| !s.is_empty()) {
                Some(language) => Some(select_voice(&voices, language).ok_or(
                    "此语言的系统声音尚未安装，请在 macOS 辅助功能的朗读内容中添加声音。",
                )?),
                None => None,
            };
            let mut active = self.playback.lock().map_err(|_| "系统朗读失败。")?;
            if active.requested.as_deref() != Some(id) {
                return Ok(());
            }
            let mut command = Command::new("/usr/bin/say");
            if let Some(voice) = voice {
                command.args(["-v", &voice.name]);
            }
            let mut child = command
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|_| "无法启动系统朗读。")?;
            let mut input = child.stdin.take().ok_or("无法启动系统朗读。")?;
            // A long paragraph may exceed the pipe capacity. Never hold the
            // playback lock while writing; stop must remain responsive.
            std::thread::spawn(move || {
                let _ = input.write_all(text.as_bytes());
            });
            active.child = Some(child);
            Ok(())
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (id, text, language);
            Err("此平台暂不支持系统朗读。".into())
        }
    }
    pub fn is_playing(&self, id: &str) -> Result<bool, String> {
        let mut active = self.playback.lock().map_err(|_| "系统朗读失败。")?;
        if active.requested.as_deref() != Some(id) {
            return Ok(false);
        }
        let Some(child) = active.child.as_mut() else {
            return Ok(true);
        };
        match child.try_wait().map_err(|_| "系统朗读失败。")? {
            None => Ok(true),
            Some(status) => {
                active.child = None;
                active.requested = None;
                if status.success() {
                    Ok(false)
                } else {
                    Err("系统朗读失败。".into())
                }
            }
        }
    }
    pub fn stop(&self, id: Option<&str>) {
        if let Ok(mut active) = self.playback.lock()
            && id.is_none_or(|id| active.requested.as_deref() == Some(id))
        {
            stop_child(&mut active);
        }
    }
}
fn stop_child(active: &mut PlaybackState) {
    active.requested = None;
    if let Some(mut child) = active.child.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}
impl Drop for SpeechPlayer {
    fn drop(&mut self) {
        self.stop(None);
    }
}

#[cfg(any(target_os = "macos", test))]
fn select_voice<'a>(
    voices: &'a [super::pronunciation::InstalledVoice],
    language: &str,
) -> Option<&'a super::pronunciation::InstalledVoice> {
    let language = language.replace('_', "-").to_lowercase();
    // Script aliases must not select a voice for the wrong Chinese variety.
    let target = match language.as_str() {
        "zh" | "zh-hans" => "zh-cn",
        "zh-hant" => "zh-tw",
        "en" => "en-us",
        _ => &language,
    };
    if target.starts_with("en-") {
        let locale = if target == "en-gb" {
            "en-GB"
        } else if target == "en-us" {
            "en-US"
        } else {
            ""
        };
        if let Some(voice) = super::pronunciation::preferred_voice(voices, locale) {
            return Some(voice);
        }
    }
    voices
        .iter()
        .find(|voice| voice.locale.eq_ignore_ascii_case(target))
        .or_else(|| {
            if target.starts_with("zh-") {
                return None;
            }
            let primary = target.split('-').next()?;
            voices.iter().find(|voice| {
                voice
                    .locale
                    .split('-')
                    .next()
                    .is_some_and(|p| p.eq_ignore_ascii_case(primary))
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selects_matching_installed_language_without_crossing_chinese_scripts() {
        let voices = [
            ("Mandarin", "zh-CN"),
            ("English", "en-US"),
            ("French", "fr-FR"),
        ]
        .map(
            |(name, locale)| super::super::pronunciation::InstalledVoice {
                name: name.into(),
                locale: locale.into(),
            },
        );
        assert_eq!(select_voice(&voices, "zh-Hans").unwrap().name, "Mandarin");
        assert_eq!(select_voice(&voices, "en").unwrap().name, "English");
        assert_eq!(select_voice(&voices, "fr-CA").unwrap().name, "French");
        assert!(select_voice(&voices, "zh-Hant").is_none());
        assert!(select_voice(&voices, "ja").is_none());
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn owned_process_can_stop_and_stale_requests_cannot_stop_its_replacement() {
        let player = SpeechPlayer::default();
        player.reserve("first").unwrap();
        player
            .start("first", "Testing local speech. ".repeat(100), Some("en"))
            .unwrap();
        assert!(player.is_playing("first").unwrap());
        player.reserve("second").unwrap();
        player
            .start(
                "second",
                "Testing replacement speech. ".repeat(100),
                Some("en"),
            )
            .unwrap();
        player.stop(Some("first"));
        assert!(!player.is_playing("first").unwrap());
        assert!(player.is_playing("second").unwrap());
        player.stop(Some("second"));
        assert!(!player.is_playing("second").unwrap());
        player.reserve("cancelled").unwrap();
        player.stop(None);
        player
            .start("cancelled", "This must never play.".into(), Some("en"))
            .unwrap();
        assert!(!player.is_playing("cancelled").unwrap());
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn native_read_aloud_finishes_and_releases_the_process() {
        for (language, text) in [("en", "Hello."), ("zh-Hans", "分析。")] {
            let voices = super::super::pronunciation::installed_voices().unwrap();
            if select_voice(&voices, language).is_none() {
                eprintln!("Native speech fixture unavailable for language {language}");
                continue;
            }
            let player = SpeechPlayer::default();
            player.reserve("completion").unwrap();
            player
                .start("completion", text.into(), Some(language))
                .unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
            while player.is_playing("completion").unwrap() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "Native speech did not finish"
                );
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            assert!(player.playback.lock().unwrap().child.is_none());
        }
    }
}
