# parish-art-tool

Developer-side asset pipeline for the [Rundale Diorama](../../../../docs/design/ideas/parish-diorama.md) scene system.
Generates, curates, post-processes, and exports AI-generated pixel-art plates and sprites.

## Workflow

```sh
# 1. Initialise (creates art/style-bible.md + art/manifest.json)
parish-art-tool --root . init

# 2. Generate candidates (requires OPENAI_API_KEY or GEMINI_API_KEY;
#    dry-runs without a key — prints the prompt and writes a pending entry)
parish-art-tool --root . gen-plate 2              # Darcy's Pub (location id 2)
parish-art-tool --root . gen-sprite 1             # Padraig Darcy (NPC id 1)
parish-art-tool --root . gen-variant 2 --variant night

# 3. Review the queue
parish-art-tool --root . list --pending
parish-art-tool --root . review plate-2-001

# 4. Accept or reject
parish-art-tool --root . accept plate-2-001 --note "cleaned stray pixels"
parish-art-tool --root . reject plate-2-002 --reason "wrong palette"

# 5. Post-process and export to the mod (only accepted assets)
parish-art-tool --root . export plate-2-001 \
  --dest mods/rundale/assets/scenes/darcys-pub/plate.png
```

## Providers

| Flag                          | Provider           | Key env var         | Model                     |
| ----------------------------- | ------------------ | ------------------- | ------------------------- |
| `--provider openai` (default) | OpenAI Images API  | `OPENAI_API_KEY`    | `gpt-image-1`             |
| `--provider google`           | Gemini/Imagen 3    | `GEMINI_API_KEY`    | `imagen-3.0-generate-002` |
| `--provider stability`        | Stability (SD 3.5) | `STABILITY_API_KEY` | `sd3.5-large`             |

`stability` is the cheaper option for the early **provider bake-off** (plan
T5.1). The tool reads the key from the process environment — `export
<KEY>=...` (or `source .env`) before a live run.

NVIDIA NIM and fal (both `black-forest-labs/flux.1-dev`) were evaluated and
**removed**: the FLUX.1-dev output had visible incongruities and missed the
target pixel-art style. (NVIDIA's FLUX safety filter also returns a black frame
on prompts containing abstract mood words like "gritty"/"melancholy" — a
separate reason it was a poor fit.)

## Style anchors

The first accepted plate and the first accepted sprite should be marked
`anchor: true` (via `--anchor` on `export`). Every subsequent generation
passes the anchors as reference images to the provider's edit endpoint to
enforce visual consistency across the full set.

## Manifest

`art/manifest.json` is committed to the repository. It records: id, kind
(plate/sprite/variant), target id, provider, model, prompt, reference images,
created timestamp, status, anchor flag, cleanup notes, and output path.

Candidate PNGs under `art/` are gitignored; only accepted, post-processed
assets land in `mods/rundale/assets/scenes/`.

## Post-processing

| Kind    | Input size | Output size | Notes                                      |
| ------- | ---------- | ----------- | ------------------------------------------ |
| Plate   | Any        | 480x270     | Lanczos downscale                          |
| Sprite  | Any        | 48x72       | Trim transparent border, Lanczos downscale |
| Variant | Any        | 480x270     | Same as plate                              |

## Export path guard

`export` rejects any destination that does not resolve under
`mods/rundale/assets/scenes/` (AGENTS rule, path traversal guard).
