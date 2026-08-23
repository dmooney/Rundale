//! Atomic v2 inference configuration + transport publication.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex, RwLock};

use parish_config::{
    Availability, ModelRouteKey, ProjectConfigV2, ResolvedInferenceSnapshot, RoutingOverrideSet,
    UserConfigV2,
};
use parish_inference::InferenceClients;
use parish_types::ParishError;

#[derive(Clone)]
pub struct InferenceRuntimeSnapshotV2 {
    pub config: Arc<ResolvedInferenceSnapshot>,
    pub clients: InferenceClients,
    pub engine_inference: parish_config::InferenceConfig,
    pub availability: Arc<BTreeMap<ModelRouteKey, Availability>>,
}

pub fn load_inference_runtime_v2(
    epoch: u64,
    project_path: &Path,
    user_path: &Path,
    overrides: &RoutingOverrideSet,
    availability: &BTreeMap<ModelRouteKey, Availability>,
    keychain_get: impl FnMut(&str) -> Option<String>,
) -> Result<(ProjectConfigV2, UserConfigV2, InferenceRuntimeSnapshotV2), ParishError> {
    let project = parish_config::load_project_config_v2(project_path)
        .map_err(|error| ParishError::Config(error.to_string()))?;
    let user = parish_config::load_user_config_v2(user_path)
        .map_err(|error| ParishError::Config(error.to_string()))?;
    let runtime = build_inference_runtime_v2(
        epoch,
        &project,
        &user,
        overrides,
        availability,
        keychain_get,
    )?;
    Ok((project, user, runtime))
}

/// Production loader that derives catalog eligibility from the exact active
/// account/origin identities. Authenticated caching is disabled for this run
/// if the install salt cannot be loaded; configuration still starts safely.
pub fn load_inference_runtime_v2_with_catalog(
    epoch: u64,
    project_path: &Path,
    user_path: &Path,
    overrides: &RoutingOverrideSet,
    store: &parish_config::CatalogStore,
    user_data_dir: &Path,
    keychain_get: impl FnMut(&str) -> Option<String>,
) -> Result<(ProjectConfigV2, UserConfigV2, InferenceRuntimeSnapshotV2), ParishError> {
    let project = parish_config::load_project_config_v2(project_path)
        .map_err(|error| ParishError::Config(error.to_string()))?;
    let user = parish_config::load_user_config_v2(user_path)
        .map_err(|error| ParishError::Config(error.to_string()))?;
    let runtime = build_inference_runtime_v2_with_catalog(
        epoch,
        &project,
        &user,
        overrides,
        store,
        user_data_dir,
        keychain_get,
    )?;
    Ok((project, user, runtime))
}

pub fn build_inference_runtime_v2_with_catalog(
    epoch: u64,
    project: &ProjectConfigV2,
    user: &UserConfigV2,
    overrides: &RoutingOverrideSet,
    store: &parish_config::CatalogStore,
    user_data_dir: &Path,
    mut keychain_get: impl FnMut(&str) -> Option<String>,
) -> Result<InferenceRuntimeSnapshotV2, ParishError> {
    let compiled = parish_config::compiled_inference_layer_v2();
    let registry = parish_config::compiled_provider_registry_v2();
    let merged =
        parish_config::merge_inference_layers(&compiled, &project.inference, &user.inference);
    let credentials = parish_config::resolve_credential_slots(
        &registry,
        &merged,
        &user.credential_bindings,
        &mut keychain_get,
    );
    let preliminary = parish_config::resolve_inference_topology_snapshot(
        epoch,
        &registry,
        &merged,
        overrides,
        &credentials,
    )
    .map_err(|error| ParishError::Config(error.to_string()))?;
    let effective_registry = parish_config::effective_provider_registry(&registry, &merged);
    let salt = match parish_config::load_or_create_catalog_salt(user_data_dir) {
        Ok(salt) => Some(salt),
        Err(error) => {
            tracing::warn!(%error, "authenticated model-catalog disk cache disabled for this run");
            None
        }
    };
    let evidence = store
        .availability_snapshot_for_routes(
            &effective_registry,
            preliminary.category_routes.values().cloned(),
            salt.as_deref(),
            chrono::Utc::now(),
        )
        .map_err(|error| ParishError::Config(error.to_string()))?;
    build_runtime_from_resolved_authorities(
        epoch,
        project,
        overrides,
        &merged,
        &evidence.constrained_registry,
        &evidence.availability,
        &credentials,
    )
}

