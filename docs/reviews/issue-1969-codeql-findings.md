# Issue #1969 — CodeQL Finding Dispositions

Date: 2026-10-05

Scope: the 19 `rust/*` alerts (#65–#83) carried across the engine rename in
PR #1967. Each disposition below traces the reported flow from source to sink
and names the boundary that controls it.

## Method

GitHub reports these alerts without their sources, so the flows were reproduced
locally with the same toolchain the repository's default setup uses (CodeQL CLI
2.27.1, `codeql/rust-queries` 0.1.43, the `rust-code-scanning.qls` suite, and the
default threat model):

```sh
codeql database create /tmp/cqldb --language=rust --build-mode=none --source-root=.
codeql database analyze /tmp/cqldb \
  codeql/rust-queries:codeql-suites/rust-code-scanning.qls \
  --format=sarif-latest --output=/tmp/results.sarif
```

On `main` at `166a89b1` this reproduces 14 of the 19 alerts. The five
`tile_cache.rs` alerts are not reported by this toolchain (see below).

One fact explains most of the path-injection alerts. The Rust axum model treats
**every** handler parameter as remote input, including
`Extension<Arc<AppState>>`. Values the server resolved at startup, such as
`AppState::saves_dir` or `AppState::mods_root()`, are therefore reported as
"user-provided" even though no request data reaches them. Every server-side flow
below starts at the `Extension::<...>` parameter, not at a request body, query,
or path parameter.

## Summary

| Alerts | Rule | Location | Disposition |
| --- | --- | --- | --- |
| #65 | `cleartext-logging` | `limerick-engine/src/headless.rs` (inference log paths) | False positive |
| #66 | `cleartext-logging` | `limerick-engine/src/headless.rs` (atmosphere text) | False positive |
| #67 | `cleartext-storage-database` | `limerick-server/src/session/persistence.rs:258` | False positive |
| #68, #70–#73 | `path-injection` | `limerick-core/src/tile_cache.rs:145,195,198,199,199` | False positive; no longer reported |
| #69, #74, #75 | `path-injection` | `limerick-diagnostics/src/bug_report.rs:805,812,821` | False positive |
| #76, #82 | `path-injection` | `limerick-server/src/routes/mods.rs:29,110` | False positive |
| #77, #78 | `path-injection` | `limerick-harness/src/dashboard/routes.rs:85,116` | Reported flow not exploitable; adjacent flaw fixed |
| #79 | `path-injection` | `limerick-editor/src/mod_io.rs:29` | False positive |
| #80, #83 | `path-injection` | `limerick-editor/src/save_inspect.rs:73,78` | False positive |
| #81 | `path-injection` | `limerick-persistence/src/picker.rs:144` | False positive |

Within a file with more than one alert, the issue lists alert numbers without
lines, so the number-to-line pairing inside that group is not established here.
Every alert in a group has the same disposition.

## Findings

### #65 — inference log path printed by the headless REPL

Flow: `InferenceFileLog::spawn` builds
`session_id = format!("{timestamp}-{pid}")` and then
`path = dir.join(format!("{session_id}.jsonl"))`
(`limerick-inference/src/file_log.rs:176-178`). The chat transcript uses the same
id (`limerick-chronicle/src/chat_transcript.rs:132`). `run_headless` prints both
paths to stdout (`headless.rs:303-307`).

The value is a timestamp and process ID that name a log file. It authenticates
nothing and is unrelated to the server's cookie session. The query matched it by
the name `session_id`. Printing the file location is the intended operator
feedback. **False positive.**

### #66 — `no_old_account()` printed by the headless REPL

Flow: `render_place_atmosphere` (`limerick-core/src/ipc/commands/listen.rs:48-95`)
returns `no_old_account()`, which is the fixed sentence "No old account of this
place comes readily to mind." The headless REPL prints it as supplemental
atmosphere (`headless.rs:1337-1338`). The query matched it by the word
"account" in the function name. **False positive.**

### #67 — `DELETE FROM oauth_accounts` during session purge

Flow: the query's source and sink are the same expression,
`format!("DELETE FROM oauth_accounts WHERE session_id IN ({placeholders})")`
(`persistence.rs:256-258`). It was matched by the variable name `oauth_sql`. The
statement deletes rows and stores nothing. Its placeholders are `?` markers,
and the bound values are session ids that were just read from `sessions`.

