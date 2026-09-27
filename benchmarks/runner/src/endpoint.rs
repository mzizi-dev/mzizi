//! The llama.cpp `llama-server` client, behind two traits so tests need no server.

use std::time::Duration;

use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub role: String,
    pub content: String,
}

impl Message {
    pub fn new(role: &str, content: &str) -> Message {
        Message {
            role: role.into(),
            content: content.into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct GenParams {
    pub model: String,
    pub temperature: f64,
    pub seed: u64,
    pub max_tokens: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChatReply {
    pub content: String,
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
}

pub trait ChatModel {
    fn chat(&self, messages: &[Message], params: &GenParams) -> Result<ChatReply, String>;
}

pub trait Tokenizer {
    fn count_tokens(&self, text: &str) -> Result<u64, String>;
}

pub struct HttpEndpoint {
    base: String,
    agent: ureq::Agent,
}

impl HttpEndpoint {
    pub fn new(base: &str, timeout: Duration) -> HttpEndpoint {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            .http_status_as_error(false)
            .build()
            .into();
        HttpEndpoint {
            base: base.trim_end_matches('/').to_string(),
            agent,
        }
    }

    fn post(&self, path: &str, body: &Value) -> Result<String, String> {
        let url = format!("{}{path}", self.base);
        let mut resp = self
            .agent
            .post(&url)
            .header("content-type", "application/json")
            .send(body.to_string())
            .map_err(|e| format!("POST {url}: {e}"))?;
        let status = resp.status().as_u16();
        let text = resp
            .body_mut()
            .with_config()
            .limit(64 * 1024 * 1024)
            .read_to_string()
            .map_err(|e| format!("POST {url}: reading body: {e}"))?;
        if !(200..300).contains(&status) {
            return Err(format!("POST {url}: HTTP {status}: {text}"));
        }
        Ok(text)
    }
}

pub fn chat_request_body(messages: &[Message], p: &GenParams) -> Value {
    let msgs: Vec<Value> = messages
        .iter()
        .map(|m| json!({"role": m.role, "content": m.content}))
        .collect();
    json!({
        "model": p.model,
        "messages": msgs,
        "temperature": p.temperature,
        "seed": p.seed,
        "max_tokens": p.max_tokens,
        "stream": false,
    })
}

pub fn parse_chat_response(body: &str) -> Result<ChatReply, String> {
    let v: Value =
        serde_json::from_str(body).map_err(|e| format!("chat response is not JSON: {e}"))?;
    let content = v
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .ok_or("chat response has no choices[0].message.content")?
        .to_string();
    Ok(ChatReply {
        content,
        prompt_tokens: v.pointer("/usage/prompt_tokens").and_then(Value::as_u64),
        completion_tokens: v
            .pointer("/usage/completion_tokens")
            .and_then(Value::as_u64),
    })
}

/// `/tokenize` returns `{"tokens":[…]}`; entries may be ids or `{id, piece}` objects
/// (`with_pieces`). Either way the count is the array length.
pub fn parse_tokenize_response(body: &str) -> Result<u64, String> {
    let v: Value =
        serde_json::from_str(body).map_err(|e| format!("tokenize response is not JSON: {e}"))?;
    let tokens = v
        .get("tokens")
        .and_then(Value::as_array)
        .ok_or("tokenize response has no tokens array")?;
    Ok(tokens.len() as u64)
}

impl ChatModel for HttpEndpoint {
    fn chat(&self, messages: &[Message], params: &GenParams) -> Result<ChatReply, String> {
        let body = self.post("/v1/chat/completions", &chat_request_body(messages, params))?;
        parse_chat_response(&body)
    }
}

impl Tokenizer for HttpEndpoint {
    fn count_tokens(&self, text: &str) -> Result<u64, String> {
        let body = self.post("/tokenize", &json!({ "content": text }))?;
        parse_tokenize_response(&body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_openai_shape_with_usage() {
        let body = r#"{"choices":[{"message":{"role":"assistant","content":"hi"}}],
                       "usage":{"prompt_tokens":12,"completion_tokens":3,"total_tokens":15}}"#;
        let r = parse_chat_response(body).unwrap();
        assert_eq!(r.content, "hi");
        assert_eq!(r.prompt_tokens, Some(12));
        assert_eq!(r.completion_tokens, Some(3));
    }

    #[test]
    fn missing_usage_is_none_not_zero() {
        let body = r#"{"choices":[{"message":{"content":"x"}}]}"#;
        let r = parse_chat_response(body).unwrap();
        assert_eq!(r.prompt_tokens, None);
        assert_eq!(r.completion_tokens, None);
        assert!(parse_chat_response(r#"{"choices":[]}"#).is_err());
    }

    #[test]
    fn tokenize_counts_ids_or_pieces() {
        assert_eq!(parse_tokenize_response(r#"{"tokens":[1,2,3]}"#).unwrap(), 3);
        assert_eq!(
            parse_tokenize_response(r#"{"tokens":[{"id":1,"piece":"a"}]}"#).unwrap(),
            1
        );
        assert!(parse_tokenize_response("{}").is_err());
    }

    #[test]
    fn request_carries_seed_and_temperature() {
        let p = GenParams {
            model: "m".into(),
            temperature: 0.2,
            seed: 7,
            max_tokens: 100,
        };
        let b = chat_request_body(&[Message::new("user", "u")], &p);
        assert_eq!(b["seed"], 7);
        assert_eq!(b["temperature"], 0.2);
        assert_eq!(b["messages"][0]["role"], "user");
    }
}
