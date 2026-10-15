//! Incremental SSE framing. TCP/HTTP chunks are not text or event boundaries.

const MAX_EVENT_BYTES: usize = 1024 * 1024;

#[derive(Default)]
pub struct Decoder {
    line: Vec<u8>,
    data: Vec<String>,
    data_bytes: usize,
    after_cr: bool,
    first_line: bool,
    started: bool,
}

impl Decoder {
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<String>, String> {
        let mut events = Vec::new();
        for &byte in chunk {
            if self.after_cr && byte == b'\n' {
                self.after_cr = false;
                continue;
            }
            self.after_cr = false;
            if byte == b'\r' || byte == b'\n' {
                self.finish_line(&mut events)?;
                self.after_cr = byte == b'\r';
            } else {
                self.line.push(byte);
                if self.line.len() + self.data_bytes > MAX_EVENT_BYTES {
                    return Err("流式响应的单个事件过大。".into());
                }
            }
        }
        Ok(events)
    }

    fn finish_line(&mut self, events: &mut Vec<String>) -> Result<(), String> {
        let bytes = std::mem::take(&mut self.line);
        let mut line = std::str::from_utf8(&bytes).map_err(|_| "接口返回了无效的 UTF-8 内容。")?;
        if !self.started {
            self.first_line = true;
            self.started = true;
        }
        if self.first_line {
            line = line.trim_start_matches('\u{feff}');
            self.first_line = false;
        }
        if line.is_empty() {
            if !self.data.is_empty() {
                events.push(self.data.join("\n"));
                self.data.clear();
                self.data_bytes = 0;
            }
            return Ok(());
        }
        if line.starts_with(':') {
            return Ok(());
        }
        let (field, value) = line.split_once(':').unwrap_or((line, ""));
        if field == "data" {
            let value = value.strip_prefix(' ').unwrap_or(value);
            self.data_bytes += value.len();
            if self.data_bytes > MAX_EVENT_BYTES {
                return Err("流式响应的单个事件过大。".into());
            }
            self.data.push(value.to_owned());
        }
        Ok(())
    }

