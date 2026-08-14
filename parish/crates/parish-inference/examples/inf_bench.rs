//! Per-category inference benchmark.
//!
//! Fires representative prompts for each Rundale inference category
//! (Intent, Reaction, Simulation, Dialogue) through the real
//! `InferenceQueue` worker against OpenAI-compatible or native Google
//! Interactions endpoints. Reports provider usage, cache ratio, ttft / tok/s /
//! total latency and PASS/FAIL
//! against the per-category latency budgets.
//!
//! Usage:
//!   cargo run -p parish-inference --release --example inf_bench -- \
//!       --base-url http://localhost:11434 \
//!       --intent-model gemma4:e4b \
//!       --main-model  gemma4:31b \
//!       [--api-key ...]
//!
//! The harness discards five warmup calls per production subrole before
//! measurement. Cold-load is reported separately from steady-state work.
//!
//! Budgets (ttft / total p95):
//!   Intent      < 1500 ms total
//!   Reaction    < 3000 ms total
//!   Simulation  reported against the configured provider timeout
//!   Dialogue    < 1500 ms TTFT / streaming (no total cap)

use std::time::Instant;

use parish_config::InferenceConfig;
use parish_inference::openai_client::OpenAiClient;
use parish_inference::{
    AnyClient, GenerateParams, GoogleClient, InferencePriority, InferenceRequest,
    InferenceWorkerConfig, JsonSchemaSpec, new_inference_log, spawn_inference_worker,
};
use tokio::sync::{mpsc, oneshot};

/// (ttft_ms, total_ms, output_tokens, tok/s) for a single call. `None` is returned
/// from [`run_one`] when the call errored.
type RunResult = (Option<u64>, u64, Option<u64>, Option<f64>, u64, u64, u64);

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
            Category::Intent => (1500, Some(1500)),
            Category::Reaction => (3000, Some(3000)),
            Category::Simulation => (5000, None),
            Category::Dialogue => (1500, None),
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
    subrole: parish_config::InferenceSubrole,
    streaming: bool,
    system: Option<String>,
    user: String,
    json_mode: bool,
    /// Strict JSON-schema constraint for this sample. When `Some` and
    /// `--schema` is on the CLI, the bench sends `response_format:
    /// json_schema` rather than `json_object` — required for vllm-mlx
    /// and LM Studio to accept structured output.
    schema: Option<(&'static str, &'static str)>,
    /// Per-sample `max_tokens` cap. Mirrors what production code passes
    /// to the inference client for this category:
    ///
    ///   - Intent: 256; Reaction: 1,024.
    ///   - Tier 2 Simulation: 2,048.
    ///   - Tier 3 Simulation / Dialogue: 4,096.
    max_tokens: Option<u32>,
    /// Expected parser result for intent cases. Keeping this beside the
    /// prompt makes the expanded calibration corpus mechanically auditable.
    expected_intent: Option<&'static str>,
}

