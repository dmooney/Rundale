use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use fs2::FileExt;
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

const MAX_CACHE_ENTRIES: usize = 64;
const MAX_CACHE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_CACHE_ENTRY_BYTES: usize = 8 * 1024 * 1024;
const MAX_CACHE_INDEX_BYTES: u64 = 256 * 1024;
const QUARANTINE_SLOTS: usize = 64;
const MAX_CATALOG_MODELS: usize = 5_000;
const MAX_PROBE_HISTORY_ROUTES: usize = 5_000;
const PROBE_HISTORY_SCHEMA_VERSION: u8 = 1;
const MAX_ACTIVE_PROBE_RECEIPTS: usize = 128;
const MAX_PROBE_RECEIPT_BYTES: u64 = 256 * 1024;
const MAX_ACTIVE_PROBE_RECEIPT_BYTES: u64 = 8 * 1024 * 1024;
pub const MODEL_CATALOG_TTL_HOURS: i64 = 24;

use super::*;

#[derive(Debug, Clone)]
pub struct CatalogRuntimeEvidence {
    pub availability: BTreeMap<ModelRouteKey, Availability>,
    pub constrained_registry: BTreeMap<String, ProviderDefinition>,
}

#[derive(Debug, Error)]
pub enum CatalogStoreError {
    #[error("catalog I/O at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("catalog JSON at {path}: {source}")]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("catalog cache {path} has an unexpected identity or schema version")]
    Identity { path: PathBuf },
    #[error("probe artifact {0} already exists; attempts are immutable")]
    ProbeExists(PathBuf),
}

#[derive(Debug, Clone)]
pub struct CatalogStore {
    cache_dir: PathBuf,
    probe_dir: PathBuf,
}

impl CatalogStore {
    pub fn new(cache_dir: PathBuf, probe_dir: PathBuf) -> Self {
        Self {
            cache_dir,
            probe_dir,
        }
    }

    pub fn for_user_config_dir(user_config_dir: &Path) -> Self {
        // Compatibility shim for callers that have not yet separated config
        // and data roots. New production code uses `for_user_data_dir`.
        Self::for_user_data_dir(user_config_dir)
    }

    pub fn for_user_data_dir(user_data_dir: &Path) -> Self {
        Self::new(
            user_data_dir.join("cache/model-catalog/v1"),
            user_data_dir.join("model-probes"),
        )
    }