pub fn build_inference_runtime_v2(
    epoch: u64,
    project: &ProjectConfigV2,
    user: &UserConfigV2,
    overrides: &RoutingOverrideSet,
    availability: &BTreeMap<ModelRouteKey, Availability>,
    keychain_get: impl FnMut(&str) -> Option<String>,
) -> Result<InferenceRuntimeSnapshotV2, ParishError> {
    let compiled = parish_config::compiled_inference_layer_v2();
    let registry = parish_config::compiled_provider_registry_v2();
    parish_config::validate_provider_registry(&registry)
        .map_err(|error| ParishError::Config(error.to_string()))?;
    let merged =
        parish_config::merge_inference_layers(&compiled, &project.inference, &user.inference);
    let credentials = parish_config::resolve_credential_slots(
        &registry,
        &merged,
        &user.credential_bindings,
        keychain_get,
    );
    let effective_registry = parish_config::effective_provider_registry(&registry, &merged);
    build_runtime_from_resolved_authorities(
        epoch,
        project,
        overrides,
        &merged,
        &effective_registry,
        availability,
        &credentials,
    )
}

fn build_runtime_from_resolved_authorities(
    epoch: u64,
    project: &ProjectConfigV2,
    overrides: &RoutingOverrideSet,
    merged: &parish_config::MergedInferenceLayer,
    effective_registry: &BTreeMap<String, parish_config::ProviderDefinition>,
    availability: &BTreeMap<ModelRouteKey, Availability>,
    credentials: &BTreeMap<String, parish_config::SecretString>,
) -> Result<InferenceRuntimeSnapshotV2, ParishError> {
    let config = parish_config::resolve_inference_snapshot_from_effective_registry(
        epoch,
        effective_registry,
        merged,
        overrides,
        availability,
        credentials,
    )
    .map_err(|error| ParishError::Config(error.to_string()))?;
    let clients = parish_inference::build_inference_clients_v2(&config, &project.engine.inference)?;
    let runtime = InferenceRuntimeSnapshotV2 {
        config: Arc::new(config),
        clients,
        engine_inference: project.engine.inference.clone(),
        availability: Arc::new(availability.clone()),
    };
    Ok(runtime)
}

