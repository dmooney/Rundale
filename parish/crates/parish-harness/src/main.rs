//! `parish-harness` CLI.
//!
//! Phase 1 ships `run` (play one session against a backend) and `db-path`
//! (print the resolved DB location). Phase 3 adds `worker` (poll and drain the
//! queue), `queue` (manage the run queue), and `compare` (A/B delta table).

use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::{Parser, Subcommand, ValueEnum};

use parish_core::ipc::GitHubBugConfig;
use parish_harness::actor::{Judge, Player};
use parish_harness::config::{ActorMode, RunConfig};
use parish_harness::dashboard::serve;
use parish_harness::error::{HarnessError, Result};
use parish_harness::persist::{Db, default_db_path};
use parish_harness::queue::{QueueBackend, QueueStore};
use parish_harness::run::{RunParams, build_actors, execute_run};
use parish_harness::score::load_version;

#[derive(Parser)]
#[command(
    name = "parish-harness",
    version,
    about = "Game quality-control harness for Rundale"
)]
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
    /// Serve the live dashboard.
    Serve(ServeArgs),
    /// Manage the run queue.
    Queue(QueueArgs),
    /// Run queued configs continuously.
    Worker(WorkerArgs),
    /// A/B compare two runs.
    Compare(CompareArgs),
    /// Ingest an externally-produced run (e.g. a quality-harness skill run).
    Ingest(IngestArgs),
    /// Link stored findings to their GitHub issues by signature.
    BackfillIssues(BackfillArgs),
}

#[derive(clap::Args)]
struct BackfillArgs {
    /// Path to the harness DB (defaults to the user-data root).
    #[arg(long)]
    db: Option<PathBuf>,
    /// Override the `owner/repo` to search (defaults to the bug-report repo).
    #[arg(long)]
    repo: Option<String>,
    /// Print the signature → issue matches without writing to the DB.
    #[arg(long)]
    dry_run: bool,
}

#[derive(clap::Args)]
struct IngestArgs {
    /// Path to the ingest payload JSON (a complete skill-run record).
    #[arg(long)]
    payload: PathBuf,
    /// Artifact root holding `runs/<uuid>/turns/NNN/frame.png`.
    #[arg(long)]
    artifacts: PathBuf,
    /// Path to the harness DB (defaults to the user-data root).
    #[arg(long)]
    db: Option<PathBuf>,
}

#[derive(clap::Args)]
struct ServeArgs {
    /// Path to the harness DB.
    #[arg(long)]
    db: Option<PathBuf>,
    /// Where per-run artifacts are stored (defaults next to the DB).
    #[arg(long)]
    artifacts: Option<PathBuf>,
    /// TCP port to listen on.
    #[arg(long, default_value_t = 8787)]
    port: u16,
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
    /// Override the player actor mode from the config. When `--judge` is
    /// omitted this also sets the judge mode (back-compat); pass `--judge`
    /// explicitly to keep the roles independent (#1363 AC5).
    #[arg(long, value_enum)]
    player: Option<ActorKind>,
    /// Override the judge actor mode from the config, independently of
    /// `--player`. The judge model stays whatever the config pins (Sonnet 4.6
    /// by default) — this only selects the judge's driver (scripted/api/subagent).
    #[arg(long, value_enum)]
    judge: Option<ActorKind>,
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
    /// File GitHub issues for each finding (dry-run when no token is set).
    #[arg(long)]
    file_issues: bool,
    /// Dashboard base URL embedded in issue bodies.
    #[arg(long, default_value = "http://localhost:8787")]
    dashboard_base: String,
}

#[derive(Copy, Clone, ValueEnum)]
enum ActorKind {
    Scripted,
    Api,
    Subagent,
}

impl From<ActorKind> for ActorMode {
    fn from(k: ActorKind) -> Self {
        match k {
            ActorKind::Scripted => ActorMode::Scripted,
            ActorKind::Api => ActorMode::Api,
            ActorKind::Subagent => ActorMode::Subagent,
        }
    }
}

// ── Queue subcommand ──────────────────────────────────────────────────────────

