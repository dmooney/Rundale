//! Core identifier types and NPC string-processing utilities.
//!
//! Contains the foundational ID newtypes (`LocationId`, `NpcId`),
//! the `Location` struct, `LanguageHint`, and streaming separator helpers
//! extracted from the world and NPC modules.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::ParishError;

/// Current weather conditions in the game world.
///
/// Affects location description templates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weather {
    Clear,
    PartlyCloudy,
    Overcast,
    LightRain,
    HeavyRain,
    Fog,
    Storm,
}

impl FromStr for Weather {
    type Err = ParishError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Clear" => Ok(Weather::Clear),
            "Partly Cloudy" | "PartlyCloudy" => Ok(Weather::PartlyCloudy),
            "Overcast" => Ok(Weather::Overcast),
            "Light Rain" | "LightRain" | "Rain" => Ok(Weather::LightRain),
            "Heavy Rain" | "HeavyRain" => Ok(Weather::HeavyRain),
            "Fog" => Ok(Weather::Fog),
            "Storm" => Ok(Weather::Storm),
            other => Err(ParishError::Config(format!("unknown weather: {}", other))),
        }
    }
}

impl fmt::Display for Weather {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Weather::Clear => write!(f, "Clear"),
            Weather::PartlyCloudy => write!(f, "Partly Cloudy"),
            Weather::Overcast => write!(f, "Overcast"),
            Weather::LightRain => write!(f, "Light Rain"),
            Weather::HeavyRain => write!(f, "Heavy Rain"),
            Weather::Fog => write!(f, "Fog"),
            Weather::Storm => write!(f, "Storm"),
        }
    }
}

/// Unique identifier for a location in the world graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LocationId(pub u32);

/// Unique identifier for an NPC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NpcId(pub u32);

/// A named location in the game world.
///
/// Locations are nodes in the world graph. Each has a textual
/// description, flags indicating whether it is indoors and/or
/// public, and geographic coordinates for map placement.
#[derive(Debug, Clone)]
pub struct Location {
    /// Unique identifier.
    pub id: LocationId,
    /// Human-readable name (e.g. "The Crossroads").
    pub name: String,
    /// Prose description shown when the player arrives.
    pub description: String,
    /// Whether this location is indoors.
    pub indoor: bool,
    /// Whether this location is publicly accessible.
    pub public: bool,
    /// Latitude in decimal degrees (WGS 84).
    pub lat: f64,
    /// Longitude in decimal degrees (WGS 84).
    pub lon: f64,
}

/// A pronunciation hint for a word in the setting's secondary language.
///
/// Extracted from NPC response metadata and displayed in the
/// pronunciation sidebar to help players with unfamiliar words.
/// The mod's prompt template instructs the LLM to produce these.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct LanguageHint {
    /// The word as it appears in text.
    pub word: String,
    /// Approximate English phonetic pronunciation.
    pub pronunciation: String,
    /// English meaning or gloss.
    #[serde(default)]
    pub meaning: Option<String>,
}

/// Backward-compatible alias for [`LanguageHint`].
pub type IrishWordHint = LanguageHint;

/// Rounds a byte offset down to the nearest UTF-8 char boundary in `s`.
///
/// If `pos` is already a char boundary, returns it unchanged. Otherwise
/// scans backwards to the nearest valid boundary. Returns 0 if no valid
/// boundary is found before `pos`.
pub fn floor_char_boundary(s: &str, pos: usize) -> usize {
    if pos >= s.len() {
        return s.len();
    }
    let mut p = pos;
    while p > 0 && !s.is_char_boundary(p) {
        p -= 1;
    }
    p
}

