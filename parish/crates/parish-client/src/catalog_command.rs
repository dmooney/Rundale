use std::collections::BTreeSet;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use parish_config::{
    CatalogCacheIdentity, CatalogStore, EndpointDefinition, MODEL_CATALOG_TTL_HOURS,
    OpenAiChatGenerationWire, RoutingOverrideSet, StructuredOutputMode,
    catalog_credential_fingerprint, compiled_inference_layer_v2, compiled_provider_registry_v2,
    load_or_create_catalog_salt, load_project_config_v2, load_user_config_v2,
    merge_inference_layers, resolve_credential_slots, resolve_inference_topology_snapshot,
};

fn keychain_secret(slot: &str) -> Option<String> {
    keyring::Entry::new("com.parish.rundale", &format!("provider:{slot}"))
        .ok()?
        .get_password()
        .ok()
}

#[derive(Parser)]
#[command(
    name = "parish catalog",
    about = "Inspect and refresh the schema-v2 model catalog"
)]
struct CatalogCli {
    #[command(subcommand)]
    command: CatalogCommand,
}

#[derive(Subcommand)]
enum CatalogCommand {
    /// Print cached discovery documents without contacting a provider.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Refresh every endpoint used by the active loadout.
    Refresh {
        #[arg(long, default_value = "parish.toml")]
        project: PathBuf,
        #[arg(long)]
        user: Option<PathBuf>,
    },
    /// Make one explicit route request and retain its immutable receipt.
    Probe {
        #[arg(long, default_value = "dialogue")]
        category: String,
        /// Required for remote routes because this command may incur cost.
        #[arg(long)]
        billable_confirm: bool,
        #[arg(long, default_value = "parish.toml")]
        project: PathBuf,
        #[arg(long)]
        user: Option<PathBuf>,
    },
}

pub fn is_invocation() -> bool {
    std::env::args_os()
        .nth(1)
        .is_some_and(|value| value == "catalog")
}

