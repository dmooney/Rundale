//! Per-category inference benchmark.
//!
//! Fires representative prompts for each Rundale inference category
//! (Intent, Reaction, Simulation, Dialogue) through the real
//! `InferenceQueue` worker against a configurable OpenAI-compat
//! endpoint. Reports ttft / tok/s / total latency and PASS/FAIL
//! against the per-category latency budgets.
//!
//! Usage:
//!   cargo run -p parish-inference --release --example inf_bench -- \
//!       --base-url http://localhost:11434 \
//!       --intent-model gemma4:e4b \
//!       --main-model  gemma4:31b \
//!       [--api-key ...]
//!
//! The harness runs each prompt twice: a warmup pass (discarded) and
//! a measurement pass. Cold-load is handled by the warmup so reported
//! numbers reflect steady-state per-call cost.
//!
//! Budgets (ttft / total p95):
//!   Intent      <  200 ms /  <  500 ms
//!   Reaction    <  400 ms /  <  800 ms
//!   Simulation  <  800 ms /  < 1500 ms
//!   Dialogue    < 1000 ms / streaming (no total cap)

use std::time::Instant;

use parish_config::InferenceConfig;
use parish_inference::openai_client::OpenAiClient;
use parish_inference::{
    AnyClient, InferencePriority, InferenceRequest, InferenceWorkerConfig, JsonSchemaSpec,
    new_inference_log, spawn_inference_worker,
};
use tokio::sync::{mpsc, oneshot};

/// (ttft_ms, total_ms, output_tokens, tok/s) for a single call. `None` is returned
/// from [`run_one`] when the call errored.
type RunResult = (Option<u64>, u64, Option<u64>, Option<f64>);

#[derive(Clone, Copy, Debug)]
enum Category {
    Intent,
    Reaction,
    Simulation,
    Dialogue,
}

impl Category {
    /// (ttft_budget_ms, total_budget_ms or None for streaming-only)
    fn budget(self) -> (u64, Option<u64>) {
        match self {
            Category::Intent => (200, Some(500)),
            Category::Reaction => (400, Some(800)),
            Category::Simulation => (800, Some(1500)),
            Category::Dialogue => (1000, None),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Category::Intent => "Intent",
            Category::Reaction => "Reaction",
            Category::Simulation => "Simulation",
            Category::Dialogue => "Dialogue",
        }
    }
}

struct Sample {
    cat: Category,
    system: Option<&'static str>,
    user: &'static str,
    json_mode: bool,
    /// Strict JSON-schema constraint for this sample. When `Some` and
    /// `--schema` is on the CLI, the bench sends `response_format:
    /// json_schema` rather than `json_object` — required for vllm-mlx
    /// and LM Studio to accept structured output.
    schema: Option<(&'static str, &'static str)>,
    /// Per-sample `max_tokens` cap. Mirrors what production code passes
    /// to the inference client for this category:
    ///
    ///   - Reaction: production caps at 100 (`arrival_reactions.rs`).
    ///   - Tier 2 Sim: production caps at 200 (`ticks.rs:run_tier2_for_group`).
    ///   - Tier 3 Batch: production caps at 600 (`ticks.rs:run_tier3`).
    ///   - Intent: production passes `None` (schema bounds output).
    ///   - Dialogue: streamed, no cap.
    ///
    /// `None` leaves the request uncapped, matching production exactly.
    max_tokens: Option<u32>,
}

const INTENT_SCHEMA: &str = r#"{
    "type": "object",
    "properties": {
        "intent": {"type": "string", "enum": ["move","talk","look","interact","examine","unknown"]},
        "target": {"type": ["string","null"]},
        "dialogue": {"type": ["string","null"]}
    },
    "required": ["intent"]
}"#;

const SIM_SCHEMA: &str = r#"{
    "type": "object",
    "properties": {
        "summary": {"type": "string"},
        "mood_changes": {"type": "array"},
        "relationship_changes": {"type": "array"}
    },
    "required": ["summary"]
}"#;