    pub fn cached_documents(&self) -> Result<Vec<CatalogCacheDocument>, CatalogStoreError> {
        if !self.cache_dir.exists() {
            return Ok(Vec::new());
        }
        let maintenance_path = self.cache_dir.join("index.lock");
        let maintenance = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&maintenance_path)
            .map_err(|source| io_error(maintenance_path.clone(), source))?;
        maintenance
            .lock_exclusive()
            .map_err(|source| io_error(maintenance_path.clone(), source))?;
        let index_path = self.cache_dir.join("index.json");
        let mut paths = read_cache_index_bounded(&index_path)
            .entries
            .into_keys()
            .take(MAX_CACHE_ENTRIES)
            .map(|name| self.cache_dir.join(name))
            .collect::<Vec<_>>();
        paths.sort();
        let mut documents: Vec<CatalogCacheDocument> = Vec::new();
        let mut admitted_count = 0usize;
        let mut admitted_bytes = 0u64;
        for path in paths {
            if path.file_name().and_then(|value| value.to_str()) == Some("index.json") {
                continue;
            }
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let size = std::fs::metadata(&path)
                .map(|metadata| metadata.len())
                .unwrap_or(u64::MAX);
            let oversized = size > MAX_CACHE_ENTRY_BYTES as u64
                || admitted_count >= MAX_CACHE_ENTRIES
                || admitted_bytes.saturating_add(size) > MAX_CACHE_BYTES;
            match (!oversized)
                .then(|| std::fs::read(&path).ok())
                .flatten()
                .and_then(|body| serde_json::from_slice::<CatalogCacheDocument>(&body).ok())
                .filter(|document| validate_cache_document(document, None, &path).is_ok())
            {
                Some(document) => {
                    admitted_count += 1;
                    admitted_bytes = admitted_bytes.saturating_add(size);
                    documents.push(document);
                }
                None => {
                    quarantine_cache_file(&path)?;
                }
            }
        }
        documents.sort_by(|left, right| {
            left.identity
                .provider_id
                .cmp(&right.identity.provider_id)
                .then(left.identity.endpoint_id.cmp(&right.identity.endpoint_id))
        });
        FileExt::unlock(&maintenance).map_err(|source| io_error(maintenance_path, source))?;
        Ok(documents)
    }

    /// Loads eligibility only for the exact active endpoint/origin/adapter and
    /// credential identities. Evidence from another account or gateway can
    /// never grant or revoke this runtime's model eligibility.
    pub fn availability_snapshot_for_routes(
        &self,
        registry: &BTreeMap<String, ProviderDefinition>,
        routes: impl IntoIterator<Item = ResolvedRoute>,
        salt: Option<&[u8]>,
        now: DateTime<Utc>,
    ) -> Result<CatalogRuntimeEvidence, CatalogStoreError> {
        let mut constrained_registry = registry.clone();
        let mut evidence = BTreeMap::<ModelRouteKey, (Availability, DateTime<Utc>)>::new();
        let mut route_identities = BTreeMap::<ModelRouteKey, CatalogCacheIdentity>::new();
        let mut seen = BTreeSet::new();
        for route in routes {
            let fingerprint = match (&route.credential, salt) {
                (None, _) => "anonymous".to_string(),
                (Some(_), None) => continue,
                (credential, Some(salt)) => {
                    catalog_credential_fingerprint(salt, credential.as_ref())
                }
            };
            // Probe evidence remains authoritative for adapters that do not
            // expose a catalog endpoint. The empty discovery URL is part of
            // that exact canonical identity; it must not make the route vanish
            // from the account-isolated observation join.
            let discovery_base_url = route.discovery_base_url.clone().unwrap_or_default();
            let identity = CatalogCacheIdentity {
                provider_id: route.key.provider_id.clone(),
                endpoint_id: route.key.endpoint_id.clone(),
                discovery_base_url,
                inference_base_url: route.inference_base_url.clone(),
                inference_adapter_version: inference_adapter_version(route.inference_adapter),
                discovery_adapter_version: discovery_adapter_version(route.discovery_adapter),
                credential_fingerprint: fingerprint.clone(),
            };
            if let Some(existing) = route_identities.insert(route.key.clone(), identity.clone())
                && existing != identity
            {
                return Err(CatalogStoreError::Identity {
                    path: self.cache_dir.join(format!(
                        "ambiguous-active-route-{}-{}",
                        route.key.provider_id, route.key.endpoint_id
                    )),
                });
            }
            if !seen.insert(identity.clone()) {
                continue;
            }
            let live_document = self
                .load_cache(&identity, now)?
                .map(|(document, _)| document);
            let live = live_document.as_ref();
            let fresh = live.filter(|document| {
                document.status == DiscoveryStatus::Success
                    && document.complete_listing
                    && document.expires_at > now
            });
            if let Some(document) = fresh {
                constrain_registry_from_document(&mut constrained_registry, document);
            }
            if let Some(provider) = registry.get(&route.key.provider_id) {
                for (key, catalog_route) in
                    merge_catalog_routes(provider, &route.key.endpoint_id, live)
                {
                    if let Some(observed_at) = catalog_route.availability.provenance.observed_at {
                        // Parish's production transport requires text in/out
                        // and may stream any admitted workload. Explicit
                        // remote false/missing-text constraints make that
                        // exact route ineligible rather than being ignored.
                        let incompatible = fresh.is_some()
                            && (catalog_route
                                .streaming
                                .as_ref()
                                .is_some_and(|value| !value.value)
                                || catalog_route
                                    .input_modalities
                                    .as_ref()
                                    .is_some_and(|value| !value.value.contains("text"))
                                || catalog_route
                                    .output_modalities
                                    .as_ref()
                                    .is_some_and(|value| !value.value.contains("text")));
                        let availability = if incompatible {
                            Availability::Incompatible
                        } else {
                            catalog_route.availability.value
                        };
                        evidence.insert(key, (availability, observed_at));
                    }
                }
            }
        }

        if let Some(history) = self.load_probe_history(&route_identities, now) {
            let cutoff = now - chrono::Duration::hours(24);
            for route in history.routes {
                if route_identities.get(&route.key) != Some(&route.catalog_identity) {
                    continue;
                }
                if let Some(observation) = route.observations.iter().rev().find(|observation| {
                    observation.observed_at >= cutoff
                        && matches!(
                            observation.kind,
                            CatalogObservationKind::ProbePassed
                                | CatalogObservationKind::ProbeNotListed
                        )
                }) {
                    let value = if observation.kind == CatalogObservationKind::ProbePassed {
                        Availability::Listed
                    } else {
                        Availability::NotListed
                    };
                    let replace = evidence
                        .get(&route.key)
                        .is_none_or(|(current, observed_at)| {
                            *current != Availability::Incompatible
                                && observation.observed_at > *observed_at
                        });
                    if replace {
                        evidence.insert(route.key, (value, observation.observed_at));
                    }
                }
            }
        }
        Ok(CatalogRuntimeEvidence {
            availability: evidence
                .into_iter()
                .map(|(key, (availability, _))| (key, availability))
                .collect(),
            constrained_registry,
        })
    }

    pub fn load_cache(
        &self,
        identity: &CatalogCacheIdentity,
        now: DateTime<Utc>,
    ) -> Result<Option<(CatalogCacheDocument, bool)>, CatalogStoreError> {
        let path = self.cache_dir.join(cache_file_name(identity));
        if std::fs::metadata(&path)
            .map(|metadata| metadata.len() > MAX_CACHE_ENTRY_BYTES as u64)
            .unwrap_or(false)
        {
            let _ = quarantine_cache_file(&path);
            tracing::warn!(path = %path.display(), "oversized catalog cache entry quarantined");
            return Ok(None);
        }
        let body = match std::fs::read(&path) {
            Ok(body) => body,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                tracing::warn!(path = %path.display(), %source, "catalog cache entry unavailable");
                return Ok(None);
            }
        };
        let document: CatalogCacheDocument = match serde_json::from_slice(&body) {
            Ok(document) => document,
            Err(source) => {
                let _ = quarantine_cache_file(&path);
                tracing::warn!(path = %path.display(), %source, "malformed catalog cache entry quarantined");
                return Ok(None);
            }
        };
        if validate_cache_document(&document, Some(identity), &path).is_err() {
            let _ = quarantine_cache_file(&path);
            tracing::warn!(path = %path.display(), "invalid catalog cache entry quarantined");
            return Ok(None);
        }
        let stale = document.expires_at <= now;
        Ok(Some((document, stale)))
    }

    /// Serializes network refreshes for one exact account/route identity.
    pub fn lock_refresh(
        &self,
        identity: &CatalogCacheIdentity,
    ) -> Result<Option<CatalogRefreshGuard>, CatalogStoreError> {
        std::fs::create_dir_all(&self.cache_dir)
            .map_err(|source| io_error(self.cache_dir.clone(), source))?;
        let path = self
            .cache_dir
            .join(cache_file_name(identity))
            .with_extension("refresh.lock");
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|source| io_error(path.clone(), source))?;
        match file.try_lock_exclusive() {
            Ok(()) => Ok(Some(CatalogRefreshGuard { file, path })),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(source) => Err(io_error(path, source)),
        }
    }

    pub fn save_cache(&self, document: &CatalogCacheDocument) -> Result<(), CatalogStoreError> {
        std::fs::create_dir_all(&self.cache_dir)
            .map_err(|source| io_error(self.cache_dir.clone(), source))?;
        let path = self.cache_dir.join(cache_file_name(&document.identity));
        validate_cache_document(document, Some(&document.identity), &path)?;
        let lock_path = path.with_extension("lock");
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)
            .map_err(|source| io_error(lock_path.clone(), source))?;
        lock.lock_exclusive()
            .map_err(|source| io_error(lock_path.clone(), source))?;
        let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
        let bytes =
            serde_json::to_vec_pretty(document).map_err(|source| CatalogStoreError::Json {
                path: path.clone(),
                source,
            })?;
        if bytes.len() > MAX_CACHE_ENTRY_BYTES {
            return Err(CatalogStoreError::Identity { path });
        }
        write_new_or_replace(&temporary, &bytes)?;
        super::loader::atomic_replace(&temporary, &path)
            .map_err(|source| io_error(path, source))?;
        FileExt::unlock(&lock).map_err(|source| io_error(lock_path, source))?;
        self.touch_index(&self.cache_dir.join(cache_file_name(&document.identity)))?;
        self.prune_cache()
    }

    /// Persists the raw billable response and pending provenance before any
    /// caller parses or validates it. `finish_probe` creates a second,
    /// immutable verdict receipt and never mutates these files.
    pub fn persist_probe_raw(
        &self,
        input: ProbeArtifactInput<'_>,
    ) -> Result<PendingProbeArtifact, CatalogStoreError> {
        if !valid_attempt_id(input.attempt_id) {
            return Err(CatalogStoreError::Identity {
                path: self.probe_dir.join("attempts"),
            });
        }
        let attempts = self.probe_dir.join("attempts");
        std::fs::create_dir_all(&attempts).map_err(|source| io_error(attempts.clone(), source))?;
        let attempt_dir = attempts.join(input.attempt_id);
        std::fs::create_dir(&attempt_dir).map_err(|source| {
            if source.kind() == std::io::ErrorKind::AlreadyExists {
                CatalogStoreError::ProbeExists(attempt_dir.clone())
            } else {
                io_error(attempt_dir.clone(), source)
            }
        })?;
        let raw_path = attempt_dir.join("raw-response.bin");
        let pending_path = attempt_dir.join("request.json");
        write_immutable(&raw_path, input.raw_response)?;
        let pending = PendingProbeArtifact {
            attempt_id: input.attempt_id.to_string(),
            route: input.route.clone(),
            catalog_identity: input.catalog_identity.clone(),
            configuration_epoch: input.configuration_epoch,
            started_at: input.started_at,
            request_hash: sha256_hex(input.request_bytes),
            request_body: serde_json::from_slice(input.request_bytes)
                .unwrap_or_else(|_| serde_json::Value::String("<non-json request>".into())),
            request_path: pending_path.to_string_lossy().into_owned(),
            inference_adapter_version: input.inference_adapter_version.to_string(),
            discovery_adapter_version: input.discovery_adapter_version.to_string(),
            provider_request_id: input.provider_request_id.map(str::to_string),
            raw_response_path: raw_path.to_string_lossy().into_owned(),
            raw_response_hash: sha256_hex(input.raw_response),
            pending_path,
            receipt_path: attempt_dir.join("receipt.json"),
        };
        let pending_bytes =
            serde_json::to_vec_pretty(&PendingProbeView::from(&pending)).map_err(|source| {
                CatalogStoreError::Json {
                    path: pending.pending_path.clone(),
                    source,
                }
            })?;
        write_immutable(&pending.pending_path, &pending_bytes)?;
        Ok(pending)
    }

    pub fn finish_probe(
        &self,
        pending: PendingProbeArtifact,
        finished_at: DateTime<Utc>,
        outcome: ProbeOutcome,
        terminal_reason: Option<String>,
        metadata: ProbeTerminalMetadata,
        error: Option<String>,
    ) -> Result<ProbeReceipt, CatalogStoreError> {
        let receipt = ProbeReceipt {
            schema_version: PROBE_RECEIPT_SCHEMA_VERSION,
            attempt_id: pending.attempt_id,
            route: pending.route,
            catalog_identity: pending.catalog_identity,
            configuration_epoch: pending.configuration_epoch,
            started_at: pending.started_at,
            finished_at,
            request_hash: pending.request_hash,
            request_path: pending.request_path,
            inference_adapter_version: pending.inference_adapter_version,
            discovery_adapter_version: pending.discovery_adapter_version,
            provider_request_id: pending.provider_request_id,
            raw_response_path: pending.raw_response_path,
            raw_response_hash: pending.raw_response_hash,
            outcome,
            terminal_reason,
            terminal_http_status: metadata.http_status,
            terminal_event: metadata.terminal_event,
            input_tokens: metadata.input_tokens,
            output_tokens: metadata.output_tokens,
            cost_usd_micros: metadata.cost_usd_micros,
            error,
        };
        let bytes =
            serde_json::to_vec_pretty(&receipt).map_err(|source| CatalogStoreError::Json {
                path: pending.receipt_path.clone(),
                source,
            })?;
        write_immutable(&pending.receipt_path, &bytes)?;
        Ok(receipt)
    }

    pub fn record_probe_observation(
        &self,
        receipt: &ProbeReceipt,
    ) -> Result<(), CatalogStoreError> {
        std::fs::create_dir_all(&self.probe_dir)
            .map_err(|source| io_error(self.probe_dir.clone(), source))?;
        let path = self.probe_dir.join("observations.json");
        let lock_path = self.probe_dir.join("observations.lock");
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)
            .map_err(|source| io_error(lock_path.clone(), source))?;
        lock.lock_exclusive()
            .map_err(|source| io_error(lock_path.clone(), source))?;
        let mut history = read_file_bounded(&path, MAX_CACHE_ENTRY_BYTES as u64)
            .ok()
            .and_then(|body| parse_probe_history_structure(&self.probe_dir, &body, Utc::now()).ok())
            .unwrap_or_else(|| {
                if path.exists() {
                    let _ = quarantine_probe_history(&path);
                }
                ProbeHistoryFile::default()
            });
        let route = if let Some(index) = history.routes.iter().position(|route| {
            route.key == receipt.route && route.catalog_identity == receipt.catalog_identity
        }) {
            &mut history.routes[index]
        } else {
            history.routes.push(ProbeRouteHistory {
                key: receipt.route.clone(),
                catalog_identity: receipt.catalog_identity.clone(),
                observations: Vec::new(),
                omitted_observation_count: 0,
            });
            history
                .routes
                .last_mut()
                .expect("just inserted route history")
        };
        let receipt_hash = serde_json::to_vec(receipt)
            .ok()
            .map(|bytes| sha256_hex(&bytes));
        let observation = CatalogObservation {
            kind: probe_observation_kind(receipt),
            observed_at: receipt.finished_at,
            receipt_id: Some(receipt.attempt_id.clone()),
            receipt_hash,
        };
        validate_probe_observation(
            &self.probe_dir,
            &route.key,
            &route.catalog_identity,
            &observation,
            Utc::now(),
            &mut ReceiptValidationBudget::default(),
        )?;
        route.observations.push(observation);
        if route.observations.len() > 32 {
            let removed = route.observations.len() - 32;
            route.observations.drain(..removed);
            route.omitted_observation_count = route
                .omitted_observation_count
                .saturating_add(removed as u64);
        }
        let bytes =
            serde_json::to_vec_pretty(&history).map_err(|source| CatalogStoreError::Json {
                path: path.clone(),
                source,
            })?;
        if bytes.len() > MAX_CACHE_ENTRY_BYTES {
            return Err(CatalogStoreError::Identity { path });
        }
        parse_probe_history_structure(&self.probe_dir, &bytes, Utc::now())?;
        write_new_or_replace(&path.with_extension("tmp"), &bytes)?;
        super::loader::atomic_replace(&path.with_extension("tmp"), &path)
            .map_err(|source| io_error(path, source))?;
        FileExt::unlock(&lock).map_err(|source| io_error(lock_path, source))
    }

    fn load_probe_history(
        &self,
        active: &BTreeMap<ModelRouteKey, CatalogCacheIdentity>,
        now: DateTime<Utc>,
    ) -> Option<ProbeHistoryFile> {
        let path = self.probe_dir.join("observations.json");
        let body = read_file_bounded(&path, MAX_CACHE_ENTRY_BYTES as u64).ok()?;
        let mut history = match parse_probe_history_structure(&self.probe_dir, &body, now) {
            Ok(history) => history,
            Err(_) => {
                quarantine_probe_history_if_unchanged(&path, &body);
                return None;
            }
        };
        let cutoff = now - chrono::Duration::hours(24);
        let mut budget = ReceiptValidationBudget::default();
        let mut selected = Vec::new();
        for route in history.routes {
            if active.get(&route.key) != Some(&route.catalog_identity) {
                continue;
            }
            let candidate = route
                .observations
                .into_iter()
                .filter(|observation| {
                    observation.observed_at >= cutoff
                        && matches!(
                            observation.kind,
                            CatalogObservationKind::ProbePassed
                                | CatalogObservationKind::ProbeNotListed
                        )
                })
                .max_by_key(|observation| observation.observed_at);
            let Some(observation) = candidate else {
                continue;
            };
            if validate_probe_observation(
                &self.probe_dir,
                &route.key,
                &route.catalog_identity,
                &observation,
                now,
                &mut budget,
            )
            .is_err()
            {
                quarantine_probe_history_if_unchanged(&path, &body);
                return None;
            }
            selected.push(ProbeRouteHistory {
                key: route.key,
                catalog_identity: route.catalog_identity,
                observations: vec![observation],
                omitted_observation_count: route.omitted_observation_count,
            });
        }
        history.routes = selected;
        Some(history)
    }

    fn touch_index(&self, cache_path: &Path) -> Result<(), CatalogStoreError> {
        std::fs::create_dir_all(&self.cache_dir)
            .map_err(|source| io_error(self.cache_dir.clone(), source))?;
        let lock_path = self.cache_dir.join("index.lock");
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)
            .map_err(|source| io_error(lock_path.clone(), source))?;
        lock.lock_exclusive()
            .map_err(|source| io_error(lock_path.clone(), source))?;
        let index_path = self.cache_dir.join("index.json");
        let mut index = read_cache_index_bounded(&index_path);
        let name = cache_path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| CatalogStoreError::Identity {
                path: cache_path.to_path_buf(),
            })?;
        let size = std::fs::metadata(cache_path)
            .map(|meta| meta.len())
            .unwrap_or(0);
        index.entries.insert(
            name.to_string(),
            CatalogCacheIndexEntry {
                accessed_at: Utc::now(),
                size_bytes: size,
            },
        );
        let bytes =
            serde_json::to_vec_pretty(&index).map_err(|source| CatalogStoreError::Json {
                path: index_path.clone(),
                source,
            })?;
        write_new_or_replace(&index_path.with_extension("tmp"), &bytes)?;
        super::loader::atomic_replace(&index_path.with_extension("tmp"), &index_path)
            .map_err(|source| io_error(index_path, source))?;
        FileExt::unlock(&lock).map_err(|source| io_error(lock_path, source))
    }

    fn prune_cache(&self) -> Result<(), CatalogStoreError> {
        let lock_path = self.cache_dir.join("index.lock");
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)
            .map_err(|source| io_error(lock_path.clone(), source))?;
        lock.lock_exclusive()
            .map_err(|source| io_error(lock_path.clone(), source))?;
        let index_path = self.cache_dir.join("index.json");
        let mut index = read_cache_index_bounded(&index_path);
        let mut entries: Vec<_> = index
            .entries
            .iter()
            .map(|(name, entry)| {
                let actual_size = std::fs::metadata(self.cache_dir.join(name))
                    .map(|metadata| metadata.len())
                    .unwrap_or(0);
                (name.clone(), entry.accessed_at, actual_size)
            })
            .collect();
        entries.sort_by_key(|(_, accessed_at, _)| *accessed_at);
        let mut total: u64 = entries.iter().map(|(_, _, size)| *size).sum();
        let mut count = entries.len();
        for (name, _, size) in entries {
            if count <= MAX_CACHE_ENTRIES && total <= MAX_CACHE_BYTES {
                break;
            }
            let path = self.cache_dir.join(&name);
            let refresh_path = path.with_extension("refresh.lock");
            let refresh_lock = OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true)
                .open(&refresh_path)
                .map_err(|source| io_error(refresh_path.clone(), source))?;
            match refresh_lock.try_lock_exclusive() {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => continue,
                Err(source) => return Err(io_error(refresh_path, source)),
            }
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(source) => return Err(io_error(path, source)),
            }
            FileExt::unlock(&refresh_lock).map_err(|source| io_error(refresh_path, source))?;
            index.entries.remove(&name);
            count = count.saturating_sub(1);
            total = total.saturating_sub(size);
        }
        let bytes =
            serde_json::to_vec_pretty(&index).map_err(|source| CatalogStoreError::Json {
                path: index_path.clone(),
                source,
            })?;
        write_new_or_replace(&index_path.with_extension("tmp"), &bytes)?;
        super::loader::atomic_replace(&index_path.with_extension("tmp"), &index_path)
            .map_err(|source| io_error(index_path, source))?;
        FileExt::unlock(&lock).map_err(|source| io_error(lock_path, source))
    }
}