pub async fn run() -> Result<()> {
    let mut arguments = std::env::args_os();
    let program = arguments.next().unwrap_or_else(|| "parish".into());
    let _catalog = arguments.next();
    let cli = CatalogCli::parse_from(std::iter::once(program).chain(arguments));
    let user_dir = parish_config::user_config::resolve_user_config_dir();
    let user_data_dir = parish_persistence::paths::resolve_user_data_dir(
        parish_persistence::paths::DEFAULT_APP_NAME,
    );
    let store = CatalogStore::for_user_data_dir(&user_data_dir);
    match cli.command {
        CatalogCommand::List { json } => {
            let documents = store.cached_documents()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&documents)?);
            } else if documents.is_empty() {
                println!("model catalog cache is empty; run `parish catalog refresh`");
            } else {
                for document in documents {
                    println!(
                        "{}:{}  {:?}  {} models  expires {}",
                        document.identity.provider_id,
                        document.identity.endpoint_id,
                        document.status,
                        document.routes.len(),
                        document.expires_at.to_rfc3339(),
                    );
                }
            }
        }
        CatalogCommand::Refresh { project, user } => {
            let user_path = user.unwrap_or_else(|| user_dir.join("parish.toml"));
            let project = load_project_config_v2(&project)?;
            let user = load_user_config_v2(&user_path)?;
            let registry = compiled_provider_registry_v2();
            let merged = merge_inference_layers(
                &compiled_inference_layer_v2(),
                &project.inference,
                &user.inference,
            );
            let credentials = resolve_credential_slots(
                &registry,
                &merged,
                &user.credential_bindings,
                keychain_secret,
            );
            let snapshot = resolve_inference_topology_snapshot(
                1,
                &registry,
                &merged,
                &RoutingOverrideSet::default(),
                &credentials,
            )?;
            let salt = match load_or_create_catalog_salt(&user_data_dir) {
                Ok(salt) => Some(salt),
                Err(error) => {
                    eprintln!(
                        "warning: authenticated catalog disk caching is disabled for this run: {error}"
                    );
                    None
                }
            };
            let mut seen = BTreeSet::new();
            for route in snapshot.category_routes.values() {
                let Some(discovery_base_url) = route.discovery_base_url.clone() else {
                    println!(
                        "{}:{} discovery unsupported",
                        route.key.provider_id, route.key.endpoint_id
                    );
                    continue;
                };
                if route.credential.is_some() && salt.is_none() {
                    eprintln!(
                        "{}:{} authenticated refresh skipped because the installation salt is unavailable",
                        route.key.provider_id, route.key.endpoint_id
                    );
                    continue;
                }
                let identity = CatalogCacheIdentity {
                    provider_id: route.key.provider_id.clone(),
                    endpoint_id: route.key.endpoint_id.clone(),
                    discovery_base_url: discovery_base_url.clone(),
                    inference_base_url: route.inference_base_url.clone(),
                    inference_adapter_version: adapter_version(route.inference_adapter),
                    discovery_adapter_version: discovery_version(route.discovery_adapter),
                    credential_fingerprint: route.credential.as_ref().map_or_else(
                        || "anonymous".to_string(),
                        |credential| {
                            catalog_credential_fingerprint(
                                salt.as_deref().expect("authenticated route checked above"),
                                Some(credential),
                            )
                        },
                    ),
                };
                if !seen.insert(identity.clone()) {
                    continue;
                }
                let Some(_refresh_guard) = store.lock_refresh(&identity)? else {
                    eprintln!(
                        "{}:{} refresh already in progress; using stale cache",
                        route.key.provider_id, route.key.endpoint_id
                    );
                    continue;
                };
                let prior = store
                    .load_cache(&identity, chrono::Utc::now())?
                    .map(|pair| pair.0);
                let endpoint = EndpointDefinition {
                    inference_base_url: route.inference_base_url.clone(),
                    discovery_base_url: Some(discovery_base_url),
                    inference_adapter: route.inference_adapter,
                    discovery_adapter: route.discovery_adapter,
                    backend_kind: route.backend_kind,
                    management_adapter: route.management_adapter,
                    auth_adapter: route.auth_adapter,
                    credential_slot: None,
                    default_reasoning_dialect: route.reasoning_dialect,
                    default_openai_generation_wire: route.openai_output_limit_field.map(|field| {
                        OpenAiChatGenerationWire {
                            output_limit_field: field,
                            structured_output: BTreeSet::from([
                                StructuredOutputMode::PromptValidatedJson,
                            ]),
                        }
                    }),
                };
                let document = parish_providers::fetch_catalog_endpoint(
                    identity,
                    &endpoint,
                    route.credential.as_ref(),
                    chrono::Duration::hours(MODEL_CATALOG_TTL_HOURS),
                    prior.as_ref(),
                )
                .await;
                store.save_cache(&document).with_context(|| {
                    format!(
                        "save catalog for {}:{}",
                        route.key.provider_id, route.key.endpoint_id
                    )
                })?;
                println!(
                    "{}:{} {:?} {} models",
                    route.key.provider_id,
                    route.key.endpoint_id,
                    document.status,
                    document.routes.len()
                );
            }
        }
        CatalogCommand::Probe {
            category,
            billable_confirm,
            project,
            user,
        } => {
            let user_path = user.unwrap_or_else(|| user_dir.join("parish.toml"));
            let project = load_project_config_v2(&project)?;
            let user = load_user_config_v2(&user_path)?;
            let registry = compiled_provider_registry_v2();
            let merged = merge_inference_layers(
                &compiled_inference_layer_v2(),
                &project.inference,
                &user.inference,
            );
            let credentials = resolve_credential_slots(
                &registry,
                &merged,
                &user.credential_bindings,
                keychain_secret,
            );
            let snapshot = resolve_inference_topology_snapshot(
                1,
                &registry,
                &merged,
                &RoutingOverrideSet::default(),
                &credentials,
            )?;
            let route = snapshot.category_routes.get(&category)
                .with_context(|| format!("unknown category {category:?}; expected dialogue, simulation, intent, or reaction"))?;
            if matches!(route.backend_kind, parish_config::BackendKind::Remote) && !billable_confirm
            {
                anyhow::bail!(
                    "remote model probes may be billable; repeat with --billable-confirm"
                );
            }
            let salt = load_or_create_catalog_salt(&user_data_dir)
                .context("model probe requires an account-isolation salt before billing")?;
            let fingerprint = catalog_credential_fingerprint(&salt, route.credential.as_ref());
            let catalog_identity = CatalogCacheIdentity {
                provider_id: route.key.provider_id.clone(),
                endpoint_id: route.key.endpoint_id.clone(),
                discovery_base_url: route.discovery_base_url.clone().unwrap_or_default(),
                inference_base_url: route.inference_base_url.clone(),
                inference_adapter_version: adapter_version(route.inference_adapter),
                discovery_adapter_version: discovery_version(route.discovery_adapter),
                credential_fingerprint: fingerprint,
            };
            let started_at = chrono::Utc::now();
            let raw = parish_providers::probe_route_raw(route).await?;
            // Persist the paid/non-deterministic response before parsing it.
            let attempt_id = uuid::Uuid::new_v4().to_string();
            let pending = store.persist_probe_raw(parish_config::ProbeArtifactInput {
                attempt_id: &attempt_id,
                route: &route.key,
                catalog_identity: &catalog_identity,
                configuration_epoch: snapshot.configuration_epoch,
                started_at,
                request_bytes: &raw.request_bytes,
                inference_adapter_version: &adapter_version(route.inference_adapter),
                discovery_adapter_version: &discovery_version(route.discovery_adapter),
                raw_response: &raw.body,
                provider_request_id: raw.provider_request_id.as_deref(),
            })?;
            let validation = match &raw.transport_error {
                Some(error) => Err(error.clone()),
                None => parish_providers::validate_probe_response(
                    route.inference_adapter,
                    raw.status,
                    &raw.body,
                ),
            };
            let (outcome, reason, input_tokens, output_tokens, error) = match validation {
                Ok((reason, input, output)) => (
                    parish_config::ProbeOutcome::Passed,
                    Some(reason),
                    input,
                    output,
                    None,
                ),
                Err(error) => (
                    if parish_providers::is_definitive_model_not_found(
                        route.inference_adapter,
                        raw.status,
                        &raw.body,
                        &route.key.model_id,
                    ) {
                        parish_config::ProbeOutcome::NotListed
                    } else if raw.transport_error.is_some()
                        || matches!(
                            raw.status,
                            0 | 401 | 403 | 408 | 409 | 425 | 429 | 500..=599
                        )
                    {
                        parish_config::ProbeOutcome::TransportFailed
                    } else {
                        parish_config::ProbeOutcome::Rejected
                    },
                    None,
                    None,
                    None,
                    Some(error),
                ),
            };
            let receipt = store.finish_probe(
                pending,
                chrono::Utc::now(),
                outcome,
                reason,
                parish_config::ProbeTerminalMetadata {
                    http_status: Some(raw.status),
                    terminal_event: None,
                    input_tokens,
                    output_tokens,
                    cost_usd_micros: None,
                },
                error,
            )?;
            if receipt.outcome != parish_config::ProbeOutcome::TransportFailed {
                store.record_probe_observation(&receipt)?;
            }
            println!(
                "{} {}:{}:{} {:?}",
                receipt.attempt_id,
                route.key.provider_id,
                route.key.endpoint_id,
                route.key.model_id,
                receipt.outcome
            );
            if receipt.outcome != parish_config::ProbeOutcome::Passed {
                anyhow::bail!("model probe was retained but did not pass validation");
            }
        }
    }
    Ok(())
}

fn adapter_version(adapter: parish_config::InferenceAdapter) -> String {
    parish_config::inference_adapter_version(adapter)
}

fn discovery_version(adapter: parish_config::DiscoveryAdapter) -> String {
    format!("{:?}@1", adapter).to_ascii_lowercase()
}
