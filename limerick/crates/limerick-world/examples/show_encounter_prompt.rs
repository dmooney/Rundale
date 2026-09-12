//! Demo: print the enrichment prompt for a few travel conditions.
//! Run with `cargo run --example show_encounter_prompt -p limerick-world`.

use limerick_types::LocationId;
use limerick_types::TimeOfDay;
use limerick_world::Weather;
use limerick_world::time::Season;
use limerick_world::wayfarers::{build_enrichment_prompt, encounter_seed, resolve_encounter};

fn main() {
    let scenarios = [
        (TimeOfDay::Morning, Season::Summer, Weather::Clear),
        (TimeOfDay::Dusk, Season::Autumn, Weather::Fog),
        (TimeOfDay::Midnight, Season::Winter, Weather::Storm),
    ];

    for (t, s, w) in scenarios {
        let seed = encounter_seed(100_000, LocationId(1), LocationId(2));
        let Some(canned) = resolve_encounter(t, s, w, seed) else {
            println!("=== {t:?} / {s:?} / {w:?} — no roll ===\n");
            continue;
        };
        let (system, context) = build_enrichment_prompt(&canned, t, s, w, seed);
        println!("=== {t:?} / {s:?} / {w:?} ===");
        println!("--- SYSTEM ---\n{system}\n");
        println!("--- CONTEXT ---\n{context}");
        println!("--- CANNED FALLBACK ---\n{}\n", canned.text);
        println!();
    }
}
