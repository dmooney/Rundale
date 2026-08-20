//! Versioned inference configuration and model-catalog contracts.
//!
//! This module is deliberately synchronous and transport-free. HTTP discovery
//! belongs in `parish-providers`; startup orchestration passes normalized
//! catalog snapshots into the pure resolver here.

mod catalog;
mod loader;
mod registry_bridge;
mod resolver;
mod schema;
mod validate;

pub use catalog::{
    CatalogRefreshGuard, CatalogRuntimeEvidence, CatalogStore, CatalogStoreError,
    MODEL_CATALOG_TTL_HOURS, PendingProbeArtifact, ProbeArtifactInput, ProbeTerminalMetadata,
    append_probe_observation, cache_file_name, catalog_credential_fingerprint,
    discovery_adapter_version, inference_adapter_version, load_or_create_catalog_salt,
    merge_catalog_routes,
};
pub use loader::{
    ConfigDocumentKind, ConfigLoadError, archive_and_reset_user_config_v2, load_project_config_v2,
    load_user_config_v2, save_user_config_v2,
};
pub use registry_bridge::{compiled_inference_layer_v2, compiled_provider_registry_v2};
pub use resolver::{
    ConfigLayerSource, MergedInferenceLayer, ResolvedInferenceSnapshot, ResolverError,
    RoutingOverrideSet, effective_provider_registry, merge_inference_layers,
    resolve_credential_slots, resolve_inference_snapshot,
    resolve_inference_snapshot_from_effective_registry, resolve_inference_topology_snapshot,
    routing_overrides_from_env,
};
pub use schema::*;
pub use validate::{
    ConfigV2Error, validate_project_config, validate_provider_registry, validate_user_config,
};