impl Sample {
    fn label(&self) -> &'static str {
        match self.subrole {
            parish_config::InferenceSubrole::ArrivalReaction => "Arrival Reaction",
            parish_config::InferenceSubrole::MessageReaction => "Message Reaction",
            parish_config::InferenceSubrole::TravelEncounter => "Travel Encounter",
            parish_config::InferenceSubrole::Tier2Simulation => "Tier2 Sim",
            parish_config::InferenceSubrole::Tier3Simulation => "Tier3 Sim",
            _ => self.cat.label(),
        }
    }

    fn budget(&self) -> (u64, Option<u64>) {
        match (self.cat, self.max_tokens) {
            (Category::Simulation, Some(2_048)) => (5_000, Some(10_000)),
            // Batch simulation is intentionally deep background work. Its
            // acceptance is bounded completion and non-interference, not an
            // interactive TTFT target.
            (Category::Simulation, _) => (30_000, Some(60_000)),
            _ => self.cat.budget(),
        }
    }
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

    let mut out = Vec::with_capacity(50);
    let intent_cases = [
        ("go to the pub", "move"),
        ("tell Padraig I saw his cow wandering near the bog", "talk"),
        ("look around", "look"),
        ("walk to the church", "move"),
        ("ask Niamh whether the road is flooded", "talk"),
        ("examine the carved stone", "examine"),
        ("pick up the fallen branch", "interact"),
        ("I came through Kilteevan yesterday", "talk"),
        ("head back to the crossroads", "move"),
        ("what can I see from here?", "look"),
    ];
    for (user, expected_intent) in intent_cases {
        out.push(Sample {
            cat: Category::Intent,
            subrole: parish_config::InferenceSubrole::Intent,
            streaming: false,
            system: Some(intent_sys.to_string()),
            user: user.to_string(),
            json_mode: true,
            schema: Some(("intent", INTENT_SCHEMA)),
            max_tokens: Some(256),
            expected_intent: Some(expected_intent),
        });
    }

    let reaction_cases = [
        (
            parish_config::InferenceSubrole::ArrivalReaction,
            true,
            "A newcomer has just arrived at Darcy's Pub. It is evening, Clear.\nYou have not met this person before. You are working here as the Publican. Introduce yourself briefly.",
        ),
        (
            parish_config::InferenceSubrole::ArrivalReaction,
            true,
            "A familiar neighbour has just arrived at Darcy's Pub at dawn in Light Rain. Welcome them briefly.",
        ),
        (
            parish_config::InferenceSubrole::ArrivalReaction,
            true,
            "A tired traveller enters Darcy's Pub near closing time during a storm. Greet them briefly.",
        ),
        (
            parish_config::InferenceSubrole::ArrivalReaction,
            true,
            "Niamh Darcy arrives at Darcy's Pub on a clear afternoon. You know her well. Greet her briefly.",
        ),
        (
            parish_config::InferenceSubrole::MessageReaction,
            false,
            "A neighbour says the bridge road is flooded after the rain. Reply briefly.",
        ),
        (
            parish_config::InferenceSubrole::MessageReaction,
            false,
            "A stranger asks whether there is a bed available tonight. Reply briefly.",
        ),
        (
            parish_config::InferenceSubrole::MessageReaction,
            false,
            "Niamh says the last turf stack is finally covered. Reply briefly.",
        ),
        (
            parish_config::InferenceSubrole::TravelEncounter,
            false,
            "A traveller walks from Kilteevan Village to the crossroads at dusk in light rain. A farmer and cart pass on the lane.",
        ),
        (
            parish_config::InferenceSubrole::TravelEncounter,
            false,
            "A traveller follows the bog road at dawn under clear skies. Curlews rise from the heather.",
        ),
        (
            parish_config::InferenceSubrole::TravelEncounter,
            false,
            "A traveller approaches the church at noon in a stiff spring wind. The bell rope knocks inside.",
        ),
    ];
    for (subrole, streaming, user) in reaction_cases {
        let system = if subrole == parish_config::InferenceSubrole::TravelEncounter {
            "You write one grounded, atmospheric travel observation for rural Ireland in 1820. Return one sentence only."
        } else {
            reaction_sys
        };
        out.push(Sample {
            cat: Category::Reaction,
            subrole,
            streaming,
            system: Some(system.to_string()),
            user: user.to_string(),
            json_mode: false,
            schema: None,
            max_tokens: Some(1_024),
            expected_intent: None,
        });
    }

    for case in 0..10 {
        out.push(Sample {
            cat: Category::Simulation,
            subrole: parish_config::InferenceSubrole::Tier2Simulation,
            streaming: true,
            system: None,
            user: format!("{sim_user}\n\nCalibration context: observation window {} of the evening; preserve the stated IDs and output contract.", case + 1),
            json_mode: true,
            schema: Some(("tier2_simulation", SIM_SCHEMA)),
            max_tokens: Some(2_048),
            expected_intent: None,
        });
    }
    for case in 0..10 {
        out.push(Sample {
            cat: Category::Simulation,
            subrole: parish_config::InferenceSubrole::Tier3Simulation,
            streaming: true,
            system: None,
            user: format!("{tier3_user}\n\nCalibration context: canonical six-hour window {}. Preserve every listed NPC ID exactly once.", case + 1),
            json_mode: true,
            schema: Some(("tier3_batch", TIER3_SCHEMA)),
            max_tokens: Some(4_096),
            expected_intent: None,
        });
    }

    let dialogue_cases = [
        "I've been having trouble sleeping. The dreams keep coming back.",
        "What do you know about the old Cailleach near the fairy fort?",
        "My ankle has swollen since I crossed the bog yesterday.",
        "Is there truth in the warning about the western road after dark?",
        "Niamh says the fever has returned in the next parish.",
        "Which herbs would you gather for a stubborn winter cough?",
        "I heard someone singing by the ruined cottage last night.",
        "Father Cathal thinks I should leave the old well alone.",
        "Can you tell me why Padraig will not speak of the harvest?",
        "I may have to travel before the weather turns. What would you advise?",
    ];
    for user in dialogue_cases {
        out.push(Sample {
            cat: Category::Dialogue,
            subrole: parish_config::InferenceSubrole::Dialogue,
            streaming: true,
            system: Some(dialogue_sys.to_string()),
            user: user.to_string(),
            json_mode: false,
            schema: None,
            max_tokens: Some(4_096),
            expected_intent: None,
        });
    }
    debug_assert_eq!(out.len(), 50);
    out
}

