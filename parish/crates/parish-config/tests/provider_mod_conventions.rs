//! Data-driven validation tests for provider-mod TOML files (TD-005, #1203).
//!
//! These tests walk every `mods/*-provider/providers/*.toml` file at test time
//! and assert the schema/convention rules described in the acceptance criteria.
//! Adding a new provider mod is automatically covered without touching this
//! file.
//!
//! A second test exercises a deliberately-broken fixture (under
//! `parish/testing/fixtures/mods/broken_provider/`) to verify that each
//! validation rule actually catches violations.

use std::path::{Path, PathBuf};

use parish_config::provider::ProviderMod;

// ── helpers ──────────────────────────────────────────────────────────────────

/// Walk up from `CARGO_MANIFEST_DIR` until a directory containing `mods/` is
/// found. Mirrors the logic in `ensure_mods_loaded()`.
fn find_mods_dir() -> PathBuf {
    let start = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut dir = start.as_path();
    loop {
        let candidate = dir.join("mods");
        if candidate.is_dir() {
            return candidate;
        }
        dir = dir
            .parent()
            .expect("walked to filesystem root without finding mods/");
    }
}

/// Returns the repo root (two levels above `CARGO_MANIFEST_DIR` for
/// `parish/crates/parish-config`).
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent() // parish/crates/
        .and_then(|p| p.parent()) // parish/
        .and_then(|p| p.parent()) // repo root
        .expect("repo root is three levels above crate root")
        .to_path_buf()
}

/// Collect every `(dir_name, file_path)` pair from `mods/*-provider/providers/*.toml`.
fn collect_provider_toml_paths(mods_dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let entries = std::fs::read_dir(mods_dir)
        .unwrap_or_else(|e| panic!("cannot read mods dir {}: {}", mods_dir.display(), e));
    for entry in entries.flatten() {
        let dir_name = entry.file_name().to_string_lossy().to_string();
        if !dir_name.ends_with("-provider") {
            continue;
        }
        let providers_dir = entry.path().join("providers");
        if !providers_dir.is_dir() {
            continue;
        }
        let files = std::fs::read_dir(&providers_dir).unwrap_or_else(|e| {
            panic!(
                "cannot read providers dir {}: {}",
                providers_dir.display(),
                e
            )
        });
        for f in files.flatten() {
            if f.path().extension().is_some_and(|x| x == "toml") {
                out.push((dir_name.clone(), f.path()));
            }
        }
    }
    out.sort_by(|a, b| a.1.cmp(&b.1));
    out
}

/// Deserialize a TOML file into `ProviderMod`, panicking with context on error.
fn parse_provider_toml(path: &Path) -> ProviderMod {
    let raw = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read {}: {}", path.display(), e));
    toml::from_str::<ProviderMod>(&raw).unwrap_or_else(|e| {
        panic!(
            "TOML parse failed for {}: {}\n\nContent:\n{}",
            path.display(),
            e,
            raw
        )
    })
}

/// Validate a single `ProviderMod` against all convention rules.
/// Returns a list of violation messages; empty means valid.
fn validate_provider_mod(dir_name: &str, file_path: &Path, m: &ProviderMod) -> Vec<String> {
    let label = file_path.display().to_string();
    let mut violations = Vec::new();

    // AC-1a: directory name must be `<id>-provider`
    let expected_dir = format!("{}-provider", m.id);
    if dir_name != expected_dir {
        violations.push(format!(
            "[{}] AC-1a: directory name '{}' does not match expected '{}' (id='{}'); \
             rename the mod directory to match the provider id",
            label, dir_name, expected_dir, m.id
        ));
    }

    // AC-1b: TOML file stem must match `id`
    let stem = file_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    if stem != m.id {
        violations.push(format!(
            "[{}] AC-1b: file stem '{}' does not match provider id '{}'; \
             rename the TOML file to {}.toml",
            label, stem, m.id, m.id
        ));
    }

    // AC-2: required string fields must be non-empty
    if m.id.is_empty() {
        violations.push(format!("[{}] AC-2: 'id' must not be empty", label));
    }
    if m.display_name.is_empty() {
        violations.push(format!(
            "[{}] AC-2: 'display_name' must not be empty (id='{}')",
            label, m.id
        ));
    }
    match &m.blurb {
        None => violations.push(format!(
            "[{}] AC-2: 'blurb' is required (id='{}')",
            label, m.id
        )),
        Some(b) if b.is_empty() => violations.push(format!(
            "[{}] AC-2: 'blurb' must not be empty (id='{}')",
            label, m.id
        )),
        _ => {}
    }
    match &m.signup_url {
        None => violations.push(format!(
            "[{}] AC-2: 'signup_url' is required (id='{}')",
            label, m.id
        )),
        Some(u) if u.is_empty() => violations.push(format!(
            "[{}] AC-2: 'signup_url' must not be empty (id='{}')",
            label, m.id
        )),
        _ => {}
    }

    // AC-3: kind must be a known variant (enforced by serde; listed here for clarity)
    // ProviderKind deserialization already rejects unknown values during parse_provider_toml,
    // so an explicit check here is belt-and-suspenders.
    use parish_config::provider::ProviderKind;
    let _kind: ProviderKind = m.kind; // type-checked at compile time

    // AC-5: default_base_url must start with http:// or https:// unless
    //       needs_base_url_from_user is true (which allows empty).
    if !m.needs_base_url_from_user && !m.default_base_url.is_empty() {
        let url = &m.default_base_url;
        if !url.starts_with("http://") && !url.starts_with("https://") {
            violations.push(format!(
                "[{}] AC-5: default_base_url '{}' must start with http:// or https:// \
                 (id='{}')",
                label, url, m.id
            ));
        }
    }

    // AC-6: preset conventions
    let has_recommended = m.presets.iter().any(|p| p.key == "recommended");
    if !m.presets.is_empty() && !has_recommended {
        violations.push(format!(
            "[{}] AC-6: provider '{}' has presets but no preset with key='recommended'; \
             add a 'recommended' preset as the default entry",
            label, m.id
        ));
    }
    for preset in &m.presets {
        if preset.key.is_empty() {
            violations.push(format!(
                "[{}] AC-6: preset key must not be empty (id='{}')",
                label, m.id
            ));
        }
        if preset.label.is_empty() {
            violations.push(format!(
                "[{}] AC-6: preset '{}' label must not be empty (id='{}')",
                label, preset.key, m.id
            ));
        }
        // All four model categories must be non-empty when the preset is present.
        for (cat_name, model) in [
            ("dialogue", &preset.dialogue),
            ("simulation", &preset.simulation),
            ("intent", &preset.intent),
            ("reaction", &preset.reaction),
        ] {
            match model {
                None => violations.push(format!(
                    "[{}] AC-6: preset '{}' is missing '{}' model (id='{}')",
                    label, preset.key, cat_name, m.id
                )),
                Some(s) if s.is_empty() => violations.push(format!(
                    "[{}] AC-6: preset '{}' has empty '{}' model string (id='{}')",
                    label, preset.key, cat_name, m.id
                )),
                _ => {}
            }
        }
    }

    violations
}

