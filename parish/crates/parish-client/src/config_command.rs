use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use parish_config::{
    CatalogStore, compiled_inference_layer_v2, compiled_provider_registry_v2,
    effective_provider_registry, load_or_create_catalog_salt, load_project_config_v2,
    load_user_config_v2, merge_inference_layers, resolve_credential_slots,
    resolve_inference_snapshot_from_effective_registry, resolve_inference_topology_snapshot,
    routing_overrides_from_env,
};

fn keychain_secret(slot: &str) -> Option<String> {
    keyring::Entry::new("com.parish.rundale", &format!("provider:{slot}"))
        .ok()?
        .get_password()
        .ok()
}

#[derive(Parser)]
#[command(
    name = "parish config",
    about = "Validate strict Parish v2 configuration"
)]
struct ConfigCli {
    #[command(subcommand)]
    command: ConfigCommand,
}

#[derive(Subcommand)]
enum ConfigCommand {
    /// Parse and semantically validate a strict schema_version=2 document.
    Validate {
        #[arg(long, value_name = "FILE", conflicts_with = "user")]
        project: Option<PathBuf>,
        #[arg(long, value_name = "FILE", conflicts_with = "project")]
        user: Option<PathBuf>,
    },
    /// Resolve the effective category routes after project, user, and env layers.
    ShowEffective {
        #[arg(long, default_value = "parish.toml")]
        project: PathBuf,
        #[arg(long)]
        user: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
}

pub fn is_invocation() -> bool {
    std::env::args_os()
        .nth(1)
        .is_some_and(|value| value == "config")
}

pub fn run() -> Result<()> {
    if let Err(error) = run_inner() {
        eprintln!("{error:#}");
        std::process::exit(78);
    }
    Ok(())
}

fn run_inner() -> Result<()> {
    let mut arguments = std::env::args_os();
    let program = arguments.next().unwrap_or_else(|| "parish".into());
    let _config = arguments.next();
    let cli = ConfigCli::parse_from(std::iter::once(program).chain(arguments));
    match cli.command {
        ConfigCommand::Validate { project, user } => {
            let (path, kind) = match (project, user) {
                (Some(path), None) => (path, "project"),
                (None, Some(path)) => (path, "user"),
                _ => anyhow::bail!("use exactly one of --project FILE or --user FILE"),
            };
            let result = match kind {
                "project" => load_project_config_v2(&path).map(|_| ()),
                "user" => load_user_config_v2(&path).map(|_| ()),
                _ => unreachable!("validated config kind"),
            };
            match result {
                Ok(()) => {
                    println!("valid schema_version=2 {} config: {}", kind, path.display());
                    Ok(())
                }
                Err(error) => Err(error.into()),
            }
        }
        ConfigCommand::ShowEffective {
            project,
            user,
            json,
        } => {
            let user_path = user.unwrap_or_else(|| {
                parish_config::user_config::resolve_user_config_dir().join("parish.toml")
            });
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
            let overrides = routing_overrides_from_env()?;
            let topology = resolve_inference_topology_snapshot(
                1,
                &registry,
                &merged,
                &overrides,
                &credentials,
            )?;
            let user_data_dir = parish_persistence::paths::resolve_user_data_dir(
                parish_persistence::paths::DEFAULT_APP_NAME,
            );
            let store = CatalogStore::for_user_data_dir(&user_data_dir);
            let salt = load_or_create_catalog_salt(&user_data_dir).ok();
            let effective_registry = effective_provider_registry(&registry, &merged);
            let evidence = store.availability_snapshot_for_routes(
                &effective_registry,
                topology.category_routes.values().cloned(),
                salt.as_deref(),
                chrono::Utc::now(),
            )?;
            let snapshot = resolve_inference_snapshot_from_effective_registry(
                1,
                &evidence.constrained_registry,
                &merged,
                &overrides,
                &evidence.availability,
                &credentials,
            )?;
            if json {
                let routes = snapshot
                    .category_routes
                    .iter()
                    .map(|(category, route)| {
                        (category.clone(), route.view(snapshot.configuration_epoch))
                    })
                    .collect::<std::collections::BTreeMap<_, _>>();
                let subroles = snapshot
                    .subrole_routes
                    .iter()
                    .map(|(subrole, route)| {
                        (subrole.clone(), route.view(snapshot.configuration_epoch))
                    })
                    .collect::<std::collections::BTreeMap<_, _>>();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "active_loadout": snapshot.active_loadout,
                        "configuration_epoch": snapshot.configuration_epoch,
                        "routes": routes,
                        "subroles": subroles,
                    }))?
                );
            } else {
                println!(
                    "loadout: {}  epoch: {}",
                    snapshot.active_loadout, snapshot.configuration_epoch
                );
                for (category, route) in snapshot.category_routes {
                    println!(
                        "{category}: {}:{}:{}  api={:?}  availability={:?}  credential={}  reasoning={:?}  diagnostics={}",
                        route.key.provider_id,
                        route.key.endpoint_id,
                        route.key.model_id,
                        route.inference_adapter,
                        route.availability,
                        route.credential.is_some(),
                        route.effective_profile.reasoning,
                        route.diagnostics.len(),
                    );
                }
                for (subrole, route) in snapshot.subrole_routes {
                    println!(
                        "subrole {subrole}: {}:{}:{} reasoning={:?} output={:?}",
                        route.key.provider_id,
                        route.key.endpoint_id,
                        route.key.model_id,
                        route.effective_profile.reasoning,
                        route.structured_output,
                    );
                }
            }
            Ok(())
        }
    }
}
