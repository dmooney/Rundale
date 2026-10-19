//! Provider registry: builtin bootstrap + runtime mod discovery.
//!
//! Owns the process-wide [`ProviderRegistry`] static and the [`registry`] /
//! [`ensure_mods_loaded`] accessors. The registry is pre-populated with the
//! engine builtins and has mod-loaded providers merged in post-init. Split
//! out of the monolithic `provider` module (#1200).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};

use limerick_types::LimerickError;

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

    pub fn lookup(&self, s: &str) -> Result<Provider, LimerickError> {
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
        Err(LimerickError::Config(format!(
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
    pub fn register_mod_providers(&self, mods: Vec<ProviderMod>) -> Result<(), LimerickError> {
        let mut guard = self.by_id.write().expect("registry poisoned");
        for m in mods {
            let id = m.id.clone();
            if let Some(recommended) = &m.recommended_preset
                && !m.presets.iter().any(|preset| &preset.key == recommended)
            {
                return Err(LimerickError::Config(format!(
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
                return Err(LimerickError::Config(format!(
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
                return Err(LimerickError::Config(format!(
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
    // (`limerick_core::game_mod::register_provider_mods_once`) still see
    // the same registry as debug/test builds. The walk is idempotent
    // (guarded by `Once`) and silently no-ops when no `mods/` directory
    // is discoverable — production deployments that need a non-default
    // location set `LIMERICK_MODS_DIR`.
    ensure_mods_loaded();
    r
}

/// Discover and register every `mods/<id>/providers/*.toml` from the
/// first directory found via, in priority order:
///
/// 1. `LIMERICK_MODS_DIR` env var (operator override for packaged builds);
/// 2. walk-up from the runtime `CARGO_MANIFEST_DIR` that Cargo sets for the
///    process it runs (`cargo test`/`cargo run`), so a test binary built in
///    another worktree still reads the checkout it runs from;
/// 3. walk-up from this crate's compile-time `CARGO_MANIFEST_DIR` to a
///    directory containing `mods/` (dev binaries launched outside Cargo).
///
/// Idempotent — guarded by an internal `Once`; safe to call repeatedly.
///
/// Returns silently when no mods directory is discoverable. The explicit
/// `limerick_core::game_mod::register_provider_mods_once` bootstrap path is
/// still authoritative for runtime mod loads with surfaced error
/// reporting; this helper exists so that any `Provider::from_id` /
/// `from_str_loose` call hit before that bootstrap (release startup,
/// downstream test crates, env-var-only cloud configs) still finds the
/// shipped provider mods.
pub fn ensure_mods_loaded() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let mods_dir = resolve_mods_dir(
            std::env::var_os("LIMERICK_MODS_DIR").map(PathBuf::from),
            std::env::var_os("CARGO_MANIFEST_DIR").map(PathBuf::from),
            Path::new(env!("CARGO_MANIFEST_DIR")),
        );
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

/// Pick the provider `mods/` directory for [`ensure_mods_loaded`].
///
/// An explicit directory wins. Otherwise walk up from the runtime manifest
/// directory, then from the compile-time one. The compile-time path is last
/// because a reused build artifact can embed a checkout that has since moved
/// or been deleted (#1708).
fn resolve_mods_dir(
    explicit: Option<PathBuf>,
    runtime_manifest_dir: Option<PathBuf>,
    compiled_manifest_dir: &Path,
) -> Option<PathBuf> {
    explicit
        .filter(|p| p.is_dir())
        .or_else(|| runtime_manifest_dir.and_then(|dir| find_mods_above(&dir)))
        .or_else(|| find_mods_above(compiled_manifest_dir))
}

fn find_mods_above(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .map(|dir| dir.join("mods"))
        .find(|candidate| candidate.is_dir())
}

#[cfg(test)]
mod resolve_mods_dir_tests {
    use super::*;

    fn checkout(root: &Path) -> PathBuf {
        std::fs::create_dir_all(root.join("mods")).unwrap();
        let crate_dir = root.join("limerick/crates/limerick-config");
        std::fs::create_dir_all(&crate_dir).unwrap();
        crate_dir
    }

    #[test]
    fn explicit_directory_wins() {
        let tmp = tempfile::tempdir().unwrap();
        let crate_dir = checkout(&tmp.path().join("worktree"));
        let explicit = tmp.path().join("packaged/mods");
        std::fs::create_dir_all(&explicit).unwrap();
        let got = resolve_mods_dir(Some(explicit.clone()), Some(crate_dir.clone()), &crate_dir);
        assert_eq!(got, Some(explicit));
    }

    #[test]
    fn missing_explicit_directory_falls_back() {
        let tmp = tempfile::tempdir().unwrap();
        let crate_dir = checkout(&tmp.path().join("worktree"));
        let got = resolve_mods_dir(Some(tmp.path().join("absent")), None, &crate_dir);
        assert_eq!(got, Some(tmp.path().join("worktree/mods")));
    }

    #[test]
    fn deleted_build_checkout_uses_the_running_checkout() {
        let tmp = tempfile::tempdir().unwrap();
        let running = checkout(&tmp.path().join("running"));
        let deleted = checkout(&tmp.path().join("deleted"));
        std::fs::remove_dir_all(tmp.path().join("deleted")).unwrap();
        let got = resolve_mods_dir(None, Some(running), &deleted);
        assert_eq!(got, Some(tmp.path().join("running/mods")));
    }

    #[test]
    fn running_checkout_wins_over_another_live_checkout() {
        let tmp = tempfile::tempdir().unwrap();
        let running = checkout(&tmp.path().join("running"));
        let other = checkout(&tmp.path().join("other"));
        let got = resolve_mods_dir(None, Some(running), &other);
        assert_eq!(got, Some(tmp.path().join("running/mods")));
    }

    #[test]
    fn compiled_checkout_is_the_last_resort() {
        let tmp = tempfile::tempdir().unwrap();
        let compiled = checkout(&tmp.path().join("compiled"));
        let got = resolve_mods_dir(None, None, &compiled);
        assert_eq!(got, Some(tmp.path().join("compiled/mods")));
    }
}
