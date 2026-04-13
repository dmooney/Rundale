//! Built-in inference simulator — a "GPT-0" text generator for offline testing.
//!
//! Generates funny Irish-village nonsense via a bigram Markov chain trained on
//! an embedded corpus. No network, no GPU, no model download — works anywhere.
//!
//! Activate with `PARISH_PROVIDER=simulator` or `/provider simulator`.

use std::collections::HashMap;

use serde::de::DeserializeOwned;
use tokio::sync::mpsc;

use parish_types::ParishError;

// ---------------------------------------------------------------------------
// Embedded corpus — the training data for our "GPT-0" model
// ---------------------------------------------------------------------------

const CORPUS: &str = "\
Ah sure what harm would that do ya now the planning application was rejected again \
himself from the council says the drainage situation is fierce altogether Father Clancy \
was spotted beyond at the crossroads arguing with the postman about the hedge which is \
no surprise at all the Caherciveen hurling team had a desperate outing at the weekend \
and no mistake herself says the weather will turn before Friday young Séamus is after \
buying a second tractor which has opinions divided along the townland the cows broke out \
beyond the back field again Pat Morrissey was in the pub saying things that would make \
the saints blush the planning board has spoken and nobody is happy which is generally \
how you know justice was done the post office is closing early on account of something \
nobody can quite explain Bridget from beyond the hill has opinions on the matter that \
could peel paint sure God help us all the matchmaker was seen in town which has set \
tongues wagging in the usual direction the parish newsletter has caused ructions again \
this month and no mistake himself is in rare form today and not in the good way the \
tractor grant scheme is under fierce scrutiny and rightly so says herself though himself \
has his doubts would you credit it at all the parish priest is after announcing a new \
collection for the roof tiles which everyone agrees is very necessary even if the timing \
is suspicious the bridge at Ballymullen has a fierce wobble in it since the lorry went \
through and nothing has been done about it the hurling final is a sore subject this year \
and best not mentioned in company the planning permission for the new shed is still in \
appeal which tells you everything you need to know about the county council the cat from \
number four has been up to no good again and Missus Hennessy is not best pleased old \
Tadhg says he knew this would happen on account of the moon which may or may not be \
relevant the Christmas lights on the bridge are still up and it is well past the time \
for them but nobody wants to be the one to climb up there the road to the lake has a \
fierce pothole that has swallowed two cars already this winter and nothing done about it \
the drama society production caused quite the stir and opinions are still divided sure \
you would not want to miss it for the world or maybe you would it depends entirely on \
your feelings about the accordion the agricultural show committee has had a falling out \
again over the best-in-show rosette and honestly nobody is surprised the new café has \
very strong opinions about sourdough which has gone down about as well as you might \
expect beyond the crossroads young Tomás from the hill road has been asking peculiar \
questions about the drainage board and we are all wondering what he knows that we do not \
the parish hall roof needs attention but so does everything in this parish at this \
particular point in time and there is only so much money and so many volunteers herself \
from the post office says there is word of a development up beyond the old McCarthy \
land and that has set the whole parish talking for better or worse";

// ---------------------------------------------------------------------------
// FNV-1a hash — deterministic seed from prompt text, no dependencies
// ---------------------------------------------------------------------------

fn fnv1a(s: &str) -> u64 {
    let mut hash: u64 = 14_695_981_039_346_656_037;
    for byte in s.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(1_099_511_628_211);
    }
    hash
}

// ---------------------------------------------------------------------------
// Knuth multiplicative LCG — fast, tiny, good enough for word selection
// ---------------------------------------------------------------------------

struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        // Mix the seed so that 0 and 1 don't produce degenerate sequences
        Self {
            state: seed ^ 0xdeadbeef_cafebabe,
        }
    }

    fn next(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.state
    }

    fn pick(&mut self, len: usize) -> usize {
        if len == 0 {
            return 0;
        }
        (self.next() as usize) % len
    }
}

// ---------------------------------------------------------------------------
// Bigram Markov chain
// ---------------------------------------------------------------------------

/// Word-level bigram chain: given a word, lists words that follow it in the corpus.
fn build_chain(corpus: &str) -> HashMap<String, Vec<String>> {
    let words: Vec<&str> = corpus.split_whitespace().collect();
    let mut chain: HashMap<String, Vec<String>> = HashMap::new();

    for window in words.windows(2) {
        chain
            .entry(window[0].to_lowercase())
            .or_default()
            .push(window[1].to_lowercase());
    }
    chain
}

