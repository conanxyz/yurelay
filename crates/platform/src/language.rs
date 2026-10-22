//! Local language ranges are UTF-16, matching NSString and browser selection.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenRange {
    pub start_utf16: usize,
    pub end_utf16: usize,
    pub surface: String,
    pub lemma: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrBlock {
    pub text: String,
    pub confidence: f64,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrLayout {
    pub width: u32,
    pub height: u32,
    pub orientation: u8,
    pub blocks: Vec<OcrBlock>,
}
pub fn token_ranges(text: &str) -> Result<Vec<TokenRange>, String> {
    if text.chars().count() > 50_000 {
        return Err("原文过长。".into());
    }
    #[cfg(target_os = "macos")]
    {
        let input = std::ffi::CString::new(text).map_err(|_| "原文含无效字符。")?;
        let mut output = vec![0u8; 8 * 1024 * 1024];
        // SAFETY: native copies bounded UTF-8 JSON and retains no pointers.
        let len = unsafe {
            super::macos::ad_token_ranges(input.as_ptr(), output.as_mut_ptr(), output.len())
        };
        if len == 0 || len >= output.len() {
            return Err("本地分词未完成。".into());
        }
        output.truncate(len);
        serde_json::from_slice(&output).map_err(|_| "本地分词数据无效。".into())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = text;
        Ok(vec![])
    }
}
#[cfg(all(test, target_os = "macos"))]
mod tests {
    #[test]
    fn ranges_preserve_emoji_and_inflections() {
        let result = super::token_ranges("😀 running with friends").unwrap();
        let token = result.iter().find(|t| t.surface == "running").unwrap();
        assert_eq!((token.start_utf16, token.end_utf16), (3, 10));
        assert_eq!(token.lemma, "run");
    }
}
