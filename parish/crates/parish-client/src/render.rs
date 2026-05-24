use crate::client::CommandResponse;

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
            "You travel from {} to {} ({} minutes).\n",
            travel.from, travel.to, travel.duration_minutes
        ));
    }

    if out.is_empty() {
        out.push_str(&format!("[{}]\n", resp.outcome));
    }

    out
}