/// Walk the chain starting from a random word, generating `target_words` words.
fn walk_chain(chain: &HashMap<String, Vec<String>>, seed: u64, target_words: usize) -> String {
    let mut rng = Lcg::new(seed);
    let all_keys: Vec<&str> = chain.keys().map(|s| s.as_str()).collect();

    // Pick a starting word
    let start = all_keys[rng.pick(all_keys.len())];
    let mut words = Vec::with_capacity(target_words);
    let mut current = start.to_string();

    for _ in 0..target_words {
        words.push(current.clone());
        match chain.get(&current) {
            Some(nexts) if !nexts.is_empty() => {
                current = nexts[rng.pick(nexts.len())].clone();
            }
            _ => {
                // Dead end — jump to a random key
                current = all_keys[rng.pick(all_keys.len())].to_string();
            }
        }
    }

    // Capitalise the first character
    let mut text = words.join(" ");
    if let Some(first) = text.get_mut(..1) {
        first.make_ascii_uppercase();
    }
    // Add a terminal flourish
    let terminals = [
        ".",
        ". Sure.",
        ". God help us.",
        ". And that's all I'll say.",
        ".",
    ];
    let t = terminals[rng.pick(terminals.len())];
    text.push_str(t);
    text
}

// ---------------------------------------------------------------------------
// Target length helpers (vary by apparent category)
// ---------------------------------------------------------------------------

fn target_length(system: Option<&str>) -> usize {
    match system {
        Some(s) if s.contains("input parser") => 6,
        Some(s) if s.contains("simulation") || s.contains("activity") => 12,
        Some(s) if s.contains("reaction") || s.contains("arrival") => 8,
        _ => 22, // dialogue — more verbose
    }
}

// ---------------------------------------------------------------------------
// JSON synthesis for generate_json
// ---------------------------------------------------------------------------