#[derive(clap::Args)]
struct QueueArgs {
    /// Path to the harness DB.
    #[arg(long)]
    db: Option<PathBuf>,
    #[command(subcommand)]
    action: QueueAction,
}

#[derive(Subcommand)]
enum QueueAction {
    /// Add a config to the queue.
    Add(QueueAddArgs),
    /// Print queue counts and recent rows.
    List,
}

#[derive(clap::Args)]
struct QueueAddArgs {
    /// Path to the run config JSON.
    #[arg(long)]
    config: PathBuf,
    /// Priority (higher is claimed first).
    #[arg(long, default_value_t = 0)]
    priority: i64,
}

// ── Worker subcommand ─────────────────────────────────────────────────────────

#[derive(clap::Args)]
struct WorkerArgs {
    /// Path to the harness DB.
    #[arg(long)]
    db: Option<PathBuf>,
    /// Where to write per-run artifacts (defaults next to the DB).
    #[arg(long)]
    artifacts: Option<PathBuf>,
    /// Backend base URL.
    #[arg(long, default_value = "http://127.0.0.1:3030")]
    base_url: String,
    /// Number of turns to play per run.
    #[arg(long, default_value_t = 100)]
    turns: u32,
    /// Drain all pending jobs then exit (no polling loop).
    #[arg(long)]
    once: bool,
    /// Stop after this many runs (no cap when absent).
    #[arg(long)]
    max_runs: Option<u64>,
    /// Worker identity embedded in claimed_by.
    #[arg(long)]
    worker_id: Option<String>,
    /// Seconds to sleep between empty polls (ignored with --once).
    #[arg(long, default_value_t = 5)]
    poll_secs: u64,
}

// ── Compare subcommand ────────────────────────────────────────────────────────

#[derive(clap::Args)]
struct CompareArgs {
    /// First run id.
    #[arg(long)]
    a: i64,
    /// Second run id.
    #[arg(long)]
    b: i64,
    /// Path to the harness DB.
    #[arg(long)]
    db: Option<PathBuf>,
}

// ── main ──────────────────────────────────────────────────────────────────────

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
        Command::Serve(args) => run_serve(args).await,
        Command::Queue(args) => run_queue(args).await,
        Command::Worker(args) => run_worker(args).await,
        Command::Compare(args) => run_compare(args).await,
        Command::Ingest(args) => run_ingest(args),
        Command::BackfillIssues(args) => run_backfill(args).await,
    }
}

fn run_ingest(args: IngestArgs) -> Result<()> {
    let db_path = args.db.unwrap_or_else(default_db_path);
    let db = Db::open(&db_path)?;
    let run_id = parish_harness::ingest::load_and_ingest(&db, &args.payload, &args.artifacts)?;
    println!("ingested run {run_id}");
    Ok(())
}

async fn run_backfill(args: BackfillArgs) -> Result<()> {
    let db_path = args.db.unwrap_or_else(default_db_path);
    let db = Db::open(&db_path)?;
    let mut cfg = GitHubBugConfig::from_env();
    if let Some(repo) = args.repo {
        cfg.repo = repo;
    }
    parish_harness::backfill::backfill_issues(&db, &cfg, args.dry_run).await?;
    Ok(())
}

async fn run_serve(args: ServeArgs) -> Result<()> {
    let db_path = args.db.unwrap_or_else(default_db_path);
    let artifact_root = args.artifacts.unwrap_or_else(|| {
        db_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    });
    serve(db_path, artifact_root, args.port).await
}

