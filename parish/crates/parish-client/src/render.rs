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
}