/// Tiny keyword-based intent detector (mirrors parse_intent_local logic).
///
/// Returns a JSON string compatible with `IntentResponse`.
fn intent_json_for(prompt: &str) -> String {
    let lower = prompt.trim().to_lowercase();

    // Movement
    let move_words = ["go", "walk", "head", "move", "travel", "run", "wander"];
    for mw in move_words {
        if lower.starts_with(mw) {
            let target = lower
                .trim_start_matches(mw)
                .trim_start_matches(|c: char| !c.is_alphanumeric())
                .trim_start_matches("to")
                .trim_start_matches(|c: char| !c.is_alphanumeric())
                .to_string();
            let target = if target.is_empty() {
                "null".to_string()
            } else {
                format!("\"{}\"", target.replace('"', "\\\""))
            };
            return format!(r#"{{"intent":"move","target":{},"dialogue":null}}"#, target);
        }
    }

    // Look
    if lower.starts_with("look") || lower == "l" || lower.starts_with("examine") {
        return r#"{"intent":"look","target":null,"dialogue":null}"#.to_string();
    }

    // Talk / dialogue
    let talk_words = [
        "talk", "say", "tell", "ask", "greet", "hello", "hi", "hiya", "howya",
    ];
    for tw in talk_words {
        if lower.starts_with(tw) {
            return format!(
                r#"{{"intent":"talk","target":null,"dialogue":"{}"}}"#,
                prompt.trim().replace('"', "\\\"")
            );
        }
    }

    // Unknown
    r#"{"intent":"unknown","target":null,"dialogue":null}"#.to_string()
}

// ---------------------------------------------------------------------------
// SimulatorClient
// ---------------------------------------------------------------------------

/// A zero-dependency LLM simulator for offline testing.
///
/// Generates plausible-looking Irish village nonsense via a bigram Markov
/// chain. Responses are deterministic: the same prompt always yields the
/// same output.
pub struct SimulatorClient {
    chain: HashMap<String, Vec<String>>,
}

impl SimulatorClient {
    /// Creates a new simulator, training the Markov chain from the embedded corpus.
    pub fn new() -> Self {
        Self {
            chain: build_chain(CORPUS),
        }
    }

    /// Synchronous text generation — same output as `generate` but without an
    /// async runtime. Useful for test harnesses that run synchronously.
    pub fn generate_sync(&self, prompt: &str, system: Option<&str>) -> String {
        let seed = fnv1a(prompt);
        walk_chain(&self.chain, seed, target_length(system))
    }

    /// Generates a plain-text response.
    ///
    /// Response length varies by category (detected from `system`).
    pub async fn generate(
        &self,
        _model: &str,
        prompt: &str,
        system: Option<&str>,
        _max_tokens: Option<u32>,
        _temperature: Option<f32>,
    ) -> Result<String, ParishError> {
        let seed = fnv1a(prompt);
        let length = target_length(system);
        Ok(walk_chain(&self.chain, seed, length))
    }

    /// Streams the response word-by-word through `token_tx`, then returns the full text.
    ///
    /// Adds per-token delays (~40 ms each) to mimic real LLM streaming so the
    /// loading spinner has time to render and the streaming UI feels natural.
    pub async fn generate_stream(
        &self,
        _model: &str,
        prompt: &str,
        system: Option<&str>,
        token_tx: mpsc::UnboundedSender<String>,
        _max_tokens: Option<u32>,
        _temperature: Option<f32>,
    ) -> Result<String, ParishError> {
        let seed = fnv1a(prompt);
        let length = target_length(system);
        let text = walk_chain(&self.chain, seed, length);

        for word in text.split_whitespace() {
            let chunk = format!("{} ", word);
            // Ignore send errors — receiver may have dropped
            let _ = token_tx.send(chunk);
            // ~40ms per token ≈ 25 tok/s, similar to a fast local model.
            tokio::time::sleep(std::time::Duration::from_millis(40)).await;
        }

        Ok(text)
    }

    /// Generates a JSON-typed response.
    ///
    /// For intent-parsing requests (detected by system prompt keywords), returns
    /// valid `IntentResponse`-shaped JSON using simple keyword matching.
    /// For all other JSON requests, returns a generic object with Markov text
    /// fields that should satisfy `#[serde(default)]` structs.
    pub async fn generate_json<T: DeserializeOwned>(
        &self,
        _model: &str,
        prompt: &str,
        system: Option<&str>,
        _max_tokens: Option<u32>,
        _temperature: Option<f32>,
    ) -> Result<T, ParishError> {
        let json = if system.is_some_and(|s| s.contains("input parser")) {
            intent_json_for(prompt)
        } else {
            let seed = fnv1a(prompt);
            let text = walk_chain(&self.chain, seed, 8);
            let escaped = text.replace('"', "\\\"").replace('\n', " ");
            format!(
                r#"{{"response":"{escaped}","activity":"{escaped}","mood":"neutral","summary":"{escaped}","text":"{escaped}"}}"#,
                escaped = escaped
            )
        };

        serde_json::from_str(&json).map_err(|e| {
            ParishError::Inference(format!("simulator json decode: {e} (raw: {json})"))
        })
    }
}

impl Default for SimulatorClient {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_builds_from_corpus() {
        let chain = build_chain(CORPUS);
        assert!(!chain.is_empty(), "chain should have entries");
        // Corpus starts with "ah" — it should have successors
        assert!(chain.contains_key("ah"), "expected 'ah' in chain");
    }

    #[test]
    fn walk_produces_non_empty_text() {
        let chain = build_chain(CORPUS);
        let text = walk_chain(&chain, 42, 15);
        assert!(!text.is_empty());
        let words: Vec<&str> = text.split_whitespace().collect();
        // May have more words due to terminal punctuation — just check reasonable length
        assert!(words.len() >= 10);
    }

    #[test]
    fn same_prompt_same_output() {
        let chain = build_chain(CORPUS);
        let a = walk_chain(&chain, fnv1a("hello there"), 20);
        let b = walk_chain(&chain, fnv1a("hello there"), 20);
        assert_eq!(a, b, "output should be deterministic");
    }

    #[test]
    fn different_prompts_different_output() {
        let chain = build_chain(CORPUS);
        let a = walk_chain(&chain, fnv1a("prompt one"), 20);
        let b = walk_chain(&chain, fnv1a("prompt two"), 20);
        assert_ne!(a, b, "different prompts should (almost always) differ");
    }

    #[test]
    fn text_starts_with_capital() {
        let chain = build_chain(CORPUS);
        let text = walk_chain(&chain, 99, 10);
        let first = text.chars().next().unwrap();
        assert!(
            first.is_uppercase(),
            "text should start with a capital: {text}"
        );
    }

    #[tokio::test]
    async fn generate_returns_text() {
        let sim = SimulatorClient::new();
        let result = sim
            .generate("sim", "what is happening", None, None, None)
            .await;
        assert!(result.is_ok());
        let text = result.unwrap();
        assert!(!text.is_empty());
    }

    #[tokio::test]
    async fn generate_stream_sends_tokens() {
        let sim = SimulatorClient::new();
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let result = sim
            .generate_stream("sim", "hello there", None, tx, None, None)
            .await;
        assert!(result.is_ok());
        let full = result.unwrap();
        // Collect tokens
        let mut received = String::new();
        while let Ok(tok) = rx.try_recv() {
            received.push_str(&tok);
        }
        // Tokens together should reconstruct the response (modulo trailing spaces)
        assert_eq!(received.trim(), full.trim());
    }

    #[test]
    fn intent_json_move() {
        let json = intent_json_for("go to the pub");
        assert!(json.contains("\"move\""));
        assert!(json.contains("the pub"));
    }

    #[test]
    fn intent_json_look() {
        let json = intent_json_for("look around");
        assert!(json.contains("\"look\""));
    }

    #[test]
    fn intent_json_talk() {
        let json = intent_json_for("say hello to padraig");
        assert!(json.contains("\"talk\""));
    }

    #[test]
    fn intent_json_unknown() {
        let json = intent_json_for("xyzzy");
        assert!(json.contains("\"unknown\""));
    }

    #[tokio::test]
    async fn generate_json_intent_roundtrip() {
        use serde::Deserialize;

        #[derive(Deserialize, Default)]
        struct IntentResponse {
            #[serde(default)]
            intent: Option<String>,
            #[serde(default)]
            target: Option<String>,
            #[serde(default)]
            dialogue: Option<String>,
        }

        let sim = SimulatorClient::new();
        let system = "You are a text adventure input parser. ...";
        let resp: IntentResponse = sim
            .generate_json("sim", "go to the pub", Some(system), None, None)
            .await
            .unwrap();
        assert_eq!(resp.intent.as_deref(), Some("move"));
    }
}
