use crate::client::CommandResponse;

/// Singular/plural for a minute count: `"minute"` for exactly 1, else
/// `"minutes"` (#1156). `parish-client` is a standalone thin HTTP client
/// with no `parish-*` dependencies by design, so it carries its own copy
/// rather than pull in `parish_types::minute_word`.
fn minute_word(minutes: u64) -> &'static str {
    if minutes == 1 { "minute" } else { "minutes" }
}

pub fn render_response(resp: &CommandResponse) -> String {
    let mut out = String::new();

    // Header line: location | time | season · weather
    if let Some(state) = &resp.state {
        let w = &state.world;
        out.push_str(&format!(
            "{} | {} | {} · {}\n",
            w.location_name, w.time_label, w.season, w.weather
        ));

        // NPC line
        if !state.npcs_here.is_empty() {
            let npc_list: Vec<String> = state
                .npcs_here
                .iter()
                .map(|n| format!("{} ({})", n.name, n.occupation))
                .collect();
            out.push_str(&format!("NPCs: {}\n", npc_list.join(" · ")));
        }
        out.push('\n');
    }

    // Body: prose from lines
    for line in &resp.lines {
        match line.role.as_str() {
            "npc" | "Npc" => {
                out.push_str(&format!("{}: {}\n", line.speaker, line.text));
            }
            "player" | "Player" => {
                out.push_str(&format!("> {}\n", line.text));
            }
            _ => {
                if !line.text.is_empty() {
                    out.push_str(&line.text);
                    out.push('\n');
                }
            }
        }
    }

    // Travel summary if available
    if let Some(travel) = &resp.travel
        && resp.lines.is_empty()
    {
        out.push_str(&format!(
            "You travel from {} to {} ({} {}).\n",
            travel.from,
            travel.to,
            travel.duration_minutes,
            minute_word(travel.duration_minutes)
        ));
    }

    if out.is_empty() {
        out.push_str(&format!("[{}]\n", resp.outcome));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::CommandResponse;

    fn travel_response(minutes: u64) -> CommandResponse {
        serde_json::from_value(serde_json::json!({
            "outcome": "moved",
            "kind": "moved",
            "echo": "go",
            "lines": [],
            "travel": { "from": "The Crossroads", "to": "Darcy's Pub", "duration_minutes": minutes },
            "elapsed_ms": 0,
        }))
        .expect("valid CommandResponse")
    }

    #[test]
    fn travel_line_singular_for_one_minute() {
        // #1156: a 1-minute leg must read "1 minute", not "1 minutes".
        let out = render_response(&travel_response(1));
        assert!(out.contains("(1 minute)"), "got: {out}");
        assert!(!out.contains("(1 minutes)"), "got: {out}");
    }

    #[test]
    fn travel_line_plural_for_many_minutes() {
        let out = render_response(&travel_response(14));
        assert!(out.contains("(14 minutes)"), "got: {out}");
    }

    // AC-3: no lines, no travel, no state → fallback to "[<outcome>]\n"
    #[test]
    fn empty_response_falls_back_to_outcome_tag() {
        let resp: CommandResponse = serde_json::from_value(serde_json::json!({
            "outcome": "ok",
            "kind": "noop",
            "echo": "",
            "lines": [],
            "elapsed_ms": 0,
        }))
        .unwrap();
        let out = render_response(&resp);
        assert_eq!(out, "[ok]\n");
    }

    // AC-3: outcome tag uses the actual outcome value, not a hardcoded string
    #[test]
    fn outcome_tag_reflects_actual_outcome() {
        let resp: CommandResponse = serde_json::from_value(serde_json::json!({
            "outcome": "error",
            "kind": "error",
            "echo": "bad",
            "lines": [],
            "elapsed_ms": 0,
        }))
        .unwrap();
        let out = render_response(&resp);
        assert_eq!(out, "[error]\n");
    }

    // AC-3: state header renders as "location | time | season · weather\n"
    #[test]
    fn state_header_renders_correctly() {
        let resp: CommandResponse = serde_json::from_value(serde_json::json!({
            "outcome": "ok",
            "kind": "look",
            "echo": "look",
            "lines": [],
            "elapsed_ms": 0,
            "state": {
                "world": {
                    "location_name": "The Crossroads",
                    "time_label": "early morning",
                    "season": "Spring",
                    "weather": "misty"
                },
                "npcs_here": []
            }
        }))
        .unwrap();
        let out = render_response(&resp);
        assert!(
            out.starts_with("The Crossroads | early morning | Spring · misty\n"),
            "got: {out:?}"
        );
    }

    // AC-3: npcs_here list renders as "NPCs: Name (occupation) · …"
    #[test]
    fn npc_list_renders_with_occupation() {
        let resp: CommandResponse = serde_json::from_value(serde_json::json!({
            "outcome": "ok",
            "kind": "look",
            "echo": "look",
            "lines": [],
            "elapsed_ms": 0,
            "state": {
                "world": {
                    "location_name": "Village Square",
                    "time_label": "noon",
                    "season": "Summer",
                    "weather": "sunny"
                },
                "npcs_here": [
                    { "name": "Bridget", "occupation": "weaver" },
                    { "name": "Seamus", "occupation": "farmer" }
                ]
            }
        }))
        .unwrap();
        let out = render_response(&resp);
        assert!(
            out.contains("NPCs: Bridget (weaver) · Seamus (farmer)"),
            "got: {out:?}"
        );
    }

    // AC-3: single NPC renders without the separator
    #[test]
    fn single_npc_renders_without_separator() {
        let resp: CommandResponse = serde_json::from_value(serde_json::json!({
            "outcome": "ok",
            "kind": "look",
            "echo": "look",
            "lines": [],
            "elapsed_ms": 0,
            "state": {
                "world": {
                    "location_name": "Church",
                    "time_label": "dusk",
                    "season": "Autumn",
                    "weather": "overcast"
                },
                "npcs_here": [
                    { "name": "Father Doyle", "occupation": "priest" }
                ]
            }
        }))
        .unwrap();
        let out = render_response(&resp);
        assert!(out.contains("NPCs: Father Doyle (priest)"), "got: {out:?}");
        // The NPC line itself must not contain the multi-NPC separator.
        let npc_line = out.lines().find(|l| l.starts_with("NPCs:")).unwrap_or("");
        assert!(
            !npc_line.contains(" · "),
            "unexpected separator in NPC line for single NPC: {npc_line:?}"
        );
    }

    // AC-3: travel line is NOT rendered when lines is non-empty (prose takes precedence)
    #[test]
    fn travel_not_rendered_when_lines_present() {
        let resp: CommandResponse = serde_json::from_value(serde_json::json!({
            "outcome": "moved",
            "kind": "moved",
            "echo": "go north",
            "lines": [{ "id": "1", "role": "narrator", "speaker": "", "text": "You walk north." }],
            "travel": { "from": "A", "to": "B", "duration_minutes": 5 },
            "elapsed_ms": 0,
        }))
        .unwrap();
        let out = render_response(&resp);
        assert!(
            !out.contains("You travel from"),
            "travel line should be suppressed: {out:?}"
        );
        assert!(out.contains("You walk north."), "got: {out:?}");
    }
}
