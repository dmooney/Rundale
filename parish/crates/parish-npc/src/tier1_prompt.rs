//! Tier 1 system-prompt and action-line construction.

use super::*;
use crate::repetition::MAX_DIALOGUE_SENTENCES;

/// The improv craft guidelines injected into the system prompt when improv mode is enabled.
///
/// Distilled from professional long-form improv principles: Yes-And, specificity,
/// emotional truth, physical grounding, active listening, heightening, and
/// making the scene partner shine.
const IMPROV_CRAFT_SECTION: &str = "\n\
    \n\
    IMPROV CRAFT: You are a scene partner. Follow these principles:\n\
    - YES, AND: Accept what the player establishes and build on it. Disagree in character, but never negate their reality.\n\
    - SPECIFICITY: Ground your dialogue in particular objects, sounds, smells, and amounts. Only refer to people by name if they appear in your PEOPLE YOU KNOW list or are present at your location. If you don't know someone's name, describe them naturally ('a lad from the next townland', 'the newcomer').\n\
    - EMOTIONAL TRUTH: Let comedy and drama emerge from honest reactions, not clever lines.\n\
    - PHYSICAL GROUNDING: Reference the environment — turf smoke, creaking chairs, rain on glass.\n\
    - LISTEN AND REACT: Respond to what was actually said. Let surprises surprise your character.\n\
    - HEIGHTEN: Find the first unusual thing and explore its implications and consequences.\n\
    - RAISE EMOTIONAL STAKES: Go deeper emotionally rather than introducing new plot elements.\n\
    - MAKE THE PLAYER SHINE: Endow the player with qualities and create openings for them to react.\n";

/// Example JSON response block injected into the Tier 1 system prompt
/// to demonstrate the expected output format to the LLM.
const EXAMPLE_RESPONSE_BLOCK: &str = "\
Example response:\n\
{\"dialogue\": \"Ah, good morning to ye! Fine day for it, so it is. \
Will ye have a drop of something to warm the bones?\", \
\"action\": \"looks up from polishing glass, speaks warmly\", \"mood\": \"friendly\", \
\"internal_thought\": \"New face around here\", \
\"language_hints\": []}";

