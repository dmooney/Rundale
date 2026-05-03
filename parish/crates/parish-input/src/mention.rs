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
    let words: Vec<&str> = rest.splitn(20, ' ').collect();
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