/// Refreshes stale/absent catalogs after an immutable runtime has already
/// been published. The task captures exact route/account identities and only
/// updates disk; it never mutates the live epoch. A later explicit reload may
/// consume the refreshed evidence.
pub fn spawn_catalog_refresh_v2(
    config: Arc<ResolvedInferenceSnapshot>,
    store: parish_config::CatalogStore,
    user_data_dir: std::path::PathBuf,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let salt = match parish_config::load_or_create_catalog_salt(&user_data_dir) {
            Ok(value) => Some(value),
            Err(error) => {
                tracing::warn!(%error, "authenticated model-catalog refresh disabled for this run");
                None
            }
        };
        let mut seen = std::collections::BTreeSet::new();
        for route in config.category_routes.values() {
            let Some(discovery_base_url) = route.discovery_base_url.clone() else {
                continue;
            };
            if route.discovery_adapter == parish_config::DiscoveryAdapter::None {
                continue;
            }
            let fingerprint = match (&route.credential, salt.as_deref()) {
                (None, _) => "anonymous".to_string(),
                (Some(_), None) => continue,
                (credential, Some(salt)) => {
                    parish_config::catalog_credential_fingerprint(salt, credential.as_ref())
                }
            };
            let identity = parish_config::CatalogCacheIdentity {
                provider_id: route.key.provider_id.clone(),
                endpoint_id: route.key.endpoint_id.clone(),
                discovery_base_url: discovery_base_url.clone(),
                inference_base_url: route.inference_base_url.clone(),
                inference_adapter_version: parish_config::inference_adapter_version(
                    route.inference_adapter,
                ),
                discovery_adapter_version: parish_config::discovery_adapter_version(
                    route.discovery_adapter,
                ),
                credential_fingerprint: fingerprint,
            };
            if !seen.insert(identity.clone()) {
                continue;
            }
            let Some(_guard) = (match store.lock_refresh(&identity) {
                Ok(guard) => guard,
                Err(error) => {
                    tracing::warn!(%error, provider = %route.key.provider_id, "catalog refresh lock failed");
                    continue;
                }
            }) else {
                continue;
            };
            // Re-read only after owning the per-identity lock. A peer may have
            // completed a refresh between task scheduling and lock acquisition.
            let now = chrono::Utc::now();
            let prior = match store.load_cache(&identity, now) {
                Ok(Some((prior, stale))) => {
                    if prior.status == parish_config::DiscoveryStatus::Unsupported {
                        continue;
                    }
                    if !stale && prior.status == parish_config::DiscoveryStatus::Success {
                        continue;
                    }
                    if prior.retry_after_at.is_some_and(|until| until > now) {
                        continue;
                    }
                    Some(prior)
                }
                Ok(None) => None,
                Err(error) => {
                    tracing::warn!(%error, provider = %route.key.provider_id, "catalog cache read failed before refresh");
                    None
                }
            };
            let endpoint = parish_config::EndpointDefinition {
                inference_base_url: route.inference_base_url.clone(),
                discovery_base_url: Some(discovery_base_url),
                inference_adapter: route.inference_adapter,
                discovery_adapter: route.discovery_adapter,
                backend_kind: route.backend_kind,
                management_adapter: route.management_adapter,
                auth_adapter: route.auth_adapter,
                credential_slot: None,
                default_reasoning_dialect: route.reasoning_dialect,
                default_openai_generation_wire: None,
            };
            let refreshed = parish_inference::fetch_catalog_endpoint(
                identity,
                &endpoint,
                route.credential.as_ref(),
                chrono::Duration::hours(parish_config::MODEL_CATALOG_TTL_HOURS),
                prior.as_ref(),
            )
            .await;
            if let Err(error) = store.save_cache(&refreshed) {
                tracing::warn!(%error, provider = %route.key.provider_id, "catalog refresh persistence failed");
            }
        }
    })
}

/// Serializes full candidate construction and atomically swaps config and all
/// clients as one epoch. Failed candidates leave the live epoch untouched.
pub struct InferenceRuntimeManagerV2 {
    publication: RwLock<Arc<InferenceRuntimeSnapshotV2>>,
    next_epoch: Mutex<u64>,
    reconfiguration: tokio::sync::Mutex<()>,
}

impl InferenceRuntimeManagerV2 {
    pub fn new(initial: InferenceRuntimeSnapshotV2) -> Self {
        let next = initial.config.configuration_epoch.saturating_add(1);
        Self {
            publication: RwLock::new(Arc::new(initial)),
            next_epoch: Mutex::new(next),
            reconfiguration: tokio::sync::Mutex::new(()),
        }
    }

    /// Holds the complete setup lifecycle—durable reads/writes, candidate
    /// construction, publication, and any rollback—in one serial order.
    pub async fn begin_reconfiguration(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.reconfiguration.lock().await
    }

    pub fn snapshot(&self) -> Arc<InferenceRuntimeSnapshotV2> {
        Arc::clone(
            &self
                .publication
                .read()
                .expect("runtime publication lock poisoned"),
        )
    }