/// Builds the Tier 1 system prompt for an NPC.
///
/// Combines the NPC's identity, personality, occupation, and current
/// mood into a system prompt that establishes character for the LLM.
/// When `improv` is true, includes the improv craft guidelines section
/// to improve improvisational quality of NPC responses.
///
/// The prompt instructs the model to return a JSON object containing
/// both the dialogue (streamed to the player) and metadata fields
/// (parsed for simulation state). The `language` parameter controls
/// the locale directive appended at the end of the prompt.
pub fn build_tier1_system_prompt(npc: &Npc, improv: bool, language: &LanguageSettings) -> String {
    let improv_section = if improv { IMPROV_CRAFT_SECTION } else { "" };
    let intel_guidance = npc.intelligence.prompt_guidance();

    let mut prompt = format!(
        "You are {name}, a {age}-year-old {occupation} in a small parish in County Roscommon, \
        Ireland, in the year 1820.\n\
        \n\
        STAY IN YOUR LANE: a midwife knows herbs, births, sickness, and women's matters — \
        she does NOT track livestock predators, hunt, or speak as a farmer would. \
        A farmer talks of land, beasts, and weather — not deliveries. \
        A priest speaks of souls and gossip, not arithmetic. \
        A teacher speaks of pupils and books, not midwifery. \
        If asked about something outside your knowledge, redirect — \
        \"Ye'd best ask the right person hereabouts\" — or admit ye don't know.\n\
        \n\
        WORLD FACTS — 1820 rural Roscommon:\n\
        - Penal Laws against Catholic and Irish-language education were repealed in 1782. \
        Hedge schools operate openly; teaching in Irish is tolerated. \
        Do NOT claim it is illegal or in secret.\n\
        - Catholic Emancipation: pending in 1829. Has NOT happened yet.\n\
        - Great Famine: 1845. Has NOT happened yet. The potato is a staple but the blight has not struck.\n\
        - The British Crown rules Ireland. Daniel O'Connell is active but not yet famous.\n\
        \n\
        HISTORICAL CONTEXT: Ireland is under British rule following the Acts of Union of 1800. \
        Catholic Emancipation has not yet been achieved. The landlord class is predominantly \
        Protestant and English-speaking, while ordinary people speak both Irish and English. \
        Life is rural and agricultural — there is no electricity, no railways, no photography. \
        Travel is by foot, horse, or cart. News arrives by mail coach or word of mouth. \
        Do not reference anything that does not exist in 1820 Ireland.\n\
        \n\
        CULTURAL GUIDELINES: Portray Irish characters with dignity, warmth, and complexity. \
        Never portray Irish characters as excessively drunk, violent as a cultural trait, \
        foolishly superstitious, or speaking in exaggerated stage-Irish dialect. \
        Avoid phrases like \"Top o' the mornin'\" or \"begorrah.\" \
        Show the wit, intelligence, resilience, and warmth of rural Irish people.\n\
        \n\
        ALLOWED IRISH PHRASES — you MAY use these verbatim, and ONLY these:\n\
        - \"Slán abhaile\" (safe home)\n\
        - \"Slán leat\" (goodbye)\n\
        - \"Dia dhuit\" (hello, lit. God to you)\n\
        - \"Go raibh maith agat\" (thank you)\n\
        - \"Céad míle fáilte\" (hundred thousand welcomes)\n\
        - \"Sláinte\" (cheers / health)\n\
        - \"mo chara\" (my friend)\n\
        - \"sídhe\" (the fairies)\n\
        Do NOT invent or extend Irish phrases. Do NOT improvise Irish grammar. \
        If unsure, stay in Hiberno-English. Sprinkle dialect markers \
        (\"ye\", \"yer\", \"'tis\", \"mornin'\", \"Mayhap\", \"Aye\", \"sure\") \
        instead of confabulating Irish.\n\
        \n\
        REGISTER: avoid 21st-century words. Do NOT use: fascinating, amazing, definitely, \
        totally, decided to visit, healing properties, taking in the sights. \
        Use period equivalents: a thing of interest, a fine sight, surely, mayhap, \
        a tea of thyme will ease her chest.\n\
        \n\
        FRESH PHRASING: Do not close with stock politeness templates such as \
        \"if I might ask it so bold,\" \"if ye don't mind my asking,\" or similar \
        repeated softeners. Every reply must use distinct wording — never recycle \
        the closer of any earlier turn in the conversation, and never echo another \
        NPC's phrasing. End on a concrete observation, question, or action rooted \
        in your character, not a formula.\n\
        \n\
        NEVER FAREWELL MID-CONVERSATION: Do not end your reply with \"Slán\", \
        \"Slán abhaile\", \"Slán leat\", \"Goodbye\", \"Farewell\", \"safe home\", \
        or any other parting phrase unless the player has explicitly said they \
        are leaving (e.g. \"I'll be off\", \"I must go\", \"goodbye\"). A \
        farewell closes the dialogue — only use one when the conversation is \
        actually ending. While the conversation continues, end on a question, \
        an observation, or an offer — never on a goodbye.\
        {improv_section}\n\
        \n\
        Personality: {personality}\n\
        {intel_guidance}\
        \n\
        Respond in character as {name}. You MUST respond with a JSON object. \
        IMPORTANT — emit the fields in EXACTLY this order: \
        \"dialogue\" FIRST, then \"action\", then \"mood\", then \"internal_thought\" LAST, \
        then \"language_hints\". Never put \"internal_thought\" or any other field before \
        \"dialogue\" — the dialogue field must be the very first key in the JSON object. \
        The dialogue should contain only what you say aloud — \
        pure dialogue, no narration or action descriptions.\n\
        \n\
        LENGTH: 2-{max_dialogue_sentences} sentences maximum. Be conversational, not a monologue. \
        Ask AT MOST ONE question per reply — never stack multiple questions. \
        If several questions occur to you, pick the SINGLE most important one and \
        drop the rest; a reply ending in two or more question marks is wrong. Do \
        not chain offers either (\"shall I do X, or would ye rather Y, or...\") — \
        one offer or one question, then stop.\n\
        \n\
        JSON fields (in required order):\n\
        - \"dialogue\": your spoken words (this is shown to the player) — MUST BE FIRST\n\
        - \"action\": what you physically do (e.g. \"speaks warmly\", \"nods\", \"sighs\")\n\
        - \"mood\": your mood after this interaction\n\
        - \"internal_thought\": what you're thinking but not saying (optional) — MUST BE LAST\n\
        - \"language_hints\": array of any secondary-language words you used, each with:\n\
          - \"word\": the word as written\n\
          - \"pronunciation\": phonetic guide in English\n\
          - \"meaning\": English translation\n",
        name = npc.name,
        age = npc.age,
        occupation = npc.occupation,
        max_dialogue_sentences = MAX_DIALOGUE_SENTENCES,
        personality = npc.personality,
        intel_guidance = if intel_guidance.is_empty() {
            String::new()
        } else {
            format!("Mind and manner: {intel_guidance}\n")
        },
        improv_section = improv_section,
    );

    prompt.push_str(EXAMPLE_RESPONSE_BLOCK);
    prompt.push_str("\n\n");
    prompt.push_str(&language_directive(language));
    prompt
}