/// Finds the byte offset of the `"dialogue"` key in a partial JSON buffer,
/// matched only when it appears as a top-level object key (depth 1).
///
/// Scans the buffer character by character, tracking brace/bracket/string depth
/// so that `"dialogue"` embedded inside a nested string or object value is
/// ignored. Returns the byte offset of the first byte of `"dialogue"` (the
/// opening double-quote), or `None` if not found.
fn find_toplevel_dialogue_key(buffer: &str) -> Option<usize> {
    let bytes = buffer.as_bytes();
    let n = bytes.len();
    let mut i = 0;
    // depth: 0 = before the root object, 1 = inside root object keys/values.
    let mut depth: i32 = 0;
    // Whether we are currently inside a JSON string.
    let mut in_string = false;

    while i < n {
        if in_string {
            match bytes[i] {
                b'\\' => {
                    // Skip escaped character — two bytes consumed.
                    i += 2;
                }
                b'"' => {
                    in_string = false;
                    i += 1;
                }
                _ => {
                    i += 1;
                }
            }
        } else {
            match bytes[i] {
                b'"' => {
                    // At depth 1 (inside root object), check if this is "dialogue":
                    if depth == 1 {
                        let key = b"\"dialogue\"";
                        if bytes.get(i..i + key.len()) == Some(key) {
                            // Verify that the next non-whitespace byte is `:` — this
                            // distinguishes a JSON key from a string value that happens
                            // to contain the text "dialogue".
                            let after_key = i + key.len();
                            let mut j = after_key;
                            while j < n
                                && (bytes[j] == b' '
                                    || bytes[j] == b'\t'
                                    || bytes[j] == b'\r'
                                    || bytes[j] == b'\n')
                            {
                                j += 1;
                            }
                            if j < n && bytes[j] == b':' {
                                return Some(i);
                            }
                            // Not a key (it's a value); fall through to enter the string.
                        }
                    }
                    // Enter string regardless.
                    in_string = true;
                    i += 1;
                }
                b'{' | b'[' => {
                    depth += 1;
                    i += 1;
                }
                b'}' | b']' => {
                    depth -= 1;
                    i += 1;
                }
                _ => {
                    i += 1;
                }
            }
        }
    }

    None
}

