# Portrait Manifest Schema

Use a separate manifest during the experiment phase. Do not edit
`mods/rundale/npcs.json` just to point at generated images.

```json
{
  "schemaVersion": 1,
  "style": "illustrated-notebook-head-v1",
  "generatedAt": "2026-07-03T00:00:00Z",
  "source": {
    "npcFile": "mods/rundale/npcs.json",
    "styleReferences": [
      "docs/graphics-v2/illustrated-parish-notebook.png"
    ]
  },
  "portraits": [
    {
      "npcId": 4,
      "npcName": "Roisin Connolly",
      "slug": "npc-0004-roisin-connolly",
      "status": "approved",
      "style": "illustrated-notebook-head-v1",
      "assets": {
        "master": "approved/npc-0004-roisin-connolly/master.png",
        "portrait256": "approved/npc-0004-roisin-connolly/portrait-256.png",
        "thumb96": "approved/npc-0004-roisin-connolly/thumb-96.png",
        "thumb64": "approved/npc-0004-roisin-connolly/thumb-64.png"
      },
      "sourcePrompt": "approved/npc-0004-roisin-connolly/source.prompt.md",
      "audit": "approved/npc-0004-roisin-connolly/audit.md",
      "expressionSet": {
        "neutral": "approved/npc-0004-roisin-connolly/master.png"
      }
    }
  ]
}
```

## Status Values

- `candidate` — generated, not yet audited.
- `rejected` — kept for provenance only.
- `approved` — human-approved neutral portrait.
- `needs-repair` — identity/style mostly right, but requires a bounded edit.

## Runtime Notes

When this leaves experiment mode, the game should read a manifest keyed by
`npcId`. That avoids churn in `npcs.json`, keeps save compatibility simple, and
lets different visual packs swap in without changing authored character data.
