//! Conservative recovery when a model omits the vocabulary sentinel.
//! Only a complete object with the requested six-field shape confirms metadata.
use serde_json::Value;

const LOOKAHEAD_LIMIT: usize = 4096;
const FIELDS: [&str; 6] = ["word", "surface", "phonetic", "pos", "meaning", "level"];

pub(super) fn source_contains_schema(source: &str) -> bool {
    FIELDS.iter().all(|field| {
        ['"', '\''].iter().any(|quote| {
            let key = format!("{quote}{field}{quote}");
            source
                .match_indices(&key)
                .any(|(at, _)| source[at + key.len()..].trim_start().starts_with(':'))
        })
    })
}

/// (array offset, still a possible prefix). Failed probes must remain visible.
pub(super) fn boundary(input: &str) -> (Option<usize>, bool) {
    let mut end = input.len().min(LOOKAHEAD_LIMIT);
    while !input.is_char_boundary(end) {
        end -= 1;
    }
    let text = &input[..end];
    let pending = || (None, end == input.len());
    let mut body = text.trim_start_matches([' ', '\t']);
    if body.is_empty() || text.len() - body.len() > 3 {
        return (None, false);
    }
    if "---".starts_with(body) {
        return pending();
    }
    if let Some(rest) = body.strip_prefix("---") {
        if !rest.starts_with(char::is_whitespace) {
            return (None, false);
        }
        body = rest.trim_start();
        if body.is_empty() {
            return (None, text.len() <= 128);
        }
    }
    if "```".starts_with(body) {
        return pending();
    }
    if let Some(rest) = body.strip_prefix("```") {
        if "json".starts_with(rest) || "JSON".starts_with(rest) {
            return pending();
        }
        body = rest
            .strip_prefix("json")
            .or_else(|| rest.strip_prefix("JSON"))
            .unwrap_or(rest);
        if !body.starts_with(char::is_whitespace) {
            return (None, false);
        }
        body = body.trim_start();
        if body.is_empty() {
            return pending();
        }
    }
    let Some(array) = body.strip_prefix('[') else {
        return (None, false);
    };
    let first = array.trim_start();
    if first.is_empty() {
        return pending();
    }
    if !first.starts_with('{') {
        return (None, false);
    }
    let parse_first = |raw: &str| {
        serde_json::Deserializer::from_str(raw)
            .into_iter::<Value>()
            .next()
    };
    let parsed = match parse_first(first) {
        // Reuse the final parser's narrow repair, within the same lookahead bound.
        Some(Err(error)) if !error.is_eof() => parse_first(&super::without_trailing_commas(first)),
        parsed => parsed,
    };
    match parsed {
        Some(Ok(Value::Object(object)))
            if object.len() == FIELDS.len()
                && FIELDS.iter().all(|key| object.contains_key(*key)) =>
        {
            (Some(text.len() - body.len()), false)
        }
        Some(Err(error)) if error.is_eof() => pending(),
        _ => (None, false),
    }
}
