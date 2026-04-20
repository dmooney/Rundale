//! Demo: retrieval-augmented NPC dialogue.
//!
//! Shows how RAG deepens an NPC's knowledge of their world and their own life.
//! For a chosen NPC and player question the demo prints (a) the baseline
//! system prompt — NPC personality alone, (b) the top-k lore passages
//! retrieved for the question, and (c) the RAG-enhanced prompt. With `--llm`
//! it also calls an OpenAI-compatible chat endpoint for both prompts and
//! prints the responses side by side.
//!
//! Runs fully offline by default (deterministic hashing-trick embedder, no
//! chat call). Pass `--embedder ollama` + `--llm` for a live demonstration.

use clap::{Parser, ValueEnum};
use parish_rag::{
    AnyEmbedder, HashEmbedder, LoreDocument, OllamaEmbedder, build_rundale_corpus,
    format_recall_block,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Copy, ValueEnum)]
enum EmbedderKind {
    /// Deterministic hashing-trick embedder. No network.
    Hash,
    /// Ollama `/api/embeddings`. Needs a running Ollama with an embedding model.
    Ollama,
}

#[derive(Parser, Debug)]
#[command(
    name = "npc_knowledge_demo",
    about = "Demo: RAG-enhanced NPC knowledge over the Rundale mod"
)]
struct Args {
    /// Path to the mod directory containing world.json, npcs.json, festivals.json.
    #[arg(long, default_value = "mods/rundale")]
    mod_dir: PathBuf,

    /// NPC to speak through.
    #[arg(long, default_value = "Padraig Darcy")]
    npc: String,

    /// Embedder backend.
    #[arg(long, value_enum, default_value_t = EmbedderKind::Hash)]
    embedder: EmbedderKind,

    /// Ollama base URL (used by the Ollama embedder and, if --llm, the chat call).
    #[arg(long, default_value = "http://localhost:11434")]
    ollama_url: String,

    /// Embedding model name (for --embedder ollama).
    #[arg(long, default_value = "nomic-embed-text")]
    embed_model: String,

    /// Top-k lore passages to retrieve per query.
    #[arg(long, default_value_t = 4)]
    top_k: usize,

    /// Optional single question instead of the scripted sample set.
    #[arg(long)]
    question: Option<String>,

    /// Actually call the LLM for baseline vs RAG responses.
    /// Without this, only prompts and retrievals are printed.
    #[arg(long, default_value_t = false)]
    llm: bool,

    /// Chat model to use when --llm is set.
    #[arg(long, default_value = "qwen2.5:7b")]
    chat_model: String,

    /// Max tokens for the chat response.
    #[arg(long, default_value_t = 400)]
    max_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct NpcsFile {
    npcs: Vec<NpcEntry>,
}

#[derive(Debug, Deserialize, Clone)]
struct NpcEntry {
    name: String,
    #[serde(default)]
    age: Option<u32>,
    #[serde(default)]
    occupation: Option<String>,
    #[serde(default)]
    personality: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    println!("=== Parish RAG NPC demo ===");
    println!("mod:      {}", args.mod_dir.display());
    println!("npc:      {}", args.npc);
    println!("embedder: {:?}", args.embedder);
    println!("top-k:    {}", args.top_k);
    if args.llm {
        println!("llm:      {} via {}", args.chat_model, args.ollama_url);
    } else {
        println!("llm:      disabled (pass --llm to enable)");
    }
    println!();

    let chunks = build_rundale_corpus(&args.mod_dir).map_err(|e| anyhow::anyhow!(e))?;
    println!(
        "Loaded {} lore chunks from {}.",
        chunks.len(),
        args.mod_dir.display()
    );

    let embedder = match args.embedder {
        EmbedderKind::Hash => AnyEmbedder::Hash(HashEmbedder::default()),
        EmbedderKind::Ollama => {
            AnyEmbedder::Ollama(OllamaEmbedder::new(&args.ollama_url, &args.embed_model))
        }
    };
    let index = embedder
        .index(chunks)
        .await
        .map_err(|e| anyhow::anyhow!("failed to build index: {e}"))?;
    println!("Indexed {} documents.", index.len());
    println!();