fn constrain_registry_from_document(
    registry: &mut BTreeMap<String, ProviderDefinition>,
    document: &CatalogCacheDocument,
) {
    let Some(models) = registry
        .get_mut(&document.identity.provider_id)
        .and_then(|provider| {
            provider
                .curated_models
                .get_mut(&document.identity.endpoint_id)
        })
    else {
        return;
    };
    for (model_id, remote) in &document.routes {
        let Some(model) = models.get_mut(model_id) else {
            continue;
        };
        if let Some(max) = remote
            .max_output_tokens
            .and_then(|value| u32::try_from(value).ok())
        {
            model.generation.max_output_tokens = model.generation.max_output_tokens.min(max);
        }
        if remote.supports_temperature == Some(false) {
            model.generation.temperature = None;
        }
        if remote.supports_frequency_penalty == Some(false) {
            model.generation.frequency_penalty = None;
        }
        if let Some(tiers) = &remote.service_tiers {
            model
                .generation
                .service_tiers
                .retain(|intent, _| tiers.contains(intent));
        }
        if let Some(levels) = &remote.reasoning_effort_levels
            && let Some(effort) = &mut model.reasoning.effort
        {
            effort
                .supported_levels
                .retain(|level| levels.contains(level));
            if effort.supported_levels.is_empty() {
                model.reasoning.effort = None;
            }
        }
        if remote.supports_reasoning_budget == Some(false) {
            model.reasoning.budget = None;
        }
        if remote.supports_reasoning_off == Some(false) {
            model.reasoning.off_dialect = None;
        }
        if let Some(output) = &remote.output_contracts {
            // PromptValidatedJson is a local prompt/parser contract, not a
            // remote wire capability. Discovery may narrow native modes only.
            model.output_contracts.native_json_object &= output.native_json_object;
            model.output_contracts.native_json_schema &= output.native_json_schema;
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ProbeTerminalMetadata {
    pub http_status: Option<u16>,
    pub terminal_event: Option<String>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cost_usd_micros: Option<u64>,
}

/// Adds one immutable probe receipt reference while retaining a bounded
/// recent history. The receipt artifact itself remains the authority.
pub fn append_probe_observation(route: &mut CatalogModelRoute, receipt: &ProbeReceipt) {
    let receipt_hash = serde_json::to_vec(receipt)
        .ok()
        .map(|bytes| sha256_hex(&bytes));
    route.observations.push(CatalogObservation {
        kind: probe_observation_kind(receipt),
        observed_at: receipt.finished_at,
        receipt_id: Some(receipt.attempt_id.clone()),
        receipt_hash,
    });
    const MAX_OBSERVATIONS: usize = 32;
    if route.observations.len() > MAX_OBSERVATIONS {
        let remove = route.observations.len() - MAX_OBSERVATIONS;
        route.observations.drain(..remove);
        route.omitted_observation_count = route
            .omitted_observation_count
            .saturating_add(remove as u64);
    }
}

fn probe_observation_kind(receipt: &ProbeReceipt) -> CatalogObservationKind {
    if receipt.outcome == ProbeOutcome::Passed {
        CatalogObservationKind::ProbePassed
    } else if receipt.outcome == ProbeOutcome::NotListed {
        CatalogObservationKind::ProbeNotListed
    } else {
        CatalogObservationKind::ProbeFailed
    }
}

#[derive(Default, Serialize, Deserialize)]
struct CatalogCacheIndex {
    entries: BTreeMap<String, CatalogCacheIndexEntry>,
}

#[derive(Serialize, Deserialize)]
struct CatalogCacheIndexEntry {
    accessed_at: DateTime<Utc>,
    size_bytes: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProbeHistoryFile {
    schema_version: u8,
    routes: Vec<ProbeRouteHistory>,
}

impl Default for ProbeHistoryFile {
    fn default() -> Self {
        Self {
            schema_version: PROBE_HISTORY_SCHEMA_VERSION,
            routes: Vec::new(),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProbeRouteHistory {
    key: ModelRouteKey,
    catalog_identity: CatalogCacheIdentity,
    observations: Vec<CatalogObservation>,
    omitted_observation_count: u64,
}

fn parse_probe_history_structure(
    probe_dir: &Path,
    body: &[u8],
    now: DateTime<Utc>,
) -> Result<ProbeHistoryFile, CatalogStoreError> {
    let path = probe_dir.join("observations.json");
    let history: ProbeHistoryFile =
        serde_json::from_slice(body).map_err(|source| CatalogStoreError::Json {
            path: path.clone(),
            source,
        })?;
    if history.schema_version != PROBE_HISTORY_SCHEMA_VERSION
        || history.routes.len() > MAX_PROBE_HISTORY_ROUTES
    {
        return Err(CatalogStoreError::Identity { path });
    }
    let mut identities = BTreeSet::new();
    for route in &history.routes {
        if route.observations.len() > 32
            || !identities.insert((route.key.clone(), route.catalog_identity.clone()))
            || route.key.provider_id.is_empty()
            || route.key.provider_id.len() > 128
            || route.key.endpoint_id.is_empty()
            || route.key.endpoint_id.len() > 128
            || route.key.model_id.is_empty()
            || route.key.model_id.len() > 512
        {
            return Err(CatalogStoreError::Identity { path });
        }
        let mut previous_time = None;
        for observation in &route.observations {
            let valid_reference = observation
                .receipt_id
                .as_deref()
                .is_some_and(valid_attempt_id)
                && observation
                    .receipt_hash
                    .as_deref()
                    .is_some_and(valid_sha256_hex);
            if !valid_reference
                || observation.observed_at > now + chrono::Duration::minutes(5)
                || previous_time.is_some_and(|previous| observation.observed_at < previous)
            {
                return Err(CatalogStoreError::Identity { path });
            }
            previous_time = Some(observation.observed_at);
        }
    }
    Ok(history)
}

#[derive(Default)]
struct ReceiptValidationBudget {
    receipts: usize,
    bytes: u64,
}

fn validate_probe_observation(
    probe_dir: &Path,
    route: &ModelRouteKey,
    identity: &CatalogCacheIdentity,
    observation: &CatalogObservation,
    now: DateTime<Utc>,
    budget: &mut ReceiptValidationBudget,
) -> Result<(), CatalogStoreError> {
    let invalid = || CatalogStoreError::Identity {
        path: probe_dir.join("observations.json"),
    };
    let attempt_id = observation.receipt_id.as_deref().ok_or_else(invalid)?;
    let expected_hash = observation.receipt_hash.as_deref().ok_or_else(invalid)?;
    if !valid_attempt_id(attempt_id)
        || !valid_sha256_hex(expected_hash)
        || observation.observed_at > now + chrono::Duration::minutes(5)
    {
        return Err(invalid());
    }
    let receipt_path = probe_dir
        .join("attempts")
        .join(attempt_id)
        .join("receipt.json");
    if budget.receipts >= MAX_ACTIVE_PROBE_RECEIPTS
        || budget.bytes >= MAX_ACTIVE_PROBE_RECEIPT_BYTES
    {
        return Err(invalid());
    }
    let remaining = MAX_ACTIVE_PROBE_RECEIPT_BYTES.saturating_sub(budget.bytes);
    let receipt_body = read_file_bounded(&receipt_path, MAX_PROBE_RECEIPT_BYTES.min(remaining))
        .map_err(|_| invalid())?;
    let receipt_size = receipt_body.len() as u64;
    budget.receipts = budget.receipts.saturating_add(1);
    budget.bytes = budget.bytes.saturating_add(receipt_size);
    if budget.receipts > MAX_ACTIVE_PROBE_RECEIPTS || budget.bytes > MAX_ACTIVE_PROBE_RECEIPT_BYTES
    {
        return Err(invalid());
    }
    let receipt: ProbeReceipt = serde_json::from_slice(&receipt_body).map_err(|_| invalid())?;
    let canonical = serde_json::to_vec(&receipt).map_err(|_| invalid())?;
    let expected_kind = probe_observation_kind(&receipt);
    if receipt.schema_version != PROBE_RECEIPT_SCHEMA_VERSION
        || receipt.attempt_id != attempt_id
        || &receipt.route != route
        || &receipt.catalog_identity != identity
        || receipt.started_at > receipt.finished_at
        || receipt.finished_at != observation.observed_at
        || observation.kind != expected_kind
        || sha256_hex(&canonical) != expected_hash
    {
        return Err(invalid());
    }
    Ok(())
}

fn valid_attempt_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn read_file_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, CatalogStoreError> {
    let file = File::open(path).map_err(|source| io_error(path.to_path_buf(), source))?;
    read_open_file_bounded(file, path, limit)
}

fn read_open_file_bounded(
    file: File,
    path: &Path,
    limit: u64,
) -> Result<Vec<u8>, CatalogStoreError> {
    if file
        .metadata()
        .map_err(|source| io_error(path.to_path_buf(), source))?
        .len()
        > limit
    {
        return Err(CatalogStoreError::Identity {
            path: path.to_path_buf(),
        });
    }
    let mut body = Vec::new();
    file.take(limit.saturating_add(1))
        .read_to_end(&mut body)
        .map_err(|source| io_error(path.to_path_buf(), source))?;
    if body.len() as u64 > limit {
        return Err(CatalogStoreError::Identity {
            path: path.to_path_buf(),
        });
    }
    Ok(body)
}

fn quarantine_probe_history(path: &Path) -> Result<(), CatalogStoreError> {
    let quarantine = path.with_file_name("observations.bad");
    match super::loader::atomic_replace(path, &quarantine) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(io_error(quarantine, source)),
    }
}

fn quarantine_probe_history_if_unchanged(path: &Path, observed_body: &[u8]) {
    let lock_path = path.with_file_name("observations.lock");
    let Ok(lock) = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
    else {
        return;
    };
    if lock.try_lock_exclusive().is_err() {
        return;
    }
    if read_file_bounded(path, MAX_CACHE_ENTRY_BYTES as u64)
        .is_ok_and(|current| current == observed_body)
    {
        let _ = quarantine_probe_history(path);
    }
    let _ = FileExt::unlock(&lock);
}

pub fn inference_adapter_version(adapter: InferenceAdapter) -> String {
    format!(
        "{}@1",
        serde_json::to_value(adapter)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_else(|| "unknown".into())
    )
}

pub fn discovery_adapter_version(adapter: DiscoveryAdapter) -> String {
    format!(
        "{}@1",
        serde_json::to_value(adapter)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_else(|| "unknown".into())
    )
}

pub struct CatalogRefreshGuard {
    file: File,
    path: PathBuf,
}

impl Drop for CatalogRefreshGuard {
    fn drop(&mut self) {
        if let Err(error) = FileExt::unlock(&self.file) {
            tracing::warn!(path = %self.path.display(), %error, "catalog refresh lock release failed");
        }
    }
}

pub fn load_or_create_catalog_salt(user_data_dir: &Path) -> Result<Vec<u8>, CatalogStoreError> {
    #[cfg(windows)]
    {
        // Rust's standard library cannot validate or create an owner-only ACL.
        // Fail closed: callers keep anonymous caching and disable authenticated
        // disk caching for this run.
        return Err(CatalogStoreError::Identity {
            path: user_data_dir.join("cache/model-catalog/install-salt.bin"),
        });
    }
    #[cfg(not(any(unix, windows)))]
    {
        return Err(CatalogStoreError::Identity {
            path: user_data_dir.join("cache/model-catalog/install-salt.bin"),
        });
    }
    #[cfg(unix)]
    {
        load_or_create_catalog_salt_unix(user_data_dir)
    }
}

#[cfg(unix)]
fn load_or_create_catalog_salt_unix(user_data_dir: &Path) -> Result<Vec<u8>, CatalogStoreError> {
    use std::io::Read as _;
    use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _};

    let root = user_data_dir.join("cache/model-catalog");
    std::fs::create_dir_all(&root).map_err(|source| io_error(root.clone(), source))?;
    let path = root.join("install-salt.bin");
    let lock_path = root.join("install-salt.lock");
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|source| io_error(lock_path.clone(), source))?;
    lock.lock_exclusive()
        .map_err(|source| io_error(lock_path.clone(), source))?;
    // SAFETY: geteuid has no preconditions and does not dereference pointers.
    let effective_uid = unsafe { libc::geteuid() };
    let result = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)
    {
        Ok(mut file) => {
            let metadata = file
                .metadata()
                .map_err(|source| io_error(path.clone(), source))?;
            if !metadata.file_type().is_file()
                || metadata.uid() != effective_uid
                || metadata.mode() & 0o077 != 0
                || metadata.nlink() != 1
            {
                return Err(CatalogStoreError::Identity { path });
            }
            let mut bytes = Vec::with_capacity(32);
            file.read_to_end(&mut bytes)
                .map_err(|source| io_error(path.clone(), source))?;
            if bytes.len() == 32 {
                Ok(bytes)
            } else {
                Err(CatalogStoreError::Identity { path })
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let bytes = rand::random::<[u8; 32]>();
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(&path)
                .map_err(|source| io_error(path.clone(), source))?;
            file.write_all(&bytes)
                .and_then(|()| file.sync_all())
                .map_err(|source| io_error(path.clone(), source))?;
            Ok(bytes.to_vec())
        }
        Err(source) => Err(io_error(path, source)),
    };
    FileExt::unlock(&lock).map_err(|source| io_error(lock_path, source))?;
    result
}

pub fn catalog_credential_fingerprint(salt: &[u8], credential: Option<&SecretString>) -> String {
    let Some(credential) = credential else {
        return "anonymous".into();
    };
    let mut mac = Hmac::<Sha256>::new_from_slice(salt).expect("HMAC accepts arbitrary key length");
    mac.update(credential.expose().trim().as_bytes());
    sha256_hex(&mac.finalize().into_bytes())
}

pub struct ProbeArtifactInput<'a> {
    pub attempt_id: &'a str,
    pub route: &'a ModelRouteKey,
    pub catalog_identity: &'a CatalogCacheIdentity,
    pub configuration_epoch: u64,
    pub started_at: DateTime<Utc>,
    pub request_bytes: &'a [u8],
    pub inference_adapter_version: &'a str,
    pub discovery_adapter_version: &'a str,
    pub raw_response: &'a [u8],
    pub provider_request_id: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct PendingProbeArtifact {
    attempt_id: String,
    route: ModelRouteKey,
    catalog_identity: CatalogCacheIdentity,
    configuration_epoch: u64,
    started_at: DateTime<Utc>,
    request_hash: String,
    request_body: serde_json::Value,
    request_path: String,
    inference_adapter_version: String,
    discovery_adapter_version: String,
    provider_request_id: Option<String>,
    raw_response_path: String,
    raw_response_hash: String,
    pending_path: PathBuf,
    receipt_path: PathBuf,
}

#[derive(serde::Serialize)]
struct PendingProbeView<'a> {
    schema_version: u8,
    status: &'static str,
    attempt_id: &'a str,
    route: &'a ModelRouteKey,
    catalog_identity: &'a CatalogCacheIdentity,
    configuration_epoch: u64,
    started_at: DateTime<Utc>,
    request_hash: &'a str,
    request_body: &'a serde_json::Value,
    request_path: &'a str,
    inference_adapter_version: &'a str,
    discovery_adapter_version: &'a str,
    provider_request_id: &'a Option<String>,
    raw_response_path: &'a str,
    raw_response_hash: &'a str,
}

impl<'a> From<&'a PendingProbeArtifact> for PendingProbeView<'a> {
    fn from(value: &'a PendingProbeArtifact) -> Self {
        Self {
            schema_version: PROBE_RECEIPT_SCHEMA_VERSION,
            status: "pending-validation",
            attempt_id: &value.attempt_id,
            route: &value.route,
            catalog_identity: &value.catalog_identity,
            configuration_epoch: value.configuration_epoch,
            started_at: value.started_at,
            request_hash: &value.request_hash,
            request_body: &value.request_body,
            request_path: &value.request_path,
            inference_adapter_version: &value.inference_adapter_version,
            discovery_adapter_version: &value.discovery_adapter_version,
            provider_request_id: &value.provider_request_id,
            raw_response_path: &value.raw_response_path,
            raw_response_hash: &value.raw_response_hash,
        }
    }
}

pub fn cache_file_name(identity: &CatalogCacheIdentity) -> String {
    format!(
        "{}.json",
        sha256_hex(
            format!(
                "{}\0{}\0{}\0{}\0{}\0{}\0{}",
                identity.provider_id,
                identity.endpoint_id,
                identity.discovery_base_url,
                identity.inference_base_url,
                identity.inference_adapter_version,
                identity.discovery_adapter_version,
                identity.credential_fingerprint
            )
            .as_bytes()
        )
    )
}

fn validate_cache_document(
    document: &CatalogCacheDocument,
    expected_identity: Option<&CatalogCacheIdentity>,
    path: &Path,
) -> Result<(), CatalogStoreError> {
    let valid_identity = expected_identity.is_none_or(|expected| expected == &document.identity);
    let valid_name = path.file_name().and_then(|value| value.to_str())
        == Some(cache_file_name(&document.identity).as_str());
    let valid_status = document.status != DiscoveryStatus::Success
        || (document.complete_listing
            && document.last_refresh_attempt_at.is_some()
            && document.payload_hash.is_some());
    let valid_hash = document
        .payload_hash
        .as_ref()
        .is_none_or(|hash| valid_sha256_hex(hash));
    let clock_tolerance = chrono::Duration::minutes(5);
    let now = Utc::now();
    let valid_time = document.last_refresh_attempt_at.is_none_or(|attempted| {
        attempted >= document.fetched_at
            && attempted <= now + clock_tolerance
            && document.expires_at
                <= attempted + chrono::Duration::hours(MODEL_CATALOG_TTL_HOURS) + clock_tolerance
    });
    let valid_inference_url = document
        .identity
        .inference_adapter_version
        .starts_with("simulator@")
        || url::Url::parse(&document.identity.inference_base_url).is_ok();
    let valid_urls = valid_inference_url
        && (document.identity.discovery_base_url.is_empty()
            || url::Url::parse(&document.identity.discovery_base_url).is_ok());
    let valid_routes = document.routes.len() <= MAX_CATALOG_MODELS
        && document.routes.iter().all(|(id, model)| {
            !id.is_empty()
                && id == &model.model_id
                && model.max_input_tokens.is_none_or(|value| value > 0)
                && model.max_output_tokens.is_none_or(|value| value > 0)
        });
    let known_conflict_fields = BTreeSet::from([
        "model-membership",
        "display_name",
        "max_input_tokens",
        "max_output_tokens",
        "input_modalities",
        "output_modalities",
        "streaming",
        "output_contracts",
        "reasoning_effort_levels",
        "supports_reasoning_budget",
        "supports_reasoning_off",
        "service_tiers",
        "supports_temperature",
        "supports_frequency_penalty",
    ]);
    let valid_conflicts = document.conflicting_observations.len() <= 32
        && document.conflicting_observations.iter().all(|conflict| {
            let membership_values = conflict.field != "model-membership"
                || matches!(
                    (
                        conflict.change_kind,
                        conflict.previous_value.as_str(),
                        conflict.observed_value.as_str()
                    ),
                    (CatalogConflictKind::Added, "absent", "listed")
                        | (CatalogConflictKind::Removed, "listed", "absent")
                );
            let typed_change = if conflict.field == "model-membership" {
                membership_values
            } else {
                match conflict.change_kind {
                    CatalogConflictKind::Added => {
                        conflict.previous_value == "null" && conflict.observed_value != "null"
                    }
                    CatalogConflictKind::Removed => {
                        conflict.previous_value != "null" && conflict.observed_value == "null"
                    }
                    CatalogConflictKind::Changed => {
                        conflict.previous_value != "null"
                            && conflict.observed_value != "null"
                            && conflict.previous_value != conflict.observed_value
                    }
                }
            };
            !conflict.model_id.is_empty()
                && conflict.model_id.len() <= 512
                && known_conflict_fields.contains(conflict.field.as_str())
                && conflict.previous_value.len() <= 4_096
                && conflict.observed_value.len() <= 4_096
                && valid_sha256_hex(&conflict.previous_payload_hash)
                && valid_sha256_hex(&conflict.payload_hash)
                && conflict.previous_observed_at <= conflict.observed_at
                && conflict.observed_at <= now + clock_tolerance
                && membership_values
                && typed_change
        });
    if document.schema_version != CATALOG_CACHE_SCHEMA_VERSION
        || !valid_identity
        || !valid_name
        || !valid_status
        || !valid_hash
        || !valid_time
        || !valid_urls
        || !valid_routes
        || !valid_conflicts
    {
        return Err(CatalogStoreError::Identity {
            path: path.to_path_buf(),
        });
    }
    Ok(())
}

fn valid_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub fn merge_catalog_routes(
    provider: &ProviderDefinition,
    endpoint_id: &str,
    live: Option<&CatalogCacheDocument>,
) -> BTreeMap<ModelRouteKey, CatalogModelRoute> {
    let curated = provider.curated_models.get(endpoint_id);
    let authoritative_live = live.filter(|cache| {
        cache.status == DiscoveryStatus::Success
            && cache.complete_listing
            && cache.expires_at >= Utc::now()
    });
    // A failed refresh never erases the last-good listing. Only a fresh,
    // complete listing may assert NotListed; stale retained routes remain
    // usable as Unverified evidence.
    let retained_models = live.map(|cache| &cache.routes);
    let ids = curated
        .into_iter()
        .flat_map(|models| models.keys())
        .chain(retained_models.into_iter().flat_map(|models| models.keys()))
        .cloned()
        .collect::<BTreeSet<_>>();
    ids.into_iter()
        .map(|model_id| {
            let key = ModelRouteKey {
                provider_id: provider.id.clone(),
                endpoint_id: endpoint_id.to_string(),
                model_id: model_id.clone(),
            };
            let discovered = retained_models.and_then(|models| models.get(&model_id));
            let availability = if authoritative_live.is_some() {
                if discovered.is_some() {
                    Availability::Listed
                } else {
                    Availability::NotListed
                }
            } else if curated.is_some_and(|models| models.contains_key(&model_id)) {
                Availability::Unknown
            } else {
                Availability::Unverified
            };
            let observed_at = live.map(|cache| cache.fetched_at);
            let payload_hash = live.and_then(|cache| cache.payload_hash.as_deref());
            let source_id = format!("{}:{endpoint_id}", provider.id);
            let route = CatalogModelRoute {
                key: key.clone(),
                availability: Provenanced {
                    value: availability.clone(),
                    provenance: Provenance {
                        kind: if discovered.is_some() {
                            ProvenanceKind::Remote
                        } else {
                            ProvenanceKind::Curated
                        },
                        source_id: source_id.clone(),
                        observed_at,
                        payload_hash: payload_hash.map(str::to_string),
                    },
                },
                display_name: discovered.and_then(|model| {
                    model.display_name.clone().map(|value| Provenanced {
                        value,
                        provenance: remote_provenance(&source_id, observed_at, payload_hash),
                    })
                }),
                max_input_tokens: discovered.and_then(|model| {
                    model.max_input_tokens.map(|value| Provenanced {
                        value,
                        provenance: remote_provenance(&source_id, observed_at, payload_hash),
                    })
                }),
                max_output_tokens: discovered.and_then(|model| {
                    model.max_output_tokens.map(|value| Provenanced {
                        value,
                        provenance: remote_provenance(&source_id, observed_at, payload_hash),
                    })
                }),
                input_modalities: discovered.and_then(|model| {
                    model.input_modalities.clone().map(|value| Provenanced {
                        value,
                        provenance: remote_provenance(&source_id, observed_at, payload_hash),
                    })
                }),
                output_modalities: discovered.and_then(|model| {
                    model.output_modalities.clone().map(|value| Provenanced {
                        value,
                        provenance: remote_provenance(&source_id, observed_at, payload_hash),
                    })
                }),
                streaming: discovered.and_then(|model| {
                    model.streaming.map(|value| Provenanced {
                        value,
                        provenance: remote_provenance(&source_id, observed_at, payload_hash),
                    })
                }),
                output_contracts: discovered.and_then(|model| {
                    model.output_contracts.clone().map(|value| Provenanced {
                        value,
                        provenance: remote_provenance(&source_id, observed_at, payload_hash),
                    })
                }),
                reasoning_effort_levels: discovered.and_then(|model| {
                    model
                        .reasoning_effort_levels
                        .clone()
                        .map(|value| Provenanced {
                            value,
                            provenance: remote_provenance(&source_id, observed_at, payload_hash),
                        })
                }),
                supports_reasoning_budget: discovered.and_then(|model| {
                    model.supports_reasoning_budget.map(|value| Provenanced {
                        value,
                        provenance: remote_provenance(&source_id, observed_at, payload_hash),
                    })
                }),
                supports_reasoning_off: discovered.and_then(|model| {
                    model.supports_reasoning_off.map(|value| Provenanced {
                        value,
                        provenance: remote_provenance(&source_id, observed_at, payload_hash),
                    })
                }),
                service_tiers: discovered.and_then(|model| {
                    model.service_tiers.clone().map(|value| Provenanced {
                        value,
                        provenance: remote_provenance(&source_id, observed_at, payload_hash),
                    })
                }),
                observations: observed_at
                    .map(|observed_at| {
                        vec![CatalogObservation {
                            kind: if availability == Availability::Listed {
                                CatalogObservationKind::Listed
                            } else {
                                CatalogObservationKind::NotListed
                            },
                            observed_at,
                            receipt_id: None,
                            receipt_hash: None,
                        }]
                    })
                    .unwrap_or_default(),
                omitted_observation_count: 0,
            };
            (key, route)
        })
        .collect()
}

fn remote_provenance(
    source_id: &str,
    observed_at: Option<DateTime<Utc>>,
    payload_hash: Option<&str>,
) -> Provenance {
    Provenance {
        kind: ProvenanceKind::Remote,
        source_id: source_id.to_string(),
        observed_at,
        payload_hash: payload_hash.map(str::to_string),
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut output, byte| {
            write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
            output
        })
}

fn write_new_or_replace(path: &Path, bytes: &[u8]) -> Result<(), CatalogStoreError> {
    let mut file = File::create(path).map_err(|source| io_error(path.to_path_buf(), source))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|source| io_error(path.to_path_buf(), source))
}

fn write_immutable(path: &Path, bytes: &[u8]) -> Result<(), CatalogStoreError> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|source| {
            if source.kind() == std::io::ErrorKind::AlreadyExists {
                CatalogStoreError::ProbeExists(path.to_path_buf())
            } else {
                io_error(path.to_path_buf(), source)
            }
        })?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|source| io_error(path.to_path_buf(), source))
}