async fn run_session(args: RunArgs) -> Result<()> {
    let mut config = RunConfig::load(&args.config)?;
    // `--player` sets the player mode. For back-compat it also sets the judge
    // mode UNLESS `--judge` is given, in which case the two roles are
    // independent (#1363 AC5: `--player api --judge api`, or `--player scripted
    // --judge api`, etc.).
    if let Some(kind) = args.player {
        let mode: ActorMode = kind.into();
        config.player.mode = mode;
        if args.judge.is_none() {
            config.judge.mode = mode;
        }
    }
    if let Some(kind) = args.judge {
        config.judge.mode = kind.into();
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
        file_issues: args.file_issues,
        dashboard_base: args.dashboard_base.clone(),
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
    if let Some(error) =
        gated_run_error(summary.id, &summary.status, summary.gate_reason.as_deref())
    {
        return Err(error);
    }
    Ok(())
}

fn gated_run_error(run_id: i64, status: &str, reason: Option<&str>) -> Option<HarnessError> {
    (status == "gated").then(|| HarnessError::RunGated {
        run_id,
        reason: reason.unwrap_or("unspecified gate").to_string(),
    })
}

// ── Queue subcommand impl ─────────────────────────────────────────────────────

async fn run_queue(args: QueueArgs) -> Result<()> {
    let db_path = args.db.unwrap_or_else(default_db_path);
    let db = Db::open(&db_path)?;
    let q = QueueStore::new(&db);

    match args.action {
        QueueAction::Add(add) => {
            let config = RunConfig::load(&add.config)?;
            let config_id = db.upsert_config(&config)?;
            let queue_id = q.enqueue(config_id, add.priority)?;
            println!(
                "enqueued queue_id={queue_id} config_id={config_id} priority={}",
                add.priority
            );
        }
        QueueAction::List => {
            let counts = q.counts()?;
            println!(
                "pending={} running={} done={} failed={}",
                counts.pending, counts.running, counts.done, counts.failed,
            );
            for state in &["pending", "running", "failed"] {
                let rows = q.list(state)?;
                if rows.is_empty() {
                    continue;
                }
                println!("\n{state}:");
                for r in &rows {
                    println!(
                        "  id={} config_id={} priority={} enqueued={} claimed_by={} run_id={}",
                        r.id,
                        r.config_id,
                        r.priority,
                        r.enqueued_at,
                        r.claimed_by.as_deref().unwrap_or("-"),
                        r.run_id
                            .map(|x| x.to_string())
                            .unwrap_or_else(|| "-".into()),
                    );
                }
            }
        }
    }
    Ok(())
}

// ── Worker subcommand impl ────────────────────────────────────────────────────

/// Execute one queued job. Returns the run_id on success, `None` when the
/// queue was empty.
async fn run_one_queued(
    db: &Db,
    worker_id: &str,
    base_url: &str,
    turns: u32,
    artifact_root: &Path,
) -> Result<Option<i64>> {
    let q = QueueStore::new(db);

    let job = match q.claim_next(worker_id)? {
        Some(j) => j,
        None => return Ok(None),
    };

    let queue_id = job.queue_id;

    // Load and verify the rubric pinned by this config.
    let rubric = match load_version(&job.config.judge.rubric_version) {
        Ok(r) => r,
        Err(e) => {
            q.fail(queue_id, &format!("rubric load failed: {e}"))?;
            return Err(e);
        }
    };
    if let Err(e) = rubric.verify_pin(job.config.judge.rubric_sha256.as_deref()) {
        q.fail(queue_id, &format!("rubric pin mismatch: {e}"))?;
        return Err(e);
    }

    let (player, judge): (Box<dyn Player>, Box<dyn Judge>) =
        match build_actors(&job.config, &rubric) {
            Ok(actors) => actors,
            Err(e) => {
                q.fail(queue_id, &format!("build_actors failed: {e}"))?;
                return Err(e);
            }
        };

    let params = RunParams {
        base_url: base_url.to_string(),
        turns,
        command_timeout_ms: 60_000,
        ready_timeout: Duration::from_secs(30),
        artifact_root: artifact_root.to_path_buf(),
        player,
        judge,
        config: job.config,
        file_issues: false,
        dashboard_base: "http://localhost:8787".to_string(),
    };

    match execute_run(db, params).await {
        Ok(summary) => {
            q.complete(queue_id, summary.id)?;
            tracing::info!(queue_id, run_id = summary.id, "job completed");
            Ok(Some(summary.id))
        }
        Err(e) => {
            q.fail(queue_id, &format!("run failed: {e}"))?;
            Err(e)
        }
    }
}

async fn run_worker(args: WorkerArgs) -> Result<()> {
    let db_path = args.db.unwrap_or_else(default_db_path);
    let artifact_root = args.artifacts.clone().unwrap_or_else(|| {
        db_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    });
    let db = Db::open(&db_path)?;

    let worker_id = args.worker_id.unwrap_or_else(default_worker_id);
    let mut runs_done: u64 = 0;

    loop {
        if let Some(max) = args.max_runs
            && runs_done >= max
        {
            tracing::info!(runs_done, "reached --max-runs; exiting");
            break;
        }

        match run_one_queued(&db, &worker_id, &args.base_url, args.turns, &artifact_root).await {
            Ok(Some(run_id)) => {
                runs_done += 1;
                tracing::info!(run_id, runs_done, "run completed");
            }
            Ok(None) => {
                if args.once {
                    tracing::info!("queue empty and --once set; exiting");
                    break;
                }
                tracing::debug!(poll_secs = args.poll_secs, "queue empty; sleeping");
                tokio::time::sleep(Duration::from_secs(args.poll_secs)).await;
            }
            Err(e) => {
                // Job was already marked failed by run_one_queued; log and
                // continue so a single broken config doesn't kill the worker.
                tracing::error!("job error: {e}; continuing");
            }
        }
    }

    println!("worker done; runs_executed={runs_done}");
    Ok(())
}

/// Generate a default worker id from hostname (env) + pid.
fn default_worker_id() -> String {
    let host = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("HOST"))
        .unwrap_or_else(|_| "worker".to_string());
    let pid = std::process::id();
    format!("{host}:{pid}")
}

