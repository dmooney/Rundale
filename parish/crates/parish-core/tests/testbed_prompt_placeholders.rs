//! mods TD-003: content-level contract test for the **testbed** mod prompts.
//!
//! The Rundale base-mod prompts are already covered by the
//! `test_tier1_*_no_unsubstituted_placeholders` tests in `parish-npc`.
//! The testbed mod deliberately uses a *different* placeholder vocabulary
//! (`{npc_name}`, `{npc_brief_description}`, …) and those single-brace
//! `.txt` templates had no contract test — a typo'd or renamed placeholder
//! would pass silently through to runtime.
//!
//! This test renders every checked-in `mods/testbed/prompts/*.txt` template
//! with the full documented placeholder set and fails if any `{placeholder}`
//! token survives substitution. It also fails if a template references a
//! placeholder that is NOT in the documented set (catching a template typo or
//! a new placeholder added without wiring it into the substitution map).

use std::collections::BTreeMap;
use std::path::PathBuf;

use regex::Regex;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("CARGO_MANIFEST_DIR should resolve to the repo root via ../../..")
}

fn testbed_prompt(name: &str) -> String {
    let path = repo_root().join("mods/testbed/prompts").join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "expected testbed prompt at {}; this test must fail loudly instead of skipping ({e})",
            path.display()
        )
    })
}

/// The complete documented placeholder vocabulary for testbed prompts, with a
/// representative value for each. A template using a key NOT listed here will
/// leave a `{key}` token behind and trip the leftover-token assertion below.
fn testbed_placeholders() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([
        // tier1_system + tier2_system
        ("npc_name", "Alpha"),
        ("npc_brief_description", "a blueprint-grey test agent"),
        ("npc_occupation", "Test Agent"),
        ("npc_personality", "Methodical and dry-humoured"),
        ("npc_mood", "focused"),
        // tier1_context
        ("location_name", "Origin"),
        ("time", "midday"),
        ("weather", "clear"),
        ("npcs_here", "Beta, Gamma"),
        ("recent_events", "The grid hummed quietly."),
        ("player_input", "What is this place?"),
    ])
}

/// Substitutes every `{key}` token using `vars`. Single-brace syntax matches
/// the testbed `.txt` template format. Unknown keys are left intact so the
/// leftover-token check can flag them.
fn substitute(template: &str, vars: &BTreeMap<&str, &str>) -> String {
    let re = Regex::new(r"\{([a-z_]+)\}").unwrap();
    re.replace_all(template, |caps: &regex::Captures| {
        let key = &caps[1];
        vars.get(key).map(|v| v.to_string()).unwrap_or_else(|| {
            // Leave unknown placeholders intact so the assertion catches them.
            format!("{{{key}}}")
        })
    })
    .into_owned()
}

#[test]
fn every_testbed_prompt_has_all_placeholders_substituted() {
    let vars = testbed_placeholders();
    let leftover = Regex::new(r"\{[a-z_]+\}").unwrap();

    for name in ["tier1_system.txt", "tier1_context.txt", "tier2_system.txt"] {
        let template = testbed_prompt(name);
        let rendered = substitute(&template, &vars);
        assert!(
            !leftover.is_match(&rendered),
            "unsubstituted placeholder in testbed prompt {name}: {:?}\n\
             (add it to testbed_placeholders() or fix the template)",
            leftover.find(&rendered).map(|m| m.as_str())
        );
    }
}

#[test]
fn testbed_system_prompt_substitutes_known_values() {
    let vars = testbed_placeholders();
    let rendered = substitute(&testbed_prompt("tier1_system.txt"), &vars);
    assert!(rendered.contains("Alpha"), "npc_name not substituted");
    assert!(
        rendered.contains("Test Agent"),
        "npc_occupation not substituted"
    );
    assert!(
        rendered.contains("Methodical and dry-humoured"),
        "npc_personality not substituted"
    );
}

#[test]
fn testbed_context_prompt_substitutes_known_values() {
    let vars = testbed_placeholders();
    let rendered = substitute(&testbed_prompt("tier1_context.txt"), &vars);
    assert!(rendered.contains("Origin"), "location_name not substituted");
    assert!(rendered.contains("midday"), "time not substituted");
    assert!(rendered.contains("clear"), "weather not substituted");
    assert!(
        rendered.contains("Beta, Gamma"),
        "npcs_here not substituted"
    );
}