    let npcs: NpcsFile = {
        let bytes = std::fs::read(args.mod_dir.join("npcs.json"))?;
        serde_json::from_slice(&bytes)?
    };
    let npc = npcs
        .npcs
        .iter()
        .find(|n| n.name.eq_ignore_ascii_case(&args.npc))
        .cloned()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "NPC '{}' not found in {} — try one of: {}",
                args.npc,
                args.mod_dir.join("npcs.json").display(),
                npcs.npcs
                    .iter()
                    .take(5)
                    .map(|n| n.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;

    let questions: Vec<String> = match args.question {
        Some(q) => vec![q],
        None => default_questions(&npc.name),
    };

    for (i, question) in questions.iter().enumerate() {
        println!("──────────────────────────────────────────────────────────");
        println!("Q{}: {}", i + 1, question);
        println!("──────────────────────────────────────────────────────────");

        let query_vec = embedder
            .embed(question)
            .await
            .map_err(|e| anyhow::anyhow!("failed to embed query: {e}"))?;
        let hits = index.search(&query_vec, args.top_k);

        print_retrievals(&hits);

        let baseline_system = build_baseline_system(&npc);
        let rag_system = build_rag_system(&npc, &hits);

        println!();
        println!("Baseline prompt length: {} chars", baseline_system.len());
        println!("RAG prompt length:      {} chars", rag_system.len());

        if args.llm {
            println!();
            println!("--- Baseline (no RAG) response ---");
            match chat_completion(
                &args.ollama_url,
                &args.chat_model,
                &baseline_system,
                question,
                args.max_tokens,
            )
            .await
            {
                Ok(answer) => println!("{}", answer.trim()),
                Err(e) => println!("[llm error: {e}]"),
            }

            println!();
            println!("--- RAG-enhanced response ---");
            match chat_completion(
                &args.ollama_url,
                &args.chat_model,
                &rag_system,
                question,
                args.max_tokens,
            )
            .await
            {
                Ok(answer) => println!("{}", answer.trim()),
                Err(e) => println!("[llm error: {e}]"),
            }
        }

        println!();
    }

    Ok(())
}

fn build_baseline_system(npc: &NpcEntry) -> String {
    let mut s = format!("You are {}.", npc.name);
    if let Some(age) = npc.age {
        s.push_str(&format!(" You are {age} years old."));
    }
    if let Some(occ) = &npc.occupation {
        s.push_str(&format!(
            " You are a {occ} in the parish of Rundale, Ireland, in 1820."
        ));
    }
    if let Some(p) = &npc.personality {
        s.push_str("\n\n");
        s.push_str(p);
    }
    s.push_str(
        "\n\nRespond in character in 1–3 sentences. Speak plainly. \
         If you do not know something, say so — do not invent names, events, or places.",
    );
    s
}

fn build_rag_system(npc: &NpcEntry, hits: &[(f32, &LoreDocument)]) -> String {
    let mut s = build_baseline_system(npc);
    s.push_str(&format_recall_block(hits));
    s
}

fn print_retrievals(hits: &[(f32, &LoreDocument)]) {
    println!();
    println!("Retrieved {} passage(s):", hits.len());
    for (score, doc) in hits {
        println!(
            "  [{:.3}] {} — {}",
            score,
            doc.source,
            truncate(&doc.content, 140)
        );
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let head: String = s.chars().take(max).collect();
        format!("{head}…")
    }
}

fn default_questions(npc_name: &str) -> Vec<String> {
    vec![
        "Tell me about the crossroads — is there anything strange about it?".to_string(),
        "What is Lughnasa and when is it celebrated?".to_string(),
        "Who is Siobhan Murphy and what does she do?".to_string(),
        format!("What do you worry about these days, {npc_name}?"),
        "Is there anything special about St. Brigid's Church?".to_string(),
    ]
}

#[derive(Serialize)]
struct ChatReq<'a> {
    model: &'a str,
    messages: Vec<ChatMsg<'a>>,
    stream: bool,
    max_tokens: u32,
    temperature: f32,
}

#[derive(Serialize)]
struct ChatMsg<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct ChatResp {
    #[serde(default)]
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    #[serde(default)]
    message: ChatMsgOwned,
}

#[derive(Deserialize, Default)]
struct ChatMsgOwned {
    #[serde(default)]
    content: Option<String>,
}

async fn chat_completion(
    base_url: &str,
    model: &str,
    system: &str,
    user: &str,
    max_tokens: u32,
) -> Result<String, String> {
    let url = format!("{}/v1/chat/completions", base_url.trim_end_matches('/'));
    let body = ChatReq {
        model,
        messages: vec![
            ChatMsg {
                role: "system",
                content: system,
            },
            ChatMsg {
                role: "user",
                content: user,
            },
        ],
        stream: false,
        max_tokens,
        temperature: 0.7,
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| format!("client build failed: {e}"))?;
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("chat request failed: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {status}: {text}"));
    }
    let parsed: ChatResp = resp
        .json()
        .await
        .map_err(|e| format!("chat JSON parse failed: {e}"))?;
    parsed
        .choices
        .into_iter()
        .next()
        .and_then(|c| c.message.content)
        .ok_or_else(|| "empty chat response".to_string())
}
