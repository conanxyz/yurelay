//! Versioned source documents and local-only transformations.
pub mod cleaner;
pub mod encounters;
pub mod layout;
use crate::tasks::Source;
use serde::{Deserialize, Serialize};

pub const MAX_SEGMENTS: usize = 10;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceSegment {
    pub id: String,
    pub kind: Source,
    pub original_text: String,
    pub captured_at: i64,
    pub asset_id: Option<String>,
    #[serde(default)]
    pub ocr_layout: Option<layout::OcrLayout>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceSnapshot {
    pub source_revision: u64,
    pub original_text: String,
    pub edited_text: Option<String>,
    pub source_language: Option<String>,
    pub target_language: String,
    pub segments: Vec<SourceSegment>,
}
impl SourceSnapshot {
    pub fn content_bytes(&self) -> usize {
        self.original_text.len()
            + self.edited_text.as_ref().map_or(0, String::len)
            + self
                .segments
                .iter()
                .map(|s| {
                    s.original_text.len()
                        + s.ocr_layout.as_ref().map_or(0, |layout| {
                            layout.blocks.iter().map(|b| b.text.len()).sum::<usize>()
                        })
                })
                .sum::<usize>()
    }
}
#[derive(Debug, Clone, Deserialize)]
pub struct SourceRequest {
    pub task_id: String,
    pub expected_revision: u64,
}
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateRequest {
    #[serde(default)]
    pub source_language: Option<String>,
    #[serde(default)]
    pub target_language: Option<String>,
    #[serde(flatten)]
    pub source: SourceRequest,
    pub text: String,
}
#[derive(Debug, Clone, Deserialize)]
pub struct CleanRequest {
    #[serde(flatten)]
    pub source: SourceRequest,
    pub mode: cleaner::CleanMode,
}
pub fn validate_text(text: &str) -> Result<(), String> {
    if text.trim().is_empty() || text.chars().count() > crate::MAX_INPUT_CHARACTERS {
        return Err("原文为空或超过 50,000 字符，请分段处理。".into());
    }
    Ok(())
}
pub fn validate_png(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() < 24
        || bytes.len() > 32 * 1024 * 1024
        || !bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || &bytes[12..16] != b"IHDR"
    {
        return Err("截图数据无效或过大。".into());
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 16 * 1024 * 1024 {
        return Err("截图像素超过限制，请缩小选区。".into());
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct PendingSummary {
    pub id: String,
    pub task_id: String,
    pub expected_revision: u64,
    pub sensitive: Option<crate::sensitive::SensitiveKind>,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct SourceAsset {
    pub id: String,
    pub segment_id: String,
    pub legacy: bool,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingAction {
    Discard,
    Append,
    New,
}
