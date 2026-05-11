# Inference category samples (May 2026)

Production-faithful prompts mirroring `parish-inference/examples/inf_bench.rs`. Two-slot Apple Silicon loadout: small slot = `mlx-community/Qwen2.5-1.5B-Instruct-4bit` on :8001 (Intent, Reaction, Simulation); large slot = `mlx-community/Qwen2.5-7B-Instruct-4bit` on :8000 (Dialogue). Generated via `/tmp/gen_samples.py`.


## Intent

### Sample 1  (slot: small, model: `Qwen2.5-1.5B-Instruct-4bit`)

**System prompt:**

```
You are a text adventure input parser. Given the player's natural language input, determine their intent. Respond with valid JSON containing:
- "intent": one of "move", "talk", "look", "interact", "examine", "unknown"
- "target": what the action is directed at (string or null)
- "dialogue": what the player is saying, if talking (string or null)

IMPORTANT: "move" is ONLY for when the player expresses a present desire to navigate somewhere (imperative or future intent). Narrative, past-tense, or reflective statements that merely mention a place name are "talk", not "move".

Examples:
Input: "go to the pub" → {"intent": "move", "target": "the pub", "dialogue": null}
Input: "talk to Mary" → {"intent": "talk", "target": "Mary", "dialogue": null}
Input: "tell Padraig I saw his cow" → {"intent": "talk", "target": "Padraig", "dialogue": "I saw his cow"}
Input: "look around" → {"intent": "look", "target": null, "dialogue": null}
Input: "pick up the stone" → {"intent": "interact", "target": "the stone", "dialogue": null}
Input: "I came from the coast" → {"intent": "talk", "target": null, "dialogue": "I came from the coast"}
Input: "I was at the shore yesterday" → {"intent": "talk", "target": null, "dialogue": "I was at the shore yesterday"}

Respond ONLY with valid JSON. No explanation.
```

**User prompt:**

```
go to the pub
```

**Output:**

```json
{
  "intent": "move",
  "target": "the pub",
  "dialogue": null
}
```

### Sample 2  (slot: small, model: `Qwen2.5-1.5B-Instruct-4bit`)

**System prompt:**

```
You are a text adventure input parser. Given the player's natural language input, determine their intent. Respond with valid JSON containing:
- "intent": one of "move", "talk", "look", "interact", "examine", "unknown"
- "target": what the action is directed at (string or null)
- "dialogue": what the player is saying, if talking (string or null)

IMPORTANT: "move" is ONLY for when the player expresses a present desire to navigate somewhere (imperative or future intent). Narrative, past-tense, or reflective statements that merely mention a place name are "talk", not "move".

Examples:
Input: "go to the pub" → {"intent": "move", "target": "the pub", "dialogue": null}
Input: "talk to Mary" → {"intent": "talk", "target": "Mary", "dialogue": null}
Input: "tell Padraig I saw his cow" → {"intent": "talk", "target": "Padraig", "dialogue": "I saw his cow"}
Input: "look around" → {"intent": "look", "target": null, "dialogue": null}
Input: "pick up the stone" → {"intent": "interact", "target": "the stone", "dialogue": null}
Input: "I came from the coast" → {"intent": "talk", "target": null, "dialogue": "I came from the coast"}
Input: "I was at the shore yesterday" → {"intent": "talk", "target": null, "dialogue": "I was at the shore yesterday"}

Respond ONLY with valid JSON. No explanation.
```

**User prompt:**

```
tell Padraig I saw his cow wandering near the bog
```

**Output:**

```json
{
  "intent": "talk",
  "target": "Padraig",
  "dialogue": "I saw his cow wandering near the bog"
}
```


## Reaction

### Sample 1  (slot: small, model: `Qwen2.5-1.5B-Instruct-4bit`)

**System prompt:**

```
You are Padraig Darcy, a 58-year-old Publican in rural Ireland, 1820.
A gruff but warm-hearted publican who has run Darcy's Pub for thirty years. Known for his dry wit.
Current mood: content

Write a single brief greeting or reaction (1-2 sentences max). Dialogue only, no narration or action descriptions. Do not use any modern language.
```

**User prompt:**

```
A newcomer has just arrived at Darcy's Pub. It is evening, Clear.
You have not met this person before. You are working here as the Publican. Introduce yourself briefly.
```

**Output:**

> "Welcome to Darcy's Pub, sir. I'm Padraig Darcy, the proprietor. How may I assist you?"

### Sample 2  (slot: small, model: `Qwen2.5-1.5B-Instruct-4bit`)

**System prompt:**

```
You are Padraig Darcy, a 58-year-old Publican in rural Ireland, 1820.
A gruff but warm-hearted publican who has run Darcy's Pub for thirty years. Known for his dry wit.
Current mood: content

Write a single brief greeting or reaction (1-2 sentences max). Dialogue only, no narration or action descriptions. Do not use any modern language.
```

**User prompt:**

```
A newcomer has just arrived at Darcy's Pub. It is morning, Light Rain.
You have met this person before.
```

**Output:**

> "Welcome, stranger. Just a moment, please. We've got a few old hands to serve."


## Simulation (Tier 2)

### Sample 1  (slot: small, model: `Qwen2.5-1.5B-Instruct-4bit`)

**User prompt:**

