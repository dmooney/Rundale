//! JSON-RPC 2.0 message types: `Request`, `Response`, `RpcError`.
//!
//! Pure wire-layer data with no I/O. Split out of the monolithic `jsonrpc`
//! module (#1200) so framing/transport (`dispatch`) can grow independently
//! of the protocol value types.

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// JSON-RPC 2.0 request / notification.
///
/// `id` is `None` for notifications. We accept any JSON value for `id`
/// (string, number, null) per the spec and echo it back verbatim.
#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    #[allow(dead_code)] // verified during deserialisation; kept for completeness
    pub jsonrpc: String,
    pub method: String,
    #[serde(default)]
    pub params: Value,
    #[serde(default, deserialize_with = "deserialize_optional_id")]
    pub id: Option<Value>,
}

/// JSON-RPC 2.0 response. Either `result` or `error` is set, never both.
#[derive(Debug, Clone, Serialize)]
pub struct Response {
    pub jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
    pub id: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

fn deserialize_optional_id<'de, D>(deserializer: D) -> Result<Option<Value>, D::Error>
where
    D: Deserializer<'de>,
{
    Value::deserialize(deserializer).map(Some)
}

impl RpcError {
    pub fn parse_error(detail: impl Into<String>) -> Self {
        Self {
            code: -32700,
            message: detail.into(),
            data: None,
        }
    }
    pub fn invalid_request(detail: impl Into<String>) -> Self {
        Self {
            code: -32600,
            message: detail.into(),
            data: None,
        }
    }
    pub fn method_not_found(method: &str) -> Self {
        Self {
            code: -32601,
            message: format!("method not found: {method}"),
            data: None,
        }
    }
    pub fn invalid_params(detail: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: detail.into(),
            data: None,
        }
    }
    pub fn internal(detail: impl Into<String>) -> Self {
        Self {
            code: -32603,
            message: detail.into(),
            data: None,
        }
    }
}

impl Response {
    pub fn ok(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            result: Some(result),
            error: None,
            id,
        }
    }
    pub fn err(id: Value, error: RpcError) -> Self {
        Self {
            jsonrpc: "2.0",
            result: None,
            error: Some(error),
            id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_serialises_only_result_when_ok() {
        let resp = Response::ok(Value::from(7), serde_json::json!({"hello": "world"}));
        let s = serde_json::to_string(&resp).unwrap();
        assert!(s.contains("\"result\""));
        assert!(!s.contains("\"error\""));
        assert!(s.contains("\"id\":7"));
    }

    #[test]
    fn response_serialises_only_error_when_err() {
        let resp = Response::err(Value::from("abc"), RpcError::method_not_found("foo"));
        let s = serde_json::to_string(&resp).unwrap();
        assert!(s.contains("\"error\""));
        assert!(!s.contains("\"result\""));
        assert!(s.contains("method not found: foo"));
    }
}
