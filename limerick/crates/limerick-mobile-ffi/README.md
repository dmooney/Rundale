# limerick-mobile-ffi

The callback-free C boundary between the native iPhone client and the shared
Limerick engine. It builds as a static library and links `limerick-core` in
its portable configuration (`default-features = false`, `features = ["mobile"]`),
so building this package never compiles the desktop-only dependencies.

A session is the mobile host of the shared turn API
(`limerick_core::turn::TurnEngine`, [design](../../../docs/design/portable-turn-api.md)):
the same pipeline, parser, intent inference, and save database as desktop. The
Swift host fulfils each model call through Limerick Endpoints. There is no
mobile-only runtime ([ADR-025](../../../docs/adr/025-mobile-runtime-on-shared-engine.md)).

## ABI

The header is `include/limerick_mobile_ffi.h`. `mobile/RundaleBridge` vendors an
identical copy next to its Clang module map; a unit test fails if the two drift.

- `limerick_mobile_open` copies a bounded UTF-8 JSON request and returns an
  opaque session handle plus the opening snapshot.
- `limerick_mobile_dispatch` runs one JSON operation (`{"op": ...}`) on a session.
- `limerick_mobile_close` waits for any operation in progress, then releases the
  session and its save lock.
- `limerick_mobile_owned_bytes_free` releases one owned response exactly once.

Responses are envelopes: `{"ok": true, "value": ...}` or
`{"ok": false, "error": {"code": ..., "message": ...}}`. Panics are contained
and reported as `LIMERICK_MOBILE_INTERNAL_ERROR`. Symbol names use the
`limerick_mobile_*` / `LIMERICK_MOBILE_*` prefix.

## Opening a session

The open request is `{"save_path": ..., "mod_dir": ...}`: the SQLite save and
the world's mod directory (the app bundles `mods/rundale`). `OPEN_NEW` refuses
a save that exists (`save_exists`); `OPEN_RESUME` continues the save, or starts
a new game there when the file does not exist. A new game journals the opening
scene as its first transcript event: a `scene_changed` event titled by
`metadata.sceneName`, whose content is the location's description and who is
there (time of day and weather are the header's; exits are `/exits`'s). Opening a save
restores the main branch's latest state, and a request an earlier process left
open ends `Interrupted`, never re-run. A save the engine cannot read
(ADR-025 §4) is moved aside unchanged (`<name>.refused-<unix seconds>.<ext>`,
with its SQLite sidecars) and a new game starts at the save path; its
transcript opens with the launch-refusal notice.

Creating a new game survives the process being killed at any point (#2210).
The save is built as `<name>.creating.<ext>` and renamed to the save path
only once its first snapshot is written; an unfinished build is discarded at
the next launch. A save whose transcript is empty was killed before its
opening scene was journaled, so the next launch journals it. A save with no
snapshot, which builds before this one could leave, holds no game: it is moved
aside as `<name>.unfinished-<unix seconds>.<ext>` and a new game starts
without the refusal notice.

## Operations

| `op`                     | Fields                                                                               | Result                                             |
| ------------------------ | ------------------------------------------------------------------------------------ | -------------------------------------------------- |
| `snapshot`               | none                                                                                 | read model, requests, newest 100 events            |
| `submit`                 | `text`, optional `draft_id`, `logical_request_id`                                    | operation result                                   |
| `retry`                  | `logical_request_id`                                                                 | operation result (new attempt)                     |
| `answer_clarification`   | `logical_request_id`, `choice_id`                                                    | operation result (same request)                    |
| `stop`                   | none                                                                                 | operation result (`cancelled`, or `ignored`)       |
| `pending_endpoint`       | none                                                                                 | the invocation the host owes, or `null`            |
| `resolve`                | `call_id`, `attempt_id`, `base_revision`, `output`                                   | operation result                                   |
| `fail`                   | `call_id`, `attempt_id`, `base_revision`, `error_kind`, `message`, optional `reason` | operation result                                   |
| `frame`                  | `call_id`, `attempt_id`, `sequence`, `text`                                          | one provisional event, or `ignored`                |
| `read_events`            | optional `after`, `limit` (1–100)                                                    | events after `after`, `cursor`, `hasMore`          |
| `read_event_page_before` | `before`, `limit` (1–100)                                                            | newest events before `before`, `cursor`, `hasMore` |
| `bug_report`             | optional `description`, `build`                                                      | `text` (the bug report), `characters`              |

An operation result carries `accepted`, `logicalRequestID`, `attemptID`,
`events` (the journaled transcript events it produced, projected onto the
`RundaleKit` `SemanticEvent` contract), `status` (`awaiting_inference`,
`awaiting_clarification`, `completed`, or `ignored`), `terminalOutcome`, the
player-facing `error` of a failed attempt, and `eventCursor`. Numeric
identities (`sequence`, `stateRevision`, `baseRevision`, cursors) are
`{"rawValue": n}`; the operations accept either form.

`bug_report` composes the plain-text report the app sends to
`limerick-bug-report` with `/bug` or a shake (#2022,
[plan](../../../docs/plans/mobile-bug-report.md)): the description, build,
scene and who is present, the open request, the newest journaled transcript
lines, and the Endpoint calls answered since the session opened (the last
eight, kept in memory only). It is at most 50,000 characters
(`limerick_diagnostics::mobile_report::REPORT_BUDGET`). It reads only: nothing
is journaled and no state changes.

A refused request (a request is still open, the request already committed, a
slash command the phone does not offer) answers
`LIMERICK_MOBILE_PROTOCOL_ERROR` with the engine's code (`request_in_progress`,
`rejected`, `command_unavailable`, ...). Nothing changes.

