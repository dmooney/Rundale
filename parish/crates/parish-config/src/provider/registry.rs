//! Provider registry: builtin bootstrap + runtime mod discovery.
//!
//! Owns the process-wide [`ProviderRegistry`] static and the [`registry`] /
//! [`ensure_mods_loaded`] accessors. The registry is pre-populated with the
//! engine builtins and has mod-loaded providers merged in post-init. Split
//! out of the monolithic `provider` module (#1200).

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use parish_types::ParishError;

use super::schema::{Provider, ProviderMod};
use crate::builtin_providers;

/// Provider registry. Uses interior mutability so the static `REGISTRY`
/// can be pre-populated with builtins and then have mod-loaded providers
/// merged in post-init via `register_mod_providers`. Existing
/// `Arc<ProviderMod>` handles handed out before a merge keep pointing at
/// their snapshot.
pub struct ProviderRegistry {
    by_id: RwLock<HashMap<String, Provider>>,
}

impl ProviderRegistry {
    /// Loads the registry pre-populated with the five engine builtins.
    /// Cloud providers are *not* loaded here; they arrive later via
    /// `register_mod_providers` after `discover_mods` runs in bootstrap.
    fn load_with_builtins() -> Self {
        let mut by_id: HashMap<String, Provider> = HashMap::new();

        for raw in builtin_providers::ALL {
            let m: ProviderMod = toml::from_str(raw)
                .expect("builtin provider TOML parse failed — check builtin_providers/*.toml");
            if let Some(recommended) = &m.recommended_preset {
                assert!(
                    m.presets.iter().any(|preset| &preset.key == recommended),
                    "builtin provider {} names missing recommended_preset {recommended}",
                    m.id
                );
            }
            let p = Provider(Arc::new(m));
            by_id.insert(p.id().to_string(), p);
        }

        ProviderRegistry {
            by_id: RwLock::new(by_id),
        }
    }

    pub fn get(&self, id: &str) -> Option<Provider> {
        self.by_id
            .read()
            .expect("registry poisoned")
            .get(id)
            .cloned()
    }

    pub fn lookup(&self, s: &str) -> Result<Provider, ParishError> {
        let lower = s.to_lowercase();
        let guard = self.by_id.read().expect("registry poisoned");
        if let Some(p) = guard.get(&lower) {
            return Ok(p.clone());
        }
        for p in guard.values() {
            if p.0.aliases.iter().any(|a| a == &lower) {
                return Ok(p.clone());
            }
        }
        let mut known: Vec<&str> = guard.keys().map(String::as_str).collect();
        known.sort();
        Err(ParishError::Config(format!(
            "unknown provider '{}'. Known: {}",
            s,
            known.join(", ")
        )))
    }

    pub fn all(&self) -> Vec<Provider> {
        let mut v: Vec<_> = self
            .by_id
            .read()
            .expect("registry poisoned")
            .values()
            .cloned()
            .collect();
        v.sort_by(|a, b| a.id().cmp(b.id()));
        v
    }

    pub fn featured(&self) -> Vec<Provider> {
        let mut v: Vec<_> = self
            .by_id
            .read()
            .expect("registry poisoned")
            .values()
            .filter(|p| p.0.featured)
            .cloned()
            .collect();
        v.sort_by(|a, b| a.id().cmp(b.id()));
        v
    }