#[derive(Default)]
struct CatStats {
    ttft_ms: Vec<u64>,
    total_ms: Vec<u64>,
    tok_per_s: Vec<f64>,
    output_tokens: Vec<u64>,
    input_tokens: u64,
    cached_tokens: u64,
    thought_tokens: u64,
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
    // Match every production entry point: a repository-local `.env` is a
    // supported key source and must not be misreported as absent.
    dotenvy::dotenv().ok();
    let mut base_url = "http://localhost:11434".to_string();
    let mut intent_model = "gemma4:e4b".to_string();
    let mut main_model = "gemma4:e4b".to_string();
    let mut api_key: Option<String> = None;
    let mut provider = "openai".to_string();
    let mut iters = 30usize;
    let mut warmup = true;
    // LM Studio rejects `response_format: {"type": "json_object"}` (it accepts
    // only "text" or "json_schema"). Pass --no-json-mode to drop the field
    // entirely and rely on prompt-only structured output.
    let mut force_no_json = false;
    // When set, send `response_format: json_schema` instead of `json_object`
    // for samples that ship a schema (Intent + Simulation). Required for
    // strict servers (vllm-mlx, LM Studio).
    let mut use_schema = false;
    let mut cache_probe = false;
    let mut thinking_override: Option<parish_config::ThinkingLevel> = None;
    let mut only_subrole: Option<String> = None;

    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--base-url" => base_url = args.next().expect("--base-url value"),
            "--provider" => provider = args.next().expect("--provider value"),
            "--intent-model" => intent_model = args.next().expect("--intent-model value"),
            "--main-model" => main_model = args.next().expect("--main-model value"),
            "--api-key" => api_key = args.next(),
            "--iters" => iters = args.next().expect("--iters").parse()?,
            "--no-warmup" => warmup = false,
            "--no-json-mode" => force_no_json = true,
            "--schema" => use_schema = true,
            "--thinking-level" => {
                thinking_override = Some(
                    match args.next().expect("--thinking-level value").as_str() {
                        "minimal" => parish_config::ThinkingLevel::Minimal,
                        "low" => parish_config::ThinkingLevel::Low,
                        "medium" => parish_config::ThinkingLevel::Medium,
                        "high" => parish_config::ThinkingLevel::High,
                        value => return Err(format!("unsupported thinking level: {value}").into()),
                    },
                )
            }
            "--only" => only_subrole = args.next(),
            "--cache-probe" => cache_probe = true,
            "-h" | "--help" => {
                println!(
                    "Usage: inf_bench [--base-url URL] [--provider openai|google] [--intent-model TAG] \
                     [--main-model TAG] [--api-key KEY] [--iters N] \
                     [--no-warmup] [--no-json-mode] [--schema] [--cache-probe]"
                );
                return Ok(());
            }
            other => return Err(format!("unknown arg: {other}").into()),
        }
    }

    println!("base_url     = {}", base_url);
    println!("provider     = {}", provider);
    println!("intent_model = {}", intent_model);
    println!("main_model   = {}", main_model);
    println!("iters        = {} (per sample)", iters);
    println!("warmup       = {}", warmup);
    println!();

    let cfg = InferenceConfig::default();
    let env_google_key = std::env::var("GOOGLE_API_KEY").ok();
    let client = match provider.as_str() {
        "google" => AnyClient::google(GoogleClient::new_with_config(
            &base_url,
            api_key.as_deref().or(env_google_key.as_deref()),
            &cfg,
        )),
        "openai" => AnyClient::open_ai(OpenAiClient::new_with_config(
            &base_url,
            api_key.as_deref().or(env_google_key.as_deref()),
            &cfg,
        )),
        other => return Err(format!("unsupported provider: {other}").into()),
    };
    if cache_probe {
        if provider != "google" {
            return Err("--cache-probe requires --provider google".into());
        }
        run_cache_probe(&client, &main_model).await?;
        return Ok(());
    }

    let (itx, irx) = mpsc::channel::<InferenceRequest>(8);
    let (btx, brx) = mpsc::channel::<InferenceRequest>(8);
    let (xtx, xrx) = mpsc::channel::<InferenceRequest>(8);
    let log = new_inference_log();
    let _h = spawn_inference_worker(
        client,
        InferenceWorkerConfig {
            interactive_rx: irx,
            background_rx: brx,
            batch_rx: xrx,
            log: log.clone(),
            file_log: parish_inference::file_log::InferenceFileLog::disabled(),
            provider: parish_config::Provider::from_str_loose(&provider).unwrap_or_default(),
            timeout_config: cfg,
        },
    );

    let mut samples = samples();
    if let Some(only) = only_subrole.as_deref() {
        samples.retain(|sample| match only {
            "intent" => sample.subrole == parish_config::InferenceSubrole::Intent,
            "arrival" => sample.subrole == parish_config::InferenceSubrole::ArrivalReaction,
            "message" => sample.subrole == parish_config::InferenceSubrole::MessageReaction,
            "travel" => sample.subrole == parish_config::InferenceSubrole::TravelEncounter,
            "tier2" => sample.subrole == parish_config::InferenceSubrole::Tier2Simulation,
            "tier3" => sample.subrole == parish_config::InferenceSubrole::Tier3Simulation,
            "dialogue" => sample.subrole == parish_config::InferenceSubrole::Dialogue,
            _ => false,
        });
        if samples.is_empty() {
            return Err(format!("--only did not match a production subrole: {only}").into());
        }
    }

    if warmup {
        println!("== warmup (discarded) ==");
        let mut warmed_subroles = std::collections::BTreeSet::new();
        for (sample_index, sample) in samples.iter().enumerate() {
            if !warmed_subroles.insert(sample.label()) {
                continue;
            }
            for warmup_round in 0..5 {
                let model = if matches!(sample.cat, Category::Intent) {
                    &intent_model
                } else {
                    &main_model
                };
                run_one(
                    &itx,
                    &btx,
                    &xtx,
                    &log,
                    sample,
                    model,
                    10_000 + warmup_round * 10 + sample_index as u64,
                    "warmup",
                    force_no_json,
                    use_schema,
                    thinking_override,
                )
                .await?;
            }
        }
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
                &btx,
                &xtx,
                &log,
                s,
                model,
                100_000 + (i * iters + k) as u64,
                "",
                force_no_json,
                use_schema,
                thinking_override,
            )
            .await?;
            let stats = by_cat.entry(s.label()).or_default();
            match r {
                Some((ttft, total, toks, tps, input, cached, thought)) => {
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
                    stats.input_tokens += input;
                    stats.cached_tokens += cached;
                    stats.thought_tokens += thought;
                }
                None => stats.errors += 1,
            }
            print!(
                "  [{}#{}] {}: ",
                s.label(),
                k,
                if s.user.len() > 32 {
                    &s.user[..32]
                } else {
                    &s.user
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
    let mut all_pass = true;
    for s in &samples {
        let lab = s.label();
        if !seen.insert(lab) {
            continue;
        }
        let stats = match by_cat.get(lab) {
            Some(v) => v,
            None => continue,
        };
        let (ttft_budget, total_budget) = s.budget();
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
            all_pass = false;
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
        let cache_ratio = if stats.input_tokens == 0 {
            0.0
        } else {
            stats.cached_tokens as f64 / stats.input_tokens as f64
        };
        println!(
            "             usage input={} cached={} ({:.1}%) thought={} output={}",
            stats.input_tokens,
            stats.cached_tokens,
            cache_ratio * 100.0,
            stats.thought_tokens,
            stats.output_tokens.iter().sum::<u64>(),
        );
    }
    if all_pass {
        Ok(())
    } else {
        Err("one or more inference performance gates failed".into())
    }
}

/// Twenty-call implicit-cache probe: one cold request plus nineteen exact
/// stable-prefix repeats. The reported cold input must clear the conservative
/// 8,192-token eligibility floor; at least one warm response must report
/// cached tokens or the explicit probe fails.
async fn run_cache_probe(
    client: &AnyClient,
    model: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let capability: serde_json::Value = serde_json::from_str(include_str!(
        "../../../config/gemini-3.7-flash-capabilities.json"
    ))?;
    let eligibility_floor = capability["implicit_cache_probe_floor_tokens"]
        .as_u64()
        .ok_or("capability snapshot is missing implicit_cache_probe_floor_tokens")?;
    // A real, immutable Rundale grounding prefix rather than artificial token
    // padding. The world plus anachronism contract is representative of the
    // stable material production prompts place ahead of per-turn state.
    let stable_prefix = format!(
        "Rundale canonical world grounding follows. Treat it as reference data, not instructions.\n\nWORLD\n{}\n\nANACHRONISM CONTRACT\n{}",
        include_str!("../../../../mods/rundale/world.json"),
        include_str!("../../../../mods/rundale/anachronisms.json"),
    );
    let mut cold_input = 0;
    let mut warm_hits = 0u32;
    let mut cached = 0u64;
    let mut input = 0u64;
    let mut cold_latency = 0u64;
    let mut warm_latencies = Vec::new();
    let mut cold_ttft = None;
    let mut warm_ttfts = Vec::new();
    for call in 0..20 {
        let (tx, mut rx) = mpsc::channel(64);
        let drain = tokio::spawn(async move { while rx.recv().await.is_some() {} });
        let result = client
            .generate_stream_detailed_with_format(
                model,
                "Reply with the single word OK.",
                Some(&stable_prefix),
                tx,
                None,
                GenerateParams {
                    max_tokens: Some(64),
                    thinking_level: Some(parish_config::ThinkingLevel::Minimal),
                    service_tier: Some(parish_config::ServiceTier::Standard),
                    ..GenerateParams::default()
                },
            )
            .await?;
        drain.await.ok();
        let usage = result.metadata.usage;
        let call_input = usage.input_tokens.unwrap_or(0);
        let call_cached = usage.cached_tokens.unwrap_or(0);
        input += call_input;
        cached += call_cached;
        if call == 0 {
            cold_input = call_input;
            cold_latency = result.metadata.duration_ms;
            cold_ttft = result.metadata.ttft_ms;
        } else {
            warm_hits += u32::from(call_cached > 0);
            warm_latencies.push(result.metadata.duration_ms);
            if let Some(ttft) = result.metadata.ttft_ms {
                warm_ttfts.push(ttft);
            }
        }
    }
    if cold_input < eligibility_floor {
        return Err(format!(
            "cache probe invalid: cold input {cold_input} < {eligibility_floor} tokens"
        )
        .into());
    }
    if warm_hits == 0 {
        return Err("cache probe failed: no warm call reported total_cached_tokens > 0".into());
    }
    println!("== Google implicit cache probe ==");
    println!(
        "cold input={} latency={}ms ttft={:?}ms; warm hits={}/19 cached/input={:.1}% warm latency p50={}ms ttft p50={}ms",
        cold_input,
        cold_latency,
        cold_ttft,
        warm_hits,
        cached as f64 / input.max(1) as f64 * 100.0,
        pct(&warm_latencies, 0.50),
        pct(&warm_ttfts, 0.50),
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn run_one(
    itx: &mpsc::Sender<InferenceRequest>,
    btx: &mpsc::Sender<InferenceRequest>,
    xtx: &mpsc::Sender<InferenceRequest>,
    log: &parish_inference::InferenceLog,
    sample: &Sample,
    model: &str,
    id: u64,
    _label: &str,
    force_no_json: bool,
    use_schema: bool,
    thinking_override: Option<parish_config::ThinkingLevel>,
) -> Result<Option<RunResult>, Box<dyn std::error::Error>> {
    let (rtx, rrx) = oneshot::channel();
    let (ttx, mut trx) = mpsc::channel::<String>(64);
    let drain = sample
        .streaming
        .then(|| tokio::spawn(async move { while trx.recv().await.is_some() {} }));
    let start = Instant::now();
    let json_schema = if use_schema && !force_no_json {
        sample.schema.map(|(name, schema_str)| JsonSchemaSpec {
            name: name.to_string(),
            schema: serde_json::from_str(schema_str).expect("static schema parses"),
        })
    } else {
        None
    };
    let tier2 = matches!(sample.cat, Category::Simulation) && sample.max_tokens == Some(2_048);
    let priority = match sample.cat {
        Category::Simulation if tier2 => InferencePriority::Background,
        Category::Simulation => InferencePriority::Batch,
        _ => InferencePriority::Interactive,
    };
    let lane = match priority {
        InferencePriority::Interactive => itx,
        InferencePriority::Background => btx,
        InferencePriority::Batch => xtx,
    };
    lane.send(InferenceRequest {
        id,
        model: model.to_string(),
        prompt: sample.user.to_string(),
        system: sample.system.clone(),
        token_tx: sample.streaming.then_some(ttx),
        response_tx: rtx,
        max_tokens: sample.max_tokens,
        temperature: None,
        frequency_penalty: None,
        enable_thinking: None,
        reasoning_effort: None,
        priority,
        role: match sample.cat {
            Category::Intent => parish_config::InferenceCategory::Intent,
            Category::Reaction => parish_config::InferenceCategory::Reaction,
            Category::Simulation => parish_config::InferenceCategory::Simulation,
            Category::Dialogue => parish_config::InferenceCategory::Dialogue,
        },
        subrole: sample.subrole,
        profile: Some({
            let mut profile = parish_config::InferenceProfile::for_subrole(sample.subrole);
            if let Some(level) = thinking_override {
                profile.thinking_level = level;
            }
            profile
        }),
        // schema wins over json_mode in the worker; setting json_mode true
        // when schema is also Some is harmless.
        json_mode: sample.json_mode && !force_no_json && json_schema.is_none(),
        json_schema,
        cancel: None,
        deferred_audit: None,
    })
    .await?;
    let resp = rrx.await?;
    if let Some(drain) = drain {
        drain.await.ok();
    }
    let total = start.elapsed().as_millis() as u64;
    if let Some(err) = resp.error.as_deref() {
        eprintln!("    error: {}", err);
        return Ok(None);
    }
    if let Err(error) = validate_sample_output(sample, &resp.text) {
        eprintln!("    invalid {} output: {error}", sample.label());
        return Ok(None);
    }
    let g = log.lock().await;
    let entry = g.iter().rev().find(|e| e.request_id == id);
    let (ttft, toks, tps, input, cached, thought) = entry
        .map(|e| {
            let tps = match (e.ttft_ms, e.output_tokens) {
                (Some(t), Some(n)) if e.duration_ms > t => {
                    Some(n as f64 / ((e.duration_ms - t) as f64 / 1000.0))
                }
                _ => None,
            };
            (
                e.ttft_ms,
                e.output_tokens,
                tps,
                e.input_tokens.unwrap_or(0),
                e.cached_tokens.unwrap_or(0),
                e.thought_tokens.unwrap_or(0),
            )
        })
        .unwrap_or((None, None, None, 0, 0, 0));
    Ok(Some((ttft, total, toks, tps, input, cached, thought)))
}

/// Production-faithful structural invariants. A transport-level 2xx is not a
/// successful benchmark sample when the gameplay apply seam would reject it.
fn validate_sample_output(sample: &Sample, text: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("empty model output".to_string());
    }
    let lower = text.to_ascii_lowercase();
    if ["thought_signature", "reasoning:", "chain of thought"]
        .iter()
        .any(|marker| lower.contains(marker))
    {
        return Err("thought/signature content leaked into visible output".to_string());
    }
    let sentence_count = text
        .split(['.', '!', '?'])
        .filter(|part| !part.trim().is_empty())
        .count();
    let sentence_cap = match sample.subrole {
        parish_config::InferenceSubrole::ArrivalReaction
        | parish_config::InferenceSubrole::MessageReaction => Some(2),
        parish_config::InferenceSubrole::TravelEncounter => Some(1),
        parish_config::InferenceSubrole::Dialogue => Some(3),
        _ => None,
    };
    if sentence_cap.is_some_and(|cap| sentence_count > cap) {
        return Err(format!(
            "visible prose has {sentence_count} sentences, above the {sentence_cap:?} contract"
        ));
    }
    if matches!(
        sample.subrole,
        parish_config::InferenceSubrole::ArrivalReaction
            | parish_config::InferenceSubrole::MessageReaction
            | parish_config::InferenceSubrole::TravelEncounter
            | parish_config::InferenceSubrole::Dialogue
    ) && ["smartphone", "internet", "okay", "website", "email"]
        .iter()
        .any(|word| lower.split_whitespace().any(|token| token.contains(word)))
    {
        return Err("anachronistic language in period prose".to_string());
    }
    if !(sample.json_mode || sample.schema.is_some()) {
        return Ok(());
    }
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|error| format!("malformed JSON: {error}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "top-level response is not an object".to_string())?;
    match (sample.cat, sample.max_tokens) {
        (Category::Intent, _) => {
            let intent = object
                .get("intent")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| "intent is missing or not a string".to_string())?;
            if !["move", "talk", "look", "interact", "examine", "unknown"].contains(&intent) {
                return Err(format!("unsupported intent {intent:?}"));
            }
            let expected = sample
                .expected_intent
                .ok_or_else(|| "intent calibration case has no expected result".to_string())?;
            if intent != expected {
                return Err(format!(
                    "semantic intent mismatch: expected {expected}, got {intent}"
                ));
            }
        }
        (Category::Simulation, Some(2_048)) => {
            if !object
                .get("summary")
                .is_some_and(serde_json::Value::is_string)
            {
                return Err("Tier 2 summary is missing or not a string".to_string());
            }
            if object["summary"]
                .as_str()
                .unwrap_or_default()
                .split_whitespace()
                .count()
                > 20
            {
                return Err("Tier 2 summary exceeds the production 20-word contract".to_string());
            }
            for field in ["mood_changes", "relationship_changes"] {
                if !object.get(field).is_some_and(serde_json::Value::is_array) {
                    return Err(format!("Tier 2 {field} is missing or not an array"));
                }
            }
        }
        (Category::Simulation, _) => {
            if !object
                .get("updates")
                .is_some_and(serde_json::Value::is_array)
            {
                return Err("Tier 3 updates is missing or not an array".to_string());
            }
            let updates = object["updates"].as_array().expect("checked above");
            let ids: std::collections::BTreeSet<u64> = updates
                .iter()
                .filter_map(|update| update.get("npc_id").and_then(serde_json::Value::as_u64))
                .collect();
            if ids != std::collections::BTreeSet::from([1, 2, 3, 4, 5, 6]) {
                return Err(format!(
                    "Tier 3 must return exactly NPC ids 1..6; got {ids:?}"
                ));
            }
        }
        _ => {}
    }
    Ok(())
}

fn print_run(r: &Option<RunResult>) {
    match r {
        Some((ttft, total, toks, tps, input, cached, thought)) => {
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
            print!(" input={} cached={} thought={}", input, cached, thought);
            println!();
        }
        None => println!("ERROR"),
    }
}
