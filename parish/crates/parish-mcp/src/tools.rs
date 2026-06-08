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

fn translate_engine_state(_args: &Value) -> Result<(String, Value), String> {
    Ok(("get_engine_state".into(), Value::Null))
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

fn translate_latest_screenshot(_args: &Value) -> Result<(String, Value), String> {
    Ok(("get_latest_screenshot".into(), Value::Null))
}

fn translate_take_screenshot(_args: &Value) -> Result<(String, Value), String> {
    // `json!({})` (not Null) so the HTTP backend dispatches as POST to
    // `/api/take-screenshot`, which triggers the bridge's round-trip capture.
    Ok(("take_screenshot".into(), json!({})))
}

fn translate_file_bug(args: &Value) -> Result<(String, Value), String> {
    let title = require_string(args, "title")?.trim().to_string();
    if title.is_empty() {
        return Err("`title` must not be empty".into());
    }
    let mut out = json!({ "title": title });
    if let Some(desc) = args.get("description").and_then(|v| v.as_str()) {
        out["description"] = Value::String(desc.to_string());
    }
    // Optional structured context (e.g. a serialized debug-panel record).
    if let Some(ctx) = args.get("context").filter(|c| !c.is_null()) {
        out["context"] = ctx.clone();
    }
    Ok(("submit_bug_report".into(), out))
}

// ── BYOK setup-flow (#933) ───────────────────────────────────────────────────
//
// Real handlers in `parish-tauri/src/mcp_bridge.rs` back these tools — they
// share the same `AppState`, secret store, and user-config dir as the
// Svelte BYOK wizard, so an MCP client and the desktop UI converge on
// identical effects.

fn translate_setup_status(_args: &Value) -> Result<(String, Value), String> {
    Ok(("get_setup_status".into(), Value::Null))
}

fn translate_byok_env_keys(_args: &Value) -> Result<(String, Value), String> {
    Ok(("get_byok_env_keys".into(), Value::Null))
}

fn translate_setup_byok(args: &Value) -> Result<(String, Value), String> {
    let provider = require_string(args, "provider")?.to_string();
    // `api_key` is optional: keyless local providers (Ollama, LM Studio, vLLM,
    // Simulator) accept None. Hosted providers will fail validation server-side
    // with a structured error if the key is missing.
    let api_key = args
        .get("api_key")
        .and_then(|v| v.as_str())
        .map(String::from);
    let base_url = args
        .get("base_url")
        .and_then(|v| v.as_str())
        .map(String::from);
    let model = args.get("model").and_then(|v| v.as_str()).map(String::from);
    Ok((
        "submit_byok".into(),
        json!({
            "provider": provider,
            "api_key": api_key,
            "base_url": base_url,
            "model": model,
        }),
    ))
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
            name: "parish_engine_state",
            description: "Reads the canonical, deterministic Parish engine state — the \
                          authoritative snapshot a QA agent asserts the UI against after \
                          each interaction. Returns `active_scene` (player location id + \
                          name + indoor), `clock` (game time, day-of-week, day-type, \
                          season, festival, paused), `weather`, `player` (location id, \
                          visited count, name), `npcs` (co-located NPCs + roster totals), \
                          and `grapevine` (gossip-network item count + distortion). \
                          Read-only and deterministic: identical engine state yields an \
                          identical snapshot. Pair with `parish_world_snapshot` / \
                          `parish_npcs_here` to detect UI-vs-engine drift, and attach the \
                          result to `parish_file_bug` when a mismatch is found.",
            input_schema: empty_object_schema(),
            translate: translate_engine_state,
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
        // ── Screenshot tools ─────────────────────────────────────────────────
        // `parish_take_screenshot` triggers a fresh capture; the bridge emits
        // a `request-screenshot` event to the live desktop window, waits for
        // the frontend to call back with the PNG metadata (up to 15 s), and
        // returns the result. Only works when a Tauri desktop window is open.
        //
        // `parish_latest_screenshot` is the read-only companion: it returns
        // the most recently captured screenshot without triggering a new one.
        ToolDef {
            name: "parish_take_screenshot",
            description: "Captures the current game view as a PNG screenshot and returns \
                          its path, ISO-8601 taken_at timestamp, and size in bytes. \
                          Requires the live desktop window — returns an error when running \
                          in headless / web-server mode or when the desktop window does not \
                          respond within 15 seconds. Use `parish_latest_screenshot` if you \
                          only need to read a previously captured image.",
            input_schema: empty_object_schema(),
            translate: translate_take_screenshot,
        },
        ToolDef {
            name: "parish_latest_screenshot",
            description: "Reads metadata for the most recently captured screenshot \
                          (path, ISO-8601 taken_at, size_bytes). Returns null when no \
                          screenshot exists yet — capture is player-initiated by pressing \
                          F2 in the live desktop window or via `parish_take_screenshot`. \
                          The path is on the host filesystem; pair this tool with a \
                          separate Read to view the PNG.",
            input_schema: empty_object_schema(),
            translate: translate_latest_screenshot,
        },
        // ── Bug reporting ────────────────────────────────────────────────────
        // Files a well-formed GitHub issue (screenshot + logs + game state) so
        // an auto-QA agent can report a reproducible bug for a fix-agent. The
        // backend captures the screenshot via the same round-trip as
        // `parish_take_screenshot` when a live desktop window is attached;
        // otherwise it proceeds without one. In dry-run / no-token mode the
        // composed report is written to disk and `created` is false.
        ToolDef {
            name: "parish_file_bug",
            description: "Files a bug report for the running game. Bundles a screenshot \
                          (captured live from the desktop window when one is attached), \
                          recent logs, and current game state into a GitHub issue on the \
                          configured repository and returns the issue URL. In dry-run or \
                          no-token mode the report is written to disk instead (created=false, \
                          bundle_path set). Use during auto-QA to file reproducible bugs. \
                          Attach a specific debug record via the optional `context` object.",
            input_schema: json!({
                "type": "object",
                "required": ["title"],
                "properties": {
                    "title": {"type": "string", "minLength": 1, "maxLength": 500},
                    "description": {"type": "string", "maxLength": 8000},
                    "context": {
                        "type": "object",
                        "description": "Optional debug-panel record for extra context.",
                        "properties": {
                            "kind": {"type": "string"},
                            "label": {"type": "string"},
                            "detail": {}
                        }
                    }
                }
            }),
            translate: translate_file_bug,
        },
        // ── BYOK setup-flow ──────────────────────────────────────────────────
        ToolDef {
            name: "parish_byok_env_keys",
            description: "Returns `{provider_id: bool}` for every supported provider — true \
                          when the standard API-key env var (ANTHROPIC_API_KEY, OPENAI_API_KEY, \
                          etc.) is set in the host process. Lets a wizard or MCP client tell \
                          the user 'leave the field blank to use your existing env var' \
                          before they commit to a provider.",
            input_schema: empty_object_schema(),
            translate: translate_byok_env_keys,
        },
        ToolDef {
            name: "parish_setup_status",
            description: "Reads the BYOK setup state. Returns `{complete, provider, model, \
                          base_url, has_api_key, has_env_key}`: `complete` is true once the \
                          user (or the model, via parish_setup_byok) has picked a provider; \
                          `has_env_key` is true if a standard provider env var \
                          (ANTHROPIC_API_KEY etc.) is already set in the host process.",
            input_schema: empty_object_schema(),
            translate: translate_setup_status,
        },
        ToolDef {
            name: "parish_setup_byok",
            description: "Persists a 'bring your own key' provider configuration: writes the \
                          API key to the OS keychain, updates the user config TOML, and \
                          rebuilds the live inference worker so subsequent dialogue uses \
                          the new provider. Returns `{ok, provider, model, base_url, \
                          has_api_key}` on success or an HTTP 500 with a structured error \
                          message on failure (missing key for a hosted provider, missing \
                          base_url for `custom`, invalid provider name, etc.).",
            input_schema: json!({
                "type": "object",
                "required": ["provider"],
                "properties": {
                    "provider": {
                        "type": "string",
                        "description": "Provider id (e.g. anthropic, openrouter, openai, groq, ollama, lmstudio, custom)."
                    },
                    "api_key": {
                        "type": "string",
                        "minLength": 1,
                        "description": "Required for hosted providers; omit for keyless local providers (ollama, lmstudio, vllm, simulator)."
                    },
                    "base_url": {
                        "type": "string",
                        "description": "Optional override for the provider's base URL; required when provider is `custom`."
                    },
                    "model": {
                        "type": "string",
                        "description": "Optional explicit model id; defaults to the provider's dialogue preset."
                    }
                }
            }),
            translate: translate_setup_byok,
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
    fn registry_exposes_full_contract_names_in_order() {
        let names: Vec<&str> = registry().iter().map(|t| t.name).collect();
        assert_eq!(
            names,
            vec![
                "tauri_invoke",
                "parish_world_snapshot",
                "parish_map",
                "parish_npcs_here",
                "parish_engine_state",
                "parish_save_state",
                "parish_submit_input",
                "parish_new_game",
                "parish_save_game",
                "parish_load_branch",
                "parish_take_screenshot",
                "parish_latest_screenshot",
                "parish_file_bug",
                "parish_byok_env_keys",
                "parish_setup_status",
                "parish_setup_byok",
            ]
        );
    }

    #[test]
    fn file_bug_requires_title() {
        let err = translate_file_bug(&json!({})).unwrap_err();
        assert!(err.contains("title"));
        let err = translate_file_bug(&json!({"title": "   "})).unwrap_err();
        assert!(err.contains("title"));
    }

    #[test]
    fn file_bug_maps_to_submit_bug_report() {
        let (cmd, args) = translate_file_bug(&json!({
            "title": "NPC stuck",
            "description": "Seán never arrives.",
            "context": {"kind": "inference", "label": "#3", "detail": {"id": 3}}
        }))
        .unwrap();
        assert_eq!(cmd, "submit_bug_report");
        assert_eq!(args["title"], "NPC stuck");
        assert_eq!(args["description"], "Seán never arrives.");
        assert_eq!(args["context"]["kind"], "inference");
    }

    #[test]
    fn file_bug_omits_absent_optionals() {
        let (_cmd, args) = translate_file_bug(&json!({"title": "x"})).unwrap();
        assert!(args.get("description").is_none());
        assert!(args.get("context").is_none());
    }

    #[test]
    fn load_branch_requires_integer_id() {
        let err = translate_load_branch(&json!({})).unwrap_err();
        assert!(err.contains("branch_id"));
        let err = translate_load_branch(&json!({"branch_id": "abc"})).unwrap_err();
        assert!(err.contains("branch_id"));
    }

    #[test]
    fn setup_status_takes_no_args() {
        let (cmd, args) = translate_setup_status(&json!({})).unwrap();
        assert_eq!(cmd, "get_setup_status");
        assert!(args.is_null());
    }

    #[test]
    fn setup_byok_requires_provider() {
        assert!(translate_setup_byok(&json!({})).is_err());
        assert!(translate_setup_byok(&json!({"api_key": "sk-..."})).is_err());
    }

    #[test]
    fn setup_byok_accepts_keyless_local_provider() {
        // Ollama needs no key; the backend will accept None on submit_byok
        // and skip the keychain write.
        let (cmd, args) = translate_setup_byok(&json!({"provider": "ollama"})).unwrap();
        assert_eq!(cmd, "submit_byok");
        assert_eq!(args["provider"], "ollama");
        assert!(args["api_key"].is_null());
    }

    #[test]
    fn setup_byok_passes_required_and_optional_fields() {
        let (cmd, args) = translate_setup_byok(&json!({
            "provider": "openrouter",
            "api_key": "sk-or-v1-abc",
            "base_url": "https://openrouter.ai/api",
            "model": "anthropic/claude-sonnet-4.5"
        }))
        .unwrap();
        assert_eq!(cmd, "submit_byok");
        assert_eq!(args["provider"], "openrouter");
        assert_eq!(args["api_key"], "sk-or-v1-abc");
        assert_eq!(args["base_url"], "https://openrouter.ai/api");
        assert_eq!(args["model"], "anthropic/claude-sonnet-4.5");
    }

    #[test]
    fn setup_byok_omits_optional_fields_as_null() {
        let (_, args) = translate_setup_byok(&json!({
            "provider": "ollama"
        }))
        .unwrap();
        assert!(args["api_key"].is_null());
        assert!(args["base_url"].is_null());
        assert!(args["model"].is_null());
    }

    #[test]
    fn registry_includes_byok_setup_stubs() {
        let names: Vec<&str> = registry().iter().map(|t| t.name).collect();
        assert!(names.contains(&"parish_setup_status"));
        assert!(names.contains(&"parish_setup_byok"));
    }

    #[test]
    fn latest_screenshot_takes_no_args_and_routes_to_get() {
        let (cmd, args) = translate_latest_screenshot(&json!({})).unwrap();
        assert_eq!(cmd, "get_latest_screenshot");
        // Null args mean the HTTP backend dispatches as GET (matches
        // `ParishHttpBackend::is_post`).
        assert!(args.is_null());
    }

    #[test]
    fn engine_state_takes_no_args_and_routes_to_get() {
        let (cmd, args) = translate_engine_state(&json!({})).unwrap();
        assert_eq!(cmd, "get_engine_state");
        // Null args ⇒ GET /api/engine-state.
        assert!(args.is_null());
    }

    #[test]
    fn registry_includes_engine_state_tool() {
        let names: Vec<&str> = registry().iter().map(|t| t.name).collect();
        assert!(names.contains(&"parish_engine_state"));
    }

    #[test]
    fn registry_includes_latest_screenshot_tool() {
        let names: Vec<&str> = registry().iter().map(|t| t.name).collect();
        assert!(names.contains(&"parish_latest_screenshot"));
    }

    #[test]
    fn take_screenshot_routes_to_post() {
        let (cmd, args) = translate_take_screenshot(&json!({})).unwrap();
        assert_eq!(cmd, "take_screenshot");
        // Non-null args mean the HTTP backend dispatches as POST to
        // `/api/take-screenshot`, triggering the bridge's round-trip.
        assert!(!args.is_null());
    }

    #[test]
    fn registry_includes_take_screenshot_tool() {
        let names: Vec<&str> = registry().iter().map(|t| t.name).collect();
        assert!(names.contains(&"parish_take_screenshot"));
    }

    // ── MCP-to-backend parity (TD-001 / #1202) ───────────────────────────────
    //
    // Every non-passthrough MCP tool in `registry()` delegates to a hard-coded
    // backend command name. The bridge router in
    // `parish-tauri/src/mcp_bridge.rs` is the canonical list of routes that
    // the MCP HTTP backend actually exposes. This test asserts that every
    // command name the tools produce maps to a `.route("/api/...")` call in
    // that file, using the same `command_to_path` translation that
    // `ParishHttpBackend` uses at runtime.
    //
    // Because the check is against the source file (not a running process) it
    // runs offline and is fully deterministic. The technique mirrors
    // `parish-core/tests/wiring_parity.rs`.
    //
    // `tauri_invoke` is the generic escape hatch — it forwards whatever command
    // name the caller supplies, so there is no single target route to pin; it
    // is excluded from the check by name.
    #[test]
    fn mcp_tool_commands_are_subset_of_bridge_routes() {
        use std::collections::HashSet;
        use std::fs;
        use std::path::PathBuf;

        // ── Resolve workspace root ────────────────────────────────────────────
        // CARGO_MANIFEST_DIR is `parish/crates/parish-mcp`; workspace root is
        // three levels up (parish-mcp → crates → parish → workspace root).
        let ws = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .expect("workspace root is three levels above parish-mcp crate root")
            .to_path_buf();

        // ── Parse routes from mcp_bridge.rs ──────────────────────────────────
        // Extract every string that appears as a `.route("/api/...")` argument.
        // The parser is deliberately simple: it looks for lines containing
        // `.route("` and extracts the first quoted path on that line.
        let bridge_path = ws.join("parish/crates/parish-tauri/src/mcp_bridge.rs");
        let bridge_src = fs::read_to_string(&bridge_path).unwrap_or_else(|e| {
            panic!(
                "could not read {}: {e}\n\nFIX: ensure parish-tauri/src/mcp_bridge.rs exists.",
                bridge_path.display()
            )
        });

        let known_routes: HashSet<String> = bridge_src
            .lines()
            .filter(|l| l.contains(".route(\""))
            .filter_map(|l| {
                let after = l.split(".route(\"").nth(1)?;
                let end = after.find('"')?;
                Some(after[..end].to_string())
            })
            .collect();

        assert!(
            !known_routes.is_empty(),
            "parsed zero routes from {} — the source format may have changed.",
            bridge_path.display()
        );

        // ── Collect target commands from every non-passthrough tool ───────────
        // Call each tool's `translate` fn with the minimal valid args so the
        // command name is exercised at runtime (not just read from a constant).
        let minimal_args: &[(&str, serde_json::Value)] = &[
            // Tools with no required fields pass an empty object.
            ("parish_world_snapshot", json!({})),
            ("parish_map", json!({})),
            ("parish_npcs_here", json!({})),
            ("parish_engine_state", json!({})),
            ("parish_save_state", json!({})),
            ("parish_new_game", json!({})),
            ("parish_save_game", json!({})),
            ("parish_take_screenshot", json!({})),
            ("parish_latest_screenshot", json!({})),
            ("parish_byok_env_keys", json!({})),
            ("parish_setup_status", json!({})),
            // Tools with required fields.
            ("parish_submit_input", json!({"text": "look"})),
            ("parish_load_branch", json!({"branch_id": 1})),
            ("parish_file_bug", json!({"title": "test"})),
            ("parish_setup_byok", json!({"provider": "ollama"})),
        ];

        // Build lookup: tool_name -> translate fn.
        type TranslateFn = fn(&serde_json::Value) -> Result<(String, serde_json::Value), String>;
        let reg: std::collections::HashMap<&str, TranslateFn> = registry()
            .into_iter()
            .filter(|t| t.name != "tauri_invoke")
            .map(|t| (t.name, t.translate))
            .collect();

        let mut failures: Vec<String> = Vec::new();

        for (tool_name, args) in minimal_args {
            let translate = reg.get(tool_name).unwrap_or_else(|| {
                panic!("minimal_args lists tool {tool_name:?} which is not in registry() — update the list")
            });
            let (cmd, _) = translate(args).unwrap_or_else(|e| {
                panic!("translate for {tool_name} failed with minimal args: {e}")
            });
            let path = crate::backend::ParishHttpBackend::command_to_path(&cmd);
            if !known_routes.contains(&path) {
                failures.push(format!(
                    "  - MCP tool {tool_name:?} targets command {cmd:?} → {path}, \
                     but that path is not registered in mcp_bridge.rs\n    \
                     FIX: add .route(\"{path}\", ...) to build_router() in \
                     parish-tauri/src/mcp_bridge.rs, or update the translate_* \
                     function for {tool_name} to target an existing route."
                ));
            }
        }

        // Also verify every non-passthrough tool in registry() is covered by
        // minimal_args (so a newly added tool can't silently escape the check).
        for tool in registry().iter().filter(|t| t.name != "tauri_invoke") {
            if !minimal_args.iter().any(|(n, _)| *n == tool.name) {
                failures.push(format!(
                    "  - registry() tool {:?} is not in minimal_args — \
                     add an entry with valid args so the parity check covers it.",
                    tool.name
                ));
            }
        }

        assert!(
            failures.is_empty(),
            "MCP-to-bridge parity violations (TD-001 / #1202):\n{}\n\n\
             Every non-passthrough MCP tool must target a command that maps to \
             a route registered in parish-tauri/src/mcp_bridge.rs.",
            failures.join("\n")
        );
    }
}
