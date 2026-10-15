use super::Error;
use crate::config::{Profile, Provider};
use reqwest::{Client, RequestBuilder, header};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub(super) fn authorize(
    request: RequestBuilder,
    key: &str,
    provider: Provider,
) -> Result<RequestBuilder, Error> {
    if key.is_empty() {
        return Ok(request);
    }
    let raw = if provider == Provider::Anthropic {
        key.to_owned()
    } else {
        format!("Bearer {key}")
    };
    let mut value = header::HeaderValue::from_str(&raw)
        .map_err(|_| Error::Configuration("API Key 包含无效字符。".into()))?;
    value.set_sensitive(true);
    Ok(if provider == Provider::Anthropic {
        request
            .header("x-api-key", value)
            .header("anthropic-version", "2023-06-01")
    } else {
        request.header(header::AUTHORIZATION, value)
    })
}

pub(super) fn request(
    client: &Client,
    profile: &Profile,
    key: &str,
    input: &str,
    languages: (&str, Option<&str>),
    instruction: &str,
    url: url::Url,
) -> Result<RequestBuilder, Error> {
    let (target, source) = languages;
    if profile.provider != Provider::Custom && key.is_empty() {
        return Err(Error::Configuration(
            "请先配置此服务的 API Key 或应用密钥。".into(),
        ));
    }
    if profile.provider == Provider::Youdao {
        let salt = uuid::Uuid::new_v4().to_string();
        let time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| Error::Configuration("系统时间无效。".into()))?
            .as_secs()
            .to_string();
        let sign = youdao_sign(&profile.app_id, input, &salt, &time, key);
        return Ok(client.post(url).form(&[
            ("q", input),
            ("from", youdao_language(source.unwrap_or("auto"))),
            ("to", youdao_language(target)),
            ("appKey", profile.app_id.trim()),
            ("salt", &salt),
            ("curtime", &time),
            ("signType", "v3"),
            ("sign", &sign),
            ("strict", "true"),
        ]));
    }
    let mut body = if profile.provider == Provider::Anthropic {
        json!({"model":profile.model.trim(),"max_tokens":8192,"stream":profile.streaming,"system":instruction,"messages":[{"role":"user","content":input}]})
    } else {
        json!({"model":profile.model.trim(),"stream":profile.streaming,"messages":[{"role":"system","content":instruction},{"role":"user","content":input}]})
    };
    if !matches!(profile.provider, Provider::Openai | Provider::Anthropic) {
        // Zhipu rejects more than two decimal places. Converting f32 directly
        // into Value widens 0.2 to 0.20000000298023224; normalize before encoding.
        body["temperature"] = if profile.provider == Provider::Zhipu {
            json!((f64::from(profile.temperature) * 100.0).round() / 100.0)
        } else {
            json!(profile.temperature)
        };
    }
    match profile.provider {
        Provider::Deepseek => {
            body["thinking"] = json!({"type":"disabled"});
        }
        Provider::Zhipu
            if ["glm-4.5", "glm-4.7"].iter().any(|family| {
                profile.model.trim() == *family
                    || profile.model.trim().starts_with(&format!("{family}-"))
            }) =>
        {
            body["thinking"] = json!({"type":"disabled"});
        }
        Provider::Siliconflow if profile.model.trim() == "Qwen/Qwen3-8B" => {
            body["enable_thinking"] = json!(false);
        }
        Provider::Gemini
            if matches!(
                profile.model.trim(),
                "gemini-2.5-flash" | "gemini-2.5-flash-lite"
            ) =>
        {
            body["reasoning_effort"] = json!("none");
        }
        Provider::Minimax | Provider::MinimaxChina => {
            body["reasoning_split"] = json!(true);
            if profile.model == "MiniMax-M3" {
                body["thinking"] = json!({"type":"disabled"});
            }
        }
        _ => {}
    }
    authorize(client.post(url).json(&body), key, profile.provider)
}

pub(super) fn response(profile: &Profile, value: &Value) -> Result<(String, String), Error> {
    if profile.provider == Provider::Youdao {
        let code = value
            .get("errorCode")
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| v.to_string())
            })
            .unwrap_or_default();
        if code != "0" {
            return Err(Error::Protocol(
                "有道请求失败，请检查应用 ID、密钥、服务开通状态和额度。".into(),
            ));
        }
        let list = value
            .pointer("/basic/explains")
            .and_then(Value::as_array)
            .or_else(|| value.get("translation").and_then(Value::as_array));
        let text = list
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join("\n");
        return Ok((text, "stop".into()));
    }
    if profile.provider == Provider::Anthropic {
        let text = value
            .get("content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|v| v["type"] == "text")
            .filter_map(|v| v["text"].as_str())
            .collect::<Vec<_>>()
            .join("");
        let reason = normalize_stop(value["stop_reason"].as_str().unwrap_or("unknown"));
        return Ok((text, reason.into()));
    }
    let choice = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|v| v.first())
        .ok_or_else(|| Error::Protocol("响应缺少 choices。".into()))?;
    let text = choice
        .pointer("/message/content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    Ok((
        text,
        choice["finish_reason"]
            .as_str()
            .unwrap_or("stop")
            .to_owned(),
    ))
}

