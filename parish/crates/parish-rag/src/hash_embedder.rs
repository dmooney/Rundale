//! A deterministic embedder using the "hashing trick".
//!
//! Tokenises lowercase alphanumeric words, hashes each into a fixed-width
//! vector bucket with a signed hash (reduces collision bias), and L2-normalises
//! the result. Good enough for retrieval over a small lore corpus — and more
//! importantly, requires no network, no external model, and produces byte-
//! identical output across runs so tests can assert on it.
//!
//! Known limitation: it captures token overlap, not semantic similarity — so
//! a query phrased with synonyms won't retrieve the matching document. The
//! [`OllamaEmbedder`](crate::OllamaEmbedder) exists for semantic retrieval.

use std::collections::HashSet;

/// Short English stopwords — removed before hashing so they don't dominate
/// the signal on short queries.
const STOPWORDS: &[&str] = &[
    "the", "a", "an", "and", "or", "but", "is", "are", "was", "were", "be", "been", "being", "of",
    "to", "in", "on", "at", "by", "for", "with", "from", "as", "it", "its", "that", "this",
    "these", "those", "i", "you", "he", "she", "we", "they", "them", "his", "her", "their", "our",
    "my", "your", "me", "do", "does", "did", "have", "has", "had", "will", "would", "can", "could",
    "should", "what", "who", "when", "where", "why", "how", "about", "tell",
];

/// Deterministic hashing-trick embedder.
#[derive(Debug, Clone)]
pub struct HashEmbedder {
    dim: usize,
}

impl Default for HashEmbedder {
    fn default() -> Self {
        Self { dim: 512 }
    }
}

impl HashEmbedder {
    pub fn new(dim: usize) -> Self {
        let dim = dim.max(16);
        Self { dim }
    }

    pub fn dim(&self) -> usize {
        self.dim
    }

    /// Embed `text` into a `dim`-length L2-normalised vector.
    pub fn embed(&self, text: &str) -> Vec<f32> {
        let mut v = vec![0.0_f32; self.dim];
        let stopwords: HashSet<&&str> = STOPWORDS.iter().collect();
        for token in tokenize(text) {
            if token.len() < 2 {
                continue;
            }
            if stopwords.contains(&token.as_str()) {
                continue;
            }
            let h = fnv1a64(token.as_bytes());
            let bucket = (h as usize) % self.dim;
            let sign = if (h >> 63) & 1 == 0 { 1.0 } else { -1.0 };
            v[bucket] += sign;
        }
        l2_normalize(&mut v);
        v
    }
}

/// Tokenises into lowercase alphanumeric words. Punctuation and whitespace
/// are treated as separators.
fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

/// FNV-1a 64-bit hash. Deterministic across runs and platforms — `HashMap`'s
/// default hasher randomises seeds per process, which would make offline
/// retrieval results irreproducible.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn l2_normalize(v: &mut [f32]) {
    let mag: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if mag > 0.0 {
        for x in v.iter_mut() {
            *x /= mag;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cosine_similarity;

    #[test]
    fn embed_is_unit_length() {
        let e = HashEmbedder::default();
        let v = e.embed("the quick brown fox");
        let mag: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((mag - 1.0).abs() < 1e-5);
    }

    #[test]
    fn embed_same_text_is_identical() {
        let e = HashEmbedder::default();
        let v1 = e.embed("Padraig runs the pub");
        let v2 = e.embed("Padraig runs the pub");
        assert_eq!(v1, v2);
    }

    #[test]
    fn embed_dimension_respects_constructor() {
        let e = HashEmbedder::new(128);
        assert_eq!(e.dim(), 128);
        assert_eq!(e.embed("hello world").len(), 128);
    }

    #[test]
    fn embed_small_dim_is_clamped() {
        let e = HashEmbedder::new(1);
        assert!(e.dim() >= 16);
    }

    #[test]
    fn embed_empty_text_is_all_zeros() {
        let e = HashEmbedder::default();
        let v = e.embed("");
        assert!(v.iter().all(|&x| x == 0.0));
    }

    #[test]
    fn similar_texts_score_higher_than_unrelated() {
        let e = HashEmbedder::default();
        let doc_pub =
            e.embed("Padraig Darcy runs the pub at the crossroads and knows local history");
        let doc_farm = e.embed("Siobhan Murphy works the fields planting potatoes on her farm");

        let query_pub = e.embed("who runs the pub");
        let query_farm = e.embed("who works the farm");

        let pub_to_pub = cosine_similarity(&query_pub, &doc_pub);
        let pub_to_farm = cosine_similarity(&query_pub, &doc_farm);
        let farm_to_farm = cosine_similarity(&query_farm, &doc_farm);
        let farm_to_pub = cosine_similarity(&query_farm, &doc_pub);

        assert!(
            pub_to_pub > pub_to_farm,
            "pub query should prefer pub doc: pub_to_pub={pub_to_pub} pub_to_farm={pub_to_farm}"
        );
        assert!(
            farm_to_farm > farm_to_pub,
            "farm query should prefer farm doc: farm_to_farm={farm_to_farm} farm_to_pub={farm_to_pub}"
        );
    }

    #[test]
    fn stopwords_do_not_dominate_short_queries() {
        // "the" and "is" are stopwords; "crossroads" carries the signal.
        let e = HashEmbedder::default();
        let q_stop = e.embed("the is a the is at the");
        let q_signal = e.embed("tell me about the crossroads");
        let doc = e.embed("The crossroads holds power in Irish folklore");

        let sim_stop = cosine_similarity(&q_stop, &doc);
        let sim_signal = cosine_similarity(&q_signal, &doc);
        assert!(
            sim_signal > sim_stop,
            "signal query should beat all-stopword query: signal={sim_signal} stop={sim_stop}"
        );
    }

    #[test]
    fn tokenize_lowercases_and_splits_on_punctuation() {
        let tokens = tokenize("Darcy's Pub, at the crossroads!");
        assert!(tokens.contains(&"darcy".to_string()));
        assert!(tokens.contains(&"pub".to_string()));
        assert!(tokens.contains(&"crossroads".to_string()));
    }

    #[test]
    fn fnv1a64_is_deterministic() {
        assert_eq!(fnv1a64(b"hello"), fnv1a64(b"hello"));
        assert_ne!(fnv1a64(b"hello"), fnv1a64(b"world"));
    }
}
