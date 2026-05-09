//! Curated MCP tools that translate cleanly to a single Parish IPC call.
//!
//! Each tool is a thin descriptor: the JSON schema clients see, plus a
//! function that turns the validated arguments into a `(command, args)`
//! pair for [`crate::backend::TauriBackend::invoke`].
//!
//! There is also a generic escape hatch — `tauri_invoke` — that lets a
//! client call any backend command by name. New high-level tools can be
//! added here as gameplay flows surface them; they exist to give the
//! model better-typed, narrower affordances than the raw IPC surface.

use serde_json::{Value, json};

/// Description of one MCP tool, exposed by `tools/list`.
#[derive(Debug, Clone)]
pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
    /// Translates the validated `tools/call` arguments into the underlying
    /// backend `(command, args)` pair. Returning `Err` produces a JSON-RPC
    /// invalid-params error.
    pub translate: fn(&Value) -> Result<(String, Value), String>,
}

impl ToolDef {
    pub fn descriptor_json(&self) -> Value {
        json!({
            "name": self.name,
            "description": self.description,
            "inputSchema": self.input_schema,
        })
    }
}

fn empty_object_schema() -> Value {
    json!({"type": "object", "properties": {}, "additionalProperties": false})
}

fn require_string<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("missing required string field: `{key}`"))
}

fn translate_world_snapshot(_args: &Value) -> Result<(String, Value), String> {
    Ok(("get_world_snapshot".into(), Value::Null))
}

fn translate_map(_args: &Value) -> Result<(String, Value), String> {
    Ok(("get_map".into(), Value::Null))
}

fn translate_npcs_here(_args: &Value) -> Result<(String, Value), String> {
    Ok(("get_npcs_here".into(), Value::Null))
}

fn translate_save_state(_args: &Value) -> Result<(String, Value), String> {
    Ok(("get_save_state".into(), Value::Null))
}

fn translate_submit_input(args: &Value) -> Result<(String, Value), String> {
    let text = require_string(args, "text")?.to_string();
    let addressed_to = args
        .get("addressed_to")
        .cloned()
        .unwrap_or_else(|| json!([]));
    if !addressed_to.is_array() {
        return Err("`addressed_to` must be an array of strings".into());
    }
    Ok((
        "submit_input".into(),
        json!({"text": text, "addressed_to": addressed_to}),
    ))
}

fn translate_new_game(_args: &Value) -> Result<(String, Value), String> {
    // The HTTP route `POST /api/new-game` takes an empty body; we send
    // `{}` so the backend treats it as a POST.
    Ok(("new_game".into(), json!({})))
}

fn translate_save_game(_args: &Value) -> Result<(String, Value), String> {
    Ok(("save_game".into(), json!({})))
}

fn translate_load_branch(args: &Value) -> Result<(String, Value), String> {
    let branch_id = args
        .get("branch_id")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| "missing required integer field: `branch_id`".to_string())?;
    Ok(("load_branch".into(), json!({"branch_id": branch_id})))
}

fn translate_invoke(args: &Value) -> Result<(String, Value), String> {
    let command = require_string(args, "command")?.to_string();
    let inner = args.get("args").cloned().unwrap_or(Value::Null);
    Ok((command, inner))
}

/// Returns the curated tool registry. Tools are listed in the order the
/// MCP client will see them via `tools/list`.
pub fn registry() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "tauri_invoke",
            description: "Generic escape hatch — invoke any backend command by name with a JSON args object. \
                 Useful for endpoints that do not yet have a dedicated tool.",
            input_schema: json!({
                "type": "object",
                "required": ["command"],
                "properties": {
                    "command": {"type": "string", "description": "Tauri-style command name (e.g. get_world_snapshot)."},
                    "args": {"description": "Arguments object forwarded as-is. Omit or null for GET-style calls."}
                }
            }),
            translate: translate_invoke,
        },
        ToolDef {
            name: "parish_world_snapshot",
            description: "Reads the current world snapshot (clock, player location, weather, recent log entries).",
            input_schema: empty_object_schema(),
            translate: translate_world_snapshot,
        },
        ToolDef {
            name: "parish_map",
            description: "Reads the location graph plus the player's current position.",
            input_schema: empty_object_schema(),
            translate: translate_map,
        },
        ToolDef {
            name: "parish_npcs_here",
            description: "Lists the NPCs co-located with the player at this moment.",
            input_schema: empty_object_schema(),
            translate: translate_npcs_here,
        },
        ToolDef {
            name: "parish_save_state",
            description: "Reads metadata about the active save file and current branch.",
            input_schema: empty_object_schema(),
            translate: translate_save_state,
        },
        ToolDef {
            name: "parish_submit_input",
            description: "Sends a line of player input (a movement, action, or dialogue) to the running game. \
                 Optionally restrict the recipients of dialogue via `addressed_to`.",
            input_schema: json!({
                "type": "object",
                "required": ["text"],
                "properties": {
                    "text": {"type": "string", "minLength": 1, "maxLength": 2000},
                    "addressed_to": {
                        "type": "array",
                        "items": {"type": "string"},
                        "default": []
                    }
                }
            }),
            translate: translate_submit_input,
        },
        ToolDef {
            name: "parish_new_game",
            description: "Starts a fresh game on a new save branch, discarding any unsaved state.",
            input_schema: empty_object_schema(),
            translate: translate_new_game,
        },
        ToolDef {
            name: "parish_save_game",
            description: "Saves the current branch to the active save file. Returns a status message.",
            input_schema: empty_object_schema(),
            translate: translate_save_game,
        },
        ToolDef {
            name: "parish_load_branch",
            description: "Loads the named branch (by integer id) from the active save file.",
            input_schema: json!({
                "type": "object",
                "required": ["branch_id"],
                "properties": {
                    "branch_id": {"type": "integer"}
                }
            }),
            translate: translate_load_branch,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn submit_input_requires_text() {
        let err = translate_submit_input(&json!({})).unwrap_err();
        assert!(err.contains("text"));
    }

    #[test]
    fn submit_input_defaults_addressed_to_empty() {
        let (cmd, args) = translate_submit_input(&json!({"text": "look"})).unwrap();
        assert_eq!(cmd, "submit_input");
        assert_eq!(args["text"], "look");
        assert_eq!(args["addressed_to"], json!([]));
    }

    #[test]
    fn submit_input_rejects_non_array_addressed_to() {
        let err =
            translate_submit_input(&json!({"text": "hi", "addressed_to": "Mary"})).unwrap_err();
        assert!(err.contains("array"));
    }

    #[test]
    fn invoke_passes_args_through() {
        let (cmd, args) =
            translate_invoke(&json!({"command": "save_game", "args": {"x": 1}})).unwrap();
        assert_eq!(cmd, "save_game");
        assert_eq!(args, json!({"x": 1}));
    }

    #[test]
    fn invoke_defaults_args_to_null() {
        let (cmd, args) = translate_invoke(&json!({"command": "get_world_snapshot"})).unwrap();
        assert_eq!(cmd, "get_world_snapshot");
        assert!(args.is_null());
    }

    #[test]
    fn registry_has_unique_names() {
        let r = registry();
        let mut names: Vec<&str> = r.iter().map(|t| t.name).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "duplicate tool names");
    }

    #[test]
    fn load_branch_requires_integer_id() {
        let err = translate_load_branch(&json!({})).unwrap_err();
        assert!(err.contains("branch_id"));
        let err = translate_load_branch(&json!({"branch_id": "abc"})).unwrap_err();
        assert!(err.contains("branch_id"));
    }
}