/// Tier 3 batch schema — mirrors `build_tier3_prompt` in
/// `parish-npc/src/ticks.rs`. Each NPC in the input group gets one
/// update; the bench fixture below uses three NPCs.
const TIER3_SCHEMA: &str = r#"{
    "type": "object",
    "properties": {
        "updates": {"type": "array"}
    },
    "required": ["updates"]
}"#;

fn samples() -> Vec<Sample> {
    // === Production-faithful prompts ===
    //
    // The bench's job is to measure what the engine actually does in
    // production, not idealized prompts. The four fixtures below
    // mirror the production prompt builders:
    //
    //   Intent:     parish-input/src/intent_llm.rs::INTENT_SYSTEM_PROMPT
    //   Reaction:   parish-npc/src/reactions/arrival_reactions.rs::build_reaction_prompt
    //   Tier 2 Sim: parish-npc/src/ticks.rs::build_tier2_prompt
    //   Tier 3 Sim: parish-npc/src/ticks.rs::build_tier3_prompt
    //
    // Dialogue is not built by a single function — assembled across
    // `build_enhanced_system_prompt` + `build_enhanced_context` — so we
    // use a representative short shape that matches the dialogue path.

    // Intent: full production system prompt with examples + past-tense
    // disambiguation rules.
    let intent_sys = "\
You are a text adventure input parser. Given the player's natural language input, \
determine their intent. Respond with valid JSON containing:\n\
- \"intent\": one of \"move\", \"talk\", \"look\", \"interact\", \"examine\", \"unknown\"\n\
- \"target\": what the action is directed at (string or null)\n\
- \"dialogue\": what the player is saying, if talking (string or null)\n\
\n\
IMPORTANT: \"move\" is ONLY for when the player expresses a present desire to \
navigate somewhere (imperative or future intent). Narrative, past-tense, or \
reflective statements that merely mention a place name are \"talk\", not \"move\".\n\
\n\
Examples:\n\
Input: \"go to the pub\" → {\"intent\": \"move\", \"target\": \"the pub\", \"dialogue\": null}\n\
Input: \"talk to Mary\" → {\"intent\": \"talk\", \"target\": \"Mary\", \"dialogue\": null}\n\
Input: \"tell Padraig I saw his cow\" → {\"intent\": \"talk\", \"target\": \"Padraig\", \"dialogue\": \"I saw his cow\"}\n\
Input: \"look around\" → {\"intent\": \"look\", \"target\": null, \"dialogue\": null}\n\
Input: \"pick up the stone\" → {\"intent\": \"interact\", \"target\": \"the stone\", \"dialogue\": null}\n\
Input: \"I came from the coast\" → {\"intent\": \"talk\", \"target\": null, \"dialogue\": \"I came from the coast\"}\n\
Input: \"I was at the shore yesterday\" → {\"intent\": \"talk\", \"target\": null, \"dialogue\": \"I was at the shore yesterday\"}\n\
\n\
Respond ONLY with valid JSON. No explanation.";

    // Reaction: mirrors `build_reaction_prompt` — name/age/occupation,
    // truncated personality, current mood, 1-2 sentence cap, dialogue-only.
    let reaction_sys = "You are Padraig Darcy, a 58-year-old Publican in rural Ireland, 1820.\n\
A gruff but warm-hearted publican who has run Darcy's Pub for thirty years. Known for his dry wit.\n\
Current mood: content\n\n\
Write a single brief greeting or reaction (1-2 sentences max). \
Dialogue only, no narration or action descriptions. \
Do not use any modern language.";

    // Tier 2 Sim: mirrors `build_tier2_prompt` output. Character lines
    // use the production format
    // `- [id] name, occupation. Currently mood. <intelligence_prose>. <relationship_summary>.`
    let sim_user = "You are simulating background interactions between characters in a small \
Irish parish in 1820.\n\n\
Location: Darcy's Pub\n\
Time: Evening\n\
Weather: Clear.\n\n\
Dramatis personae (id in brackets — reuse these in your JSON):\n\
- [1] Padraig Darcy, Publican. Currently content. He is even-tempered and well-spoken. He's known Niamh his whole life.\n\
- [2] Niamh Darcy, Barmaid. Currently tired. She is quick-witted and observant. She is Padraig's daughter.\n\
- [3] Sean Murphy, Farmer. Currently hungry. He is plain-spoken and stubborn.\n\n\
Write one short sentence (max 20 words) describing what these characters are \
doing right now. Most exchanges are uneventful — leave mood_changes and \
relationship_changes as empty arrays unless a character's mood has clearly \
shifted or a relationship has meaningfully strengthened or strained.\n\n\
Respond with a JSON object, using the bracketed ids. Default shape (use this \
when nothing notable changes):\n\
{\"summary\": \"...\", \"mood_changes\": [], \"relationship_changes\": []}\n\n\
Only when something actually changes, include entries:\n\
  mood_changes:        {\"npc_id\": <id>, \"new_mood\": \"<mood>\"}\n\
  relationship_changes: {\"from\": <id>, \"to\": <id>, \"delta\": <-0.1 to 0.1>}";

    // Tier 3 Batch: mirrors `build_tier3_prompt` — six NPCs, six-hour
    // window. Output is one update per NPC.
    let tier3_user = "You are simulating background NPC activity in a rural Irish parish in 1820. \
Simulate 6 hours of activity for the people below. \
The weather is Clear, the season is Summer, the time is afternoon.\n\n\
NPCs (id in brackets — reuse these in your JSON):\n\
- [1] Padraig Darcy, 58, Publican — at Darcy's Pub, content (even-tempered, well-spoken).\n\
  Known Niamh his whole life; long-standing friendship with Tommy Maguire.\n\
- [2] Niamh Darcy, 24, Barmaid — at Darcy's Pub, tired (quick-witted, observant).\n\
  Daughter of Padraig.\n\
- [3] Sean Murphy, 41, Farmer — at the bog, hungry (plain-spoken, stubborn).\n\
- [4] Tommy Maguire, 62, Farmer — at the crossroads, restless (storyteller).\n\
- [5] Brigid O'Brien, 42, Midwife — at her cottage, focused (kind, direct, knowledgeable).\n\
- [6] Father Cathal, 51, Priest — at the church, contemplative (eloquent, severe).\n\n\
For each NPC, return one update describing their mood, what they did, \
whether they moved, and any relationship shifts. Respond with JSON, \
using the bracketed ids:\n\
{\"updates\":[{\"npc_id\":<id>,\"mood\":\"...\",\"activity_summary\":\"...\",\
\"new_location\":<id|null>,\
\"relationship_changes\":[{\"from\":<id>,\"to\":<id>,\"delta\":<-0.1..0.1>}]}]}";

    let dialogue_sys = "You are Brigid O'Brien, a 42-year-old midwife in rural Ireland, 1820. \
You are kind but direct, with a deep knowledge of local plants and folk medicine. \
You have known the player's family for years.\n\n\
Stay in character. Speak in 1-3 sentences. Do not use modern language.";

    vec![
        // Intent — short user input, structured output. Production passes
        // max_tokens=None (intent JSON is naturally short).
        Sample {
            cat: Category::Intent,
            system: Some(intent_sys),
            user: "go to the pub",
            json_mode: true,
            schema: Some(("intent", INTENT_SCHEMA)),
            max_tokens: None,
        },
        Sample {
            cat: Category::Intent,
            system: Some(intent_sys),
            user: "tell Padraig I saw his cow wandering near the bog",
            json_mode: true,
            schema: Some(("intent", INTENT_SCHEMA)),
            max_tokens: None,
        },
        Sample {
            cat: Category::Intent,
            system: Some(intent_sys),
            user: "look around",
            json_mode: true,
            schema: Some(("intent", INTENT_SCHEMA)),
            max_tokens: None,
        },
        // Reaction — context body mirrors `build_reaction_prompt`.
        // Production caps at 100 tokens (`arrival_reactions.rs:770`).
        Sample {
            cat: Category::Reaction,
            system: Some(reaction_sys),
            user: "A newcomer has just arrived at Darcy's Pub. It is evening, Clear.\n\
You have not met this person before. You are working here as the Publican. \
Introduce yourself briefly.",
            json_mode: false,
            schema: None,
            max_tokens: Some(100),
        },
        Sample {
            cat: Category::Reaction,
            system: Some(reaction_sys),
            user: "A newcomer has just arrived at Darcy's Pub. It is morning, Light Rain.\n\
You have met this person before.",
            json_mode: false,
            schema: None,
            max_tokens: Some(100),
        },
        // Tier 2 Sim — Background lane. Production caps at 200
        // (`ticks.rs:run_tier2_for_group`).
        Sample {
            cat: Category::Simulation,
            system: None,
            user: sim_user,
            json_mode: true,
            schema: Some(("tier2_simulation", SIM_SCHEMA)),
            max_tokens: Some(200),
        },
        // Tier 3 Batch Sim — Batch lane, larger NPC group, longer time
        // window. Most expensive background path. Production caps at 600
        // (`ticks.rs:run_tier3`) to keep a single batch under the 1500 ms
        // simulation budget.
        Sample {
            cat: Category::Simulation,
            system: None,
            user: tier3_user,
            json_mode: true,
            schema: Some(("tier3_batch", TIER3_SCHEMA)),
            max_tokens: Some(600),
        },
        // Dialogue — streamed prose, no cap (player reads as it streams).
        Sample {
            cat: Category::Dialogue,
            system: Some(dialogue_sys),
            user: "I've been having trouble sleeping. The dreams keep coming back.",
            json_mode: false,
            schema: None,
            max_tokens: None,
        },
        Sample {
            cat: Category::Dialogue,
            system: Some(dialogue_sys),
            user: "What do you know about the old Cailleach who lives near the fairy fort?",
            json_mode: false,
            schema: None,
            max_tokens: None,
        },
    ]
}