    /// Registers runtime providers with fatal ID/alias collision checks.
    pub fn register_mod_providers(&self, mods: Vec<ProviderMod>) -> Result<(), ParishError> {
        let mut guard = self.by_id.write().expect("registry poisoned");
        for m in mods {
            let id = m.id.clone();
            if let Some(recommended) = &m.recommended_preset
                && !m.presets.iter().any(|preset| &preset.key == recommended)
            {
                return Err(ParishError::Config(format!(
                    "provider {id:?} names missing recommended_preset {recommended:?}"
                )));
            }
            if let Some(existing) = guard.get(&id) {
                // Silent no-op when re-registering identical content — the
                // auto-loader and bootstrap may both fire in debug builds
                // and that overlap should not log spam.
                if existing.0.as_ref() == &m {
                    continue;
                }
                return Err(ParishError::Config(format!(
                    "provider id collision: {id:?} is already registered"
                )));
            }
            let names = guard
                .values()
                .flat_map(|provider| {
                    std::iter::once(provider.id().to_string())
                        .chain(provider.0.aliases.iter().cloned())
                })
                .collect::<std::collections::BTreeSet<_>>();
            if let Some(collision) = std::iter::once(&m.id)
                .chain(m.aliases.iter())
                .find(|name| names.contains(*name))
            {
                return Err(ParishError::Config(format!(
                    "provider id/alias collision: {collision:?} is already registered"
                )));
            }
            guard.insert(id, Provider(Arc::new(m)));
        }
        Ok(())
    }
}

static REGISTRY: OnceLock<ProviderRegistry> = OnceLock::new();

pub fn registry() -> &'static ProviderRegistry {
    let r = REGISTRY.get_or_init(ProviderRegistry::load_with_builtins);
    // Auto-discover and register provider mods on first access. This
    // runs in all build profiles so that release startup paths which
    // resolve provider config before the explicit bootstrap
    // (`parish_core::game_mod::register_provider_mods_once`) still see
    // the same registry as debug/test builds. The walk is idempotent
    // (guarded by `Once`) and silently no-ops when no `mods/` directory
    // is discoverable — production deployments that need a non-default
    // location set `PARISH_MODS_DIR`.
    ensure_mods_loaded();
    r
}

/// Discover and register every `mods/<id>/providers/*.toml` from the
/// first directory found via, in priority order:
///
/// 1. `PARISH_MODS_DIR` env var (operator override for packaged builds);
/// 2. walk-up from this crate's compile-time `CARGO_MANIFEST_DIR` to a
///    directory containing `mods/` (dev tree, `cargo test`/`cargo run`).
///
/// Idempotent — guarded by an internal `Once`; safe to call repeatedly.
///
/// Returns silently when no mods directory is discoverable. The explicit
/// `parish_core::game_mod::register_provider_mods_once` bootstrap path is
/// still authoritative for runtime mod loads with surfaced error
/// reporting; this helper exists so that any `Provider::from_id` /
/// `from_str_loose` call hit before that bootstrap (release startup,
/// downstream test crates, env-var-only cloud configs) still finds the
/// shipped provider mods.
pub fn ensure_mods_loaded() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let mods_dir = std::env::var_os("PARISH_MODS_DIR")
            .map(std::path::PathBuf::from)
            .filter(|p| p.is_dir())
            .or_else(|| {
                let crate_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
                let mut dir = crate_dir.to_path_buf();
                loop {
                    let candidate = dir.join("mods");
                    if candidate.is_dir() {
                        break Some(candidate);
                    }
                    if !dir.pop() {
                        break None;
                    }
                }
            });
        let Some(mods_dir) = mods_dir else { return };
        let Ok(read) = std::fs::read_dir(&mods_dir) else {
            return;
        };
        let mut all: Vec<ProviderMod> = Vec::new();
        for entry in read.flatten() {
            let providers = entry.path().join("providers");
            if !providers.is_dir() {
                continue;
            }
            let Ok(files) = std::fs::read_dir(&providers) else {
                continue;
            };
            for f in files.flatten() {
                if f.path().extension().is_none_or(|x| x != "toml") {
                    continue;
                }
                let Ok(raw) = std::fs::read_to_string(f.path()) else {
                    continue;
                };
                if let Ok(m) = toml::from_str::<ProviderMod>(&raw) {
                    all.push(m);
                }
            }
        }
        // Avoid recursing back through `registry()` — it would re-enter this
        // `Once::call_once` and deadlock. Use the underlying static directly,
        // which is guaranteed initialized by the caller in `registry()`.
        let r = REGISTRY.get_or_init(ProviderRegistry::load_with_builtins);
        if let Err(error) = r.register_mod_providers(all) {
            panic!("provider registry bootstrap failed: {error}");
        }
    });
}