    /// EOF does not dispatch an unfinished event. Treat it as truncation rather
    /// than silently displaying a partial JSON object as a completed response.
    pub fn has_incomplete_event(&self) -> bool {
        !self.line.is_empty() || !self.data.is_empty()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ChatEvent {
    Chunk {
        text: String,
        finish_reason: Option<String>,
    },
    Done,
    Ignored,
}

pub fn parse_chat_event(event: &str) -> Result<ChatEvent, String> {
    if event.trim() == "[DONE]" {
        return Ok(ChatEvent::Done);
    }
    let value: serde_json::Value =
        serde_json::from_str(event).map_err(|_| "接口返回的流式数据不是有效 JSON。")?;
    if value.get("error").is_some() {
        return Err("模型服务在流式响应中返回错误，请检查服务配置或稍后重试。".into());
    }
    let Some(choice) = value
        .get("choices")
        .and_then(|v| v.as_array())
        .and_then(|v| v.first())
    else {
        return Ok(ChatEvent::Ignored);
    };
    let text = choice
        .pointer("/delta/content")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let finish_reason = choice
        .get("finish_reason")
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    if !text.is_empty() || finish_reason.is_some() {
        return Ok(ChatEvent::Chunk {
            text: text.to_owned(),
            finish_reason,
        });
    }
    Ok(ChatEvent::Ignored)
}

pub fn parse_anthropic_event(data: &str) -> Result<ChatEvent, String> {
    let value: serde_json::Value =
        serde_json::from_str(data).map_err(|_| "Claude 流式数据不是有效 JSON。")?;
    match value["type"].as_str().unwrap_or("") {
        "error" => Err("Claude 返回流式错误，请检查模型、额度或稍后重试。".into()),
        "message_stop" => Ok(ChatEvent::Done),
        "content_block_delta" if value["delta"]["type"] == "text_delta" => Ok(ChatEvent::Chunk {
            text: value["delta"]["text"].as_str().unwrap_or_default().into(),
            finish_reason: None,
        }),
        "content_block_start" if value["content_block"]["type"] == "text" => Ok(ChatEvent::Chunk {
            text: value["content_block"]["text"]
                .as_str()
                .unwrap_or_default()
                .into(),
            finish_reason: None,
        }),
        "message_delta" => Ok(ChatEvent::Chunk {
            text: String::new(),
            finish_reason: value["delta"]["stop_reason"]
                .as_str()
                .map(|reason| match reason {
                    "end_turn" | "stop_sequence" => "stop".into(),
                    "max_tokens" => "length".into(),
                    other => other.into(),
                }),
        }),
        _ => Ok(ChatEvent::Ignored),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_crlf_and_events_can_split_at_every_byte_boundary() {
        let wire = "\u{feff}: heartbeat\r\ndata: {\"choices\":[{\"delta\":{\"content\":\"翻译\"}}]}\r\n\r\ndata: [DONE]\n\n";
        for split in 0..=wire.len() {
            let mut d = Decoder::default();
            let mut events = d.push(&wire.as_bytes()[..split]).unwrap();
            events.extend(d.push(&wire.as_bytes()[split..]).unwrap());
            assert_eq!(events.len(), 2, "split {split}");
            assert_eq!(
                parse_chat_event(&events[0]).unwrap(),
                ChatEvent::Chunk {
                    text: "翻译".into(),
                    finish_reason: None
                }
            );
            assert_eq!(parse_chat_event(&events[1]).unwrap(), ChatEvent::Done);
            assert!(!d.has_incomplete_event());
        }
    }

    #[test]
    fn handles_multiline_data_and_cr_only_streams() {
        let mut d = Decoder::default();
        assert_eq!(
            d.push(b"event: message\rdata: first\rdata: second\r\r")
                .unwrap(),
            vec!["first\nsecond"]
        );
    }

    #[test]
    fn ignores_reasoning_and_never_treats_heartbeat_as_translation() {
        let data = r#"{"choices":[{"delta":{"reasoning_content":"private reasoning"}}]}"#;
        assert_eq!(parse_chat_event(data).unwrap(), ChatEvent::Ignored);
        assert_eq!(Decoder::default().push(b": ping\n\n").unwrap().len(), 0);
    }

    #[test]
    fn retains_finish_reason_when_last_chunk_also_contains_content() {
        let data = r#"{"choices":[{"delta":{"content":"最后一段"},"finish_reason":"length"}]}"#;
        assert_eq!(
            parse_chat_event(data).unwrap(),
            ChatEvent::Chunk {
                text: "最后一段".into(),
                finish_reason: Some("length".into())
            }
        );
    }

    #[test]
    fn detects_truncated_and_malformed_streams() {
        let mut d = Decoder::default();
        assert!(d.push(b"data: {\"partial\"").unwrap().is_empty());
        assert!(d.has_incomplete_event());
        assert!(parse_chat_event("not json").is_err());
        assert!(
            parse_chat_event(r#"{"error":{"message":"secret-containing upstream body"}}"#).is_err()
        );
    }

    #[test]
    fn bounds_an_event_even_when_server_never_sends_delimiter() {
        assert!(
            Decoder::default()
                .push(&vec![b'a'; MAX_EVENT_BYTES + 1])
                .is_err()
        );
    }
}

/// MiniMax may return reasoning wrapped in <think> despite requesting a split.
/// Buffer just tag prefixes; reasoning never reaches the translation callback.
#[derive(Default)]
pub struct ThoughtFilter {
    pending: String,
    state: u8, // 0 = opening prefix, 1 = reasoning, 2 = visible content
}
impl ThoughtFilter {
    pub fn push(&mut self, value: &str) -> String {
        if self.state == 2 {
            return value.into();
        }
        self.pending.push_str(value);
        if self.state == 0 {
            let candidate = self.pending.trim_start();
            if let Some(rest) = candidate.strip_prefix("<think>") {
                self.pending = rest.into();
                self.state = 1;
            } else if "<think>".starts_with(candidate) {
                return String::new();
            } else {
                self.state = 2;
                return std::mem::take(&mut self.pending);
            }
        }
        if let Some(index) = self.pending.find("</think>") {
            let output = self.pending[index + 8..].to_string();
            self.pending.clear();
            self.state = 2;
            output
        } else {
            // Keep a suffix that could be the start of the closing ASCII tag.
            let suffix = (1..8)
                .rev()
                .find(|&len| self.pending.ends_with(&"</think>"[..len]))
                .unwrap_or(0);
            self.pending = self.pending[self.pending.len() - suffix..].to_string();
            String::new()
        }
    }
    pub fn finish(&mut self) -> String {
        if self.state == 1 {
            self.pending.clear();
            String::new()
        } else {
            self.state = 2;
            std::mem::take(&mut self.pending)
        }
    }
}

#[cfg(test)]
mod thought_tests {
    use super::ThoughtFilter;
    #[test]
    fn reasoning_tags_can_split_at_any_character_boundary() {
        let input = " \n<think>private reasoning 中文</think>最终译文";
        for split in input
            .char_indices()
            .map(|(i, _)| i)
            .chain(std::iter::once(input.len()))
        {
            let mut filter = ThoughtFilter::default();
            let result =
                filter.push(&input[..split]) + &filter.push(&input[split..]) + &filter.finish();
            assert_eq!(result, "最终译文");
        }
        let mut filter = ThoughtFilter::default();
        let result = input
            .chars()
            .map(|c| filter.push(&c.to_string()))
            .collect::<String>()
            + &filter.finish();
        assert_eq!(result, "最终译文");
    }
    #[test]
    fn ordinary_translation_is_not_buffered_and_unclosed_reasoning_is_hidden() {
        let mut filter = ThoughtFilter::default();
        assert_eq!(filter.push("Hello"), "Hello");
        assert_eq!(filter.push(" world"), " world");
        let mut filter = ThoughtFilter::default();
        assert_eq!(filter.push("<think>never show this"), "");
        assert_eq!(filter.finish(), "");
    }
}
