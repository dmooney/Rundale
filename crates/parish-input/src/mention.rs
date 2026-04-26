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

    // Name runs until we hit a delimiter that signals end of the name portion.
    // Find where the name ends. Name = sequence of words where each word
    // starts with an uppercase letter or is a short connector (e.g., "O'Brien").
    // Once we hit a word starting with lowercase (and it's not a name particle),
    // that's the start of the remaining text.
    let words: Vec<&str> = rest.splitn(20, ' ').collect();
    let mut name_end = 0;

    for (i, word) in words.iter().enumerate() {
        let first_char = word.chars().next().unwrap_or(' ');
        if i == 0 {
            // First word is always part of the name
            name_end = 1;
            continue;
        }
        // If word starts with uppercase, it's likely part of the name
        if first_char.is_uppercase() {
            name_end = i + 1;
        } else {
            break;
        }
    }

    let name = words[..name_end].join(" ");
    // Remaining = text before the @mention + text after the name
    let before = trimmed[..at_pos].trim();
    let after = words[name_end..].join(" ");
    let remaining = match (before.is_empty(), after.trim().is_empty()) {
        (true, true) => String::new(),
        (true, false) => after.trim().to_string(),
        (false, true) => before.to_string(),
        (false, false) => format!("{} {}", before, after.trim()),
    };

    if name.is_empty() {
        return None;
    }

    Some(MentionExtraction { name, remaining })
}
