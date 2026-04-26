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
/// Affects color palette tinting and location description templates.
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

/// Extracts the dialogue field value from a partial JSON string during streaming.
///
/// Scans the accumulated JSON buffer for the `"dialogue"` field and extracts
/// its string value as it streams in. Returns `Some(text)` with the dialogue
/// content extracted so far, or `None` if the dialogue field hasn't started yet.
///
/// This enables token-by-token streaming of NPC dialogue to the player while
/// the full JSON response (including metadata) is still being generated.
pub fn extract_dialogue_from_partial_json(buffer: &str) -> Option<String> {
    let key = "\"dialogue\"";
    let key_pos = buffer.find(key)?;
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
                        if i + 5 < value_bytes.len() {
                            if let Ok(hex) = std::str::from_utf8(&value_bytes[i + 2..i + 6])
                                && let Ok(code) = u32::from_str_radix(hex, 16)
                                && let Some(c) = char::from_u32(code)
                            {
                                result.push(c);
                            }
                            i += 6;
                            continue;
                        } else {
                            return Some(result);
                        }
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