fn read_cache_index_bounded(path: &Path) -> CatalogCacheIndex {
    let valid_size = std::fs::metadata(path)
        .map(|metadata| metadata.len() <= MAX_CACHE_INDEX_BYTES)
        .unwrap_or(true);
    if !valid_size {
        let _ = quarantine_index_file(path);
        return CatalogCacheIndex::default();
    }
    match std::fs::read(path)
        .ok()
        .and_then(|body| serde_json::from_slice::<CatalogCacheIndex>(&body).ok())
    {
        Some(index)
            if index.entries.len() <= MAX_CACHE_ENTRIES.saturating_mul(4)
                && index
                    .entries
                    .keys()
                    .all(|name| valid_cache_entry_name(name)) =>
        {
            index
        }
        Some(_) | None => {
            if path.exists() {
                let _ = quarantine_index_file(path);
            }
            CatalogCacheIndex::default()
        }
    }
}

fn valid_cache_entry_name(name: &str) -> bool {
    name.len() == 69
        && name.ends_with(".json")
        && name[..64]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn quarantine_index_file(path: &Path) -> Result<(), CatalogStoreError> {
    let quarantine_dir = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("quarantine");
    std::fs::create_dir_all(&quarantine_dir)
        .map_err(|source| io_error(quarantine_dir.clone(), source))?;
    let quarantine = quarantine_dir.join("index.bad");
    match super::loader::atomic_replace(path, &quarantine) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(io_error(quarantine, source)),
    }
}