#[derive(Default)]
struct CatStats {
    ttft_ms: Vec<u64>,
    total_ms: Vec<u64>,
    tok_per_s: Vec<f64>,
    output_tokens: Vec<u64>,
    errors: u32,
}

fn pct(values: &[u64], q: f64) -> u64 {
    if values.is_empty() {
        return 0;
    }
    let mut v = values.to_vec();
    v.sort_unstable();
    let idx = ((v.len() as f64 - 1.0) * q).round() as usize;
    v[idx.min(v.len() - 1)]
}

fn pct_f(values: &[f64], q: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut v = values.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let idx = ((v.len() as f64 - 1.0) * q).round() as usize;
    v[idx.min(v.len() - 1)]
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut base_url = "http://localhost:11434".to_string();
    let mut intent_model = "gemma4:e4b".to_string();
    let mut main_model = "gemma4:e4b".to_string();
    let mut api_key: Option<String> = None;
    let mut iters = 3usize;
    let mut warmup = true;
    // LM Studio rejects `response_format: {"type": "json_object"}` (it accepts
    // only "text" or "json_schema"). Pass --no-json-mode to drop the field
    // entirely and rely on prompt-only structured output.
    let mut force_no_json = false;
    // When set, send `response_format: json_schema` instead of `json_object`
    // for samples that ship a schema (Intent + Simulation). Required for
    // strict servers (vllm-mlx, LM Studio).
    let mut use_schema = false;

    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--base-url" => base_url = args.next().expect("--base-url value"),
            "--intent-model" => intent_model = args.next().expect("--intent-model value"),
            "--main-model" => main_model = args.next().expect("--main-model value"),
            "--api-key" => api_key = args.next(),
            "--iters" => iters = args.next().expect("--iters").parse()?,
            "--no-warmup" => warmup = false,
            "--no-json-mode" => force_no_json = true,
            "--schema" => use_schema = true,
            "-h" | "--help" => {
                println!(
                    "Usage: inf_bench [--base-url URL] [--intent-model TAG] \
                     [--main-model TAG] [--api-key KEY] [--iters N] \
                     [--no-warmup] [--no-json-mode] [--schema]"
                );
                return Ok(());
            }
            other => return Err(format!("unknown arg: {other}").into()),
        }
    }

    println!("base_url     = {}", base_url);
    println!("intent_model = {}", intent_model);
    println!("main_model   = {}", main_model);
    println!("iters        = {} (per sample)", iters);
    println!("warmup       = {}", warmup);
    println!();

    let cfg = InferenceConfig::default();
    let client = AnyClient::open_ai(OpenAiClient::new_with_config(
        &base_url,
        api_key.as_deref(),
        &cfg,
    ));

    let (itx, irx) = mpsc::channel::<InferenceRequest>(8);
    let (_btx, brx) = mpsc::channel::<InferenceRequest>(8);
    let (_xtx, xrx) = mpsc::channel::<InferenceRequest>(8);
    let log = new_inference_log();
    let _h = spawn_inference_worker(
        client,
        InferenceWorkerConfig {
            interactive_rx: irx,
            background_rx: brx,
            batch_rx: xrx,
            log: log.clone(),
            file_log: parish_inference::file_log::InferenceFileLog::disabled(),
            provider: parish_config::Provider::from_str_loose("openai").unwrap_or_default(),
            timeout_config: cfg,
        },
    );

    let samples = samples();

    if warmup {
        println!("== warmup (discarded) ==");
        run_one(
            &itx,
            &log,
            &samples[0],
            &intent_model,
            0,
            "warmup-intent",
            force_no_json,
            use_schema,
        )
        .await?;
        run_one(
            &itx,
            &log,
            &samples[5],
            &main_model,
            0,
            "warmup-main",
            force_no_json,
            use_schema,
        )
        .await?;
        println!();
    }

    let mut by_cat: std::collections::BTreeMap<&str, CatStats> = std::collections::BTreeMap::new();

    println!(
        "== runs ({} per sample, {} samples) ==",
        iters,
        samples.len()
    );
    for (i, s) in samples.iter().enumerate() {
        let model = match s.cat {
            Category::Intent => &intent_model,
            _ => &main_model,
        };
        for k in 0..iters {
            let r = run_one(
                &itx,
                &log,
                s,
                model,
                i as u64 + 1,
                "",
                force_no_json,
                use_schema,
            )
            .await?;
            let stats = by_cat.entry(s.cat.label()).or_default();
            match r {
                Some((ttft, total, toks, tps)) => {
                    stats.total_ms.push(total);
                    if let Some(ttft) = ttft {
                        stats.ttft_ms.push(ttft);
                    }
                    if let Some(t) = toks {
                        stats.output_tokens.push(t);
                    }
                    if let Some(tps) = tps {
                        stats.tok_per_s.push(tps);
                    }
                }
                None => stats.errors += 1,
            }
            print!(
                "  [{}#{}] {}: ",
                s.cat.label(),
                k,
                if s.user.len() > 32 {
                    &s.user[..32]
                } else {
                    s.user
                }
            );
            print_run(&r);
        }
    }

    println!();
    println!("== summary ==");
    println!(
        "{:<12} {:>9} {:>9} {:>9} {:>9} {:>9} {:>7} verdict",
        "category", "ttft.p50", "ttft.p95", "tot.p50", "tot.p95", "tok/s.p50", "errs"
    );
    let mut seen = std::collections::BTreeSet::new();
    for s in &samples {
        let lab = s.cat.label();
        if !seen.insert(lab) {
            continue;
        }
        let stats = match by_cat.get(lab) {
            Some(v) => v,
            None => continue,
        };
        let (ttft_budget, total_budget) = s.cat.budget();
        let ttft_p50 = pct(&stats.ttft_ms, 0.50);
        let ttft_p95 = pct(&stats.ttft_ms, 0.95);
        let total_p50 = pct(&stats.total_ms, 0.50);
        let total_p95 = pct(&stats.total_ms, 0.95);
        let tps_p50 = pct_f(&stats.tok_per_s, 0.50);
        let pass_ttft = ttft_p95 <= ttft_budget && stats.errors == 0;
        let pass_total = total_budget.is_none_or(|b| total_p95 <= b);
        let verdict = if pass_ttft && pass_total {
            "PASS"
        } else {
            "FAIL"
        };
        println!(
            "{:<12} {:>9} {:>9} {:>9} {:>9} {:>9.1} {:>7} {} (budget ttft<{}ms{})",
            lab,
            ttft_p50,
            ttft_p95,
            total_p50,
            total_p95,
            tps_p50,
            stats.errors,
            verdict,
            ttft_budget,
            total_budget.map_or(String::new(), |b| format!(" total<{b}ms")),
        );
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn run_one(
    itx: &mpsc::Sender<InferenceRequest>,
    log: &parish_inference::InferenceLog,
    sample: &Sample,
    model: &str,
    id: u64,
    _label: &str,
    force_no_json: bool,
    use_schema: bool,
) -> Result<Option<RunResult>, Box<dyn std::error::Error>> {
    let (rtx, rrx) = oneshot::channel();
    let (ttx, mut trx) = mpsc::channel::<String>(64);
    let drain = tokio::spawn(async move { while trx.recv().await.is_some() {} });
    let start = Instant::now();
    let json_schema = if use_schema && !force_no_json {
        sample.schema.map(|(name, schema_str)| JsonSchemaSpec {
            name: name.to_string(),
            schema: serde_json::from_str(schema_str).expect("static schema parses"),
        })
    } else {
        None
    };
    itx.send(InferenceRequest {
        id,
        model: model.to_string(),
        prompt: sample.user.to_string(),
        system: sample.system.map(String::from),
        token_tx: Some(ttx),
        response_tx: rtx,
        max_tokens: sample.max_tokens,
        temperature: None,
        frequency_penalty: None,
        priority: InferencePriority::Interactive,
        // schema wins over json_mode in the worker; setting json_mode true
        // when schema is also Some is harmless.
        json_mode: sample.json_mode && !force_no_json && json_schema.is_none(),
        json_schema,
        cancel: None,
        deferred_audit: None,
    })
    .await?;
    let resp = rrx.await?;
    drain.await.ok();
    let total = start.elapsed().as_millis() as u64;
    if let Some(err) = resp.error.as_deref() {
        eprintln!("    error: {}", err);
        return Ok(None);
    }
    let g = log.lock().await;
    let entry = g.iter().rev().find(|e| e.request_id == id);
    let (ttft, toks, tps) = entry
        .map(|e| {
            let tps = match (e.ttft_ms, e.output_tokens) {
                (Some(t), Some(n)) if e.duration_ms > t => {
                    Some(n as f64 / ((e.duration_ms - t) as f64 / 1000.0))
                }
                _ => None,
            };
            (e.ttft_ms, e.output_tokens, tps)
        })
        .unwrap_or((None, None, None));
    Ok(Some((ttft, total, toks, tps)))
}

fn print_run(r: &Option<RunResult>) {
    match r {
        Some((ttft, total, toks, tps)) => {
            print!("total={}ms", total);
            if let Some(t) = ttft {
                print!(" ttft={}ms", t);
            }
            if let Some(n) = toks {
                print!(" toks={}", n);
            }
            if let Some(s) = tps {
                print!(" tok/s={:.1}", s);
            }
            println!();
        }
        None => println!("ERROR"),
    }
}