// ── main validation test ──────────────────────────────────────────────────────

/// AC-1 through AC-6, data-driven: every real provider mod must pass all
/// convention checks. Adding a new mod is automatically covered (AC-7).
#[test]
fn provider_mod_conventions() {
    let mods_dir = find_mods_dir();
    let entries = collect_provider_toml_paths(&mods_dir);
    assert!(
        !entries.is_empty(),
        "no *-provider/providers/*.toml files found under {}; \
         check that the test is running from the correct workspace",
        mods_dir.display()
    );

    let mut all_violations: Vec<String> = Vec::new();
    let mut provider_count = 0_usize;
    let mut featured_count = 0_usize;

    for (dir_name, path) in &entries {
        let m = parse_provider_toml(path);
        if m.featured {
            featured_count += 1;
        }
        provider_count += 1;

        let violations = validate_provider_mod(dir_name, path, &m);
        all_violations.extend(violations);
    }

    // AC-4: featured count sanity cap
    if featured_count > 8 {
        all_violations.push(format!(
            "AC-4: {} providers are featured (cap is 8); \
             review featured=true assignments to avoid UI overcrowding",
            featured_count
        ));
    }

    assert!(
        all_violations.is_empty(),
        "Provider-mod convention violations found across {} provider mods:\n\n{}\n\n\
         Fix the TOML files listed above to satisfy the naming/schema conventions \
         documented in .proofs/td005-mods-provider-validation/acceptance-criteria.md",
        provider_count,
        all_violations.join("\n")
    );
}

// ── broken-fixture rejection tests ───────────────────────────────────────────

/// AC-8: the deliberately-broken fixture must fail validation with descriptive messages.
///
/// The fixture lives at
/// `parish/testing/fixtures/mods/broken_provider/providers/broken.toml`
/// and contains multiple injected violations so this test doubles as a
/// proof that each validation rule actually fires.
#[test]
fn broken_provider_fixture_is_rejected() {
    let fixture_path =
        repo_root().join("parish/testing/fixtures/mods/broken_provider/providers/broken.toml");
    assert!(
        fixture_path.exists(),
        "broken fixture not found at {}; the fixture must be checked in",
        fixture_path.display()
    );

    // The broken fixture has an empty id so the directory/file-stem checks
    // are skipped (they depend on id matching dir/file name); we call
    // validate_provider_mod with the actual dir name and stem so the
    // file-stem check fires against the empty id too.
    let m = parse_provider_toml(&fixture_path);
    let dir_name = "broken_provider"; // intentionally not `<id>-provider` (id is "")
    let violations = validate_provider_mod(dir_name, &fixture_path, &m);

    assert!(
        !violations.is_empty(),
        "broken fixture should have produced violations but produced none; \
         check that the fixture at {} actually contains intentional violations",
        fixture_path.display()
    );

    let joined = violations.join("\n");

    // AC-2: empty id
    assert!(
        violations
            .iter()
            .any(|v| v.contains("'id' must not be empty")),
        "expected AC-2 violation for empty id; violations:\n{}",
        joined
    );

    // AC-2: empty display_name
    assert!(
        violations
            .iter()
            .any(|v| v.contains("'display_name' must not be empty")),
        "expected AC-2 violation for empty display_name; violations:\n{}",
        joined
    );

    // AC-5: bad base URL scheme
    assert!(
        violations.iter().any(|v| v.contains("AC-5")),
        "expected AC-5 violation for ftp:// base URL; violations:\n{}",
        joined
    );

    // AC-6: empty dialogue model in preset
    assert!(
        violations
            .iter()
            .any(|v| v.contains("AC-6") && v.contains("dialogue")),
        "expected AC-6 violation for empty dialogue model; violations:\n{}",
        joined
    );

    // AC-6: missing 'recommended' preset key
    assert!(
        violations
            .iter()
            .any(|v| v.contains("AC-6") && v.contains("recommended")),
        "expected AC-6 violation for missing 'recommended' preset key; violations:\n{}",
        joined
    );
}
