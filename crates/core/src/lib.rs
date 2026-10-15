//! Platform-independent translation and persistence logic.

pub const MAX_INPUT_CHARACTERS: usize = 50_000;

pub mod config;
pub mod engine;
pub mod provider;
pub mod sse;
pub mod storage;
pub mod tasks;

pub mod sensitive;
pub mod vocabulary;
