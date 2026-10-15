//! Entirely local, deliberately conservative credential detection. Never log input.
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, sync::LazyLock};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SensitiveKind {
    Mnemonic,
    PrivateKey,
    ApiSecret,
}
static WORDS: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| include_str!("../data/bip39-english.txt").lines().collect());
static KEYS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?m)(?:sk-(?:ant-)?[a-zA-Z0-9_-]{32,}|AKIA[0-9A-Z]{16}|gh[pousr]_[A-Za-z0-9_]{36,})",
    )
    .unwrap()
});
static PEM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"-----BEGIN (?:RSA |OPENSSH |EC |PGP )?PRIVATE KEY-----").unwrap()
});
static PRIVATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:(?:0x)?[a-fA-F0-9]{64}|[5KL][1-9A-HJ-NP-Za-km-z]{50,51}|[1-9A-HJ-NP-Za-km-z]{87,88})$").unwrap()
});
fn words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|s| {
            s.trim_matches(|c: char| !c.is_alphanumeric())
                .to_ascii_lowercase()
        })
        .filter(|s| !s.is_empty())
        .collect()
}
fn mnemonic(words: &[String]) -> bool {
    matches!(words.len(), 12 | 15 | 18 | 21 | 24)
        && words.iter().filter(|w| WORDS.contains(w.as_str())).count() * 10 >= words.len() * 9
}
pub fn detect(text: &str) -> Option<SensitiveKind> {
    if PEM.is_match(text) {
        return Some(SensitiveKind::PrivateKey);
    }
    if KEYS.is_match(text) {
        return Some(SensitiveKind::ApiSecret);
    }
    if text
        .split(|c: char| {
            c.is_whitespace() || matches!(c, '\"' | '\'' | ',' | ':' | '=' | '(' | ')' | ';')
        })
        .any(|s| PRIVATE.is_match(s.trim_matches(|c: char| !c.is_ascii_alphanumeric())))
    {
        return Some(SensitiveKind::PrivateKey);
    }
    // Also inspect embedded arrays, not only inputs consisting of a key.
    for part in text.split('[').skip(1) {
        if let Some(end) = part.find(']')
            && let Ok(bytes) = serde_json::from_str::<Vec<u8>>(&format!("[{}]", &part[..end]))
            && bytes.len() == 64
        {
            return Some(SensitiveKind::PrivateKey);
        }
    }
    let tokens = words(text);
    if mnemonic(&tokens) || text.lines().any(|line| mnemonic(&words(line))) {
        return Some(SensitiveKind::Mnemonic);
    }
    // A mnemonic surrounded by prose remains dangerous. Require 12 consecutive
    // dictionary words for the embedded form to avoid broad prose false positives.
    if tokens
        .windows(12)
        .any(|w| w.iter().all(|s| WORDS.contains(s.as_str())))
    {
        return Some(SensitiveKind::Mnemonic);
    }
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rules_and_boundaries() {
        assert_eq!(WORDS.len(), 2048);
        for n in [12, 15, 18, 21, 24] {
            assert_eq!(
                detect(&vec!["abandon,"; n].join(" \n")),
                Some(SensitiveKind::Mnemonic)
            );
        }
        assert!(detect("abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon example-not-bip39").is_some());
        for text in [
            "a".repeat(64),
            format!("0x{}", "1".repeat(64)),
            format!("5{}", "A".repeat(50)),
            "A".repeat(88),
            format!("sk-ant-{}", "x".repeat(40)),
            format!("ghp_{}", "x".repeat(36)),
            format!("AKIA{}", "1234567890123456"),
            "-----BEGIN OPENSSH PRIVATE KEY-----".into(),
            format!("[{}]", vec!["255"; 64].join(",")),
        ] {
            assert!(detect(&format!("prefix {text} suffix")).is_some());
        }
        assert!(detect("A natural sentence about translation and vocabulary learning.").is_none());
        assert!(detect(&format!("[{}]", vec!["256"; 64].join(","))).is_none());
        assert!(detect("sk-short").is_none());
    }
}
