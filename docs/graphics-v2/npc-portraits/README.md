# NPC Portrait Pipeline

This folder defines the Graphics V2 pipeline for generated NPC portraits: small
human-readable head icons/busts like the left-side people list in the UI
concepts.

The target is the **Illustrated Parish Notebook** treatment: small
ink-and-watercolor head sketches on warm paper like the left-side people list
in `illustrated-parish-notebook.png`. This is not a cinematic dialogue portrait
pipeline and not the darker `concept-7a` painted-card style.

## Source References

- `../illustrated-parish-notebook.png` — authoritative portrait treatment.
- `../../research/clothing-textiles.md` — period clothing and class cues.
- `../../../mods/rundale/npcs.json` — source roster and persona data.

## Pipeline

1. **Roster Brief**

   - Read `mods/rundale/npcs.json`.
   - For each NPC, write a compact art brief from stable identity fields:
     `id`, `name`, `age`, `pronouns`, `occupation`, `brief_description`,
     `personality`, `mood`, `home`, and `workplace`.
   - Translate personality into visible posture/expression only. Do not include
     private knowledge, secrets, or plot facts in the portrait prompt.

2. **Wardrobe Pass**

   - Add period clothing/class cues from `docs/research/clothing-textiles.md`.
   - Prefer visual social markers: frieze coat, linen shirt, shawl, kerchief,
     red petticoat, apron, cloak, homespun wool, brogues, bare feet when visible.
   - Keep clothing plausible for 1820 rural Roscommon. Avoid Victorian fashion,
     modern collars, modern caps, contemporary makeup, studio lighting, and
     fantasy costume.

3. **Prompt Build**

   - Use `prompt-template.md`.
   - Build one prompt track only: `illustrated-notebook-head-v1`.
   - The portrait should look like it belongs in a notebook margin: spare line,
     soft wash, warm paper, no card frame, no background scene.
   - Save every prompt beside the generated image.

4. **Pilot Render**

   - Start with 6 NPCs that cover the range of age, gender, role, and status:
     Roisin Connolly, Padraig Darcy, Siobhan Murphy, Fr. Declan Tierney,
     Colm Gallagher, and Brigid Ni Fhatharta.
   - Generate 2 candidates per NPC in `illustrated-notebook-head-v1`.
   - Stop after the 12-image pilot and make a contact sheet at full size plus
     UI size before generating the remaining roster.

5. **Clean-Context Audit**

   - Use a separate audit subagent from the render context.
   - Judge each candidate against:
     - recognizability from the NPC brief,
     - age plausibility,
     - occupation/class readability,
     - 1820 rural Irish clothing,
     - consistent illustrated-notebook style,
     - readable face at 64-96 px,
     - no text, border, UI chrome, watermark, modern objects, or glamour shot.
   - The renderer never approves its own output.

6. **Human Pick**

   - Save a contact sheet and short audit report.
   - Human selects one approved neutral portrait per NPC.
   - Rejected images stay in experiments; approved images move to the approved
     asset folder only after review.

7. **Approved Assets**

   - Keep an archival master and deterministic UI derivatives:
     - `master.png` — 1024x1024 or native generation size.
     - `portrait-72x82.png` — native illustrated-notebook people-list portrait.
     - `portrait-96.png` — enlarged/detail UI fallback.
     - `thumb-64.png` — compact/mobile list.
   - Store a manifest keyed by NPC id rather than editing `npcs.json` during the
     experiment phase.
   - Judge approval primarily on `portrait-72x82.png`, because the concept
     portraits are native tiny sketches. Full-size masters are provenance, not
     the visual target.

8. **Expression Variants Later**
   - After a neutral portrait is approved, generate expression edits from that
     portrait rather than fresh-rendering identity:
     `neutral`, `wary`, `pleased`, `angry`, `sad`, `tired`.
   - Expression edits must preserve face, age, clothing, crop, and style.

## File Layout

```text
docs/graphics-v2/npc-portraits/
  README.md
  prompt-template.md
  pipeline-experiments/
    cycle-a/
      npc-0004-roisin-connolly/
        a1.png
        a1.prompt.md
        a1.report.md
        a2.png
        a2.prompt.md
        a2.report.md
      cycle-a-contact-sheet.png
      cycle-a-audit.md
  approved/
    manifest.json
    npc-0004-roisin-connolly/
      master.png
      portrait-256.png
      thumb-96.png
      thumb-64.png
      source.prompt.md
      audit.md
```

Runtime integration can later copy approved assets into `mods/rundale/assets/`
or keep them in a downloaded asset pack. During exploration, keep them here.

## Acceptance Criteria For A Batch

- Every portrait reads as the intended NPC at small UI size.
- The batch looks like one hand-drawn notebook-margin set, not 23 unrelated
  model outputs.
- The portraits stay small, sparse, and sketch-like enough to sit in the
  left-side UI list without feeling like pasted concept-art cards.
- Approved portraits have a native `72 x 82` derivative that matches the
  concept people-list scale.
- Clothing and grooming communicate age, work, class, and era.
- No portrait contains text, fake signatures, ornate frame borders, modern
  objects, or fantasy styling.
- No NPC is made implausibly beautiful, young, wealthy, clean, or theatrical
  unless the authored character specifically calls for it.
- Women and children are depicted with historical dignity and no pin-up framing.
- Approved assets have prompt and audit sidecars.
