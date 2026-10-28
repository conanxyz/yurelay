//! Validation for explicit local read-aloud requests.

pub fn prepare(text: &str) -> Result<String, String> {
    if text.trim().is_empty()
        || text.chars().count() > crate::MAX_INPUT_CHARACTERS
        || text
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
        || crate::sensitive::detect(text).is_some()
    {
        return Err("朗读内容无效。".into());
    }
    // macOS `say` interprets [[commands]] even on stdin. Preserve readable
    // bracketed text while making command delimiters impossible.
    Ok(text.replace('[', "（").replace(']', "）"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_paragraphs_and_neutralizes_native_speech_commands() {
        assert_eq!(
            prepare("Analytics.\n数据分析。\tNext paragraph!").unwrap(),
            "Analytics.\n数据分析。\tNext paragraph!"
        );
        assert_eq!(
            prepare("Read [[rate 1]] [this].").unwrap(),
            "Read （（rate 1）） （this）."
        );
        assert!(prepare(&"a".repeat(crate::MAX_INPUT_CHARACTERS)).is_ok());
    }
    #[test]
    fn rejects_empty_oversized_sensitive_and_control_text() {
        for value in [
            "".into(),
            " \n".into(),
            "hello\0world".into(),
            "hello\u{1b}world".into(),
            "x".repeat(crate::MAX_INPUT_CHARACTERS + 1),
            format!("sk-{}", "x".repeat(40)),
        ] {
            assert!(prepare(&value).is_err());
        }
    }
}
