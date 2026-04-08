//! Prompt loading and rendering for `.prompt.yml` files.
//!
//! Parish stores LLM prompts in [GitHub Models prompt format][gh-models]:
//! versioned YAML files alongside the Rust code that uses them. They can be
//! opened directly in the GitHub Models playground for fast iteration on
//! wording and for evaluating different cloud models against the same prompt.
//!
//! [gh-models]: https://docs.github.com/en/github-models/use-github-models/storing-prompts-in-github-repositories
//!
//! # Conditional sections
//!
//! The `.prompt.yml` format only supports flat `{{key}}` substitution — no
//! conditionals, loops, or sections. To stay compatible, Parish pre-renders
//! every conditional or repeated chunk in Rust into a named string variable
//! (which may be empty), then performs substitution. See
//! [`crate::npc::build_tier1_system_prompt`] for the canonical example.
//!
//! # Loading
//!
//! Prompt files are embedded at compile time via `include_str!()` and parsed
//! once into a [`PromptFile`], typically behind a [`std::sync::LazyLock`]:
//!
//! ```ignore
//! use std::sync::LazyLock;
//! use parish_core::prompts::PromptFile;
//!
//! static MY_PROMPT: LazyLock<PromptFile> = LazyLock::new(|| {
//!     PromptFile::parse(include_str!("../../prompts/my_prompt.prompt.yml"))
//! });
//! ```

use serde::Deserialize;

/// A parsed `.prompt.yml` file.
///
/// Only the runtime fields (`messages`) are required. Playground metadata
/// (`name`, `description`, `model`, `modelParameters`, `testData`,
/// `evaluators`) is permitted but ignored at runtime — those fields exist
/// for the GitHub Models playground and any CI eval workflows.
#[derive(Debug, Deserialize)]
pub struct PromptFile {
    /// Ordered list of role/content message templates.
    pub messages: Vec<Message>,
}

/// A single role/content pair from a prompt file's `messages` array.
///
/// `role` is conventionally `"system"`, `"user"`, or `"assistant"`.
/// `content` is the raw template text containing `{{key}}` placeholders.
#[derive(Debug, Clone, Deserialize)]
pub struct Message {
    /// The chat role for this message.
    pub role: String,
    /// The template text, with `{{key}}` placeholders to be substituted.
    pub content: String,
}

impl PromptFile {
    /// Parses a YAML string into a [`PromptFile`].
    ///
    /// # Panics
    ///
    /// Panics if the YAML is malformed or missing the `messages` field.
    /// Prompt files are embedded at compile time via [`include_str!`], so a
    /// parse failure indicates a development-time bug in the prompt file
    /// itself, not a recoverable runtime condition.
    pub fn parse(yaml: &str) -> Self {
        serde_yml::from_str(yaml).expect("malformed .prompt.yml — fix the embedded file")
    }

    /// Returns all `system`-role messages joined with a blank line, after
    /// `{{key}}` substitution.
    ///
    /// `vars` is a slice of `(key, value)` pairs. Each `{{key}}` placeholder
    /// in the message content is replaced with its corresponding value.
    /// Unknown placeholders are left unchanged so the rendered output remains
    /// debuggable.
    pub fn render_system(&self, vars: &[(&str, &str)]) -> String {
        let parts: Vec<String> = self
            .messages
            .iter()
            .filter(|m| m.role == "system")
            .map(|m| substitute(&m.content, vars))
            .collect();
        parts.join("\n\n")
    }
}

/// Replaces every `{{key}}` placeholder with its corresponding value.
///
/// Performs a single forward pass over `template`. Substituted values are
/// appended to the output verbatim and are never re-scanned, so a value
/// containing `{{...}}` will pass through unchanged. Placeholders whose key
/// is not present in `vars` are left as-is in the output (to keep render
/// failures debuggable). Whitespace inside the braces is trimmed, so both
/// `{{key}}` and `{{ key }}` resolve to the same binding.
fn substitute(template: &str, vars: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after_open = &rest[start + 2..];
        let Some(end_rel) = after_open.find("}}") else {
            // Unterminated placeholder — emit the remainder verbatim and stop.
            out.push_str(&rest[start..]);
            return out;
        };
        let key = after_open[..end_rel].trim();
        let consumed = start + 2 + end_rel + 2;
        if let Some((_, val)) = vars.iter().find(|(k, _)| *k == key) {
            out.push_str(val);
        } else {
            // Unknown key — leave the placeholder intact for visibility.
            out.push_str(&rest[start..consumed]);
        }
        rest = &rest[consumed..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
name: Test
messages:
  - role: system
    content: |-
      Hello {{name}}, you are {{age}}.
  - role: user
    content: "What is {{thing}}?"
"#;

    #[test]
    fn parses_minimal_file() {
        let file = PromptFile::parse(SAMPLE);
        assert_eq!(file.messages.len(), 2);
        assert_eq!(file.messages[0].role, "system");
        assert_eq!(file.messages[1].role, "user");
    }

    #[test]
    fn substitutes_known_variables() {
        let out = substitute("Hello {{name}}!", &[("name", "world")]);
        assert_eq!(out, "Hello world!");
    }

    #[test]
    fn leaves_unknown_variables_unchanged() {
        let out = substitute("Hello {{name}}, age {{age}}.", &[("name", "Padraig")]);
        assert_eq!(out, "Hello Padraig, age {{age}}.");
    }

    #[test]
    fn substitutes_multiple_occurrences() {
        let out = substitute("{{x}} and {{x}} again", &[("x", "foo")]);
        assert_eq!(out, "foo and foo again");
    }

    #[test]
    fn does_not_rescan_substituted_values() {
        // A value containing {{other}} should pass through unchanged even if
        // we also have a binding for `other`.
        let out = substitute("{{a}}", &[("a", "{{b}}"), ("b", "should-not-appear")]);
        assert_eq!(out, "{{b}}");
    }

    #[test]
    fn render_system_only_returns_system_messages() {
        let file = PromptFile::parse(SAMPLE);
        let out = file.render_system(&[("name", "Padraig"), ("age", "58")]);
        assert_eq!(out, "Hello Padraig, you are 58.");
    }

    #[test]
    fn render_system_joins_multiple_system_messages_with_blank_line() {
        let yaml = r#"
messages:
  - role: system
    content: "First."
  - role: user
    content: "Skipped."
  - role: system
    content: "Second."
"#;
        let file = PromptFile::parse(yaml);
        let out = file.render_system(&[]);
        assert_eq!(out, "First.\n\nSecond.");
    }

    #[test]
    fn substitution_handles_braces_in_template_safely() {
        // Single braces (e.g. JSON examples in the prompt body) must survive.
        let out = substitute(
            r#"Output: {"key": "value"} for {{name}}"#,
            &[("name", "test")],
        );
        assert_eq!(out, r#"Output: {"key": "value"} for test"#);
    }
}