Underlying storage (`session_store_impl.rs:263-269`): `oauth_accounts` holds
`provider`, `provider_user_id`, `session_id`, and `display_name`. No OAuth
access, refresh, or ID token is persisted. `auth.rs` uses the access token only
in memory to fetch user info during the callback. **False positive.**

See the related observation below on session ids as bearer values.

### #68, #70–#73 — tile cache reads and writes

Flow: `GET /tiles/{*path}` → `parse_tile_path` parses `z`, `x`, and `y` as
`u32` (`limerick-server/src/tile_routes.rs`). `source_id` must match a
registered tile source in config. `TileCache::get` then:

1. rejects any `source_id` outside `[A-Za-z0-9_-]+`;
2. replaces the request value with the key stored in the trusted config map;
3. reduces that key with `Path::file_name()`; and
4. joins only the config key and decimal integers under the startup-resolved
   `cache_dir` / `bundled_dir`.

No request byte except a validated slug and three integers reaches the path,
and none of them can express a separator or `..`. Regression tests already
cover unsafe and unknown source ids (`tile_cache.rs` tests). The 2.27.1
toolchain does not report these five alerts on unchanged code, so the next
default-branch analysis should close them. If any remain open, dismiss them as
false positives. **False positive.**

### #69, #74, #75 — offline bug-report bundle

Flow: `submit_bug_report`'s `Extension<Arc<AppState>>` →
`state.saves_dir.join("bug-reports")` (`limerick-server/src/routes/world.rs:474`)
→ `write_offline_bundle` → `bundle_root.join(id)` → writes `screenshot.png` and
`issue.md` (`bug_report.rs:804-821`).

`saves_dir` is resolved once at startup (`resolve_project_saves_dir`), and `id`
is a server-generated UUID v4 (`bug_report.rs:757`). The request supplies only
the title, description, and screenshot bytes, which are written as file
*contents*, never as path components. **False positive.**

### #76, #82 — mod selector

Flow: `list_mods` / `switch_mod`'s `Extension<Arc<AppState>>` →
`state.mods_root()`, which is the parent of the loaded mod's directory
(`limerick-server/src/state.rs:424-441`). It is then read with `read_dir`
(`mods.rs:29`), or `root.join("mod-list.toml")` is written (`mods.rs:108-110`).

The directory and the file name are both server-determined. The request's
`mod_id` is checked against the ids discovered on disk before anything is
written, and it reaches only the file's contents. **False positive** for path
injection. See the related observation on who may switch mods.

### #77, #78 — harness dashboard frame and transcript reads

Reported flow: the `Path<(i64, u32)>` parameters of `get_frame` /
`get_turn_transcript` → `format!("turns/{turn_idx:03}/…")` → `artifact_dir.join`
→ `std::fs::read` (`limerick-harness/src/dashboard/routes.rs:84-85,115-116`).
`turn_idx` is a `u32`, so the request cannot introduce a separator or `..`. As
reported, the flow is not exploitable.

Adjacent flaw found while tracing it: the directory being joined is
`runs.artifact_dir`, read from `harness.db`. For ingested skill runs that
column came from `artifacts_root.join("runs").join(payload.uuid)` with no
validation. Per-turn `frame_path` / `lines_path` / `llm_transcript_path` were
joined the same way (`ingest.rs`). A payload with `"uuid": "../.."` or an
absolute `frame_path` made ingest stat and read files outside the run
directory, and stored an arbitrary directory that the dashboard later served.
The dashboard binds `0.0.0.0` with no authentication. Ingest payloads are
written by an agent driving the live game, so they are not fully trusted
input.

Fixed in this change:

- `ingest::build_record` accepts only a `uuid` that is exactly one plain path
  component. Per-turn artifact paths must be relative and made only of plain
  components (`ensure_run_dir_name`, `ensure_relative_artifact_path`).
- The dashboard serves a frame or transcript only if its canonical path
  (symlinks resolved) is inside the canonical `--artifacts` root
  (`read_within_artifact_root`). Otherwise it returns `404`.
- Regression tests: `uuid_that_escapes_runs_dir_rejected`,
  `turn_paths_outside_run_dir_rejected`,
  `artifacts_outside_root_are_not_served`, and
  `symlinked_artifact_escaping_root_is_not_served`. Each fails with its check
  disabled. The existing ingest and transcript tests still pass, so supported
  layouts keep working.

