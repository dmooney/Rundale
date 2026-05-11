//! Live spike for `VllmMlxProcess::ensure_running`.
//!
//! Spawns a vllm-mlx server via the new helper, fires one request,
//! and exits (Drop kills the server). Run manually with:
//!
//! ```sh
//! cargo run -p parish-inference --release --example vllm_mlx_spawn
//! ```
//!
//! Verifies the auto-launch path end-to-end without needing the full
//! `setup_provider_client` integration.

use parish_inference::setup::VllmMlxProcess;
use std::time::Instant;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let base_url = "http://localhost:8000/v1";
    let model = "mlx-community/gemma-3-4b-it-4bit";

    println!("starting vllm-mlx for {model}...");
    let t0 = Instant::now();
    let mut proc = VllmMlxProcess::ensure_running(base_url, model).await?;
    println!(
        "ready in {} ms (started_by_us={})",
        t0.elapsed().as_millis(),
        proc.was_started_by_us()
    );

    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": "hi"}],
        "stream": false,
        "max_tokens": 5
    });
    let t1 = Instant::now();
    let resp = client
        .post(format!("{base_url}/chat/completions"))
        .json(&body)
        .send()
        .await?;
    let status = resp.status();
    let text = resp.text().await?;
    println!(
        "first request: {} in {} ms",
        status,
        t1.elapsed().as_millis()
    );
    println!("body (first 200ch): {}", &text[..text.len().min(200)]);

    proc.stop();
    println!("stopped");
    Ok(())
}