    pub fn next_epoch(&self) -> u64 {
        *self.next_epoch.lock().expect("runtime epoch lock poisoned")
    }

    pub fn publish_candidate(
        &self,
        candidate: InferenceRuntimeSnapshotV2,
    ) -> Result<Arc<InferenceRuntimeSnapshotV2>, ParishError> {
        let mut next = self.next_epoch.lock().expect("runtime epoch lock poisoned");
        if candidate.config.configuration_epoch != *next {
            return Err(ParishError::Config(format!(
                "stale inference candidate epoch {}; expected {}",
                candidate.config.configuration_epoch, *next
            )));
        }
        let candidate = Arc::new(candidate);
        *self
            .publication
            .write()
            .expect("runtime publication lock poisoned") = Arc::clone(&candidate);
        *next = next.saturating_add(1);
        Ok(candidate)
    }

    pub fn reload(
        &self,
        project_path: &Path,
        user_path: &Path,
        overrides: &RoutingOverrideSet,
        availability: &BTreeMap<ModelRouteKey, Availability>,
        keychain_get: impl FnMut(&str) -> Option<String>,
    ) -> Result<Arc<InferenceRuntimeSnapshotV2>, ParishError> {
        let mut next = self.next_epoch.lock().expect("runtime epoch lock poisoned");
        let (_, _, candidate) = load_inference_runtime_v2(
            *next,
            project_path,
            user_path,
            overrides,
            availability,
            keychain_get,
        )?;
        let candidate = Arc::new(candidate);
        *self
            .publication
            .write()
            .expect("runtime publication lock poisoned") = Arc::clone(&candidate);
        *next = next.saturating_add(1);
        Ok(candidate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn clean_install_builds_keyless_simulator_runtime() {
        let runtime = build_inference_runtime_v2(
            1,
            &ProjectConfigV2::default(),
            &UserConfigV2::default(),
            &RoutingOverrideSet::default(),
            &BTreeMap::new(),
            |_| None,
        )
        .expect("onboarding must be reachable without a cloud credential");
        for route in runtime.config.category_routes.values() {
            assert_eq!(route.key.provider_id, "simulator");
            assert!(route.credential.is_none());
        }
    }

    #[tokio::test]
    async fn reconfiguration_gate_serializes_setup_lifecycles() {
        let runtime = build_inference_runtime_v2(
            1,
            &ProjectConfigV2::default(),
            &UserConfigV2::default(),
            &RoutingOverrideSet::default(),
            &BTreeMap::new(),
            |_| None,
        )
        .unwrap();
        let manager = Arc::new(InferenceRuntimeManagerV2::new(runtime));
        let first = manager.begin_reconfiguration().await;
        let contender = {
            let manager = Arc::clone(&manager);
            tokio::spawn(async move {
                let _guard = manager.begin_reconfiguration().await;
                true
            })
        };
        tokio::task::yield_now().await;
        assert!(!contender.is_finished());
        drop(first);
        assert!(contender.await.unwrap());
    }

    #[tokio::test]
    async fn post_publication_refresh_updates_disk_without_mutating_epoch() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{"id": "fresh-model"}]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let runtime = build_inference_runtime_v2(
            41,
            &ProjectConfigV2::default(),
            &UserConfigV2::default(),
            &RoutingOverrideSet::default(),
            &BTreeMap::new(),
            |_| None,
        )
        .unwrap();
        let mut config = (*runtime.config).clone();
        for route in config.category_routes.values_mut() {
            route.key.provider_id = "refresh-test".into();
            route.key.endpoint_id = "api".into();
            route.inference_base_url = server.uri();
            route.discovery_base_url = Some(server.uri());
            route.inference_adapter = parish_config::InferenceAdapter::OpenaiChatV1;
            route.discovery_adapter = parish_config::DiscoveryAdapter::OpenaiModelsV1;
            route.backend_kind = parish_config::BackendKind::Remote;
            route.management_adapter = parish_config::ManagementAdapter::None;
            route.auth_adapter = parish_config::AuthAdapter::None;
        }
        let published = Arc::new(config);
        let directory = tempfile::tempdir().unwrap();
        let store = parish_config::CatalogStore::for_user_data_dir(directory.path());
        let first = spawn_catalog_refresh_v2(
            Arc::clone(&published),
            store.clone(),
            directory.path().to_path_buf(),
        );
        let second = spawn_catalog_refresh_v2(
            Arc::clone(&published),
            store.clone(),
            directory.path().to_path_buf(),
        );
        let (first, second) = tokio::join!(first, second);
        first.unwrap();
        second.unwrap();

        assert_eq!(published.configuration_epoch, 41);
        let documents = store.cached_documents().unwrap();
        assert_eq!(documents.len(), 1);
        assert_eq!(documents[0].status, parish_config::DiscoveryStatus::Success);
        assert!(documents[0].routes.contains_key("fresh-model"));
    }