// ── Compare subcommand impl ───────────────────────────────────────────────────

async fn run_compare(args: CompareArgs) -> Result<()> {
    let db_path = args.db.unwrap_or_else(default_db_path);
    let db = Db::open(&db_path)?;

    let cmp = db.compare(args.a, args.b)?.ok_or_else(|| {
        HarnessError::Config(format!(
            "one or both run ids not found: a={} b={}",
            args.a, args.b
        ))
    })?;

    println!("A/B compare: run {} vs run {}", cmp.a.id, cmp.b.id);
    println!(
        "  A: status={} quality={} branch={} sha={}",
        cmp.a.status,
        cmp.a
            .quality_score
            .map(|q| format!("{q:.2}"))
            .unwrap_or_else(|| "-".into()),
        cmp.a.git_branch,
        &cmp.a.git_sha[..cmp.a.git_sha.len().min(8)],
    );
    println!(
        "  B: status={} quality={} branch={} sha={}",
        cmp.b.status,
        cmp.b
            .quality_score
            .map(|q| format!("{q:.2}"))
            .unwrap_or_else(|| "-".into()),
        cmp.b.git_branch,
        &cmp.b.git_sha[..cmp.b.git_sha.len().min(8)],
    );

    println!("\nGate: A={} B={}", cmp.gate_diff.0, cmp.gate_diff.1);

    if !cmp.axis_deltas.is_empty() {
        println!("\nAxis deltas (B - A):");
        println!("  {:<28} {:>6} {:>6} {:>8}", "axis", "A", "B", "delta");
        println!("  {}", "-".repeat(52));
        for d in &cmp.axis_deltas {
            println!(
                "  {:<28} {:>6} {:>6} {:>+8}",
                d.axis, d.a_score, d.b_score, d.delta
            );
        }
    }

    if !cmp.findings_only_in_a.is_empty() {
        println!("\nFindings only in A:");
        for sig in &cmp.findings_only_in_a {
            println!("  {sig}");
        }
    }
    if !cmp.findings_only_in_b.is_empty() {
        println!("\nFindings only in B:");
        for sig in &cmp.findings_only_in_b {
            println!("  {sig}");
        }
    }

    Ok(())
}

#[cfg(test)]
mod cli_tests {
    use super::*;

    #[test]
    fn gated_run_becomes_process_level_error() {
        let error = gated_run_error(17, "gated", Some("timeout")).expect("gate must fail");
        assert!(matches!(
            error,
            HarnessError::RunGated {
                run_id: 17,
                reason
            } if reason == "timeout"
        ));
        assert!(gated_run_error(18, "completed", None).is_none());
    }
}