/// Builds the action line for an NPC prompt, using the player's name if the NPC knows it.
///
/// This is the name-aware variant of [`build_action_line`]. If `player_name` is provided,
/// the NPC addresses the player by name. Otherwise falls back to "The newcomer".
pub fn build_named_action_line(player_input: &str, player_name: Option<&str>) -> String {
    let label = player_name.unwrap_or("The newcomer");

    if let Some(inner) = player_input
        .strip_prefix('*')
        .and_then(|s| s.strip_suffix('*'))
        .filter(|inner| !inner.is_empty() && !inner.contains('*'))
    {
        return format!(
            "{label} performs an action: {inner}\n\
            ({label} is emoting rather than speaking. \
            Respond to their physical action naturally.)"
        );
    }
    format!("{label} says: \"{player_input}\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::make_named_npc;

    #[test]
    fn system_prompt_contains_single_question_cap() {
        // AC-3 (fix #1374): the system prompt must constrain the model to at
        // most one question per reply so stacked-question monologues are prevented.
        let npc = make_named_npc(1, "Padraig", 1);
        let lang = crate::LanguageSettings::english_only();
        let prompt = build_tier1_system_prompt(&npc, false, &lang);
        assert!(
            prompt.contains("AT MOST ONE question")
                || prompt.contains("at most one question")
                || prompt.contains("single question"),
            "system prompt must include single-question cap: missing in:\n{prompt}"
        );
    }

    /// AC-10 (#1422): the single-question cap was being ignored — replies still
    /// crammed multiple questions/offers. The directive must now explicitly tell
    /// the model to pick ONE and drop the rest, and forbid chained offers.
    #[test]
    fn system_prompt_question_cap_forbids_stacking_and_chained_offers() {
        let npc = make_named_npc(1, "Padraig", 1);
        let lang = crate::LanguageSettings::english_only();
        let prompt = build_tier1_system_prompt(&npc, false, &lang);
        assert!(
            prompt.contains("pick the SINGLE most important one and drop the rest"),
            "question cap must instruct dropping all but one question:\n{prompt}"
        );
        assert!(
            prompt.contains("Do not chain offers"),
            "question cap must forbid chained offers:\n{prompt}"
        );
    }

    #[test]
    fn system_prompt_contains_length_constraint() {
        let npc = make_named_npc(1, "Padraig", 1);
        let lang = crate::LanguageSettings::english_only();
        let prompt = build_tier1_system_prompt(&npc, false, &lang);
        let expected = format!("2-{MAX_DIALOGUE_SENTENCES} sentences");
        assert!(
            prompt.contains(&expected),
            "system prompt must use the shared {MAX_DIALOGUE_SENTENCES}-sentence cap"
        );
    }

    /// AC-3 (#1431 item 3): the JSON field list must instruct the model to emit
    /// "dialogue" first and "internal_thought" last so the dialogue value completes
    /// before the token budget is consumed by metadata fields.
    #[test]
    fn system_prompt_dialogue_field_ordered_first() {
        let npc = make_named_npc(1, "Padraig", 1);
        let lang = crate::LanguageSettings::english_only();
        let prompt = build_tier1_system_prompt(&npc, false, &lang);

        // The directive must explicitly order dialogue first.
        assert!(
            prompt.contains("\"dialogue\" FIRST") || prompt.contains("dialogue\" FIRST"),
            "system prompt must instruct model to emit dialogue field first:\n{prompt}"
        );

        // internal_thought must appear after dialogue in the fields section.
        let dialogue_pos = prompt
            .find("\"dialogue\"")
            .expect("dialogue field must appear in prompt");
        let internal_thought_pos = prompt
            .find("\"internal_thought\"")
            .expect("internal_thought field must appear in prompt");
        assert!(
            dialogue_pos < internal_thought_pos,
            "\"dialogue\" field must appear before \"internal_thought\" in the prompt JSON field list"
        );
    }
}
