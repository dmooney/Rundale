//! Retrieval-augmented generation for Parish NPC knowledge.
//!
//! Builds an in-memory vector index from parish lore (world locations, NPC
//! biographies, festivals) and returns the most relevant passages for a query,
//! so those passages can be injected into an NPC's system prompt as recalled
//! knowledge.
//!
//! The crate is deliberately small and self-contained — it is a demo of a
//! pattern, not a replacement for the existing keyword recall in
//! `parish-npc::memory`. Two embedders ship: a deterministic hashing-trick
//! embedder (offline, no network) and an Ollama `/api/embeddings` client.

pub mod corpus;
pub mod hash_embedder;
pub mod ollama_embedder;

pub use corpus::{LoreChunk, build_rundale_corpus};
pub use hash_embedder::HashEmbedder;
pub use ollama_embedder::OllamaEmbedder;

use serde::{Deserialize, Serialize};

/// A single retrievable lore passage with its embedding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoreDocument {
    pub id: String,
    pub source: String,
    pub content: String,
    pub embedding: Vec<f32>,
}

impl LoreDocument {
    pub fn new(
        id: impl Into<String>,
        source: impl Into<String>,
        content: impl Into<String>,
        embedding: Vec<f32>,
    ) -> Self {
        Self {
            id: id.into(),
            source: source.into(),
            content: content.into(),
            embedding,
        }
    }
}

/// Cosine similarity between two equal-length vectors.
///
/// Returns 0.0 when either vector is empty, lengths differ, or either
/// magnitude is zero — callers can treat that as "no signal".
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || b.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let mut dot = 0.0;
    let mut mag_a = 0.0;
    let mut mag_b = 0.0;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        mag_a += a[i] * a[i];
        mag_b += b[i] * b[i];
    }
    if mag_a == 0.0 || mag_b == 0.0 {
        return 0.0;
    }
    dot / (mag_a.sqrt() * mag_b.sqrt())
}

/// In-memory vector index over `LoreDocument`s.
#[derive(Debug, Clone, Default)]
pub struct LoreIndex {
    docs: Vec<LoreDocument>,
}

impl LoreIndex {
    pub fn new() -> Self {
        Self { docs: Vec::new() }
    }

    pub fn from_docs(docs: Vec<LoreDocument>) -> Self {
        Self { docs }
    }

    pub fn push(&mut self, doc: LoreDocument) {
        self.docs.push(doc);
    }

    pub fn len(&self) -> usize {
        self.docs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.docs.is_empty()
    }

    pub fn docs(&self) -> &[LoreDocument] {
        &self.docs
    }

    /// Returns the top-k documents ranked by cosine similarity to `query`.
    pub fn search(&self, query: &[f32], k: usize) -> Vec<(f32, &LoreDocument)> {
        let mut scored: Vec<(f32, &LoreDocument)> = self
            .docs
            .iter()
            .map(|d| (cosine_similarity(query, &d.embedding), d))
            .collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(k);
        scored
    }
}

/// A unified embedder handle that covers every supported backend.
///
/// Mirrors the `AnyClient` enum in `parish-inference` so callers don't need to
/// care which backend is in use.
#[derive(Clone)]
pub enum AnyEmbedder {
    Hash(HashEmbedder),
    Ollama(OllamaEmbedder),
}

impl AnyEmbedder {
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        match self {
            Self::Hash(e) => Ok(e.embed(text)),
            Self::Ollama(e) => e.embed(text).await,
        }
    }

    /// Embeds every chunk and returns them as a `LoreIndex`. Errors abort the
    /// build — a partial index is rarely what callers want.
    pub async fn index(&self, chunks: Vec<LoreChunk>) -> Result<LoreIndex, String> {
        let mut docs = Vec::with_capacity(chunks.len());
        for chunk in chunks {
            let embedding = self.embed(&chunk.content).await?;
            docs.push(LoreDocument::new(
                chunk.id,
                chunk.source,
                chunk.content,
                embedding,
            ));
        }
        Ok(LoreIndex::from_docs(docs))
    }
}

/// Builds the "RECALLED KNOWLEDGE" block that is appended to an NPC's system
/// prompt when RAG is enabled.
///
/// Returns an empty string when `retrieved` is empty so callers can append it
/// unconditionally.
pub fn format_recall_block(retrieved: &[(f32, &LoreDocument)]) -> String {
    if retrieved.is_empty() {
        return String::new();
    }
    let mut out = String::from("\n\nKNOWLEDGE YOU RECALL (things you know from living here):\n");
    for (_, doc) in retrieved {
        out.push_str("- ");
        out.push_str(&doc.content);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_similarity_identical_vectors_is_one() {
        let v = vec![0.5, 0.5, 0.5, 0.5];
        let s = cosine_similarity(&v, &v);
        assert!((s - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_similarity_orthogonal_vectors_is_zero() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!(cosine_similarity(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn cosine_similarity_empty_or_mismatched_is_zero() {
        assert_eq!(cosine_similarity(&[], &[1.0]), 0.0);
        assert_eq!(cosine_similarity(&[1.0], &[]), 0.0);
        assert_eq!(cosine_similarity(&[1.0, 2.0], &[1.0]), 0.0);
    }

    #[test]
    fn cosine_similarity_zero_magnitude_is_zero() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 1.0, 1.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn lore_index_search_ranks_by_similarity() {
        let mut index = LoreIndex::new();
        index.push(LoreDocument::new("a", "s", "c_a", vec![1.0, 0.0, 0.0]));
        index.push(LoreDocument::new("b", "s", "c_b", vec![0.0, 1.0, 0.0]));
        index.push(LoreDocument::new("c", "s", "c_c", vec![0.7, 0.7, 0.0]));

        let results = index.search(&[1.0, 0.0, 0.0], 3);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].1.id, "a");
        assert_eq!(results[1].1.id, "c");
        assert_eq!(results[2].1.id, "b");
    }

    #[test]
    fn lore_index_search_respects_k() {
        let mut index = LoreIndex::new();
        for i in 0..10 {
            index.push(LoreDocument::new(
                format!("{i}"),
                "s",
                "c",
                vec![i as f32, 0.0],
            ));
        }
        let results = index.search(&[1.0, 0.0], 3);
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn lore_index_empty_search_returns_empty() {
        let index = LoreIndex::new();
        let results = index.search(&[1.0, 0.0], 5);
        assert!(results.is_empty());
    }

    #[test]
    fn format_recall_block_empty_is_empty_string() {
        assert_eq!(format_recall_block(&[]), "");
    }

    #[test]
    fn format_recall_block_contains_every_doc() {
        let d1 = LoreDocument::new("1", "s", "Alpha fact", vec![]);
        let d2 = LoreDocument::new("2", "s", "Beta fact", vec![]);
        let retrieved = vec![(0.9, &d1), (0.7, &d2)];
        let block = format_recall_block(&retrieved);
        assert!(block.contains("Alpha fact"));
        assert!(block.contains("Beta fact"));
        assert!(block.contains("KNOWLEDGE YOU RECALL"));
    }
}