pub(crate) fn normalize_stop(reason: &str) -> &str {
    match reason {
        "end_turn" | "stop_sequence" => "stop",
        "max_tokens" => "length",
        other => other,
    }
}
fn youdao_language(language: &str) -> &str {
    match language {
        "zh-Hans" | "zh-CN" | "简体中文" => "zh-CHS",
        "zh-Hant" | "zh-TW" | "繁體中文" => "zh-CHT",
        other => other,
    }
}
pub(super) fn youdao_sign(app_id: &str, input: &str, salt: &str, time: &str, key: &str) -> String {
    let chars: Vec<_> = input.chars().collect();
    let truncated = if chars.len() > 20 {
        format!(
            "{}{}{}",
            chars[..10].iter().collect::<String>(),
            chars.len(),
            chars[chars.len() - 10..].iter().collect::<String>()
        )
    } else {
        input.into()
    };
    let mut hash = Sha256::new();
    for part in [app_id.trim(), &truncated, salt, time, key] {
        hash.update(part.as_bytes());
    }
    format!("{:x}", hash.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn body(provider: Provider, input: &str) -> (reqwest::Request, Profile) {
        let p = Profile {
            provider,
            model: "fixture-model".into(),
            app_id: "app-fixture".into(),
            ..Profile::default()
        };
        let r = request(
            &Client::new(),
            &p,
            "fixture-secret",
            input,
            ("zh-Hans", Some("en")),
            "Return only translated text.",
            p.request_url().unwrap(),
        )
        .unwrap()
        .build()
        .unwrap();
        (r, p)
    }
    #[test]
    fn zhipu_temperature_on_the_wire_has_at_most_two_decimal_places() {
        for (temperature, expected) in [
            (0.2, "0.2"),
            (0.3, "0.3"),
            (0.12, "0.12"),
            (0.123, "0.12"),
            (0.0, "0.0"),
            (1.0, "1.0"),
        ] {
            let p = Profile {
                provider: Provider::Zhipu,
                model: "glm-4-flash-250414".into(),
                temperature,
                ..Profile::default()
            };
            let request = request(
                &Client::new(),
                &p,
                "fixture-key",
                "This is a connection test.",
                ("zh-Hans", None),
                "Translate into Chinese.",
                p.request_url().unwrap(),
            )
            .unwrap()
            .build()
            .unwrap();
            let payload: Value =
                serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap();
            assert_eq!(payload["temperature"].to_string(), expected);
            assert!(payload.get("thinking").is_none());
            assert!(payload.get("max_tokens").is_none());
        }
    }
    #[test]
    fn compatible_services_use_their_own_host_and_sensitive_bearer_auth() {
        for (provider, host, path) in [
            (
                Provider::Gemini,
                "generativelanguage.googleapis.com",
                "/v1beta/openai/chat/completions",
            ),
            (Provider::Openai, "api.openai.com", "/v1/chat/completions"),
            (
                Provider::Qwen,
                "dashscope.aliyuncs.com",
                "/compatible-mode/v1/chat/completions",
            ),
            (
                Provider::Zhipu,
                "open.bigmodel.cn",
                "/api/paas/v4/chat/completions",
            ),
            (
                Provider::Siliconflow,
                "api.siliconflow.cn",
                "/v1/chat/completions",
            ),
            (
                Provider::Openrouter,
                "openrouter.ai",
                "/api/v1/chat/completions",
            ),
        ] {
            let (r, _) = body(provider, "hello");
            assert_eq!(r.url().host_str(), Some(host));
            assert_eq!(r.url().path(), path);
            assert!(r.headers()[header::AUTHORIZATION].is_sensitive());
            let v: Value = serde_json::from_slice(r.body().unwrap().as_bytes().unwrap()).unwrap();
            assert_eq!(v["messages"][1]["content"], "hello");
            if provider == Provider::Openai {
                assert!(v.get("temperature").is_none());
            }
        }
    }
    #[test]
    fn claude_uses_native_headers_system_and_text_blocks() {
        let (r, p) = body(Provider::Anthropic, "hello");
        assert_eq!(r.url().path(), "/v1/messages");
        assert_eq!(
            p.models_url().unwrap().as_str(),
            "https://api.anthropic.com/v1/models"
        );
        assert!(r.headers()["x-api-key"].is_sensitive());
        assert_eq!(r.headers()["anthropic-version"], "2023-06-01");
        assert!(!r.headers().contains_key(header::AUTHORIZATION));
        let v: Value = serde_json::from_slice(r.body().unwrap().as_bytes().unwrap()).unwrap();
        assert_eq!(v["system"], "Return only translated text.");
        assert_eq!(v["messages"].as_array().unwrap().len(), 1);
        assert!(v.get("temperature").is_none());
        let (text,reason)=response(&p,&json!({"content":[{"type":"thinking","thinking":"hidden"},{"type":"text","text":"你好"}],"stop_reason":"end_turn"})).unwrap();
        assert_eq!(text, "你好");
        assert_eq!(reason, "stop");
    }
    #[test]
    fn youdao_signs_utf8_input_without_sending_the_secret() {
        assert_eq!(
            youdao_sign(
                "app-fixture",
                "一二三四五六七八九十甲乙丙丁戊己庚辛壬癸子丑寅卯",
                "fixture-salt",
                "12345678",
                "fixture-secret"
            ),
            "7329abff9120a58d590eef4b1c8e1cb77c28998df76eabbe599c1f0e0830f7c1"
        );
        let (r, p) = body(Provider::Youdao, "hello 你好");
        assert_eq!(r.url().as_str(), "https://openapi.youdao.com/api");
        assert!(r.url().query().is_none());
        let values: std::collections::HashMap<_, _> =
            url::form_urlencoded::parse(r.body().unwrap().as_bytes().unwrap())
                .into_owned()
                .collect();
        assert_eq!(values["q"], "hello 你好");
        assert_eq!(values["to"], "zh-CHS");
        assert_eq!(values["from"], "en");
        assert_eq!(values["signType"], "v3");
        assert_eq!(
            values["sign"],
            youdao_sign(
                &p.app_id,
                &values["q"],
                &values["salt"],
                &values["curtime"],
                "fixture-secret"
            )
        );
        assert!(
            !String::from_utf8_lossy(r.body().unwrap().as_bytes().unwrap())
                .contains("fixture-secret")
        );
        assert!(response(&p, &json!({"errorCode":"202"})).is_err());
        assert_eq!(
            response(&p, &json!({"errorCode":"0","translation":["你好"]}))
                .unwrap()
                .0,
            "你好"
        );
    }
    #[test]
    fn translation_disables_thinking_only_for_supported_provider_models() {
        for (provider, model, field, expected) in [
            (
                Provider::Zhipu,
                "glm-4.7-flash",
                "thinking",
                json!({"type":"disabled"}),
            ),
            (
                Provider::Zhipu,
                "glm-4.5-flash",
                "thinking",
                json!({"type":"disabled"}),
            ),
            (
                Provider::Siliconflow,
                "Qwen/Qwen3-8B",
                "enable_thinking",
                json!(false),
            ),
            (
                Provider::Gemini,
                "gemini-2.5-flash",
                "reasoning_effort",
                json!("none"),
            ),
            (
                Provider::Gemini,
                "gemini-2.5-flash-lite",
                "reasoning_effort",
                json!("none"),
            ),
            (
                Provider::Gemini,
                "gemini-2.5-pro",
                "reasoning_effort",
                Value::Null,
            ),
            (
                Provider::Gemini,
                "gemini-3.8-flash",
                "reasoning_effort",
                Value::Null,
            ),
            (Provider::Zhipu, "glm-z1-flash", "thinking", Value::Null),
            (
                Provider::Zhipu,
                "glm-4-flash-250414",
                "thinking",
                Value::Null,
            ),
            (
                Provider::Siliconflow,
                "Qwen/Qwen2.5-7B-Instruct",
                "enable_thinking",
                Value::Null,
            ),
            (
                Provider::Custom,
                "Qwen/Qwen3-8B",
                "enable_thinking",
                Value::Null,
            ),
        ] {
            let p = Profile {
                provider,
                model: model.into(),
                endpoint: "http://localhost:10000/v1".into(),
                ..Default::default()
            };
            let r = request(
                &Client::new(),
                &p,
                "synthetic-key",
                "Hello",
                ("zh-Hans", None),
                "Translate",
                p.request_url().unwrap(),
            )
            .unwrap()
            .build()
            .unwrap();
            let body: Value =
                serde_json::from_slice(r.body().unwrap().as_bytes().unwrap()).unwrap();
            assert_eq!(body[field], expected, "{provider:?} {model}");
        }
    }
}
