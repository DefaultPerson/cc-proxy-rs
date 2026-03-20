//! Claude Code CLI NDJSON message types.
//!
//! When the CLI runs with `--output-format stream-json`, each stdout line
//! is a JSON object discriminated by the `"type"` field. We parse only the
//! types we care about and pass `stream_event.event` through as raw JSON.

use std::collections::HashMap;

use serde::Deserialize;

/// Top-level CLI message discriminated on `"type"`.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum CliMessage {
    #[serde(rename = "system")]
    System(SystemMessage),

    #[serde(rename = "stream_event")]
    StreamEvent(StreamEventMessage),

    #[serde(rename = "assistant")]
    Assistant(AssistantMessage),

    #[serde(rename = "result")]
    Result(ResultMessage),

    #[serde(rename = "rate_limit_event")]
    RateLimit(RateLimitMessage),

    /// Catch-all for types we don't need: tool_progress, auth_status,
    /// hooks, tasks, prompt_suggestion, etc.
    #[serde(other)]
    Unknown,
}

// ---------------------------------------------------------------------------
// system (subtype: "init")
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SystemMessage {
    pub subtype: Option<String>,
    pub session_id: Option<String>,
    pub model: Option<String>,
    pub claude_code_version: Option<String>,
    pub tools: Option<Vec<String>>,
}

// ---------------------------------------------------------------------------
// stream_event — contains a raw Anthropic streaming event in `.event`
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct StreamEventMessage {
    /// The raw Anthropic streaming event (content_block_delta, etc.)
    /// Kept as Value to avoid re-serializing — we pass it through to SSE.
    pub event: serde_json::Value,

    pub parent_tool_use_id: Option<String>,

    #[allow(dead_code)]
    pub session_id: Option<String>,
}

