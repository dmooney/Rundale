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
use std::sync::OnceLock;

/// Translates a tool's validated `tools/call` arguments into the underlying
/// backend `(command, args)` pair. Returning `Err` produces a JSON-RPC
/// invalid-params error.
pub type TranslateFn = fn(&Value) -> Result<(String, Value), String>;

/// Description of one MCP tool, exposed by `tools/list`.
#[derive(Debug, Clone)]
pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
    pub translate: TranslateFn,
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

fn require_string<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("missing required string field: `{key}`"))
}

fn translate_turn(args: &Value) -> Result<(String, Value), String> {
    // `since` is optional. When absent we issue a plain `GET /api/turn`.
    // When present we forward it as `?since=N` via the `_qs` convention
    // understood by `ParishHttpBackend::invoke` (#1389): a `_qs` object in
    // the args is extracted and appended as query-string parameters; the
    // presence of `_qs` is not itself treated as a POST trigger.
    let since = args.get("since").and_then(|v| v.as_u64());
    let out_args = match since {
        Some(cursor) => json!({"_qs": {"since": cursor}}),
        None => Value::Null,
    };
    Ok(("get_turn".into(), out_args))
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

/// All `translate` functions keyed by tool name. This is the Rust half of the
/// tool contract; `manifest.json` is the wire half (name + description +
/// inputSchema). `manifest_translate_bijection` asserts the two halves agree,
/// so a tool can never appear in one without the other.
fn translate_for(name: &str) -> Option<TranslateFn> {
    Some(match name {
        "tauri_invoke" => translate_invoke,
        "parish_world_snapshot" => translate_world_snapshot,
        "parish_map" => translate_map,
        "parish_npcs_here" => translate_npcs_here,
        "parish_engine_state" => translate_engine_state,
        "parish_save_state" => translate_save_state,
        "parish_turn" => translate_turn,
        "parish_submit_input" => translate_submit_input,
        "parish_new_game" => translate_new_game,
        "parish_save_game" => translate_save_game,
        "parish_load_branch" => translate_load_branch,
        "parish_take_screenshot" => translate_take_screenshot,
        "parish_latest_screenshot" => translate_latest_screenshot,
        "parish_file_bug" => translate_file_bug,
        "parish_byok_env_keys" => translate_byok_env_keys,
        "parish_setup_status" => translate_setup_status,
        "parish_setup_byok" => translate_setup_byok,
        _ => return None,
    })
}

/// One tool's wire descriptor, parsed once from the committed `manifest.json`.
/// `name`/`description` are leaked to `'static` so `ToolDef` keeps its
/// `&'static str` fields (the protocol layer and every caller are unchanged);
/// the leak is one-time and bounded by the tool count.
struct ManifestTool {
    name: &'static str,
    description: &'static str,
    input_schema: Value,
}

/// `manifest.json` is the single source of truth for the tool wire shape. It is
/// compiled in via `include_str!` and parsed exactly once. The no-build cold
/// shim (`parish/scripts/parish-mcp-cold-shim.py`) serves the very same file, so
/// `tools/list` is identical whether answered by this binary or the shim.
static MANIFEST_JSON: &str = include_str!("../manifest.json");

fn manifest_tools() -> &'static [ManifestTool] {
    static CELL: OnceLock<Vec<ManifestTool>> = OnceLock::new();
    CELL.get_or_init(|| {
        let parsed: Value = serde_json::from_str(MANIFEST_JSON)
            .expect("parish-mcp manifest.json must be valid JSON");
        let tools = parsed
            .get("tools")
            .and_then(Value::as_array)
            .expect("manifest.json must have a `tools` array");
        tools
            .iter()
            .map(|t| {
                let name = t
                    .get("name")
                    .and_then(Value::as_str)
                    .expect("manifest tool missing `name`");
                let description = t
                    .get("description")
                    .and_then(Value::as_str)
                    .expect("manifest tool missing `description`");
                let input_schema = t
                    .get("inputSchema")
                    .cloned()
                    .expect("manifest tool missing `inputSchema`");
                ManifestTool {
                    name: Box::leak(name.to_owned().into_boxed_str()),
                    description: Box::leak(description.to_owned().into_boxed_str()),
                    input_schema,
                }
            })
            .collect()
    })
}