fn quarantine_cache_file(path: &Path) -> Result<(), CatalogStoreError> {
    let valid_direct_entry = path
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(valid_cache_entry_name)
        && path
            .parent()
            .is_some_and(|parent| !parent.as_os_str().is_empty());
    if !valid_direct_entry {
        return Err(CatalogStoreError::Identity {
            path: path.to_path_buf(),
        });
    }
    let entry_lock_path = path.with_extension("lock");
    let entry_lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&entry_lock_path)
        .map_err(|source| io_error(entry_lock_path.clone(), source))?;
    match entry_lock.try_lock_exclusive() {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
        Err(source) => return Err(io_error(entry_lock_path, source)),
    }
    let quarantine_dir = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("quarantine");
    std::fs::create_dir_all(&quarantine_dir)
        .map_err(|source| io_error(quarantine_dir.clone(), source))?;
    let hash = sha256_hex(path.as_os_str().as_encoded_bytes());
    let slot = usize::from_str_radix(&hash[..8], 16).unwrap_or(0) % QUARANTINE_SLOTS;
    let quarantine = quarantine_dir.join(format!("slot-{slot:02}.bad"));
    match super::loader::atomic_replace(path, &quarantine) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(source) => return Err(io_error(quarantine, source)),
    }
    FileExt::unlock(&entry_lock).map_err(|source| io_error(entry_lock_path, source))
}