// ---------------------------------------------------------------------------
// assistant — full message after each turn
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct AssistantMessage {
    pub message: Option<AssistantMessageBody>,

    #[allow(dead_code)]
    pub session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AssistantMessageBody {
    pub model: Option<String>,
    pub content: Option<Vec<serde_json::Value>>,
    pub stop_reason: Option<String>,
    pub usage: Option<Usage>,
}

// ---------------------------------------------------------------------------
// result — final message with usage/cost
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ResultMessage {
    pub subtype: Option<String>,
    pub result: Option<String>,
    pub session_id: Option<String>,
    pub is_error: Option<bool>,
    pub num_turns: Option<u64>,
    pub duration_ms: Option<u64>,
    pub duration_api_ms: Option<u64>,
    pub total_cost_usd: Option<f64>,
    pub stop_reason: Option<String>,
    pub usage: Option<Usage>,
    #[serde(rename = "modelUsage")]
    pub model_usage: Option<HashMap<String, ModelUsage>>,
    /// Only present on error subtypes
    pub errors: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Usage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct ModelUsage {
    #[serde(rename = "inputTokens")]
    pub input_tokens: Option<u64>,
    #[serde(rename = "outputTokens")]
    pub output_tokens: Option<u64>,
    #[serde(rename = "cacheReadInputTokens")]
    pub cache_read_input_tokens: Option<u64>,
    #[serde(rename = "cacheCreationInputTokens")]
    pub cache_creation_input_tokens: Option<u64>,
    #[serde(rename = "costUSD")]
    pub cost_usd: Option<f64>,
}

// ---------------------------------------------------------------------------
// rate_limit_event
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RateLimitMessage {
    pub rate_limit_info: Option<RateLimitInfo>,
}

#[derive(Debug, Deserialize)]
pub struct RateLimitInfo {
    pub status: Option<String>,
    #[serde(rename = "resetsAt")]
    pub resets_at: Option<u64>,
    #[serde(rename = "rateLimitType")]
    pub rate_limit_type: Option<String>,
    pub utilization: Option<f64>,
}

impl ResultMessage {
    pub fn is_success(&self) -> bool {
        self.subtype.as_deref() == Some("success")
    }

    pub fn is_error(&self) -> bool {
        self.subtype
            .as_deref()
            .is_some_and(|s| s.starts_with("error"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_system_init() {
        let json = r#"{"type":"system","subtype":"init","session_id":"abc-123","model":"claude-sonnet-4-6","claude_code_version":"2.1.63","tools":["Read","Edit","Bash"]}"#;
        let msg: CliMessage = serde_json::from_str(json).unwrap();
        match msg {
            CliMessage::System(sys) => {
                assert_eq!(sys.subtype.as_deref(), Some("init"));
                assert_eq!(sys.session_id.as_deref(), Some("abc-123"));
                assert_eq!(sys.model.as_deref(), Some("claude-sonnet-4-6"));
            }
            _ => panic!("expected System"),
        }
    }

    #[test]
    fn parse_stream_event() {
        let json = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}},"parent_tool_use_id":null,"session_id":"abc"}"#;
        let msg: CliMessage = serde_json::from_str(json).unwrap();
        match msg {
            CliMessage::StreamEvent(se) => {
                assert_eq!(se.event["type"], "content_block_delta");
                assert_eq!(se.event["delta"]["text"], "Hello");
                assert!(se.parent_tool_use_id.is_none());
            }
            _ => panic!("expected StreamEvent"),
        }
    }

    #[test]
    fn parse_result_success() {
        let json = r#"{"type":"result","subtype":"success","result":"Done","session_id":"abc","is_error":false,"num_turns":3,"duration_ms":5000,"total_cost_usd":0.01,"stop_reason":"end_turn","usage":{"input_tokens":100,"output_tokens":50}}"#;
        let msg: CliMessage = serde_json::from_str(json).unwrap();
        match msg {
            CliMessage::Result(r) => {
                assert!(r.is_success());
                assert!(!r.is_error());
                assert_eq!(r.result.as_deref(), Some("Done"));
                assert_eq!(r.stop_reason.as_deref(), Some("end_turn"));
                let usage = r.usage.unwrap();
                assert_eq!(usage.input_tokens, Some(100));
                assert_eq!(usage.output_tokens, Some(50));
            }
            _ => panic!("expected Result"),
        }
    }

    #[test]
    fn parse_result_error() {
        let json = r#"{"type":"result","subtype":"error_max_turns","is_error":true,"errors":["Max turns reached"],"session_id":"abc","num_turns":50,"duration_ms":1000,"total_cost_usd":0.0,"usage":{"input_tokens":0,"output_tokens":0}}"#;
        let msg: CliMessage = serde_json::from_str(json).unwrap();
        match msg {
            CliMessage::Result(r) => {
                assert!(r.is_error());
                assert_eq!(r.errors.as_ref().unwrap()[0], "Max turns reached");
            }
            _ => panic!("expected Result"),
        }
    }

    #[test]
    fn parse_unknown_type() {
        let json = r#"{"type":"tool_progress","tool_use_id":"x","tool_name":"Bash"}"#;
        let msg: CliMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, CliMessage::Unknown));
    }

    #[test]
    fn parse_rate_limit_event() {
        let json = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed_warning","rateLimitType":"five_hour","utilization":0.85}}"#;
        let msg: CliMessage = serde_json::from_str(json).unwrap();
        match msg {
            CliMessage::RateLimit(rl) => {
                let info = rl.rate_limit_info.unwrap();
                assert_eq!(info.status.as_deref(), Some("allowed_warning"));
                assert_eq!(info.utilization, Some(0.85));
            }
            _ => panic!("expected RateLimit"),
        }
    }

    #[test]
    fn parse_rate_limit_rejected() {
        let json = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","resetsAt":1711000000,"rateLimitType":"five_hour"}}"#;
        let msg: CliMessage = serde_json::from_str(json).unwrap();
        match msg {
            CliMessage::RateLimit(rl) => {
                let info = rl.rate_limit_info.unwrap();
                assert_eq!(info.status.as_deref(), Some("rejected"));
                assert!(info.resets_at.is_some());
            }
            _ => panic!("expected RateLimit"),
        }
    }

    #[test]
    fn parse_assistant_message() {
        let json = r#"{"type":"assistant","message":{"model":"claude-sonnet-4-6","content":[{"type":"text","text":"Hello!"}],"stop_reason":"end_turn","usage":{"input_tokens":10,"output_tokens":5}},"session_id":"abc"}"#;
        let msg: CliMessage = serde_json::from_str(json).unwrap();
        match msg {
            CliMessage::Assistant(a) => {
                let body = a.message.unwrap();
                assert_eq!(body.model.as_deref(), Some("claude-sonnet-4-6"));
                assert_eq!(body.stop_reason.as_deref(), Some("end_turn"));
            }
            _ => panic!("expected Assistant"),
        }
    }
}
