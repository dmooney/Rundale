//! `@mention` extraction from player input.
//!
//! Recognises `@Name` tokens at the start of input or after whitespace,
//! including multi-word names (`@Padraig Darcy`). Used to bind dialogue
//! and addressed actions to specific NPCs.

/// The result of extracting an `@mention` from player input.
///
/// Contains the mentioned name and the remaining input text with the
/// `@mention` stripped out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MentionExtraction {
    /// The name that was mentioned (without the `@` prefix).
    pub name: String,
    /// The remaining input text after stripping the mention.
    pub remaining: String,
}

/// Extracts an `@mention` from the beginning of player input.
///
/// Recognises `@Name` anywhere in input where `@` appears at the start or
/// after whitespace. The name runs from the `@` until the next punctuation,
/// double-space, or end of string — so both single-word names (`@Padraig`)
/// and multi-word names (`@Padraig Darcy`) are supported.
///
/// Names must start with a capital letter. Single-character connector words
/// (like "O'" in "O'Brien") are allowed within multi-word names.
///
/// Trailing punctuation is excluded from the name (e.g., `@Padraig,` yields `"Padraig"`).
///
/// Returns `None` if no valid `@mention` is found.
///
/// # Examples
///
/// ```
/// use parish_input::extract_mention;
///
/// let result = extract_mention("@Padraig hello there");
/// assert_eq!(result.unwrap().name, "Padraig");
///
/// let result = extract_mention("hello @Padraig");
/// assert_eq!(result.unwrap().name, "Padraig"); // also matches after whitespace
///
/// let result = extract_mention("no mention here");
/// assert!(result.is_none());
/// ```
/// Maximum number of words consumed for a multi-word `@mention` name.
const MAX_MENTION_NAME_WORDS: usize = 20;

pub fn extract_mention(raw: &str) -> Option<MentionExtraction> {
    let trimmed = raw.trim();

    // Find `@` anywhere in the input (at start, or preceded by a space)
    let at_pos = trimmed.find('@')?;
    if at_pos > 0 && !trimmed.as_bytes()[at_pos - 1].is_ascii_whitespace() {
        return None;
    }

    let rest = &trimmed[at_pos + 1..];
    if rest.is_empty() || rest.starts_with(' ') {
        return None;
    }

    // Name runs until we hit trailing punctuation or a word starting with lowercase.
    // Collect words, stripping trailing punctuation from the final word.
    let words: Vec<&str> = rest.splitn(MAX_MENTION_NAME_WORDS, ' ').collect();
    let mut name_end = 0;
    let mut final_word = String::new();

    for (i, word) in words.iter().enumerate() {
        if word.is_empty() {
            break;
        }

        let first_char = word.chars().next().unwrap_or(' ');

        if i == 0 {
            // First word must start with uppercase letter
            if !first_char.is_uppercase() {
                return None;
            }
            // Strip trailing punctuation from this word
            let word_stripped = word.trim_end_matches(|c: char| ".,:;!?".contains(c));
            final_word = word_stripped.to_string();
            name_end = 1;
            continue;
        }

        // For subsequent words:
        // - Single-character connector words (like "O'" or "D'") are always part of the name
        // - Multi-character words must start with uppercase to be part of the name
        // - Any word with trailing punctuation ends the name
        if word.ends_with(|c: char| ".,:;!?".contains(c)) {
            // This word has trailing punctuation - include it (stripped) and stop
            let word_stripped = word.trim_end_matches(|c: char| ".,:;!?".contains(c));
            if word_stripped.is_empty() {
                break;
            }
            // Check if this word is a valid name continuation
            let word_first_char = word_stripped.chars().next().unwrap_or(' ');
            if word_stripped.len() == 1 || word_first_char.is_uppercase() {
                name_end = i + 1;
            }
            break;
        }

        // Word without trailing punctuation
        if word.len() == 1 {
            // Single-character connector word - always include
            name_end = i + 1;
        } else if first_char.is_uppercase() {
            // Multi-character word starting with uppercase - include
            name_end = i + 1;
        } else {
            // Multi-character word starting with lowercase - stop
            break;
        }
    }

    // Build final name, using the stripped version for the first word
    let mut name_parts = vec![final_word];
    for i in 1..name_end {
        if i < words.len() {
            let word_stripped = words[i].trim_end_matches(|c: char| ".,:;!?".contains(c));
            name_parts.push(word_stripped.to_string());
        }
    }

    let name = name_parts.join(" ");
    if name.is_empty() {
        return None;
    }

    // Remaining = text before the @mention + text after the name
    let before = trimmed[..at_pos].trim();
    let remaining_words = if name_end < words.len() {
        words[name_end..].join(" ")
    } else {
        String::new()
    };
    let remaining = match (before.is_empty(), remaining_words.trim().is_empty()) {
        (true, true) => String::new(),
        (true, false) => remaining_words.trim().to_string(),
        (false, true) => before.to_string(),
        (false, false) => format!("{} {}", before, remaining_words.trim()),
    };

    Some(MentionExtraction { name, remaining })
}
#[cfg(test)]
mod tests {
    use super::*;

