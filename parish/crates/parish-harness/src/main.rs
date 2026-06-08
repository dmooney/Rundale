//! `parish-harness` CLI.
//!
//! Phase 1 ships `run` (play one session against a backend) and `db-path`
//! (print the resolved DB location). `serve` / `queue` / `worker` / `compare`
//! are declared so the surface is stable and land in later phases.

use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, Subcommand, ValueEnum};

use parish_harness::actor::{
    ApiJudge, ApiPlayer, Judge, Player, ScriptedJudge, ScriptedPlayer, make_client,
};
use parish_harness::config::{ActorMode, RunConfig};
use parish_harness::error::{HarnessError, Result};
use parish_harness::persist::{Db, default_db_path};
use parish_harness::run::{RunParams, execute_run};
use parish_harness::score::{Rubric, load_version};

#[derive(Parser)]
#[command(name = "parish-harness", version, about = "Game quality-control harness for Rundale")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Play one session against a running backend, score it, and persist.
    Run(RunArgs),
    /// Print the resolved harness DB path.
    DbPath,
    /// Serve the live dashboard (Phase 2).
    Serve,
    /// Manage the run queue (Phase 3).
    Queue,
    /// Run queued configs continuously (Phase 3).
    Worker,
    /// A/B compare two runs (Phase 3).
    Compare,
}

#[derive(clap::Args)]
struct RunArgs {
    /// Path to the run config JSON.
    #[arg(long)]
    config: PathBuf,
    /// Number of turns to play.
    #[arg(long, default_value_t = 100)]
    turns: u32,
    /// Backend base URL.
    #[arg(long, default_value = "http://127.0.0.1:3030")]
    base_url: String,
    /// Override the player/judge actor mode from the config.
    #[arg(long, value_enum)]
    player: Option<ActorKind>,
    /// Path to the harness DB (defaults to the user-data root).
    #[arg(long)]
    db: Option<PathBuf>,
    /// Where to write per-run artifacts (defaults next to the DB).
    #[arg(long)]
    artifacts: Option<PathBuf>,
    /// Per-command timeout in milliseconds.
    #[arg(long, default_value_t = 60_000)]
    command_timeout_ms: u64,
    /// How long to wait for the backend to become ready, in seconds.
    #[arg(long, default_value_t = 30)]
    ready_timeout_secs: u64,
}

#[derive(Copy, Clone, ValueEnum)]
enum ActorKind {
    Scripted,
    Api,
}

impl From<ActorKind> for ActorMode {
    fn from(k: ActorKind) -> Self {
        match k {
            ActorKind::Scripted => ActorMode::Scripted,
            ActorKind::Api => ActorMode::Api,
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    if let Err(e) = run().await {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::DbPath => {
            println!("{}", default_db_path().display());
            Ok(())
        }
        Command::Run(args) => run_session(args).await,
        Command::Serve | Command::Queue | Command::Worker | Command::Compare => {
            Err(HarnessError::Config(
                "this subcommand lands in a later phase; see docs/plans/game-quality-harness.md"
                    .into(),
            ))
        }
    }
}

async fn run_session(args: RunArgs) -> Result<()> {
    let mut config = RunConfig::load(&args.config)?;
    if let Some(kind) = args.player {
        let mode: ActorMode = kind.into();
        config.player.mode = mode;
        config.judge.mode = mode;
    }

    // Load + pin the rubric.
    let rubric = load_version(&config.judge.rubric_version)?;
    rubric.verify_pin(config.judge.rubric_sha256.as_deref())?;

    // Resolve DB + artifact roots.
    let db_path = args.db.unwrap_or_else(default_db_path);
    let artifact_root = args.artifacts.unwrap_or_else(|| {
        db_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    });
    let db = Db::open(&db_path)?;

    let (player, judge) = build_actors(&config, &rubric)?;

    let params = RunParams {
        base_url: args.base_url.clone(),
        turns: args.turns,
        command_timeout_ms: args.command_timeout_ms,
        ready_timeout: Duration::from_secs(args.ready_timeout_secs),
        artifact_root,
        player,
        judge,
        config,
    };

    let summary = execute_run(&db, params).await?;

    println!(
        "run_id={} status={} turns={} gate_reason={} gate_turn={} quality_score={} findings={} rubric_sha256={} git_sha={} branch={}",
        summary.id,
        summary.status,
        summary.turn_count,
        summary.gate_reason.as_deref().unwrap_or("-"),
        summary
            .gate_turn
            .map(|t| t.to_string())
            .unwrap_or_else(|| "-".into()),
        summary
            .quality_score
            .map(|q| format!("{q:.2}"))
            .unwrap_or_else(|| "-".into()),
        summary.finding_count,
        summary.rubric_sha256,
        summary.git_sha,
        summary.git_branch,
    );
    Ok(())
}

/// Build the player + judge actors for the configured modes.
fn build_actors(
    config: &RunConfig,
    rubric: &Rubric,
) -> Result<(Box<dyn Player>, Box<dyn Judge>)> {
    let rubric_sha = rubric.sha256.clone();
    let player: Box<dyn Player> = match config.player.mode {
        ActorMode::Scripted => Box::new(ScriptedPlayer::default()),
        ActorMode::Api => {
            let key = std::env::var("ANTHROPIC_API_KEY").ok();
            let client = make_client("anthropic", None, key.as_deref())?;
            let model = config
                .player
                .model
                .clone()
                .unwrap_or_else(|| "claude-sonnet-4-6".to_string());
            Box::new(ApiPlayer::new(client, model))
        }
    };
    let judge: Box<dyn Judge> = match config.judge.mode {
        ActorMode::Scripted => Box::new(ScriptedJudge::new(rubric_sha)),
        ActorMode::Api => {
            let key = std::env::var("ANTHROPIC_API_KEY").ok();
            let client = make_client("anthropic", None, key.as_deref())?;
            let model = config
                .judge
                .model
                .clone()
                .unwrap_or_else(|| "claude-sonnet-4-6".to_string());
            Box::new(ApiJudge::new(
                client,
                model,
                rubric.manifest.rubric.clone(),
                rubric_sha,
            ))
        }
    };
    Ok((player, judge))
}