/// Extracts the dialogue field value from a partial JSON string during streaming.
///
/// Scans the accumulated JSON buffer for the `"dialogue"` field and extracts
/// its string value as it streams in. Returns `Some(text)` with the dialogue
/// content extracted so far, or `None` if the dialogue field hasn't started yet.
///
/// This enables token-by-token streaming of NPC dialogue to the player while
/// the full JSON response (including metadata) is still being generated.
///
/// # Security (fix #649)
/// Uses depth-aware scanning via [`find_toplevel_dialogue_key`] so that
/// `"dialogue"` embedded inside another field's string value cannot hijack
/// extraction.
///
/// # Unicode (fix #655)
/// JSON `\uXXXX` surrogate pairs (0xD800–0xDBFF followed by 0xDC00–0xDFFF)
/// are combined into the correct non-BMP code point rather than silently dropped.
pub fn extract_dialogue_from_partial_json(buffer: &str) -> Option<String> {
    let key = b"\"dialogue\"";
    let key_pos = find_toplevel_dialogue_key(buffer)?;
    let after_key = key_pos + key.len();

    // Skip whitespace between key and colon
    let rest = &buffer[after_key..];
    let colon_offset = rest.find(':')?;
    let after_colon = after_key + colon_offset + 1;

    // Skip whitespace between colon and opening quote
    let rest = &buffer[after_colon..];
    let quote_offset = rest.find('"')?;
    let start = after_colon + quote_offset + 1;

    let value_bytes = &buffer.as_bytes()[start..];

    let mut result = String::new();
    let mut i = 0;
    while i < value_bytes.len() {
        match value_bytes[i] {
            b'"' => {
                return Some(result);
            }
            b'\\' => {
                if i + 1 >= value_bytes.len() {
                    // Incomplete escape at end of buffer — stop before it so
                    // the next chunk can complete the sequence.
                    return Some(result);
                }
                match value_bytes[i + 1] {
                    b'"' => result.push('"'),
                    b'\\' => result.push('\\'),
                    b'n' => result.push('\n'),
                    b'r' => result.push('\r'),
                    b't' => result.push('\t'),
                    b'/' => result.push('/'),
                    b'u' => {
                        // Need at least \uXXXX (6 bytes total from i).
                        if i + 5 >= value_bytes.len() {
                            // Incomplete \u escape at end of buffer — stop here.
                            return Some(result);
                        }
                        let hex1 = match std::str::from_utf8(&value_bytes[i + 2..i + 6])
                            .ok()
                            .and_then(|s| u32::from_str_radix(s, 16).ok())
                        {
                            Some(v) => v,
                            None => {
                                i += 6;
                                continue;
                            }
                        };
                        // Check for surrogate pair: high surrogate 0xD800–0xDBFF.
                        if (0xD800..=0xDBFF).contains(&hex1) {
                            // Expect a low surrogate immediately following: \uXXXX.
                            // Total offset from i: 6 (first \uXXXX) + 2 (\\u) + 4 (hex) = 12.
                            if i + 11 < value_bytes.len()
                                && value_bytes[i + 6] == b'\\'
                                && value_bytes[i + 7] == b'u'
                            {
                                let hex2 = std::str::from_utf8(&value_bytes[i + 8..i + 12])
                                    .ok()
                                    .and_then(|s| u32::from_str_radix(s, 16).ok());
                                if let Some(low) = hex2
                                    && (0xDC00..=0xDFFF).contains(&low)
                                {
                                    // Combine surrogate pair into a scalar value.
                                    let code_point =
                                        0x10000 + ((hex1 - 0xD800) << 10) + (low - 0xDC00);
                                    if let Some(c) = char::from_u32(code_point) {
                                        result.push(c);
                                    }
                                    i += 12;
                                    continue;
                                }
                            } else if i + 11 >= value_bytes.len() {
                                // Low surrogate not yet in buffer — stop here and wait
                                // for the next chunk.
                                return Some(result);
                            }
                            // Malformed: high surrogate without a valid low surrogate.
                            // Skip the high surrogate silently.
                            i += 6;
                            continue;
                        }
                        // Normal BMP code point.
                        if let Some(c) = char::from_u32(hex1) {
                            result.push(c);
                        }
                        i += 6;
                        continue;
                    }
                    _ => {
                        result.push('\\');
                        result.push(value_bytes[i + 1] as char);
                    }
                }
                i += 2;
            }
            _ => {
                if let Some(rest) = buffer.get(start + i..) {
                    if let Some(ch) = rest.chars().next() {
                        result.push(ch);
                        i += ch.len_utf8();
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
        }
    }

    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Weather FromStr ──────────────────────────────────────────────────────

    #[test]
    fn weather_from_str_canonical_forms() {
        assert_eq!("Clear".parse::<Weather>().unwrap(), Weather::Clear);
        assert_eq!(
            "Partly Cloudy".parse::<Weather>().unwrap(),
            Weather::PartlyCloudy
        );
        assert_eq!("Overcast".parse::<Weather>().unwrap(), Weather::Overcast);
        assert_eq!("Light Rain".parse::<Weather>().unwrap(), Weather::LightRain);
        assert_eq!("Heavy Rain".parse::<Weather>().unwrap(), Weather::HeavyRain);
        assert_eq!("Fog".parse::<Weather>().unwrap(), Weather::Fog);
        assert_eq!("Storm".parse::<Weather>().unwrap(), Weather::Storm);
    }

    #[test]
    fn weather_from_str_alternate_spellings() {
        assert_eq!(
            "PartlyCloudy".parse::<Weather>().unwrap(),
            Weather::PartlyCloudy
        );
        assert_eq!("LightRain".parse::<Weather>().unwrap(), Weather::LightRain);
        assert_eq!("Rain".parse::<Weather>().unwrap(), Weather::LightRain);
        assert_eq!("HeavyRain".parse::<Weather>().unwrap(), Weather::HeavyRain);
    }

    #[test]
    fn weather_from_str_unknown_returns_error() {
        let err = "Blizzard".parse::<Weather>().unwrap_err();
        match err {
            ParishError::Config(msg) => {
                assert!(msg.contains("Blizzard"));
                assert!(msg.contains("unknown weather"));
            }
            other => panic!("expected ParishError::Config, got {:?}", other),
        }
    }

    #[test]
    fn weather_from_str_empty_errors() {
        assert!("".parse::<Weather>().is_err());
    }

    #[test]
    fn weather_from_str_is_case_sensitive() {
        // Lowercase is not accepted — documents current behaviour.
        assert!("clear".parse::<Weather>().is_err());
        assert!("STORM".parse::<Weather>().is_err());
    }

    // ── Weather Display ──────────────────────────────────────────────────────

    #[test]
    fn weather_display_matches_canonical_form() {
        assert_eq!(Weather::Clear.to_string(), "Clear");
        assert_eq!(Weather::PartlyCloudy.to_string(), "Partly Cloudy");
        assert_eq!(Weather::Overcast.to_string(), "Overcast");
        assert_eq!(Weather::LightRain.to_string(), "Light Rain");
        assert_eq!(Weather::HeavyRain.to_string(), "Heavy Rain");
        assert_eq!(Weather::Fog.to_string(), "Fog");
        assert_eq!(Weather::Storm.to_string(), "Storm");
    }

    #[test]
    fn weather_display_then_parse_is_identity() {
        for w in [
            Weather::Clear,
            Weather::PartlyCloudy,
            Weather::Overcast,
            Weather::LightRain,
            Weather::HeavyRain,
            Weather::Fog,
            Weather::Storm,
        ] {
            let s = w.to_string();
            let parsed: Weather = s.parse().unwrap();
            assert_eq!(parsed, w, "round-trip failed for {:?}", w);
        }
    }

    // ── LocationId / NpcId ───────────────────────────────────────────────────

    #[test]
    fn location_id_serde_round_trip() {
        let id = LocationId(42);
        let json = serde_json::to_string(&id).unwrap();
        // Transparent newtype: serialises to bare number.
        assert_eq!(json, "42");
        let back: LocationId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
    }

    #[test]
    fn npc_id_serde_round_trip() {
        let id = NpcId(7);
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "7");
        let back: NpcId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
    }

    #[test]
    fn location_id_ordering() {
        let mut ids = vec![LocationId(3), LocationId(1), LocationId(2)];
        ids.sort();
        assert_eq!(ids, vec![LocationId(1), LocationId(2), LocationId(3)]);
    }

    #[test]
    fn npc_id_usable_in_hashmap() {
        use std::collections::HashMap;
        let mut m: HashMap<NpcId, &'static str> = HashMap::new();
        m.insert(NpcId(1), "Siobhan");
        m.insert(NpcId(2), "Padraig");
        assert_eq!(m.get(&NpcId(1)), Some(&"Siobhan"));
        assert_eq!(m.get(&NpcId(3)), None);
    }

    // ── floor_char_boundary ──────────────────────────────────────────────────

    #[test]
    fn floor_char_boundary_ascii_stays_put() {
        let s = "hello world";
        assert_eq!(floor_char_boundary(s, 5), 5);
        assert_eq!(floor_char_boundary(s, 0), 0);
    }

    #[test]
    fn floor_char_boundary_clamps_beyond_end() {
        let s = "hi";
        assert_eq!(floor_char_boundary(s, 100), s.len());
    }

    #[test]
    fn floor_char_boundary_moves_off_multibyte() {
        // "é" is two bytes (0xC3 0xA9) in UTF-8.
        let s = "café";
        // Position 4 is inside the "é" code unit sequence — should fall back to 3.
        assert_eq!(floor_char_boundary(s, 4), 3);
        // Position 3 is the start of "é" — a valid boundary.
        assert_eq!(floor_char_boundary(s, 3), 3);
        // Position 5 is the end of the string — valid boundary.
        assert_eq!(floor_char_boundary(s, 5), 5);
    }

    #[test]
    fn floor_char_boundary_empty_string() {
        assert_eq!(floor_char_boundary("", 0), 0);
        assert_eq!(floor_char_boundary("", 10), 0);
    }

    #[test]
    fn floor_char_boundary_four_byte_char() {
        // 🍀 is 4 bytes (F0 9F 8D 80)
        let s = "a🍀b";
        // bytes: a(0) F0(1) 9F(2) 8D(3) 80(4) b(5)
        assert_eq!(floor_char_boundary(s, 0), 0);
        assert_eq!(floor_char_boundary(s, 1), 1);
        assert_eq!(floor_char_boundary(s, 2), 1);
        assert_eq!(floor_char_boundary(s, 3), 1);
        assert_eq!(floor_char_boundary(s, 4), 1);
        assert_eq!(floor_char_boundary(s, 5), 5);
    }

    // ── extract_dialogue_from_partial_json ─────────────────────────────────

    #[test]
    fn extract_dialogue_complete() {
        let buf = r#"{"dialogue": "Hello there!", "action": "speaks"}"#;
        assert_eq!(
            extract_dialogue_from_partial_json(buf),
            Some("Hello there!".to_string())
        );
    }

    #[test]
    fn extract_dialogue_streaming() {
        let buf = r#"{"dialogue": "Hello th"#;
        assert_eq!(
            extract_dialogue_from_partial_json(buf),
            Some("Hello th".to_string())
        );
    }

    #[test]
    fn extract_dialogue_not_yet_started() {
        assert_eq!(extract_dialogue_from_partial_json(r#"{"act"#), None);
    }

    #[test]
    fn extract_dialogue_with_escapes() {
        let buf = r#"{"dialogue": "He said \"hello\" to me"}"#;
        assert_eq!(
            extract_dialogue_from_partial_json(buf),
            Some("He said \"hello\" to me".to_string())
        );
    }

    #[test]
    fn extract_dialogue_with_newlines() {
        let buf = r#"{"dialogue": "Line one\nLine two"}"#;
        assert_eq!(
            extract_dialogue_from_partial_json(buf),
            Some("Line one\nLine two".to_string())
        );
    }

    #[test]
    fn extract_dialogue_with_unicode() {
        let buf = r#"{"dialogue": "Sláinte!"}"#;
        assert_eq!(
            extract_dialogue_from_partial_json(buf),
            Some("Sláinte!".to_string())
        );
    }

    #[test]
    fn extract_dialogue_no_space_after_colon() {
        let buf = r#"{"dialogue":"Hello!"}"#;
        assert_eq!(
            extract_dialogue_from_partial_json(buf),
            Some("Hello!".to_string())
        );
    }

    #[test]
    fn extract_dialogue_empty_string() {
        let buf = r#"{"dialogue": ""}"#;
        assert_eq!(extract_dialogue_from_partial_json(buf), Some(String::new()));
    }

    #[test]
    fn extract_dialogue_extra_whitespace_around_colon() {
        let buf = r#"{"dialogue" : "Spaced out!"}"#;
        assert_eq!(
            extract_dialogue_from_partial_json(buf),
            Some("Spaced out!".to_string())
        );
    }

    #[test]
    fn extract_dialogue_newline_after_colon() {
        let buf = "{ \"dialogue\" :\n  \"Multiline format!\" }";
        assert_eq!(
            extract_dialogue_from_partial_json(buf),
            Some("Multiline format!".to_string())
        );
    }

    #[test]
    fn extract_dialogue_trailing_backslash_at_chunk_boundary() {
        // Buffer ends mid-escape — should stop before the incomplete escape
        let buf = r#"{"dialogue": "Hello \"#;
        let result = extract_dialogue_from_partial_json(buf).unwrap();
        assert_eq!(result, "Hello ");
        // When more data arrives, the full escape is re-parsed correctly
        let buf2 = r#"{"dialogue": "Hello \"world\""}"#;
        let result2 = extract_dialogue_from_partial_json(buf2).unwrap();
        assert_eq!(result2, "Hello \"world\"");
    }

    // ── Issue #649: key injection via nested "dialogue" substring ───────────

    #[test]
    fn extract_dialogue_ignores_dialogue_in_value_of_other_key() {
        // The string "dialogue" appears inside another field's value — should
        // not be mistaken for the top-level "dialogue" key.
        let buf = r#"{"action": "says \"dialogue\": \"injected\"", "dialogue": "real"}"#;
        assert_eq!(
            extract_dialogue_from_partial_json(buf),
            Some("real".to_string()),
            "must use the top-level 'dialogue' key, not an occurrence inside a value"
        );
    }

    #[test]
    fn extract_dialogue_ignores_dialogue_in_nested_object() {
        // "dialogue" as a key inside a nested object must not match.
        let buf = r#"{"meta": {"dialogue": "fake"}, "dialogue": "real"}"#;
        assert_eq!(
            extract_dialogue_from_partial_json(buf),
            Some("real".to_string()),
            "must match only the top-level 'dialogue' key"
        );
    }

    #[test]
    fn extract_dialogue_no_false_match_when_only_nested() {
        // If there is no top-level "dialogue" key, return None — even if the
        // word appears inside a nested value.
        let buf = r#"{"meta": {"dialogue": "nested only"}}"#;
        assert_eq!(
            extract_dialogue_from_partial_json(buf),
            None,
            "must return None when 'dialogue' only appears inside a nested object"
        );
    }

    #[test]
    fn extract_dialogue_no_false_match_when_value_equals_key_literal() {
        // Regression for Gemini feedback: a string VALUE that is exactly
        // "dialogue" (including surrounding quotes) must not be mistaken for
        // the top-level "dialogue" key.  The scanner must check for a trailing
        // `:` before accepting a match.
        let buf = r#"{"foo": "dialogue", "dialogue": "real"}"#;
        assert_eq!(
            extract_dialogue_from_partial_json(buf),
            Some("real".to_string()),
            "value 'dialogue' must not shadow the real top-level key"
        );
    }

    #[test]
    fn extract_dialogue_returns_none_when_dialogue_only_in_value() {
        // If "dialogue" appears solely as a string value and never as a key,
        // the function must return None rather than misidentifying the value.
        let buf = r#"{"foo": "dialogue", "bar": "other"}"#;
        assert_eq!(
            extract_dialogue_from_partial_json(buf),
            None,
            "must return None when 'dialogue' only appears as a string value"
        );
    }

    // ── Issue #655: surrogate pair handling for non-BMP characters ──────────

    #[test]
    fn extract_dialogue_surrogate_pair_emoji() {
        // U+1F600 GRINNING FACE is encoded in JSON as 😀.
        let buf = r#"{"dialogue": "Hello 😀!"}"#;
        assert_eq!(
            extract_dialogue_from_partial_json(buf),
            Some("Hello 😀!".to_string()),
            "surrogate pair must be decoded to the correct non-BMP code point"
        );
    }

    #[test]
    fn extract_dialogue_surrogate_pair_ancient_script() {
        // U+10000 LINEAR B SYLLABLE B008 A = 𐀀.
        let buf = r#"{"dialogue": "𐀀"}"#;
        let result = extract_dialogue_from_partial_json(buf).unwrap();
        let mut chars = result.chars();
        let ch = chars.next().expect("should have one character");
        assert_eq!(ch as u32, 0x10000, "must decode to U+10000");
        assert!(chars.next().is_none());
    }

    #[test]
    fn extract_dialogue_bmp_unicode_escape_still_works() {
        // Regression: ordinary \uXXXX BMP escapes must still work after the
        // surrogate-pair changes.
        let buf = r#"{"dialogue": "élève"}"#;
        assert_eq!(
            extract_dialogue_from_partial_json(buf),
            Some("élève".to_string())
        );
    }

    #[test]
    fn extract_dialogue_incomplete_surrogate_pair_at_chunk_boundary() {
        // Buffer ends after the high surrogate — must stop cleanly and not panic.
        let buf = r#"{"dialogue": "Hi \uD83D"#;
        let result = extract_dialogue_from_partial_json(buf);
        // Must return Some("Hi ") — stops before the incomplete surrogate.
        assert_eq!(result, Some("Hi ".to_string()));
    }

    // ── LanguageHint serde ───────────────────────────────────────────────────

    #[test]
    fn language_hint_round_trip_with_meaning() {
        let hint = LanguageHint {
            word: "dúlra".to_string(),
            pronunciation: "DOOL-rah".to_string(),
            meaning: Some("nature".to_string()),
        };
        let json = serde_json::to_string(&hint).unwrap();
        let back: LanguageHint = serde_json::from_str(&json).unwrap();
        assert_eq!(back, hint);
    }

    #[test]
    fn language_hint_defaults_meaning_when_missing() {
        let json = r#"{"word":"fáilte","pronunciation":"FAWL-cheh"}"#;
        let hint: LanguageHint = serde_json::from_str(json).unwrap();
        assert_eq!(hint.word, "fáilte");
        assert!(hint.meaning.is_none());
    }
}