A failed dialogue call shows the line the mod gives for its `reason`
(`offline`, `busy`, `unavailable`, `timed_out`, `refused`, `garbled`,
`cancelled`; `[failure_lines]` in the mod's `loading.toml`), or the engine's
generic retry line when the host gives none. `message` is for diagnosis and is
never shown. An unknown `reason` is a `protocol_error`.

## Slash commands

The engine runs the phone's slash commands as ordinary requests, answered
locally with no Endpoint call (`limerick_core::turn::LocalCommand`). The read
model's `commands` lists the advertised ones for `/help` and the app's Commands
list: `/look`, `/people` (also `/npcs`), `/exits`, and `/help`. `/wait
[minutes]`, `/pause`, `/resume`, `/debug [view]`, and `/flags` work in every
build but are not advertised. Any other `/` input is refused with
`command_unavailable`.

Completion comes from the same registry (`limerick_core::turn::COMMANDS`).
`commandCompletions` lists every command as a tree of words: each has `word`,
`summary`, `next` (the words that may follow), and `takesNpc` (a name follows
instead). `everyone` lists every NPC in the world (`id`, `name`) for those
names. `/debug` finds a name without case or diacritics, so `micheal` finds
Mícheál Connolly.

## Inference

`pending_endpoint` returns the call the attempt is suspended on:

```json
{
  "callID": "<attempt>#2",
  "logicalRequestID": "...",
  "attemptID": "...",
  "baseRevision": { "rawValue": 3 },
  "stream": true,
  "endpoint": { "role": "dialogue", "slug": "rundale-dialogue", "version": 1 },
  "input": { "...": "EndpointCall::invocation" }
}
```

The host posts `{"input": <input>}` to that Endpoint version: to its `/stream`
route when `stream` is true (the definition declares
`inferenceConfig.streaming`, as `rundale-dialogue` does), otherwise to the
JSON route, whose body is the output object. It answers the engine:

- each dialogue `text_delta` → `frame`: shown as provisional transcript text
  (`provisional: true`, appended, ordered by `streamSequence`). Frames are not
  journaled and carry the durable cursor rather than advancing it. The
  committed line later replaces the streamed row in place (same
  `transcriptItemID`).
- the `final` frame's `output` object, or the JSON route's body → `resolve`;
- a transport, authentication, or protocol failure → `fail` (`transport`,
  `protocol`, `missing_terminal`, `timed_out`, `interrupted`).

After each result the turn may ask for the next call (an intent call, then the
dialogue call). Stop is `stop`; a result or frame for anything but the awaited
call, attempt, and base revision is ignored.

Travel encounters and arrival reactions have no Endpoint; the session routes
them as unavailable, so the pipeline uses its canned lines. Content guards are
off on this path (`limerick_npc::DIALOGUE_CONTENT_GUARDS_FLAG`): a reply is
committed after the structural contract only, as the owner decided for the
Endpoint path (`docs/agent/inference-rules.md`). Background simulation that
needs inference waits for #2025; before each player action the session runs
the deterministic world pump (weather, NPC schedules, tiers).

## Build

`mobile/scripts/build-rust-mobile.sh` builds the device and simulator slices
with the toolchain pinned in `rust-toolchain.toml` and packages them as
`mobile/.build/rust-mobile/LimerickMobileFFI.xcframework`.

## Tests

`cargo test -p limerick-mobile-ffi` drives the C entry points against the
canonical world with a scripted Endpoint host: acceptance before any call,
provisional streaming and in-place commit, Stop and late results, failure and
retry, restart interruption, clarification across restart, content guards off,
and bounded paging. `mobile/RundaleTests/RundaleEngineLifecycleTests.swift`
runs the RundaleKit lifecycle tests through the same boundary in the
simulator.