    // --- extract_mention tests ---
    #[test]
    fn test_extract_mention_simple_name() {
        let result = extract_mention("@Padraig hello there").unwrap();
        assert_eq!(result.name, "Padraig");
        assert_eq!(result.remaining, "hello there");
    }
    #[test]
    fn test_extract_mention_full_name() {
        let result = extract_mention("@Padraig Darcy hello").unwrap();
        assert_eq!(result.name, "Padraig Darcy");
        assert_eq!(result.remaining, "hello");
    }
    #[test]
    fn test_extract_mention_name_only() {
        let result = extract_mention("@Padraig").unwrap();
        assert_eq!(result.name, "Padraig");
        assert_eq!(result.remaining, "");
    }
    #[test]
    fn test_extract_mention_no_at() {
        assert!(extract_mention("hello there").is_none());
    }
    #[test]
    fn test_extract_mention_at_mid_input() {
        let result = extract_mention("hello @Padraig").unwrap();
        assert_eq!(result.name, "Padraig");
        assert_eq!(result.remaining, "hello");
    }
    #[test]
    fn test_extract_mention_at_not_after_space() {
        assert!(extract_mention("email@Padraig").is_none());
    }
    #[test]
    fn test_extract_mention_bare_at() {
        assert!(extract_mention("@").is_none());
    }
    #[test]
    fn test_extract_mention_at_space() {
        assert!(extract_mention("@ hello").is_none());
    }
    #[test]
    fn test_extract_mention_with_sentence() {
        let result = extract_mention("@Siobhan how are you today?").unwrap();
        assert_eq!(result.name, "Siobhan");
        assert_eq!(result.remaining, "how are you today?");
    }
    #[test]
    fn test_extract_mention_whitespace_trimmed() {
        let result = extract_mention("  @Padraig  hello  ").unwrap();
        assert_eq!(result.name, "Padraig");
        assert_eq!(result.remaining, "hello");
    }
    #[test]
    fn test_extract_mention_mid_with_rest() {
        let result = extract_mention("hello @Padraig how are you").unwrap();
        assert_eq!(result.name, "Padraig");
        assert_eq!(result.remaining, "hello how are you");
    }
    // --- Bug #669 tests: trailing punctuation, first-word casing, connector words ---
    #[test]
    fn test_extract_mention_trailing_punctuation() {
        // Bug 1: trailing punctuation should be stripped from the name
        let result = extract_mention("@Padraig, hello there").unwrap();
        assert_eq!(result.name, "Padraig");
        assert_eq!(result.remaining, "hello there");

        let result = extract_mention("@Siobhan! how are you").unwrap();
        assert_eq!(result.name, "Siobhan");
        assert_eq!(result.remaining, "how are you");

        let result = extract_mention("@Mary? yes").unwrap();
        assert_eq!(result.name, "Mary");
        assert_eq!(result.remaining, "yes");

        let result = extract_mention("@John: hello").unwrap();
        assert_eq!(result.name, "John");
        assert_eq!(result.remaining, "hello");

        let result = extract_mention("@Liam; great").unwrap();
        assert_eq!(result.name, "Liam");
        assert_eq!(result.remaining, "great");
    }
    #[test]
    fn test_extract_mention_trailing_punctuation_multiword() {
        // Trailing punctuation on the last word of a multi-word name
        let result = extract_mention("@Padraig Darcy, hello").unwrap();
        assert_eq!(result.name, "Padraig Darcy");
        assert_eq!(result.remaining, "hello");

        let result = extract_mention("@Mary Ann! how are you").unwrap();
        assert_eq!(result.name, "Mary Ann");
        assert_eq!(result.remaining, "how are you");
    }
    #[test]
    fn test_extract_mention_first_word_must_be_uppercase() {
        // Bug 2: First word must start with capital letter, return None otherwise
        assert!(extract_mention("@padraig hello").is_none());
        assert!(extract_mention("@john welcome").is_none());
        assert!(extract_mention("hello @siobhan").is_none());
        assert!(extract_mention("@mary").is_none());
        assert!(extract_mention("@123abc hello").is_none());

        // But uppercase should work
        let result = extract_mention("@Padraig hello").unwrap();
        assert_eq!(result.name, "Padraig");

        let result = extract_mention("@Mary there").unwrap();
        assert_eq!(result.name, "Mary");
    }
    #[test]
    fn test_extract_mention_connector_words() {
        // Bug 3: Single-character connector words like "O'" should be allowed in names
        let result = extract_mention("@Padraig O'Brien hello").unwrap();
        assert_eq!(result.name, "Padraig O'Brien");
        assert_eq!(result.remaining, "hello");

        // Single letter "D" in the middle should also be allowed
        let result = extract_mention("@Jean D'Arc").unwrap();
        assert_eq!(result.name, "Jean D'Arc");
        assert_eq!(result.remaining, "");

        // But a single lowercase letter still stops the name (unless preceded by connector punctuation)
        // Actually, single letters at the start should not be allowed as first word
        assert!(extract_mention("@o hello").is_none());

        // Single-character words can appear in the middle even without connector punctuation
        let result = extract_mention("@Mary A Smith").unwrap();
        assert_eq!(result.name, "Mary A Smith");
        assert_eq!(result.remaining, "");
    }
}