After the fix, CodeQL still reports the `u32` flow into the joined path (see
"Fresh analysis" below). Dismiss #77 and #78 as false positives that cite this
section.

### #79 — editor mod listing

Flow: `editor_list_mods`'s `Extension<Arc<AppState>>` → `state.mods_root()` →
`handle_editor_list_mods` → `mod_io::list_mods` → `read_dir(mods_root)`
(`limerick-editor/src/mod_io.rs:29`). This lists the server-determined mods
directory and takes no request input. The editor routes that *do* take a path
(`editor-open-mod`, `editor-list-branches`, and others) canonicalise it and
require containment under `mods_root()` / `saves_dir` via
`editor::validate_within` (#371). Those routes are not part of this alert.
**False positive.**

### #80, #83 — editor save listing

Flow: `editor_list_saves`'s `Extension<Arc<AppState>>` → `state.saves_dir` →
`save_inspect::list_saves` → `is_dir` / `read_dir`
(`limerick-editor/src/save_inspect.rs:73,78`). This is the startup-resolved
saves directory with no request input. **False positive.**

### #81 — save picker discovery

Flow: `discover_save_files`'s `Extension<Arc<AppState>>` → `state.saves_dir` →
`picker::discover_saves` → `read_dir(saves_dir)`
(`limerick-persistence/src/picker.rs:144`). This is the same startup-resolved
directory with no request input. **False positive.**

## Fresh analysis

On this branch, the same command reports 14 alerts, the same set as on `main`.
The harness ingest and dashboard fix is a boundary CodeQL's Rust model does not
recognise as a sanitizer, so it does not change the count. No CodeQL
configuration, severity threshold, or path filter was changed.

## Dismissal

Dismissing an alert requires write access to code scanning, which the agent
session did not have. With that access, run the following from the repository
root to apply the dispositions above:

```sh
dismiss() {
  gh api -X PATCH "repos/dmooney/Rundale/code-scanning/alerts/$1" \
    -f state=dismissed -f dismissed_reason="false positive" \
    -f dismissed_comment="$2 See docs/reviews/issue-1969-codeql-findings.md."
}
dismiss 65 "Path of a log file named by timestamp-pid; not a credential."
dismiss 66 "Fixed gameplay sentence; matched on the word 'account'."
dismiss 67 "DELETE statement text; no secret is stored in oauth_accounts."
for n in 68 70 71 72 73; do
  dismiss "$n" "source_id allowlisted and replaced by config key; z/x/y are u32."
done
for n in 69 74 75; do
  dismiss "$n" "Startup saves_dir plus server-generated UUID; request data is file content only."
done
for n in 76 82; do
  dismiss "$n" "Server-derived mods_root and constant file name; mod_id validated and used as content."
done
for n in 77 78; do
  dismiss "$n" "u32 turn index; artifact dir confined to the artifact root by #1969 fix."
done
dismiss 79 "Lists server-derived mods_root; no request input."
for n in 80 83; do
  dismiss "$n" "Lists startup-resolved saves_dir; no request input."
done
dismiss 81 "Lists startup-resolved saves_dir; no request input."
```

If GitHub renumbers an alert, match it by rule, file, and line to the table
above.

## Related observations (not CodeQL alerts)

These came up while tracing the flows. None is fixed here, because each one
depends on a trust-model decision that belongs to the owner.

- **Session ids are bearer values.** The web server's per-visitor session id is
  accepted from a raw `limerick_sid` cookie when it exists in `sessions.db`
  (`middleware.rs`, pre-migration recovery). It is stored in cleartext, names
  the `saves/<id>/` directory, and appears in `tracing` fields. Hashing the
  database column alone would not help while the directory name exposes the
  same value. In production, Cloudflare Access still gates every request.
- **Mod switching is server-wide.** Any Cloudflare Access–authenticated user can
  call `POST /api/mods/switch` and change the base mod used after the next
  restart. The editor routes already let the same users write shared mod files,
  so this matches the current "every authenticated user is a trusted operator"
  model. Revisit it if the hosted server admits untrusted players.
- **Harness dashboard exposure.** `limerick-harness serve` binds `0.0.0.0` with
  no authentication and serves playtest transcripts to anyone on the network.
  The fix above confines it to its artifact root but does not restrict who can
  read that root.