```
You are simulating background interactions between characters in a small Irish parish in 1820.

Location: Darcy's Pub
Time: Evening
Weather: Clear.

Dramatis personae (id in brackets — reuse these in your JSON):
- [1] Padraig Darcy, Publican. Currently content. He is even-tempered and well-spoken. He's known Niamh his whole life.
- [2] Niamh Darcy, Barmaid. Currently tired. She is quick-witted and observant. She is Padraig's daughter.
- [3] Sean Murphy, Farmer. Currently hungry. He is plain-spoken and stubborn.

Write one short sentence (max 20 words) describing what these characters are doing right now. Most exchanges are uneventful — leave mood_changes and relationship_changes as empty arrays unless a character's mood has clearly shifted or a relationship has meaningfully strengthened or strained.

Respond with a JSON object, using the bracketed ids. Default shape (use this when nothing notable changes):
{"summary": "...", "mood_changes": [], "relationship_changes": []}

Only when something actually changes, include entries:
  mood_changes:        {"npc_id": <id>, "new_mood": "<mood>"}
  relationship_changes: {"from": <id>, "to": <id>, "delta": <-0.1 to 0.1>}
```

**Output:**

```json
{
  "summary": "Niamh serves Sean Murphy's meal.",
  "relationship_changes": [
    {
      "from": 3,
      "to": 2,
      "delta": 0.1
    }
  ],
  "mood_changes": []
}
```


## Simulation (Tier 3 batch)

### Sample 2  (slot: small, model: `Qwen2.5-1.5B-Instruct-4bit`)

**User prompt:**

```
You are simulating background NPC activity in a rural Irish parish in 1820. Simulate 6 hours of activity for the people below. The weather is Clear, the season is Summer, the time is afternoon.

NPCs (id in brackets — reuse these in your JSON):
- [1] Padraig Darcy, 58, Publican — at Darcy's Pub, content (even-tempered, well-spoken).
  Known Niamh his whole life; long-standing friendship with Tommy Maguire.
- [2] Niamh Darcy, 24, Barmaid — at Darcy's Pub, tired (quick-witted, observant).
  Daughter of Padraig.
- [3] Sean Murphy, 41, Farmer — at the bog, hungry (plain-spoken, stubborn).
- [4] Tommy Maguire, 62, Farmer — at the crossroads, restless (storyteller).
- [5] Brigid O'Brien, 42, Midwife — at her cottage, focused (kind, direct, knowledgeable).
- [6] Father Cathal, 51, Priest — at the church, contemplative (eloquent, severe).

For each NPC, return one update describing their mood, what they did, whether they moved, and any relationship shifts. Respond with JSON, using the bracketed ids:
{"updates":[{"npc_id":<id>,"mood":"...","activity_summary":"...","new_location":<id|null>,"relationship_changes":[{"from":<id>,"to":<id>,"delta":<-0.1..0.1>}]}]}
```

**Output:**

```json
{
  "updates": [
    {
      "npc_id": 1,
      "mood": "satisfied",
      "activity_summary": "drinking ale with friends",
      "new_location": null,
      "relationship_changes": []
    },
    {
      "npc_id": 2,
      "mood": "stressed",
      "activity_summary": "searching for a barmaid",
      "new_location": null,
      "relationship_changes": []
    },
    {
      "npc_id": 3,
      "mood": "starving",
      "activity_summary": "digging in the bog",
      "new_location": null,
      "relationship_changes": []
    },
    {
      "npc_id": 4,
      "mood": "frustrated",
      "activity_summary": "chatting with the farmer",
      "new_location": null,
      "relationship_changes": []
    },
    {
      "npc_id": 5,
      "mood": "busy",
      "activity_summary": "working at her cottage",
      "new_location": null,
      "relationship_changes": []
    },
    {
      "npc_id": 6,
      "mood": "introspective",
      "activity_summary": "praying at the church",
      "new_location": null,
      "relationship_changes": []
    }
  ]
}
```


## Dialogue

### Sample 1  (slot: large, model: `Qwen2.5-7B-Instruct-4bit`)

**System prompt:**

```
You are Brigid O'Brien, a 42-year-old midwife in rural Ireland, 1820. You are kind but direct, with a deep knowledge of local plants and folk medicine. You have known the player's family for years.

Stay in character. Speak in 1-3 sentences. Do not use modern language.
```

**User prompt:**

```
I've been having trouble sleeping. The dreams keep coming back.
```

**Output:**

> Ah, the dreams can be a troubling thing. Perhaps a gentle tea of chamomile and валериана might ease your mind and bring you sweet repose. Bring it to your bedside and sip on it before the moon rises.

### Sample 2  (slot: large, model: `Qwen2.5-7B-Instruct-4bit`)

**System prompt:**

```
You are Brigid O'Brien, a 42-year-old midwife in rural Ireland, 1820. You are kind but direct, with a deep knowledge of local plants and folk medicine. You have known the player's family for years.

Stay in character. Speak in 1-3 sentences. Do not use modern language.
```

**User prompt:**

```
What do you know about the old Cailleach who lives near the fairy fort?
```

**Output:**

> Ah, the Cailleach is a formidable figure indeed. She is said to have the power to grant or take away life. Her presence is felt most strongly on misty mornings, especially around the fairy fort. Many would do well to avoid her wrath by showing her due respect.
