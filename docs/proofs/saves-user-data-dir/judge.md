Evidence type: live smoke transcripts (server + CLI), test counts, clippy + fmt status

Verdict: sufficient

Technical debt: clear

Review notes: Path-resolution refactor is mod-driven and rule-#9-compliant (no cwd walks at runtime). New `paths::resolve_user_data_dir(app_name)` lives in the right leaf crate; entry points (parish-server, parish-tauri, parish-cli) consistently pass `gm.manifest.meta.app_name()` with `DEFAULT_APP_NAME` fallback. `ModMeta.save_root` is additive (older manifests round-trip — covered by `test_load_mod_from_directory`). The CLI `/load` bug fix (line 440) closes a latent rule-#9 violation that pre-existed. Three live smoke transcripts in `transcript.md` show the home-dir, in-repo, and env-override paths behaving correctly. Workspace tests: 2764 passed, 15 ignored (no regressions); fmt + clippy clean. No migration of existing in-repo saves is explicitly accepted per plan decision — players with prior saves copy manually.
