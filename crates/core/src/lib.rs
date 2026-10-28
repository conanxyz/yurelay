//! Platform-independent translation and persistence logic.

pub const MAX_INPUT_CHARACTERS: usize = 50_000;

pub mod config;
pub mod dictionary;
pub mod engine;
mod hex;
pub mod onboarding;
pub mod provider;
pub mod sse;
pub mod storage;
pub mod tasks;

pub mod operations;
pub mod reliability;
pub mod sensitive;
pub mod source;
pub mod speech;
pub mod vocabulary;