    #[test]
    fn concurrent_reload_publishes_unique_monotonic_complete_epochs() {
        let directory = tempfile::tempdir().unwrap();
        let project_path = directory.path().join("project.toml");
        let user_path = directory.path().join("user.toml");
        std::fs::write(
            &project_path,
            "schema_version = 2\n[inference]\nactive_loadout = 'test'\n\
             [inference.loadouts.test.default]\nprovider = 'simulator'\nmodel = 'simulator'\n\
             allow_unverified_model = true\n",
        )
        .unwrap();
        parish_config::save_user_config_v2(&user_path, &parish_config::UserConfigV2::default())
            .unwrap();
        let (_, _, initial) = load_inference_runtime_v2(
            1,
            &project_path,
            &user_path,
            &Default::default(),
            &Default::default(),
            |_| None,
        )
        .unwrap();
        let manager = Arc::new(InferenceRuntimeManagerV2::new(initial));
        let barrier = Arc::new(std::sync::Barrier::new(3));
        let mut handles = Vec::new();
        for _ in 0..2 {
            let manager = Arc::clone(&manager);
            let barrier = Arc::clone(&barrier);
            let project_path = project_path.clone();
            let user_path = user_path.clone();
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                manager
                    .reload(
                        &project_path,
                        &user_path,
                        &Default::default(),
                        &Default::default(),
                        |_| None,
                    )
                    .unwrap()
                    .config
                    .configuration_epoch
            }));
        }
        barrier.wait();
        let mut epochs: Vec<_> = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect();
        epochs.sort_unstable();
        assert_eq!(epochs, vec![2, 3]);
        let published = manager.snapshot();
        assert_eq!(published.config.configuration_epoch, 3);
        assert_eq!(published.clients.dialogue_client().1, "simulator");
    }

    #[test]
    fn exact_cached_listing_bootstraps_unqualified_custom_route() {
        let directory = tempfile::tempdir().unwrap();
        let cache = parish_config::CatalogStore::for_user_data_dir(directory.path());
        let inference_base_url = "https://gateway.example.test/api/v1".to_string();
        let discovery_base_url = "https://gateway.example.test/api/v1".to_string();
        let model_id = "private-model".to_string();

        let capability = parish_config::UserModelCapabilityOverride {
            reasoning: Some(parish_config::ReasoningCapabilities {
                mandatory: false,
                default_by_subrole: BTreeMap::new(),
                effort: None,
                budget: None,
                off_dialect: None,
                translations: Vec::new(),
            }),
            generation: Some(parish_config::GenerationCapabilities {
                min_output_tokens: 1,
                max_output_tokens: 32_768,
                temperature: Some(parish_config::NumericRange { min: 0.0, max: 2.0 }),
                frequency_penalty: Some(parish_config::NumericRange {
                    min: -2.0,
                    max: 2.0,
                }),
                service_tiers: BTreeMap::new(),
            }),
            output_contracts: Some(parish_config::OutputContractCapabilities {
                prompt_validated_json: true,
                native_json_object: false,
                native_json_schema: false,
            }),
            openai_generation_wire: Some(parish_config::OpenAiChatGenerationWire {
                output_limit_field: parish_config::OutputLimitField::MaxTokens,
                structured_output: BTreeSet::from([
                    parish_config::StructuredOutputMode::PromptValidatedJson,
                ]),
            }),
        };
        let mut user = UserConfigV2::default();
        user.inference.active_loadout = Some("cached-custom".into());
        user.inference.loadouts.insert(
            "cached-custom".into(),
            parish_config::LoadoutDefinition {
                default: parish_config::RoutePatch {
                    provider: Some("custom:boot".into()),
                    endpoint: Some("api".into()),
                    model: Some(model_id.clone()),
                    allow_unverified_model: Some(false),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        user.inference.providers.insert(
            "boot".into(),
            parish_config::CustomProviderDefinition {
                display_name: "Bootstrap test".into(),
                default_endpoint: Some("api".into()),
                endpoints: BTreeMap::from([(
                    "api".into(),
                    parish_config::CustomEndpointDefinition {
                        inference_base_url: inference_base_url.clone(),
                        discovery_base_url: Some(discovery_base_url.clone()),
                        inference_adapter: parish_config::InferenceAdapter::OpenaiChatV1,
                        discovery_adapter: parish_config::DiscoveryAdapter::OpenaiModelsV1,
                        auth_adapter: parish_config::AuthAdapter::None,
                        default_reasoning_dialect: parish_config::ReasoningDialect::None,
                        allow_insecure_http: false,
                        default_openai_generation_wire: Some(
                            parish_config::OpenAiChatGenerationWire {
                                output_limit_field: parish_config::OutputLimitField::MaxTokens,
                                structured_output: BTreeSet::from([
                                    parish_config::StructuredOutputMode::PromptValidatedJson,
                                ]),
                            },
                        ),
                    },
                )]),
                models: BTreeMap::from([(
                    "api".into(),
                    BTreeMap::from([(model_id.clone(), capability)]),
                )]),
            },
        );
        let now = chrono::Utc::now();
        let identity = parish_config::CatalogCacheIdentity {
            provider_id: "custom:boot".into(),
            endpoint_id: "api".into(),
            discovery_base_url,
            inference_base_url,
            inference_adapter_version: "openai-chat-v1@1".into(),
            discovery_adapter_version: "openai-models-v1@1".into(),
            credential_fingerprint: "anonymous".into(),
        };
        cache
            .save_cache(&parish_config::CatalogCacheDocument {
                schema_version: parish_config::CATALOG_CACHE_SCHEMA_VERSION,
                identity,
                fetched_at: now,
                last_refresh_attempt_at: Some(now),
                expires_at: now + chrono::Duration::hours(1),
                status: parish_config::DiscoveryStatus::Success,
                complete_listing: true,
                etag: None,
                last_modified: None,
                payload_hash: Some("a".repeat(64)),
                retry_after_at: None,
                consecutive_failures: 0,
                routes: BTreeMap::from([(
                    model_id.clone(),
                    parish_config::DiscoveredModel {
                        model_id: model_id.clone(),
                        ..Default::default()
                    },
                )]),
                diagnostics: Vec::new(),
                conflicting_observations: Vec::new(),
            })
            .unwrap();

        let runtime = build_inference_runtime_v2_with_catalog(
            1,
            &ProjectConfigV2::default(),
            &user,
            &RoutingOverrideSet::default(),
            &cache,
            directory.path(),
            |_| None,
        )
        .expect("the topology-only phase must reach the exact cached listing");
        assert_eq!(
            runtime.config.category_routes["dialogue"].availability,
            Availability::Listed
        );
        assert_eq!(
            runtime.config.category_routes["dialogue"].key.provider_id,
            "custom:boot"
        );
    }
}
