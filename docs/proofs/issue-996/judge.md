# Judge: issue-996

Verdict: sufficient
Technical debt: clear
Acceptance criteria: met

## Per-criterion verification

Criterion 1 (`vllm.toml` declares `[presets.base_urls]`): `vllm.toml:28-32` — `[presets.base_urls]` block present with `dialogue = "http://localhost:8000"`, `simulation = "http://localhost:8001"`, `intent = "http://localhost:8002"`, `reaction = "http://localhost:8001"`. Layout matches issue #996's proposal exactly (`:8000` 14B, `:8001` shared 8B, `:8002` 4B).

Criterion 2 (`preset_base_url` returns expected URL per category): `transcript.txt:10` — `test ipc::config::tests::vllm_preset_supplies_per_category_base_url ... ok`. Test body at `config.rs:1042-1057` asserts all four URLs.

Criterion 3 (`fill_missing_models_from_presets` populates `category_base_url` for all four categories): same passing test, `config.rs:1066-1100`. Asserts `changed == true` and all four `category_base_url` + `category_model` entries match the expected preset values (8000/14B, 8001/8B, 8002/4B, 8001/8B).

Criterion 4 (`vllm_extra_slots` emits three slots, dialogue elided, 8B@:8001 appears twice): same passing test, `config.rs:1107-1127`. Sets `model_name = Qwen3-14B` so dialogue collapses to base, asserts `slots.len() == 3`, verifies presence of `(:8001, 8B)` and `(:8002, 4B)`, and counts exactly two `(:8001, 8B)` entries (sim + reaction).

Criterion 5 (engine still parses modified TOML and boots a headless session): `transcript.txt:18-22` — clean JSON for `/status`, `look`, `/time`, `/npcs`, `/quit`. No TOML parse error, no panic, engine reached end of script.

## Notes

- TOML structure is correct. The `[presets.base_urls]` sub-table sits after the four scalar category keys (`dialogue/simulation/intent/reaction`) on the parent `[[presets]]` entry. Because the parent table's scalar fields are all set before the sub-table header, TOML parsing places `base_urls` as a child of the same preset rather than orphaning it — the harness smoke run confirms this empirically.
- URL/model alignment matches the issue's intended layout: 14B on `:8000` (also the user-level `default_base_url`), 8B on `:8001` shared by sim + reaction, 4B on `:8002`. Each unique model size gets its own port, so `VllmProcess::ensure_slots`' dedup logic spawns exactly two extra processes beyond the base.
- Mirrors the sister `vllm_mlx.toml` schema landed in PR #990. No drift between the two provider configs at the schema layer.
- Test is not passing for the wrong reason: it directly inspects the populated maps and the `vllm_extra_slots` output, not just provider-load success. Dialogue is correctly elided by setting `model_name = Qwen3-14B` before calling `vllm_extra_slots`, otherwise the dialogue slot would also appear.
- No scope creep: the change is purely the TOML port plus its regression test plus the smoke fixture. No unrelated edits.
- Live-proof tier: `parish-config` is not in the matrix, so the harness smoke run is defense-in-depth rather than a hard requirement. Author correctly notes this in evidence.md.
- Acceptance-criteria-first ordering: criteria file is present, all five criteria mapped to observable signals in transcript or files.
