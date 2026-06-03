//! Scriptable inference mock — a deterministic, caller-controlled LLM stand-in.
//!
//! Unlike [`crate::simulator::SimulatorClient`], which emits uncontrolled Markov
//! nonsense, `MockClient` returns *exactly* the completions a test enqueues. That
//! determinism is the precondition for differential ("shadow") testing of the
//! game-test harness against the real `parish_core::game_loop`: with the LLM
//! boundary pinned to scripted output, both engines become pure functions of
//! `(input, seeded RNG, scripted completions) -> events`, so any divergence is a
//! real behavioral difference rather than model noise.
//!
//! Never opens a socket. Activate via [`crate::AnyClient::mock`] /
//! [`crate::AnyClient::Mock`].

use std::collections::VecDeque;
use std::sync::Mutex;

use serde::de::DeserializeOwned;
use tokio::sync::mpsc;

use parish_types::ParishError;

use crate::simulator::intent_json_for;

/// Returned for plain-text generation when the script queue holds no matching
/// entry. A fixed string keeps the empty-queue path deterministic.
pub const MOCK_FALLBACK_TEXT: &str = "Mock: (no scripted response)";

/// Selects which queued completion answers a given request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MockMatcher {
    /// Matches any request. Use for ordered, position-based scripting.
    Any,
    /// Matches when the prompt *or* system text contains this substring
    /// (case-insensitive). Use to key a completion to the addressed NPC by
    /// name, since the dialogue prompt embeds the NPC persona.
    Contains(String),
}

impl MockMatcher {
    fn matches(&self, prompt: &str, system: Option<&str>) -> bool {
        match self {
            MockMatcher::Any => true,
            MockMatcher::Contains(needle) => {
                let needle = needle.to_lowercase();
                prompt.to_lowercase().contains(&needle)
                    || system.is_some_and(|s| s.to_lowercase().contains(&needle))
            }
        }
    }
}

struct MockEntry {
    matcher: MockMatcher,
    completion: String,
}

/// A deterministic, scriptable LLM client.
///
/// Holds a FIFO queue of `(matcher, completion)` entries. Each generation
/// request consumes the first entry whose matcher matches the request,
/// returning its completion; if none match, the deterministic fallback is
/// returned and the queue is left untouched.
pub struct MockClient {
    queue: Mutex<VecDeque<MockEntry>>,
}

impl MockClient {
    /// Creates an empty mock. Without scripted entries, every request returns
    /// the deterministic fallback.
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
        }
    }

    /// Enqueues a scripted `completion`, served the next time a request matches
    /// `matcher`. Entries are consumed in FIFO order among those that match.
    pub fn push(&self, matcher: MockMatcher, completion: impl Into<String>) {
        self.queue.lock().unwrap().push_back(MockEntry {
            matcher,
            completion: completion.into(),
        });
    }

    /// Convenience: enqueue a completion that matches any request.
    pub fn push_any(&self, completion: impl Into<String>) {
        self.push(MockMatcher::Any, completion);
    }

    /// Convenience: enqueue a completion keyed to requests mentioning `needle`
    /// (typically an addressed NPC's name).
    pub fn push_for(&self, needle: impl Into<String>, completion: impl Into<String>) {
        self.push(MockMatcher::Contains(needle.into()), completion);
    }

    /// Number of scripted entries still queued.
    pub fn pending(&self) -> usize {
        self.queue.lock().unwrap().len()
    }

    /// Consumes and returns the first queued completion whose matcher matches,
    /// or `None` if the queue is empty / nothing matches.
    fn take_match(&self, prompt: &str, system: Option<&str>) -> Option<String> {
        let mut queue = self.queue.lock().unwrap();
        let idx = queue
            .iter()
            .position(|e| e.matcher.matches(prompt, system))?;
        queue.remove(idx).map(|e| e.completion)
    }

    /// The scripted completion for a request, or the deterministic fallback.
    fn completion_for(&self, prompt: &str, system: Option<&str>) -> String {
        self.take_match(prompt, system)
            .unwrap_or_else(|| MOCK_FALLBACK_TEXT.to_string())
    }

    /// Plain-text generation — returns the scripted completion verbatim.
    pub async fn generate(
        &self,
        _model: &str,
        prompt: &str,
        system: Option<&str>,
        _max_tokens: Option<u32>,
        _temperature: Option<f32>,
    ) -> Result<String, ParishError> {
        Ok(self.completion_for(prompt, system))
    }

    /// Streaming generation — emits the scripted completion as whitespace
    /// tokens through `token_tx`, then returns the full text. No artificial
    /// delay (tests want speed); back-pressure is respected.
    pub async fn generate_stream(
        &self,
        _model: &str,
        prompt: &str,
        system: Option<&str>,
        token_tx: mpsc::Sender<String>,
        _max_tokens: Option<u32>,
        _temperature: Option<f32>,
    ) -> Result<String, ParishError> {
        let text = self.completion_for(prompt, system);
        stream_words(&text, &token_tx).await;
        Ok(text)
    }

    /// Streaming JSON generation. Mirrors the dialogue envelope the rest of the
    /// engine expects, with the scripted completion as the `dialogue` field, so
    /// the real Tier-1 dialogue path parses it unchanged.
    pub async fn generate_stream_json(
        &self,
        _model: &str,
        prompt: &str,
        system: Option<&str>,
        token_tx: mpsc::Sender<String>,
        _max_tokens: Option<u32>,
        _temperature: Option<f32>,
    ) -> Result<String, ParishError> {
        let json = self.json_for(prompt, system);
        stream_words(&json, &token_tx).await;
        Ok(json)
    }

    /// Typed JSON generation. Input-parser requests get deterministic intent
    /// JSON (shared with the simulator); everything else gets the dialogue
    /// envelope wrapping the scripted completion.
    pub async fn generate_json<T: DeserializeOwned>(
        &self,
        _model: &str,
        prompt: &str,
        system: Option<&str>,
        _max_tokens: Option<u32>,
        _temperature: Option<f32>,
    ) -> Result<T, ParishError> {
        let json = self.json_for(prompt, system);
        serde_json::from_str(&json)
            .map_err(|e| ParishError::Inference(format!("mock json decode: {e} (raw: {json})")))
    }

    /// Builds the JSON body for the JSON-typed paths.
    fn json_for(&self, prompt: &str, system: Option<&str>) -> String {
        // Intent parsing must classify deterministically regardless of script —
        // reuse the simulator's keyword detector so movement/look/talk routing
        // behaves identically across both engines under shadow comparison.
        if system.is_some_and(|s| s.contains("input parser")) {
            return intent_json_for(prompt);
        }
        let dialogue = self.completion_for(prompt, system);
        let escaped = dialogue.replace('"', "\\\"").replace('\n', " ");
        format!(
            r#"{{"dialogue":"{escaped}","action":"","mood":"neutral","internal_thought":null,"language_hints":[],"mentioned_people":[]}}"#
        )
    }
}