fn io_error(path: PathBuf, source: std::io::Error) -> CatalogStoreError {
    CatalogStoreError::Io { path, source }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simulator_route() -> (BTreeMap<String, ProviderDefinition>, ResolvedRoute) {
        let mut registry = crate::v2::compiled_provider_registry_v2();
        let template = registry
            .values()
            .flat_map(|provider| provider.curated_models.values())
            .flat_map(|models| models.values())
            .next()
            .expect("compiled registry has curated route capabilities")
            .clone();
        let simulator_endpoint = registry["simulator"].default_endpoint.clone();
        registry
            .get_mut("simulator")
            .unwrap()
            .curated_models
            .entry(simulator_endpoint)
            .or_default()
            .insert("simulator".into(), template);
        let provider = registry.get("simulator").unwrap();
        let endpoint = provider.endpoints.get(&provider.default_endpoint).unwrap();
        let model_id = "simulator".to_string();
        let profile = GenerationProfile {
            max_output_tokens: 256,
            temperature: None,
            frequency_penalty: None,
            reasoning: ReasoningIntent::Auto,
            service_tier: ServiceTierIntent::Auto,
        };
        let route = ResolvedRoute {
            key: ModelRouteKey {
                provider_id: provider.id.clone(),
                endpoint_id: provider.default_endpoint.clone(),
                model_id,
            },
            inference_base_url: endpoint.inference_base_url.clone(),
            discovery_base_url: endpoint.discovery_base_url.clone(),
            credential: None,
            inference_adapter: endpoint.inference_adapter,
            discovery_adapter: endpoint.discovery_adapter,
            backend_kind: endpoint.backend_kind,
            management_adapter: endpoint.management_adapter,
            auth_adapter: endpoint.auth_adapter,
            reasoning_dialect: endpoint.default_reasoning_dialect,
            openai_output_limit_field: endpoint
                .default_openai_generation_wire
                .as_ref()
                .map(|wire| wire.output_limit_field),
            requested_profile: profile.clone(),
            effective_profile: profile,
            structured_output: None,
            availability: Availability::Unknown,
            diagnostics: Vec::new(),
        };
        (registry, route)
    }

    fn route_identity(route: &ResolvedRoute) -> CatalogCacheIdentity {
        CatalogCacheIdentity {
            provider_id: route.key.provider_id.clone(),
            endpoint_id: route.key.endpoint_id.clone(),
            discovery_base_url: route.discovery_base_url.clone().unwrap_or_default(),
            inference_base_url: route.inference_base_url.clone(),
            inference_adapter_version: inference_adapter_version(route.inference_adapter),
            discovery_adapter_version: discovery_adapter_version(route.discovery_adapter),
            credential_fingerprint: "anonymous".into(),
        }
    }

    fn probe_receipt(
        route: &ResolvedRoute,
        identity: CatalogCacheIdentity,
        attempt_id: &str,
        finished_at: DateTime<Utc>,
        outcome: ProbeOutcome,
    ) -> ProbeReceipt {
        ProbeReceipt {
            schema_version: PROBE_RECEIPT_SCHEMA_VERSION,
            attempt_id: attempt_id.into(),
            route: route.key.clone(),
            catalog_identity: identity,
            configuration_epoch: 1,
            started_at: finished_at - chrono::Duration::seconds(1),
            finished_at,
            request_hash: "request".into(),
            request_path: "request.json".into(),
            inference_adapter_version: inference_adapter_version(route.inference_adapter),
            discovery_adapter_version: discovery_adapter_version(route.discovery_adapter),
            provider_request_id: None,
            raw_response_path: "raw-response.bin".into(),
            raw_response_hash: "raw".into(),
            outcome,
            terminal_reason: None,
            terminal_http_status: Some(200),
            terminal_event: Some("completed".into()),
            input_tokens: None,
            output_tokens: None,
            cost_usd_micros: None,
            error: None,
        }
    }

    fn record_test_probe(store: &CatalogStore, receipt: ProbeReceipt) {
        write_test_receipt(store, &receipt);
        store.record_probe_observation(&receipt).unwrap();
    }

    fn write_test_receipt(store: &CatalogStore, receipt: &ProbeReceipt) {
        let attempt_dir = store.probe_dir.join("attempts").join(&receipt.attempt_id);
        std::fs::create_dir_all(&attempt_dir).unwrap();
        std::fs::write(
            attempt_dir.join("receipt.json"),
            serde_json::to_vec_pretty(&receipt).unwrap(),
        )
        .unwrap();
    }

    fn identity() -> CatalogCacheIdentity {
        CatalogCacheIdentity {
            provider_id: "provider".into(),
            endpoint_id: "chat".into(),
            discovery_base_url: "https://example.test/v1".into(),
            inference_base_url: "https://example.test/v1".into(),
            inference_adapter_version: "openai-chat-v1@1".into(),
            discovery_adapter_version: "openai-models-v1@1".into(),
            credential_fingerprint: "credential-hmac".into(),
        }
    }

    fn valid_cache_document(
        identity: CatalogCacheIdentity,
        now: DateTime<Utc>,
    ) -> CatalogCacheDocument {
        CatalogCacheDocument {
            schema_version: CATALOG_CACHE_SCHEMA_VERSION,
            identity,
            fetched_at: now,
            last_refresh_attempt_at: Some(now),
            expires_at: now + chrono::Duration::hours(1),
            status: DiscoveryStatus::Success,
            complete_listing: true,
            etag: None,
            last_modified: None,
            payload_hash: Some("a".repeat(64)),
            retry_after_at: None,
            consecutive_failures: 0,
            routes: BTreeMap::new(),
            diagnostics: Vec::new(),
            conflicting_observations: Vec::new(),
        }
    }

    #[test]
    fn cache_identity_never_contains_raw_credential_material() {
        let name = cache_file_name(&identity());
        assert_eq!(name.len(), 69);
        assert!(!name.contains("credential-hmac"));
    }

    #[cfg(unix)]
    #[test]
    fn catalog_salt_is_created_owner_only_and_rejects_insecure_existing_mode() {
        use std::os::unix::fs::PermissionsExt as _;

        let directory = tempfile::tempdir().unwrap();
        let salt = load_or_create_catalog_salt(directory.path()).unwrap();
        assert_eq!(salt.len(), 32);
        let path = directory
            .path()
            .join("cache/model-catalog/install-salt.bin");
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0);

        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(
            load_or_create_catalog_salt(directory.path()),
            Err(CatalogStoreError::Identity { .. })
        ));
    }

    #[test]
    fn cache_round_trip_reports_staleness() {
        let directory = tempfile::tempdir().unwrap();
        let store = CatalogStore::new(
            directory.path().join("cache"),
            directory.path().join("probe"),
        );
        let now = Utc::now();
        let document = CatalogCacheDocument {
            schema_version: CATALOG_CACHE_SCHEMA_VERSION,
            identity: identity(),
            fetched_at: now,
            last_refresh_attempt_at: Some(now),
            expires_at: now,
            status: DiscoveryStatus::Success,
            complete_listing: true,
            etag: None,
            last_modified: None,
            payload_hash: Some("a".repeat(64)),
            retry_after_at: None,
            consecutive_failures: 0,
            routes: BTreeMap::new(),
            diagnostics: Vec::new(),
            conflicting_observations: Vec::new(),
        };
        store.save_cache(&document).unwrap();
        let (_, stale) = store.load_cache(&identity(), now).unwrap().unwrap();
        assert!(stale);
    }

    #[test]
    fn corrupt_exact_cache_is_quarantined_without_blocking_runtime_evidence() {
        let directory = tempfile::tempdir().unwrap();
        let cache = directory.path().join("cache");
        let store = CatalogStore::new(cache.clone(), directory.path().join("probe"));
        let (registry, route) = simulator_route();
        let route_identity = route_identity(&route);
        let path = cache.join(cache_file_name(&route_identity));
        let now = Utc::now();
        std::fs::create_dir_all(&cache).unwrap();

        for bytes in [b"{".to_vec(), vec![b'x'; MAX_CACHE_ENTRY_BYTES + 1], {
            let mut invalid = valid_cache_document(route_identity.clone(), now);
            invalid.complete_listing = false;
            serde_json::to_vec(&invalid).unwrap()
        }] {
            std::fs::write(&path, bytes).unwrap();
            let evidence = store
                .availability_snapshot_for_routes(&registry, [route.clone()], None, now)
                .unwrap();
            assert_ne!(
                evidence.availability.get(&route.key),
                Some(&Availability::Listed)
            );
            assert!(!path.exists());
        }
    }

    #[test]
    fn exact_cache_read_does_not_wait_for_or_write_lru_index() {
        let directory = tempfile::tempdir().unwrap();
        let cache = directory.path().join("cache");
        let store = CatalogStore::new(cache.clone(), directory.path().join("probe"));
        let now = Utc::now();
        store
            .save_cache(&valid_cache_document(identity(), now))
            .unwrap();
        let index_lock_path = cache.join("index.lock");
        let index_lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&index_lock_path)
            .unwrap();
        index_lock.lock_exclusive().unwrap();
        assert!(store.load_cache(&identity(), now).unwrap().is_some());
        FileExt::unlock(&index_lock).unwrap();
    }

    #[test]
    fn oversized_index_is_bounded_and_quarantined() {
        let directory = tempfile::tempdir().unwrap();
        let cache = directory.path().join("cache");
        let store = CatalogStore::new(cache.clone(), directory.path().join("probe"));
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(
            cache.join("index.json"),
            vec![b'x'; MAX_CACHE_INDEX_BYTES as usize + 1],
        )
        .unwrap();
        assert!(store.cached_documents().unwrap().is_empty());
        assert!(cache.join("quarantine/index.bad").exists());
    }

    #[test]
    fn poisoned_index_cannot_move_or_delete_files_outside_cache_root() {
        let directory = tempfile::tempdir().unwrap();
        let cache = directory.path().join("cache");
        let store = CatalogStore::new(cache.clone(), directory.path().join("probe"));
        std::fs::create_dir_all(&cache).unwrap();
        let victim = directory.path().join("victim.json");
        std::fs::write(&victim, b"preserve me").unwrap();
        let poisoned = serde_json::json!({
            "entries": {
                "../victim.json": {
                    "accessed_at": Utc::now(),
                    "size_bytes": MAX_CACHE_BYTES + 1
                }
            }
        });
        std::fs::write(
            cache.join("index.json"),
            serde_json::to_vec(&poisoned).unwrap(),
        )
        .unwrap();
        assert!(store.cached_documents().unwrap().is_empty());
        assert_eq!(std::fs::read(&victim).unwrap(), b"preserve me");

        std::fs::write(
            cache.join("index.json"),
            serde_json::to_vec(&poisoned).unwrap(),
        )
        .unwrap();
        store.prune_cache().unwrap();
        assert_eq!(std::fs::read(&victim).unwrap(), b"preserve me");
        assert!(cache.join("quarantine/index.bad").exists());
    }

    #[test]
    fn cache_semantics_reject_incomplete_success_and_oversized_model_sets() {
        let directory = tempfile::tempdir().unwrap();
        let store = CatalogStore::new(
            directory.path().join("cache"),
            directory.path().join("probe"),
        );
        let now = Utc::now();
        let mut document = CatalogCacheDocument {
            schema_version: CATALOG_CACHE_SCHEMA_VERSION,
            identity: identity(),
            fetched_at: now,
            last_refresh_attempt_at: Some(now),
            expires_at: now + chrono::Duration::hours(1),
            status: DiscoveryStatus::Success,
            complete_listing: false,
            etag: None,
            last_modified: None,
            payload_hash: Some("a".repeat(64)),
            retry_after_at: None,
            consecutive_failures: 0,
            routes: BTreeMap::new(),
            diagnostics: Vec::new(),
            conflicting_observations: Vec::new(),
        };
        assert!(store.save_cache(&document).is_err());
        document.complete_listing = true;
        document.payload_hash = None;
        assert!(store.save_cache(&document).is_err());
        document.payload_hash = Some("a".repeat(64));
        document.expires_at = now + chrono::Duration::hours(MODEL_CATALOG_TTL_HOURS + 1);
        assert!(store.save_cache(&document).is_err());
        document.expires_at = now + chrono::Duration::hours(1);
        document.conflicting_observations = vec![CatalogConflictObservation {
            model_id: "model-a".into(),
            field: "unknown-field".into(),
            change_kind: CatalogConflictKind::Changed,
            previous_value: "1".into(),
            observed_value: "2".into(),
            previous_payload_hash: "not-a-hash".into(),
            payload_hash: "b".repeat(64),
            previous_observed_at: now,
            observed_at: now,
        }];
        assert!(store.save_cache(&document).is_err());
        document.conflicting_observations.clear();
        document.routes = (0..=MAX_CATALOG_MODELS)
            .map(|index| {
                let id = format!("model-{index}");
                (
                    id.clone(),
                    DiscoveredModel {
                        model_id: id,
                        ..Default::default()
                    },
                )
            })
            .collect();
        assert!(store.save_cache(&document).is_err());
    }

    #[test]
    fn bulk_cache_scan_quarantines_wrong_schema_and_filename_identity() {
        let directory = tempfile::tempdir().unwrap();
        let cache = directory.path().join("cache");
        let store = CatalogStore::new(cache.clone(), directory.path().join("probe"));
        std::fs::create_dir_all(&cache).unwrap();
        let now = Utc::now();
        let mut document = CatalogCacheDocument {
            schema_version: CATALOG_CACHE_SCHEMA_VERSION + 1,
            identity: identity(),
            fetched_at: now,
            last_refresh_attempt_at: Some(now),
            expires_at: now,
            status: DiscoveryStatus::Success,
            complete_listing: true,
            etag: None,
            last_modified: None,
            payload_hash: Some("a".repeat(64)),
            retry_after_at: None,
            consecutive_failures: 0,
            routes: BTreeMap::new(),
            diagnostics: Vec::new(),
            conflicting_observations: Vec::new(),
        };
        std::fs::write(
            cache.join(cache_file_name(&document.identity)),
            serde_json::to_vec(&document).unwrap(),
        )
        .unwrap();
        let indexed_name = cache_file_name(&document.identity);
        std::fs::write(
            cache.join("index.json"),
            serde_json::to_vec(&CatalogCacheIndex {
                entries: BTreeMap::from([
                    (
                        indexed_name,
                        CatalogCacheIndexEntry {
                            accessed_at: now,
                            size_bytes: 1,
                        },
                    ),
                    (
                        "wrong-name.json".into(),
                        CatalogCacheIndexEntry {
                            accessed_at: now,
                            size_bytes: 1,
                        },
                    ),
                ]),
            })
            .unwrap(),
        )
        .unwrap();
        assert!(store.cached_documents().unwrap().is_empty());

        document.schema_version = CATALOG_CACHE_SCHEMA_VERSION;
        std::fs::write(
            cache.join("wrong-name.json"),
            serde_json::to_vec(&document).unwrap(),
        )
        .unwrap();
        assert!(store.cached_documents().unwrap().is_empty());
        assert!(
            std::fs::read_dir(&cache)
                .unwrap()
                .filter_map(Result::ok)
                .any(|entry| entry.path().to_string_lossy().contains("quarantine"))
        );
    }

    #[test]
    fn bulk_cache_listing_enforces_index_entry_cap() {
        let directory = tempfile::tempdir().unwrap();
        let cache = directory.path().join("cache");
        let store = CatalogStore::new(cache.clone(), directory.path().join("probe"));
        std::fs::create_dir_all(&cache).unwrap();
        let now = Utc::now();
        let mut index_entries = BTreeMap::new();
        for index in 0..=MAX_CACHE_ENTRIES {
            let mut route_identity = identity();
            route_identity.provider_id = format!("provider-{index:03}");
            let document = CatalogCacheDocument {
                schema_version: CATALOG_CACHE_SCHEMA_VERSION,
                identity: route_identity.clone(),
                fetched_at: now,
                last_refresh_attempt_at: Some(now),
                expires_at: now,
                status: DiscoveryStatus::Success,
                complete_listing: true,
                etag: None,
                last_modified: None,
                payload_hash: Some("a".repeat(64)),
                retry_after_at: None,
                consecutive_failures: 0,
                routes: BTreeMap::new(),
                diagnostics: Vec::new(),
                conflicting_observations: Vec::new(),
            };
            std::fs::write(
                cache.join(cache_file_name(&route_identity)),
                serde_json::to_vec(&document).unwrap(),
            )
            .unwrap();
            index_entries.insert(
                cache_file_name(&route_identity),
                CatalogCacheIndexEntry {
                    accessed_at: now,
                    size_bytes: 1,
                },
            );
        }
        std::fs::write(
            cache.join("index.json"),
            serde_json::to_vec(&CatalogCacheIndex {
                entries: index_entries,
            })
            .unwrap(),
        )
        .unwrap();
        assert_eq!(store.cached_documents().unwrap().len(), MAX_CACHE_ENTRIES);
    }

    #[test]
    fn probe_raw_artifact_exists_before_verdict_and_is_immutable() {
        let directory = tempfile::tempdir().unwrap();
        let store = CatalogStore::new(
            directory.path().join("cache"),
            directory.path().join("probe"),
        );
        let route = ModelRouteKey {
            provider_id: "provider".into(),
            endpoint_id: "chat".into(),
            model_id: "model".into(),
        };
        let input = ProbeArtifactInput {
            attempt_id: "attempt-1",
            route: &route,
            catalog_identity: &identity(),
            configuration_epoch: 3,
            started_at: Utc::now(),
            request_bytes: b"request",
            inference_adapter_version: "openai-chat-v1@1",
            discovery_adapter_version: "openai-models-v1@1",
            raw_response: b"paid response",
            provider_request_id: Some("req-1"),
        };
        let pending = store.persist_probe_raw(input).unwrap();
        assert_eq!(
            std::fs::read(&pending.raw_response_path).unwrap(),
            b"paid response"
        );
        let receipt = store
            .finish_probe(
                pending,
                Utc::now(),
                ProbeOutcome::Rejected,
                Some("length".into()),
                ProbeTerminalMetadata {
                    http_status: Some(200),
                    terminal_event: Some("message_stop".into()),
                    ..Default::default()
                },
                Some("partial".into()),
            )
            .unwrap();
        assert_eq!(receipt.outcome, ProbeOutcome::Rejected);
        assert!(matches!(
            store.persist_probe_raw(ProbeArtifactInput {
                attempt_id: "attempt-1",
                route: &route,
                catalog_identity: &identity(),
                configuration_epoch: 3,
                started_at: Utc::now(),
                request_bytes: b"request",
                inference_adapter_version: "openai-chat-v1@1",
                discovery_adapter_version: "openai-models-v1@1",
                raw_response: b"replacement",
                provider_request_id: None,
            }),
            Err(CatalogStoreError::ProbeExists(_))
        ));
    }

    #[test]
    fn probe_evidence_bootstraps_route_without_discovery_and_ignores_transient_failure() {
        let directory = tempfile::tempdir().unwrap();
        let store = CatalogStore::new(
            directory.path().join("cache"),
            directory.path().join("probe"),
        );
        let (registry, route) = simulator_route();
        assert!(route.discovery_base_url.is_none());
        let identity = route_identity(&route);
        let now = Utc::now();
        record_test_probe(
            &store,
            probe_receipt(
                &route,
                identity.clone(),
                "passed",
                now - chrono::Duration::minutes(2),
                ProbeOutcome::Passed,
            ),
        );
        record_test_probe(
            &store,
            probe_receipt(
                &route,
                identity,
                "transient",
                now - chrono::Duration::minutes(1),
                ProbeOutcome::TransportFailed,
            ),
        );

        let runtime = store
            .availability_snapshot_for_routes(&registry, [route.clone()], None, now)
            .unwrap();
        assert_eq!(
            runtime.availability.get(&route.key),
            Some(&Availability::Listed)
        );
    }

    #[test]
    fn forged_or_future_probe_summary_cannot_grant_eligibility() {
        let directory = tempfile::tempdir().unwrap();
        let store = CatalogStore::new(
            directory.path().join("cache"),
            directory.path().join("probe"),
        );
        let (registry, route) = simulator_route();
        let now = Utc::now();
        let identity = route_identity(&route);
        record_test_probe(
            &store,
            probe_receipt(
                &route,
                identity.clone(),
                "future",
                now,
                ProbeOutcome::Passed,
            ),
        );
        let history_path = store.probe_dir.join("observations.json");
        let mut history: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&history_path).unwrap()).unwrap();
        history["routes"][0]["observations"][0]["observed_at"] =
            serde_json::json!((now + chrono::Duration::days(2)).to_rfc3339());
        std::fs::write(&history_path, serde_json::to_vec(&history).unwrap()).unwrap();
        let evidence = store
            .availability_snapshot_for_routes(&registry, [route.clone()], None, now)
            .unwrap();
        assert_ne!(
            evidence.availability.get(&route.key),
            Some(&Availability::Listed)
        );
        assert!(store.probe_dir.join("observations.bad").exists());

        record_test_probe(
            &store,
            probe_receipt(
                &route,
                identity.clone(),
                "forged",
                now,
                ProbeOutcome::Passed,
            ),
        );
        let mut history: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&history_path).unwrap()).unwrap();
        history["routes"][0]["observations"][0]["receipt_hash"] = serde_json::json!("f".repeat(64));
        std::fs::write(&history_path, serde_json::to_vec(&history).unwrap()).unwrap();
        let evidence = store
            .availability_snapshot_for_routes(&registry, [route.clone()], None, now)
            .unwrap();
        assert_ne!(
            evidence.availability.get(&route.key),
            Some(&Availability::Listed)
        );

        record_test_probe(
            &store,
            probe_receipt(
                &route,
                identity,
                "oversized",
                now + chrono::Duration::seconds(1),
                ProbeOutcome::Passed,
            ),
        );
        std::fs::write(
            store.probe_dir.join("attempts/oversized/receipt.json"),
            vec![b'x'; MAX_PROBE_RECEIPT_BYTES as usize + 1],
        )
        .unwrap();
        let evidence = store
            .availability_snapshot_for_routes(&registry, [route.clone()], None, now)
            .unwrap();
        assert_ne!(
            evidence.availability.get(&route.key),
            Some(&Availability::Listed)
        );
    }

    #[test]
    fn committed_probe_evidence_remains_available_while_writer_lock_is_held() {
        let directory = tempfile::tempdir().unwrap();
        let store = CatalogStore::new(
            directory.path().join("cache"),
            directory.path().join("probe"),
        );
        let (registry, route) = simulator_route();
        let now = Utc::now();
        record_test_probe(
            &store,
            probe_receipt(
                &route,
                route_identity(&route),
                "committed",
                now,
                ProbeOutcome::Passed,
            ),
        );
        let writer_lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(store.probe_dir.join("observations.lock"))
            .unwrap();
        writer_lock.lock_exclusive().unwrap();
        let evidence = store
            .availability_snapshot_for_routes(&registry, [route.clone()], None, now)
            .unwrap();
        assert_eq!(
            evidence.availability.get(&route.key),
            Some(&Availability::Listed)
        );
        FileExt::unlock(&writer_lock).unwrap();
    }

    #[test]
    fn irrelevant_probe_history_does_not_open_receipts_before_active_filter() {
        let directory = tempfile::tempdir().unwrap();
        let store = CatalogStore::new(
            directory.path().join("cache"),
            directory.path().join("probe"),
        );
        let (registry, route) = simulator_route();
        let now = Utc::now();
        let active_identity = route_identity(&route);
        record_test_probe(
            &store,
            probe_receipt(
                &route,
                active_identity.clone(),
                "active",
                now,
                ProbeOutcome::Passed,
            ),
        );
        let history_path = store.probe_dir.join("observations.json");
        let mut history: ProbeHistoryFile =
            serde_json::from_slice(&std::fs::read(&history_path).unwrap()).unwrap();
        history
            .routes
            .extend((0..100).map(|index| ProbeRouteHistory {
                key: ModelRouteKey {
                    provider_id: route.key.provider_id.clone(),
                    endpoint_id: route.key.endpoint_id.clone(),
                    model_id: format!("irrelevant-{index}"),
                },
                catalog_identity: active_identity.clone(),
                observations: vec![CatalogObservation {
                    kind: CatalogObservationKind::ProbePassed,
                    observed_at: now,
                    receipt_id: Some(format!("missing-{index}")),
                    receipt_hash: Some("a".repeat(64)),
                }],
                omitted_observation_count: 0,
            }));
        std::fs::write(&history_path, serde_json::to_vec(&history).unwrap()).unwrap();
        let evidence = store
            .availability_snapshot_for_routes(&registry, [route.clone()], None, now)
            .unwrap();
        assert_eq!(
            evidence.availability.get(&route.key),
            Some(&Availability::Listed)
        );
    }

    #[test]
    fn probe_history_writer_preserves_prior_at_route_and_byte_boundaries() {
        let directory = tempfile::tempdir().unwrap();
        let store = CatalogStore::new(
            directory.path().join("cache"),
            directory.path().join("probe"),
        );
        std::fs::create_dir_all(&store.probe_dir).unwrap();
        let (_, route) = simulator_route();
        let now = Utc::now();
        let receipt = probe_receipt(
            &route,
            route_identity(&route),
            "route-overflow",
            now,
            ProbeOutcome::Passed,
        );
        write_test_receipt(&store, &receipt);
        let history_path = store.probe_dir.join("observations.json");
        let full = ProbeHistoryFile {
            schema_version: PROBE_HISTORY_SCHEMA_VERSION,
            routes: (0..MAX_PROBE_HISTORY_ROUTES)
                .map(|index| ProbeRouteHistory {
                    key: ModelRouteKey {
                        provider_id: "simulator".into(),
                        endpoint_id: "builtin".into(),
                        model_id: format!("existing-{index}"),
                    },
                    catalog_identity: route_identity(&route),
                    observations: Vec::new(),
                    omitted_observation_count: 0,
                })
                .collect(),
        };
        let prior = serde_json::to_vec(&full).unwrap();
        assert!(prior.len() < MAX_CACHE_ENTRY_BYTES);
        std::fs::write(&history_path, &prior).unwrap();
        assert!(store.record_probe_observation(&receipt).is_err());
        assert_eq!(std::fs::read(&history_path).unwrap(), prior);

        let size_receipt = probe_receipt(
            &route,
            route_identity(&route),
            "byte-overflow",
            now,
            ProbeOutcome::Passed,
        );
        write_test_receipt(&store, &size_receipt);
        let mut huge_identity = route_identity(&route);
        let mut near_limit = ProbeHistoryFile {
            schema_version: PROBE_HISTORY_SCHEMA_VERSION,
            routes: vec![ProbeRouteHistory {
                key: ModelRouteKey {
                    provider_id: "simulator".into(),
                    endpoint_id: "builtin".into(),
                    model_id: "padding".into(),
                },
                catalog_identity: huge_identity.clone(),
                observations: Vec::new(),
                omitted_observation_count: 0,
            }],
        };
        let base_len = serde_json::to_vec_pretty(&near_limit).unwrap().len();
        huge_identity.discovery_base_url =
            "x".repeat(MAX_CACHE_ENTRY_BYTES.saturating_sub(base_len + 512));
        near_limit.routes[0].catalog_identity = huge_identity;
        let prior = serde_json::to_vec_pretty(&near_limit).unwrap();
        assert!(prior.len() < MAX_CACHE_ENTRY_BYTES);
        std::fs::write(&history_path, &prior).unwrap();
        assert!(store.record_probe_observation(&size_receipt).is_err());
        assert_eq!(std::fs::read(&history_path).unwrap(), prior);
    }

    #[test]
    fn bounded_reader_stays_on_opened_inode_across_path_replacement() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("receipt.json");
        let replacement = directory.path().join("replacement.tmp");
        std::fs::write(&path, b"committed").unwrap();
        let opened = File::open(&path).unwrap();
        std::fs::write(&replacement, vec![b'x'; 1025]).unwrap();
        super::super::loader::atomic_replace(&replacement, &path).unwrap();
        assert_eq!(
            read_open_file_bounded(opened, &path, 1024).unwrap(),
            b"committed"
        );
        assert!(read_file_bounded(&path, 1024).is_err());
    }

    #[test]
    fn probe_evidence_from_another_route_identity_is_ignored() {
        let directory = tempfile::tempdir().unwrap();
        let store = CatalogStore::new(
            directory.path().join("cache"),
            directory.path().join("probe"),
        );
        let (registry, route) = simulator_route();
        let mut other_identity = route_identity(&route);
        other_identity.inference_base_url.push_str("/other-gateway");
        record_test_probe(
            &store,
            probe_receipt(
                &route,
                other_identity,
                "other-origin",
                Utc::now(),
                ProbeOutcome::Passed,
            ),
        );

        let runtime = store
            .availability_snapshot_for_routes(&registry, [route.clone()], None, Utc::now())
            .unwrap();
        assert_ne!(
            runtime.availability.get(&route.key),
            Some(&Availability::Listed)
        );
    }

    #[test]
    fn newer_complete_listing_wins_over_older_probe_not_listed() {
        let directory = tempfile::tempdir().unwrap();
        let store = CatalogStore::new(
            directory.path().join("cache"),
            directory.path().join("probe"),
        );
        let (registry, route) = simulator_route();
        let identity = route_identity(&route);
        let now = Utc::now();
        record_test_probe(
            &store,
            probe_receipt(
                &route,
                identity.clone(),
                "old-not-listed",
                now - chrono::Duration::minutes(2),
                ProbeOutcome::NotListed,
            ),
        );
        let mut routes = BTreeMap::new();
        routes.insert(
            route.key.model_id.clone(),
            DiscoveredModel {
                model_id: route.key.model_id.clone(),
                ..Default::default()
            },
        );
        store
            .save_cache(&CatalogCacheDocument {
                schema_version: CATALOG_CACHE_SCHEMA_VERSION,
                identity,
                fetched_at: now - chrono::Duration::minutes(1),
                last_refresh_attempt_at: Some(now - chrono::Duration::minutes(1)),
                expires_at: now + chrono::Duration::hours(1),
                status: DiscoveryStatus::Success,
                complete_listing: true,
                etag: None,
                last_modified: None,
                payload_hash: Some("a".repeat(64)),
                retry_after_at: None,
                consecutive_failures: 0,
                routes,
                diagnostics: Vec::new(),
                conflicting_observations: Vec::new(),
            })
            .unwrap();

        let runtime = store
            .availability_snapshot_for_routes(&registry, [route.clone()], None, now)
            .unwrap();
        assert_eq!(
            runtime.availability.get(&route.key),
            Some(&Availability::Listed)
        );
    }

    #[test]
    fn newer_nonstream_probe_cannot_override_streaming_incompatibility() {
        let directory = tempfile::tempdir().unwrap();
        let store = CatalogStore::new(
            directory.path().join("cache"),
            directory.path().join("probe"),
        );
        let (registry, route) = simulator_route();
        let identity = route_identity(&route);
        let now = Utc::now();
        store
            .save_cache(&CatalogCacheDocument {
                schema_version: CATALOG_CACHE_SCHEMA_VERSION,
                identity: identity.clone(),
                fetched_at: now - chrono::Duration::minutes(2),
                last_refresh_attempt_at: Some(now - chrono::Duration::minutes(2)),
                expires_at: now + chrono::Duration::hours(1),
                status: DiscoveryStatus::Success,
                complete_listing: true,
                etag: None,
                last_modified: None,
                payload_hash: Some("a".repeat(64)),
                retry_after_at: None,
                consecutive_failures: 0,
                routes: BTreeMap::from([(
                    route.key.model_id.clone(),
                    DiscoveredModel {
                        model_id: route.key.model_id.clone(),
                        streaming: Some(false),
                        ..Default::default()
                    },
                )]),
                diagnostics: Vec::new(),
                conflicting_observations: Vec::new(),
            })
            .unwrap();
        record_test_probe(
            &store,
            probe_receipt(
                &route,
                identity,
                "newer-nonstream-probe",
                now - chrono::Duration::minutes(1),
                ProbeOutcome::Passed,
            ),
        );
        let runtime = store
            .availability_snapshot_for_routes(&registry, [route.clone()], None, now)
            .unwrap();
        assert_eq!(
            runtime.availability.get(&route.key),
            Some(&Availability::Incompatible)
        );
    }

    #[test]
    fn remote_capabilities_only_narrow_curated_model_contract() {
        let (mut registry, route) = simulator_route();
        let model = registry
            .get(&route.key.provider_id)
            .unwrap()
            .curated_models
            .get(&route.key.endpoint_id)
            .unwrap()
            .get(&route.key.model_id)
            .unwrap()
            .clone();
        let lower_max = model.generation.max_output_tokens.saturating_sub(1).max(1);
        let mut routes = BTreeMap::new();
        routes.insert(
            route.key.model_id.clone(),
            DiscoveredModel {
                model_id: route.key.model_id.clone(),
                max_output_tokens: Some(lower_max.into()),
                supports_temperature: Some(false),
                supports_frequency_penalty: Some(false),
                output_contracts: Some(OutputContractCapabilities {
                    prompt_validated_json: false,
                    native_json_object: false,
                    native_json_schema: false,
                }),
                reasoning_effort_levels: Some(BTreeSet::new()),
                supports_reasoning_budget: Some(false),
                supports_reasoning_off: Some(false),
                service_tiers: Some(BTreeSet::new()),
                ..Default::default()
            },
        );
        constrain_registry_from_document(
            &mut registry,
            &CatalogCacheDocument {
                schema_version: CATALOG_CACHE_SCHEMA_VERSION,
                identity: route_identity(&route),
                fetched_at: Utc::now(),
                last_refresh_attempt_at: Some(Utc::now()),
                expires_at: Utc::now() + chrono::Duration::hours(1),
                status: DiscoveryStatus::Success,
                complete_listing: true,
                etag: None,
                last_modified: None,
                payload_hash: Some("a".repeat(64)),
                retry_after_at: None,
                consecutive_failures: 0,
                routes,
                diagnostics: Vec::new(),
                conflicting_observations: Vec::new(),
            },
        );
        let constrained = registry
            .get(&route.key.provider_id)
            .unwrap()
            .curated_models
            .get(&route.key.endpoint_id)
            .unwrap()
            .get(&route.key.model_id)
            .unwrap();
        assert_eq!(constrained.generation.max_output_tokens, lower_max);
        assert!(constrained.generation.temperature.is_none());
        assert!(constrained.generation.frequency_penalty.is_none());
        assert!(constrained.reasoning.effort.is_none());
        assert!(constrained.reasoning.budget.is_none());
        assert!(constrained.reasoning.off_dialect.is_none());
        assert!(constrained.generation.service_tiers.is_empty());
        assert!(constrained.output_contracts.prompt_validated_json);
        assert!(!constrained.output_contracts.native_json_object);
        assert!(!constrained.output_contracts.native_json_schema);
    }

    #[test]
    fn stale_remote_metadata_does_not_narrow_curated_capabilities() {
        let directory = tempfile::tempdir().unwrap();
        let store = CatalogStore::new(
            directory.path().join("cache"),
            directory.path().join("probe"),
        );
        let (registry, route) = simulator_route();
        let original_max = registry[&route.key.provider_id].curated_models[&route.key.endpoint_id]
            [&route.key.model_id]
            .generation
            .max_output_tokens;
        let now = Utc::now();
        store
            .save_cache(&CatalogCacheDocument {
                schema_version: CATALOG_CACHE_SCHEMA_VERSION,
                identity: route_identity(&route),
                fetched_at: now - chrono::Duration::hours(2),
                last_refresh_attempt_at: Some(now - chrono::Duration::hours(2)),
                expires_at: now - chrono::Duration::hours(1),
                status: DiscoveryStatus::Success,
                complete_listing: true,
                etag: None,
                last_modified: None,
                payload_hash: Some("a".repeat(64)),
                retry_after_at: None,
                consecutive_failures: 0,
                routes: BTreeMap::from([(
                    route.key.model_id.clone(),
                    DiscoveredModel {
                        model_id: route.key.model_id.clone(),
                        max_output_tokens: Some(1),
                        streaming: Some(false),
                        input_modalities: Some(BTreeSet::from(["image".into()])),
                        output_modalities: Some(BTreeSet::from(["audio".into()])),
                        ..Default::default()
                    },
                )]),
                diagnostics: Vec::new(),
                conflicting_observations: Vec::new(),
            })
            .unwrap();
        let evidence = store
            .availability_snapshot_for_routes(&registry, [route.clone()], None, now)
            .unwrap();
        assert_eq!(
            evidence.constrained_registry[&route.key.provider_id].curated_models
                [&route.key.endpoint_id][&route.key.model_id]
                .generation
                .max_output_tokens,
            original_max
        );
        assert_ne!(
            evidence.availability[&route.key],
            Availability::Incompatible,
            "expired transport observations must not permanently brick a route"
        );
    }
}