/// Returns the curated tool registry, in `manifest.json` order. Each manifest
/// tool is paired with its Rust `translate` fn; a manifest entry with no fn is
/// dropped here and caught by `manifest_translate_bijection`.
pub fn registry() -> Vec<ToolDef> {
    manifest_tools()
        .iter()
        .filter_map(|t| {
            let translate = translate_for(t.name)?;
            Some(ToolDef {
                name: t.name,
                description: t.description,
                input_schema: t.input_schema.clone(),
                translate,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Manifest-as-source-of-truth guards (parish-mcp-cold-register) ─────────
    #[test]
    fn manifest_translate_bijection() {
        let parsed: serde_json::Value =
            serde_json::from_str(MANIFEST_JSON).expect("manifest.json is valid JSON");
        let manifest_names: std::collections::BTreeSet<String> = parsed["tools"]
            .as_array()
            .expect("manifest has a `tools` array")
            .iter()
            .map(|t| t["name"].as_str().expect("tool has a `name`").to_string())
            .collect();

        // Every manifest tool resolves a Rust translate fn.
        for n in &manifest_names {
            assert!(
                translate_for(n).is_some(),
                "manifest tool {n:?} has no translate fn — add it to translate_for()"
            );
        }
        // registry() only yields tools that have BOTH halves, so its name set
        // must equal the manifest's — no orphan translate fn, no dropped tool.
        let registry_names: std::collections::BTreeSet<String> =
            registry().iter().map(|t| t.name.to_string()).collect();
        assert_eq!(
            registry_names, manifest_names,
            "registry() and manifest.json tool sets diverged — every tool needs a \
             manifest entry AND a translate fn"
        );
    }

    #[test]
    fn manifest_protocol_and_server_match_consts() {
        // The cold shim serves protocolVersion + serverInfo.name straight from
        // manifest.json; the real binary serves the mcp.rs consts. Pin them equal
        // so the shim→binary handoff is seamless.
        let parsed: serde_json::Value =
            serde_json::from_str(MANIFEST_JSON).expect("manifest.json is valid JSON");
        assert_eq!(
            parsed["protocolVersion"],
            crate::mcp::PROTOCOL_VERSION,
            "manifest protocolVersion must equal mcp::PROTOCOL_VERSION"
        );
        assert_eq!(
            parsed["serverInfo"]["name"],
            crate::mcp::SERVER_NAME,
            "manifest serverInfo.name must equal mcp::SERVER_NAME"
        );
    }

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
                "parish_turn",
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
    fn turn_tool_takes_no_required_args_and_routes_to_get() {
        // No `since` → null args → GET /api/turn.
        let (cmd, args) = translate_turn(&json!({})).unwrap();
        assert_eq!(cmd, "get_turn");
        assert!(
            args.is_null(),
            "turn with no since should be a GET (null args)"
        );
    }

    #[test]
    fn turn_tool_with_since_encodes_qs_param() {
        // `since` must be forwarded as a `_qs` object so `ParishHttpBackend`
        // appends `?since=42` to the URL and issues a GET (#1389).
        let (cmd, args) = translate_turn(&json!({"since": 42})).unwrap();
        assert_eq!(cmd, "get_turn");
        // Must NOT be null — contains the query-string carrier.
        assert!(!args.is_null(), "since must produce _qs args");
        assert_eq!(
            args["_qs"]["since"], 42,
            "since value must be inside _qs object"
        );
    }

    #[test]
    fn turn_tool_with_since_is_still_a_get() {
        // A `_qs`-only args object must NOT trigger POST in the backend.
        use crate::backend::ParishHttpBackend;
        let (_, args) = translate_turn(&json!({"since": 42})).unwrap();
        assert!(
            !ParishHttpBackend::is_post_pub(&args),
            "turn with since must still dispatch as GET"
        );
    }

    #[test]
    fn registry_includes_turn_tool() {
        let names: Vec<&str> = registry().iter().map(|t| t.name).collect();
        assert!(names.contains(&"parish_turn"));
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
            ("parish_turn", json!({})),
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

    /// The two compact turn tools are intentionally supported by both HTTP
    /// runtimes: the embedded desktop bridge and the standalone Axum server.
    /// The MCP backend may point at either one, so a route present in only one
    /// mode is a shipped parity failure (#1777 / #1778).
    #[test]
    fn compact_turn_tools_have_routes_in_desktop_and_server() {
        use std::fs;
        use std::path::PathBuf;

        let ws = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .expect("workspace root is three levels above parish-mcp crate root")
            .to_path_buf();
        let runtime_sources = [
            ws.join("parish/crates/parish-tauri/src/mcp_bridge.rs"),
            ws.join("parish/crates/parish-server/src/lib.rs"),
        ];
        let required_routes = ["/api/submit-input", "/api/turn"];

        for source_path in runtime_sources {
            let source = fs::read_to_string(&source_path)
                .unwrap_or_else(|e| panic!("could not read {}: {e}", source_path.display()));
            for route in required_routes {
                assert!(
                    source.contains(&format!(".route(\"{route}\"")),
                    "{} is missing {route}; parish_submit_input/parish_turn must work against both runtimes",
                    source_path.display(),
                );
            }
        }
    }
}