impl Default for MockClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Sends `text` to `token_tx` one whitespace-delimited word at a time (each
/// suffixed with a space, matching the simulator's chunking), stopping early if
/// the receiver has dropped.
async fn stream_words(text: &str, token_tx: &mpsc::Sender<String>) {
    for word in text.split_whitespace() {
        if token_tx.send(format!("{word} ")).await.is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TOKEN_CHANNEL_CAPACITY;

    #[tokio::test]
    async fn generate_returns_exact_scripted_text() {
        let mock = MockClient::new();
        mock.push_any("Good morning to you.");
        let out = mock
            .generate("m", "say hello", None, None, None)
            .await
            .unwrap();
        assert_eq!(out, "Good morning to you.");
    }

    #[tokio::test]
    async fn empty_queue_returns_deterministic_fallback() {
        let mock = MockClient::new();
        let a = mock
            .generate("m", "anything", None, None, None)
            .await
            .unwrap();
        let b = mock
            .generate("m", "different", None, None, None)
            .await
            .unwrap();
        assert_eq!(a, MOCK_FALLBACK_TEXT);
        assert_eq!(a, b, "fallback must be deterministic");
    }

    #[tokio::test]
    async fn generate_stream_emits_scripted_text_as_tokens() {
        let mock = MockClient::new();
        mock.push_any("Have you any news from the fair?");
        let (tx, mut rx) = mpsc::channel::<String>(TOKEN_CHANNEL_CAPACITY);
        let full = mock
            .generate_stream("m", "say news", None, tx, None, None)
            .await
            .unwrap();
        let mut received = String::new();
        while let Ok(tok) = rx.try_recv() {
            received.push_str(&tok);
        }
        // Multiple tokens were emitted, and together they reconstruct the text.
        assert!(
            received.split_whitespace().count() > 1,
            "expected multiple tokens"
        );
        assert_eq!(received.trim(), full.trim());
        assert_eq!(full, "Have you any news from the fair?");
    }

    #[tokio::test]
    async fn contains_matcher_keys_completion_to_addressed_npc() {
        let mock = MockClient::new();
        mock.push_for("Peig", "Peig speaks first.");
        mock.push_for("Tadhg", "Tadhg replies.");
        // Tadhg's line is requested first; matcher selects it out of FIFO order.
        let tadhg = mock
            .generate("m", "Conversation with Tadhg the smith", None, None, None)
            .await
            .unwrap();
        assert_eq!(tadhg, "Tadhg replies.");
        let peig = mock
            .generate("m", "Conversation with Peig", None, None, None)
            .await
            .unwrap();
        assert_eq!(peig, "Peig speaks first.");
        assert_eq!(mock.pending(), 0, "both scripted entries consumed");
    }

    #[tokio::test]
    async fn input_parser_path_classifies_intent_deterministically() {
        #[derive(serde::Deserialize, Default)]
        struct Intent {
            #[serde(default)]
            intent: Option<String>,
        }
        let mock = MockClient::new();
        let system = "You are a text adventure input parser.";
        let resp: Intent = mock
            .generate_json("m", "go to the church", Some(system), None, None)
            .await
            .unwrap();
        assert_eq!(resp.intent.as_deref(), Some("move"));
    }

    #[tokio::test]
    async fn json_path_wraps_scripted_completion_as_dialogue() {
        #[derive(serde::Deserialize, Default)]
        struct Reply {
            #[serde(default)]
            dialogue: String,
        }
        let mock = MockClient::new();
        mock.push_any("A fine soft day.");
        let resp: Reply = mock
            .generate_json("m", "chat", Some("dialogue system"), None, None)
            .await
            .unwrap();
        assert_eq!(resp.dialogue, "A fine soft day.");
    }
}
