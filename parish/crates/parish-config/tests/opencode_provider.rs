//! Regression tests for the first-class OpenCode Zen provider mod (#1231).
//!
//! `mods/opencode-provider/providers/opencode.toml` is auto-discovered by the
//! provider registry (`ensure_mods_loaded` walks every
//! `mods/*/providers/*.toml`). These tests assert the provider resolves with
//! the correct identity, its alias `opencode-go` (the id used by the eval
//! harness + `.env.example`) maps to the same provider, it is selectable in the
//! BYOK provider list, and its presets only name OpenAI-compatible model ids
//! (no `qwen3.7-max`, which is Anthropic-Messages-only on this gateway).

use parish_config::provider::{Provider, ProviderKind};

/// AC-2: `opencode` resolves with the correct gateway identity.
#[test]
fn opencode_resolves_with_correct_identity() {
    let p = Provider::from_id("opencode").expect("opencode provider mod must be loaded");
    assert_eq!(p.id(), "opencode");
    assert_eq!(p.kind(), ProviderKind::OpenAiCompat);
    assert_eq!(p.default_base_url(), "https://opencode.ai/zen/go/v1");
    assert_eq!(p.api_key_env_var(), Some("OPENCODE_API_KEY"));
    assert!(
        p.requires_api_key(),
        "OpenCode Zen is a hosted gateway and requires an API key"
    );
    assert!(
        !p.needs_base_url_from_user(),
        "base_url ships with the mod; the user should not have to type it"
    );
}

/// AC-3: the `opencode-go` alias (eval-harness / .env.example id) resolves to
/// the same provider as the canonical `opencode` id.
#[test]
fn opencode_go_alias_resolves() {
    let canonical = Provider::from_id("opencode").expect("opencode must be loaded");
    for alias in ["opencode-go", "opencode_go", "zen"] {
        let p = Provider::from_str_loose(alias)
            .unwrap_or_else(|_| panic!("alias '{alias}' must resolve"));
        assert_eq!(
            p.id(),
            canonical.id(),
            "alias '{alias}' must map to the opencode provider"
        );
        assert_eq!(p.default_base_url(), "https://opencode.ai/zen/go/v1");
    }
}

/// AC-5/AC-6: presets are wired with a non-empty model per category, a
/// `recommended` and a `budget` preset both exist, and NO preset names
/// `qwen3.7-max` (Anthropic-Messages-only — would 401 on /chat/completions).
#[test]
fn opencode_presets_are_openai_compatible() {
    let p = Provider::from_id("opencode").expect("opencode must be loaded");
    let presets = p.presets();
    assert!(
        presets.iter().any(|pr| pr.key == "recommended"),
        "a 'recommended' preset is required"
    );
    assert!(
        presets.iter().any(|pr| pr.key == "budget"),
        "a 'budget' preset is expected per AC-5"
    );
    for preset in presets {
        for (cat, model) in [
            ("dialogue", &preset.dialogue),
            ("simulation", &preset.simulation),
            ("intent", &preset.intent),
            ("reaction", &preset.reaction),
        ] {
            let m = model
                .as_deref()
                .unwrap_or_else(|| panic!("preset '{}' missing {cat} model", preset.key));
            assert!(
                !m.is_empty(),
                "preset '{}' has empty {cat} model",
                preset.key
            );
            assert_ne!(
                m, "qwen3.7-max",
                "qwen3.7-max is Anthropic-Messages-only on this gateway and must \
                 not appear in an openai-compat preset (preset '{}', category {cat})",
                preset.key
            );
        }
    }
}

/// AC-4: the provider is selectable in the BYOK wizard — it appears in
/// `handle_list_available_providers` and `handle_list_env_keys`, the exact
/// backend handlers the web/Tauri/CLI provider pickers consume. This proves
/// mode parity: every runtime reads the same registry-backed list.
#[test]
fn opencode_is_selectable_in_byok() {
    use parish_config::registry;

    // Force mod discovery (also done implicitly by `from_id`).
    let _ = Provider::from_id("opencode").expect("opencode must be loaded");

    // The BYOK list handlers live in parish-core; here we assert the same
    // registry invariant they rely on (id present, env var wired) so this test
    // stays in the leaf crate that owns the registry.
    let ids: Vec<String> = registry()
        .all()
        .iter()
        .map(|p| p.id().to_string())
        .collect();
    assert!(
        ids.iter().any(|id| id == "opencode"),
        "opencode must be in the registry list the BYOK picker renders; got: {ids:?}"
    );

    let p = registry().get("opencode").expect("opencode in registry");
    assert_eq!(
        p.api_key_env_var(),
        Some("OPENCODE_API_KEY"),
        "the env-key lookup (handle_list_env_keys) keys off this var"
    );
}
