# Multimodal — Voice, Portraits, Ambient Art

**Target crate:** new `crates/parish-audio/` (TTS/ASR), integration in
`apps/ui/` for playback, optional `crates/parish-imagen/` for portraits.

## Problem

Rundale is text-first. That suits a 1820s setting but leaves obvious upside:
hearing a publican's voice is more evocative than reading it, and a sketched
portrait for every NPC would ground the hand-authored personas in
`mods/rundale/npcs.json`.

## SOTA techniques

### 1. Offline TTS with per-NPC voice

Modern local TTS can do convincingly varied voices at <1× realtime on CPU:

- **Piper** (Rhasspy): tiny, CPU-only, many voices, MIT.
- **Coqui XTTS v2**: voice-cloning from 6s reference clips. More expressive
  but heavier.
- **Kokoro-82M** (2024): small, high-quality, Apache-licensed — good
  default for a Rust shipping target.

Per-NPC configuration in `mods/rundale/npcs.json` adds `voice: "piper/en_IE/irish_male_02"`.
No cloud dependency.

**Period pronunciation hook.** `mods/rundale/pronunciations.json` already
exists and is currently unused by the runtime. It is the natural input for
a pronunciation-override layer: before synthesis, replace known tokens
(placenames, surnames, Irish loanwords) with their authored phonetic
spelling so the engine doesn't anglicise *Cill Taobhach* into *kill tay-ow-
ack*. Treat the file as authoritative; fall back to the TTS engine's
grapheme-to-phoneme model only for unknown strings.

### 2. Streaming TTS tied to Tier 1 streaming

Tier 1 already streams tokens. TTS can consume those chunks and begin speaking
before the full response arrives:

- Chunk boundary = sentence or punctuation.
- Audio playback queue in `apps/ui/` driven by web-audio API.
- Total perceived latency ≈ time-to-first-token + first-sentence TTS (~400ms).

### 3. Whisper-based ASR for voice input

ADR-006 already covers natural-language input. Upgrade with:

- **whisper.cpp** local (tiny/base models run real-time on CPU).
- **Distil-Whisper** for 6× speedup at similar accuracy.
- VAD (Voice Activity Detection) with Silero so players can just talk.

Pipeline: mic → VAD → whisper → intent parser (3B model, existing) → scene.

### 4. Portrait diffusion

Generate one portrait per NPC once at world-gen time, cache in
`mods/rundale/portraits/`:

- **SDXL Turbo / FLUX.schnell** for one-step generation — 1–2s per portrait on
  a mid-range GPU.
- **LoRA trained on period art** (Wilkie, Mulready, 1820s engravings) for
  stylistic consistency.
- Deterministic seed = NPC id, so the same NPC always regenerates identically
  if the cache is lost.

Prompt built from the existing `brief_description`, occupation, age, mood.

### 5. Dynamic scene illustration

Beyond static portraits, render a scene illustration when the player enters a
new location:

- Prompt built from `world.json` location description + time-of-day +
  weather.
- Cache keyed by `(location_id, season, weather_bucket, hour_bucket)` to keep
  cost bounded.
- Diffusion via a local Stable Diffusion service (Automatic1111/ComfyUI) or
  remote API. Feature-flag with `scene-art`.

### 6. Ambient audio LLM control

Already have `docs/design/ambient-sound.md`. Add a tiny LLM pass that
selects/sequences loops per scene based on mood + time + weather. Maps
semantic cues ("tense, rain") to a short playlist of pre-recorded loops.

### 7. Vision in for mod authoring

Let authors drop a sketched map into the Designer Editor; a VLM (Qwen2-VL,
LLaVA-Next) extracts nodes and edges into `world.json` skeleton. Huge time
saver for `parish-geo-tool` workflows — see `docs/design/geo-tool.md`.

### 8. Expression & gesture metadata

Even without 3D, a short `gesture` enum in tier 1 output
(shakes head, raises eyebrow, crosses self) can drive sprite animation in the
Svelte UI. Emit through the grammar-constrained JSON (doc 02).

## Minimal first cut

1. Add `crates/parish-audio` with Piper wrapped as a subprocess; streaming
   chunk playback through a new WS channel.
2. Per-NPC voice mapping file; fallback voice per role.
3. Write a one-shot `just generate-portraits` that runs SDXL-Turbo against
   the NPC roster; commit outputs under `mods/rundale/portraits/`.
4. Behind flag `voice-io`, wire whisper.cpp for mic input.

## Risks

- Disk size of TTS voices + portraits can balloon the repo; use Git LFS or
  fetch on first run.
- TTS accents are stereotyped; vet carefully for period authenticity vs
  caricature. Curate per role with community review.
- Diffusion licensing: prefer Apache/MIT-licensed checkpoints for shipped
  content.
- Mode parity: web build has no local TTS/ASR; fall back to browser
  SpeechSynthesis + MediaRecorder + cloud Whisper.

## Papers / references

- Radford et al., *Whisper* (2022).
- Gandhi et al., *Distil-Whisper* (2023).
- Sauer et al., *SDXL Turbo — Adversarial Diffusion Distillation* (2023).
- Black Forest Labs, *FLUX.1* model card (2024).
- Kim et al., *VITS* (backbone for many neural TTS).
