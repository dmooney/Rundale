# Task engine requirements: the Phase 6 task and work referrals

> Status: **Proposed, awaiting owner. Nothing here is accepted.** ·
> Issue: [#1891](https://github.com/dmooney/Rundale/issues/1891) (task engine:
> revisit requirements first; consolidates #1899, #1890 and #1900) ·
> Blocks: [#2308](https://github.com/dmooney/Rundale/issues/2308) (Phase 6 task
> proof) · Milestone: Living world (spec Milestone 6)

This is the requirements-first step the owner asked for on 2026-10-04: record the
intended task gameplay and the authoritative engine contract for the current
mobile direction before any implementation. It is a proposal. It does not change
engine code, Endpoints or prompts, it does not mark any requirement accepted, and
it does not wire model `assigned_task` metadata into durable state or restore the
old task behaviour. The decisions that remain the owner's are in
[section 6](#6-open-decisions-for-the-owner); each is marked **NOT DECIDED**.

Every statement is labelled by kind:

- **Current**: what this checkout does, with `file:line` (taken at `8e6ad5d9e`).
- **Historical**: an assumption from the desktop era or an old report. Not authority.
- **Model promise**: something a model said or emitted. Never a fact until the
  engine commits it.
- **Proposed**: this document's contract for the mobile product.

## 1. Governing requirements and traceability

Numbering note: the technical vision's "Phase 5 — Living-world proof" is spec
Milestone 6 and the test plans' "Phase 6". Citations below to "Vision Phase 5"
mean that section.

| Id  | Requirement                                                                                                                       | Source                                                                                                                                                                                        |
| --- | --------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| G1  | One NPC can assign one simple, concrete task to the player.                                                                       | [Spec §17 Milestone 6](../product-specs/mobile-product-technical-spec.md) (living-world checklist)                                                                                            |
| G2  | The task has an authoritative state that distinguishes at least assignment, progress where applicable, and completion.            | Spec §17 Milestone 6; [vision §11.6](../product-specs/software-technical-vision.md)                                                                                                           |
| G3  | The task state survives save/resume.                                                                                              | Spec §17 Milestone 6; vision §7.2 (task state is domain data, not a dialogue-log field)                                                                                                       |
| G4  | Completing the task through the intended world action is reflected in later NPC interaction.                                      | Spec §17 Milestone 6                                                                                                                                                                          |
| G5  | A model's statement that a task is complete does not complete it. Tasks are not inferred from dialogue wording.                   | Vision §11.6 and the Phase 5 "protect later phases" paragraph                                                                                                                                 |
| G6  | Model output is a proposal; the engine validates it against current state before commit. Rejected generation cannot mutate state. | Vision §6.2 and §11.6; [engineering rules](../agent/engineering-rules.md)                                                                                                                     |
| G7  | Cancelling a conversation does not complete a task.                                                                               | Vision Phase 5 exit evidence                                                                                                                                                                  |
| G8  | Reproducible in the canonical three-location, three-NPC world; the sheet gains only the facts the task needs.                     | Spec §13 and §17 Milestone 6; [`mods/rundale/world-sheet.txt`](../../mods/rundale/world-sheet.txt)                                                                                            |
| G9  | Record the explainable cause: "the action that fulfilled a task".                                                                 | Vision Phase 5 implementation paragraph                                                                                                                                                       |
| G10 | Generated dialogue does not casually contradict who is present, what has happened, or what an NPC knows.                          | Spec §17 Milestone 6                                                                                                                                                                          |
| G11 | The Phase 6 proof has a before, an action, an after, save/resume and a repeat run.                                                | [Phase 6 test plan](../test-plans/phase-6-test-cases.md) (P6-06)                                                                                                                              |
| G12 | Nothing inspects, rewrites or replaces an NPC reply; the only reply check is the structural contract.                             | [`limerick-core/AGENTS.md:27`](../../limerick/crates/limerick-core/AGENTS.md), [inference rules](../agent/inference-rules.md) (issue [#2295](https://github.com/dmooney/Rundale/issues/2295)) |

The authored task is `mods/rundale/living-world.json` (`task`) and the `TASK` block
of the world sheet: **Róisín asks the player to tell Peig Hannigan at the Letter
Office to hold any letter that comes for the Connollys until Róisín calls for it.**
Its four states are written as prose ("assigned", "in progress", "completed",
"afterwards"). They are loaded and validated, and nothing at runtime consumes them
(section 2, C12).

## 2. Current behaviour

### 2.1 What the engine does today

| Id  | Current behaviour                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   | Where                                                                                                                                                                                                                                   |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| C1  | A durable ledger exists: `PlayerProgress` holds `PlayerTask` records (`id`, `description`, `assigned_by`, `location`, `assigned_at`, `status`, `started_at`, `completed_at`, `last_matching_action`). Statuses are `Assigned`, `InProgress`, `Completed`. There is no `Offered`, `Declined`, `Cancelled` or `Failed` status.                                                                                                                                                                        | `limerick-types/src/player_progress.rs:35`, `:47`                                                                                                                                                                                       |
| C2  | Bounds: 128 records, 240-character descriptions and actions. Completed history is evicted oldest-first; active tasks are never evicted. Ids are monotonic and never reused.                                                                                                                                                                                                                                                                                                                         | `player_progress.rs:19`, `:22`, `:25`, `:117`                                                                                                                                                                                           |
| C3  | Assignment path: after a dialogue candidate is accepted, `apply_npc_dialogue_turn` reads `metadata.assigned_task` and creates a record only if all hold: the kill switch `player-task-progression` is on (default on); the player's input matches `is_task_request_input` (a fixed list of English phrases); the proposal is grounded in a clause of the delivered dialogue; the speaker is where the player is; neither the proposal nor the grounding clause names a different authored location. | `limerick-core/src/dialogue_apply.rs:64`, `:73`, `:165`, `:519`, `:790-826`                                                                                                                                                             |
| C4  | The recorded `location` is the speaker's location at assignment time, not an authored task site. Progress later requires the player to be at that same location.                                                                                                                                                                                                                                                                                                                                    | `dialogue_apply.rs:802-822`, `player_progress.rs:269`                                                                                                                                                                                   |
| C5  | Progress path: `apply_player_action` runs for an `Interact` intent. One same-location `Assigned` task whose description overlaps the player's action on a meaningful token, with an affirmative direct work verb, becomes `InProgress`. A vocabulary of about 25 farm verbs gates this. Arriving somewhere does not progress a task.                                                                                                                                                                | `game_session.rs:135-170`, `game_loop/input.rs:162`, `player_progress.rs:269`, `:397`, `dialogue_apply.rs:340`                                                                                                                          |
| C6  | Completion: `complete_task_explicitly` exists, needs an exact id, the task's location, status `InProgress` and a nonblank action. **No production code calls it**; only tests do. There is no completed event.                                                                                                                                                                                                                                                                                      | `player_progress.rs:301`; callers only in `ipc/engine_state.rs:340` (test) and `player_progress.rs:1083` (test); events `limerick-types/src/events.rs:232`, `:242` are `PlayerTaskAssigned` and `PlayerTaskProgressed` only             |
| C7  | There is no cancel, decline, abandon or expiry transition.                                                                                                                                                                                                                                                                                                                                                                                                                                          | `player_progress.rs` (no such method)                                                                                                                                                                                                   |
| C8  | Persistence: a committed turn writes its request, events, task batch and state snapshot in one SQLite transaction. The snapshot carries `player_progress`; replay applies task payloads idempotently and never moves a task backwards.                                                                                                                                                                                                                                                              | `limerick-core/src/turn/journal.rs:213`, `turn/engine.rs:1350`, `limerick-persistence/src/snapshot/restore.rs:40`, `:259`, `player_progress.rs:210`, `limerick-core/AGENTS.md:34`                                                       |
| C9  | Turns that can assign a task, and every turn while a task is active, run staged: on isolated state, with the task batch journaled before the candidate installs.                                                                                                                                                                                                                                                                                                                                    | `game_loop/staged_turn.rs:41-47`, `:55-80`                                                                                                                                                                                              |
| C10 | Observability: `engine_state` exposes `player.active_tasks`; the chronicle logs and the debug snapshot render the two task events. There is no `/debug tasks` command and no task row in the Swift wire projection.                                                                                                                                                                                                                                                                                 | `ipc/engine_state.rs:177`, `limerick-chronicle/src/character_log.rs:346`, `limerick-diagnostics/src/debug_snapshot/build.rs:260`; `/debug tasks` appears only in the pending test (`limerick-mobile-ffi/src/tests/living_world.rs:392`) |
| C11 | The production dialogue Endpoint output is `{ "dialogue": string }` only. Its instructions say "You can only talk. Your words cannot give, take, move or change anything." The model therefore cannot emit `assigned_task` on the phone path, so the C3 path never fires there.                                                                                                                                                                                                                     | `mods/rundale/endpoints/rundale-dialogue.v1.json` (`outputSchema`, `instructions`)                                                                                                                                                      |
| C12 | `living-world.json` `task` is parsed and cross-checked against NPC and location ids on load and against the sheet in a test. Nothing creates a ledger record from it. (The gossip fact, by contrast, is seeded into the network.)                                                                                                                                                                                                                                                                   | `limerick-mod/src/living_world.rs`, `limerick-mod/src/world.rs:22`, `:31-60`, `limerick-engine/tests/world_sheet.rs:679`                                                                                                                |
| C13 | Replies are committed exactly as written. The structural check (complete JSON, non-empty dialogue, within 4,096 characters) is the only gate; there is no replacement line, grounding pass or fallback.                                                                                                                                                                                                                                                                                             | `limerick-core/AGENTS.md:27`, `docs/agent/inference-rules.md`                                                                                                                                                                           |
| C14 | Advice or a referral never becomes a task on the desktop assignment path (a unit test pins it).                                                                                                                                                                                                                                                                                                                                                                                                     | `game_session.rs:2533-2563`                                                                                                                                                                                                             |
| C15 | Dialogue learns state through deterministic sentences in existing Endpoint input fields: what the speaker knows (`aboutYou.knows`) and what it knows of the player (`thisMoment.theStranger`, built from state the NPC holds, issue [#2196](https://github.com/dmooney/Rundale/issues/2196)). No input field carries task state.                                                                                                                                                                    | `limerick-core/src/endpoint_input.rs:438`, `:614-632`                                                                                                                                                                                   |
| C16 | The desktop prompt carries a `WORK REFERRALS` paragraph. The phone Endpoint has its own rule instead: send the stranger only to a person or place the request names, for a reason the request gives. `theVillage.people` carries each person's authored `work` and `workplace`.                                                                                                                                                                                                                     | `limerick-npc/src/ticks/prompt.rs:152`; `rundale-dialogue.v1.json` (`instructions`, `theVillage`)                                                                                                                                       |
| C17 | Phase 6 gate P6-06 exists as a pending test that fails on purpose until #2308. It presupposes `/debug tasks`, an assignment caused by two scripted lines, progress caused by arriving at the Letter Office, and completion caused by a line to Peig.                                                                                                                                                                                                                                                | `limerick-mobile-ffi/src/tests/living_world.rs:377-440`, `docs/test-plans/phase-6-test-cases.md` (P6-06)                                                                                                                                |

### 2.2 Gaps between the current engine and the authored task

These follow from the table. They are why "wire `assigned_task` into durable state"
would not be enough, even if it were wanted.

1. **The authored task cannot reach the ledger on the phone path** (C11, C12).
   There is no producer.
2. **The authored description would be rejected by the existing assignment
   grammar.** "Tell Peig Hannigan at the Letter Office ..." starts with `tell`, which
   is not in the work-verb list (`dialogue_apply.rs:340`), and it names the Letter
   Office while the speaker is at the Connolly Cottage, which the remote-location
   check refuses (`dialogue_apply.rs:519`, C3).
3. **The recorded site is wrong** (C4). The authored site is the Letter Office; the
   engine would record the cottage.
4. **"In progress" has two incompatible meanings.** The authored state is "the
   player is at the Letter Office with the task still assigned". The engine
   progresses on a matching work action (C5), and "I walk to the Letter Office" is
   not one.
5. **There is no completion trigger** (C6) and no completion event.
6. **Dialogue is not told the task state** (C15), so "afterwards" (G4) has no
   deterministic carrier.
7. **No player-visible or tester-visible read model exists** (C10).
8. **The P6-06 test and the classifier disagree.** "Is there anything I can do for
   you?" matches none of the `is_task_request_input` phrases, so even the desktop
   path would not assign on the scripted line.

### 2.3 Historical assumptions and model promises, separated

| Kind              | Item                                                                                                                       | Status under this proposal                                                                                                                                          |
| ----------------- | -------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Historical        | The model emits `assigned_task` JSON metadata alongside dialogue and the engine adopts it (#1781, #1833).                  | Not carried forward. The phone Endpoint has no such field (C11). Whether the dormant code is kept is open decision D8.                                              |
| Historical        | Open-ended farm jobs ("clear the stones", "break the clods") invented by the model in the moment, matched by word overlap. | Not in scope. The Phase 6 task is one authored definition.                                                                                                          |
| Historical        | A hard "advertised durable-work contract": any grounded work talk must produce a ledger record.                            | Not carried forward. Only the authored task is a ledger task. Casual or invented work talk is just talk.                                                            |
| Historical        | Post-generation guards that replace a referral reply with "I beg your pardon; I lost the thread of that." (#1890, #1900).  | Removed by [#2295](https://github.com/dmooney/Rundale/issues/2295); the string survives only in a UI e2e fixture (`limerick/apps/ui/e2e/interactions.spec.ts:203`). |
| Model promise     | "Aye. Take a spade and clear the stones from the potato ridge." with `assigned_task` set (#1891, #1899).                   | A proposal. Under this contract it commits nothing unless it matches the authored definition and the engine's preconditions.                                        |
| Model promise     | "I will inspect it" after the player says the stones are stacked (#1899).                                                  | A promise in prose. It completes nothing.                                                                                                                           |
| Model promise     | A Gemini referral "Ye might try Siobhan Murphy up at Murphy's Farm" (#1890).                                               | Grounded speech, not state. See section 4.                                                                                                                          |
| Current/ambiguous | Task `location` meaning "where it was assigned".                                                                           | Replaced by an authored site (section 3.2).                                                                                                                         |

## 3. Proposed contract

The principle: **the task is authored data and engine state. The model's only
roles are to speak it and to be told its state.** Nothing in prose creates,
advances or completes a task (G5, G6, G12).

### 3.1 Scope

In scope for Milestone 6 (proposed):

- One authored task definition, one giver, one recipient, one site, in the canonical
  world.
- A lifecycle from offer to a terminal state, committed only by the engine.
- Deterministic state sentences fed to dialogue so later talk reflects state.
- A tester-visible read model, and journaled events that explain each transition.

Out of scope (proposed):

- Model-invented tasks, open-ended work, rewards, wages, inventory, reputation,
  deadlines and failure by timeout (see D7 for the one timing question).
- More than one active task, task chains, and tasks for any NPC other than the giver.
- A task user interface beyond what a tester needs to read state (D9).
- Hiring, pay and employer state for work referrals (section 4).

### 3.2 The authored definition

Proposed: the mod's `task` becomes the sole source of a task. It gains a stable
`id` and structured fields next to the existing prose, so the engine never parses
prose to learn who or where:

| Field                  | Meaning                                                                           | Today                                                                         |
| ---------------------- | --------------------------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| `id`                   | Stable task id; references use it, never the wording.                             | absent                                                                        |
| `assigned_by`          | Giver NPC id.                                                                     | present (`assigned_by`)                                                       |
| `recipient`            | The NPC the message goes to (Peig).                                               | implied only in prose; the sheet test derives it by searching the description |
| `giver_site`           | Where the giver can offer it (the Connolly Cottage).                              | absent                                                                        |
| `site`                 | Where it is advanced and completed (the Letter Office).                           | present (`location`)                                                          |
| `description`          | The ledger text and the offer's content, as authored.                             | present                                                                       |
| `afterwards` sentences | Deterministic lines for the giver's and recipient's later dialogue (section 3.5). | prose only                                                                    |

The ledger record gains the authored `id` and the authored `site`. The current
`location` field (C4) must stop meaning "where the speaker stood". This is a
data-model change the implementation issue owns; it needs a migration note because
saves already carry `PlayerTask` (C8).

### 3.3 Lifecycle

States and transitions are committed only by the engine, each inside a turn commit
(so a failed or cancelled turn changes nothing, G7), and each journaled with its
cause (G9).

| State          | Entered when (engine-validated)                                                                                                                                                                                                         | Inference may propose                                             | Event (proposed)                                                     |
| -------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------- | -------------------------------------------------------------------- |
| (none)         | New game, or no offer yet.                                                                                                                                                                                                              | nothing                                                           | none                                                                 |
| **Offered**    | The player is at the giver's site, the giver is present, the task is unoffered, and the player's committed line is a request for work or help from the giver. The engine adds the authored task sentence to that turn's dialogue input. | a request classification (D1); the offer's wording                | `PlayerTaskOffered`                                                  |
| **Assigned**   | While `Offered`, the player's next committed line to the giver accepts it (D2). The record is created from the authored definition, not from model text.                                                                                | an accept or decline classification                               | `PlayerTaskAssigned` (exists)                                        |
| **InProgress** | While `Assigned`, the player arrives at the authored site (a committed movement). Deterministic. No text matching.                                                                                                                      | nothing                                                           | `PlayerTaskProgressed` (exists; the cause becomes "arrived at site") |
| **Completed**  | While `InProgress`, the recipient is present at the site and the player's committed line to the recipient delivers the message (D3). The engine, not the model's reply, decides.                                                        | a "delivers the task message" classification of the player's line | `PlayerTaskCompleted` (new)                                          |
| **Declined**   | While `Offered`, the player's next committed line to the giver declines, or the player leaves without accepting (D4).                                                                                                                   | a decline classification                                          | `PlayerTaskDeclined` (new)                                           |
| **Cancelled**  | While `Assigned` or `InProgress`, the player explicitly abandons it, or the giver releases it (D5).                                                                                                                                     | a classification of an abandon request                            | `PlayerTaskCancelled` (new)                                          |

Rules that hold across states (proposed):

1. **One transition per cause, idempotent.** Replaying a journal, retrying a request
   or a duplicate delivery never creates a second record or repeats an event. The
   existing id and replay guarantees (C2, C8) are the model.
2. **No backwards moves.** A stale payload cannot reopen a terminal task (C8).
3. **Preconditions are world facts.** Presence, location, game-time order and the
   task's current state are read from authoritative state at commit time, not from
   the player's or the model's words.
4. **A turn that fails, is cancelled, is interrupted or whose reply is rejected
   commits no transition** (G7, C9).
5. **Time passing alone changes nothing** in the minimal task (D7 asks whether the
   owner wants any timing).
6. **Movement and waiting are not completion.** Reaching the site progresses;
   it never completes.

### 3.4 What the engine validates and commits, and what inference may propose

| Concern                                                              | Engine (validates and commits)                                                                                                                                                 | Inference (may only propose)                                                                                                                       |
| -------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| Whether a task exists, its text, giver, site, recipient              | Authored data only.                                                                                                                                                            | Nothing. A model never names a task, an id, a site or an NPC for the ledger.                                                                       |
| Whether the player asked, accepted, declined, abandoned or delivered | The engine's gate on a structured decision with a confidence threshold (the pattern the intent Endpoint already uses for recipient and action), plus the preconditions in 3.3. | A classification of the player's own line, with probabilities. A low-confidence or missing decision commits no transition (the line is just talk). |
| Presence, location, state order, time                                | Authoritative world state.                                                                                                                                                     | Nothing.                                                                                                                                           |
| What the NPC says                                                    | Nothing beyond the structural reply contract (G12). The engine does not read the reply to decide state.                                                                        | The words, informed by deterministic state sentences (3.5).                                                                                        |
| Event creation and journaling                                        | Engine, in the same transaction as the turn.                                                                                                                                   | Nothing.                                                                                                                                           |

Two consequences worth stating plainly:

- The offer is **driven by state, not read from prose**. The engine decides to put
  the authored task into the giver's input, then records `Offered` when that turn
  commits. This keeps the "no reply inspection" rule (G12) intact; it differs from
  the desktop path, which reads the reply (C3).
- The model's wording of an offer may drift from the authored text. The record
  always carries the authored text. Whether a drifting offer is acceptable is a
  product-quality question measured by the grounding checks in 7.2 item 6, not a state
  question.

### 3.5 Dialogue consistency with authoritative state

Proposed carrier: deterministic sentences derived from the ledger, delivered through
the fields NPC state already uses (C15), so **no Endpoint schema change is
required**:

- `Offered`: the giver's `aboutYou.knows` holds one sentence stating the
  errand it wants done, in the authored wording.
- `Assigned` and `InProgress`: the giver's `theStranger` (or `knows`) says the player
  agreed to carry the message; the recipient's input says nothing, because the
  message has not reached them (they must not behave as though they know it, the
  same "should know" and "must not yet know" test as gossip).
- `Completed`: the recipient's `knows` holds "Róisín asked you to hold any letter
  that comes for the Connollys until she calls for it"; the giver's `theStranger`
  says the player says the message was given. These are the authored `afterwards`
  sentences.
- `Declined` or `Cancelled`: the giver's input says so; the recipient's input is
  unchanged.

The sentences are facts about what was said and agreed, not claims about the world
beyond the engine's knowledge. Peig "holding the letters" is a fiction the
sentences state as her instruction, not a mechanic (no letter object exists).

Constraint carried from the Endpoint: the dialogue Endpoint input order is frozen
(issue [#2299](https://github.com/dmooney/Rundale/issues/2299)). Using existing
fields avoids reopening it. If the owner prefers a dedicated field, that is an
Endpoint change in its own PR (D6).

What the engine will not do: scan a reply for a promise ("I will hold them") and
treat it as state, or replace a reply that contradicts the ledger (G12). A
contradiction is a grounding defect fixed through the facts, the prompt or the
model, and measured, not guarded.

### 3.6 Persistence and save/resume

Proposed requirements, mostly satisfied by the existing journal (C8):

1. Every transition is part of the committing turn's single transaction, with the
   post-mutation record, so a crash leaves either the old or the new state.
2. Restoring a save restores the ledger exactly; relaunching must not re-run
   content initialisation that would re-offer a completed or declined task
   (vision §11.1: loading a save must not rerun initialisation and overwrite progress).
3. A save made at each state (`Offered`, `Assigned`, `InProgress`, `Completed`,
   `Declined`, `Cancelled`) resumes to that state, and the next valid action works.
4. Old saves with no task ledger entries load unchanged; saves with existing
   `PlayerTask` records load with the added fields defaulted (migration note in the
   implementation PR).
5. Branching or forking a save copies the ledger with the branch; branch state does
   not leak between branches (vision §7.5: separate branches, no last-write-wins merging of tasks).
6. Replay never calls a provider and never re-runs classification: the journaled
   decision is authoritative.

### 3.7 Events and observability

1. Each transition journals one semantic event naming its cause (G9): which turn,
   which classification and confidence where one was used, which world fact
   (arrival at the site, recipient present) satisfied the precondition.
2. A tester-visible read model exists: `/debug tasks` (the seam P6-06 already
   presupposes), listing id, state, giver, site and the cause of the last
   transition. It reads the ledger and never writes (D9 asks whether more is wanted).
3. The Swift wire projection carries the task events so the transcript or a debug
   view can show them; player-facing treatment is an owner call (D9).
4. A rejected or ignored classification logs a debug event with the reason (low
   confidence, wrong location, recipient absent) so a tester can tell "not
   recognised" from "not allowed".
5. The kill switch `player-task-progression` (C3) is retained or replaced by an
   equivalent; the owner's call is part of D8.

## 4. Work referrals (#1890, #1900)

Both reports are a player asking an NPC who nearby might have work, and the NPC
answering with the generic fallback although a grounded answer existed. Their
mechanism, a post-generation guard replacing the reply, is gone (C13, #2295). The
contract still needs to say what a grounded referral is, so later quality
work has something to measure.

### 4.1 Proposed definition

A **work referral** is an NPC naming a person or place and a reason, to a player who
asked who might have work. It is **speech, not state**:

1. **Grounded** means the named person or place is in the speaker's request context
   (`theVillage.people` or `places`) and the stated reason is something the request
   gives about them: their authored `work` or `workplace`. Friendship, proximity or
   a guess is not a reason. The phone Endpoint already instructs this (C16).
2. **Non-promissory.** A referral never says or implies a job exists, is offered, was
   agreed, or will be given. "You might ask Peig, she keeps the Letter Office" is a
   referral. "Peig has work for you" is not, unless authored state says so.
3. **No state effect.** A referral creates no ledger record, no offer, no reputation
   and no knowledge on the named person's side. The engine never reads the referral
   to change state (G12). A referral whose subject is the authored task's giver does
   **not** offer the task; only the offer rule in 3.3 does.
4. **Honest when nothing fits.** If no person in context has relevant authored work,
   the NPC says so plainly (the Endpoint already requires admitting what it does not
   know and giving something back).
5. **Validation is measurement, not interception.** Whether a referral is grounded
   is checked offline and live against the request that produced it (names exist,
   reason matches authored work, no promised job), reported as a quality rate. It
   never replaces or blocks a delivered reply (G12).

### 4.2 Assessment of the current iPhone and Endpoint behaviour

- **Mechanism (Current):** on the phone path a referral reply is committed as
  written if it passes the structural contract (C13). A well-formed referral
  therefore reaches the player. The failure in #1890 and #1900 cannot recur by that
  mechanism. This is a code-reading assessment; **no live Endpoint run was made for
  this document**, so referral quality on the Endpoint is unmeasured.
- **Grounding (Current):** the Endpoint input carries each person's `work` and
  `workplace` and instructs "send the stranger only to a person or place the request
  names, and only for a reason the request gives about them" (C16).
- **Gap:** in the canonical three-NPC world the authored occupations are Peig's
  (letter-office keeper), Mícheál's (smallholder and cattle drover) and Róisín's
  (spinner and household bookkeeper). A farm-work question to Peig or Róisín can
  be grounded by pointing at Mícheál (a smallholder), but nothing authored says he
  has work to give, so the grounded answer is "he farms; whether he needs hands is
  his own affair", and an honest "I do not know of any" is also a pass. A grounded
  answer must not claim a vacancy. Whether the tiny world should carry a
  worker-seeking referral check at all is a product choice (D10).
- **Gap:** no deterministic test exercises a referral on the shipped Endpoint input
  (the desktop proof `play_1853_grounded_work_referral.txt` scripts replies at the
  desktop path). The phone path has no referral case in the Phase 6 gates.

## 5. Assessment of the historical reproductions against this contract

They are evidence of dialogue and state divergence on the desktop path, not authority
for the design (owner direction, 2026-10-04).

| Report                                               | What it showed                                                                                                                                                                                    | Verdict under the proposed contract                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| ---------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| #1891 (original)                                     | Siobhan gave a grounded job and the model emitted `assigned_task` twice; `active_tasks` stayed `[]` after the directive and the acceptance. Delivered dialogue accepted; no `PlayerTaskAssigned`. | Under the proposal there is no model-created task, so the premise (that the model's metadata should have created one) is rejected. Siobhan's farm job is not an authored task, so an empty ledger is correct. The dialogue/state divergence it reveals is real but is addressed by 3.5 only for the authored task. Why the desktop path recorded nothing is not established here: the classifier gate, the grounding gate and the location check (C3) are candidates, and this was not reproduced. |
| #1899 (consolidated)                                 | Coherent farm job flow over five turns including a physical action and `/wait 180`; `active_tasks` empty throughout; Siobhan said she would inspect. A recurrence of closed #1833.                | Same verdict. "She will inspect" is a promise in prose and completes nothing (G5). Stones cleared by typed action is not a world action the engine verifies, so nothing in that flow is a valid completion trigger. Out of scope for the Phase 6 task.                                                                                                                                                                                                                                             |
| #1890 (Peig, grounded referral replaced by fallback) | The raw log showed a correct authored referral; delivered text was the generic fallback.                                                                                                          | Contract-compliant behaviour is the raw reply. The replacement mechanism is gone (C13). Residual risk is referral grounding quality on the Endpoint, unmeasured (4.2).                                                                                                                                                                                                                                                                                                                             |
| #1900 (Sean Ruadh Kelly present, fallback)           | An in-character request at Murphy's Farm produced the fallback although Siobhan was co-located.                                                                                                   | Same as #1890. Siobhan does not exist in the canonical three-NPC world, so the reproduction is a desktop-world case; its tiny-world analogue is D10.                                                                                                                                                                                                                                                                                                                                               |

## 6. Open decisions for the owner

**None of these is decided.** Each lists options and a recommendation; the
recommendation is a proposal only. Implementation of #2308 should not start on any
decision it depends on until the owner records an answer. Decisions needed before
issue #2308 can be built: D1, D2, D3, D4, D5, D6. The rest can follow.

| Id  | Decision                                                                                  | Options                                                                                                                                                                                                                                                                                     | Recommendation (not decided)                                                                                                                                                                                                           |
| --- | ----------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| D1  | How does the engine recognise that the player is asking the giver for work or help?       | (a) Add a decision to the intent Endpoint (validated with a confidence threshold). (b) Keep the fixed English phrase list (`is_task_request_input`). (c) No recognition: the giver offers the task to anyone who speaks to them at the site, once.                                          | (a). (b) is wording-matching on an English list and cannot match paraphrases, as the scripted P6-06 line shows (2.2 item 8). (c) is the cheapest and a fair fallback if the owner wants no Endpoint change. (a) is an Endpoint change. |
| D2  | How is acceptance established?                                                            | (a) No acceptance step; the offer assigns it. (b) A validated accept/decline classification of the player's next line to the giver. (c) A phrase list. (d) An explicit app control.                                                                                                         | (b). The spec and P6-06 step 2 say "agree", and (a) would assign tasks the player never agreed to. (a) is simplest if the owner treats "she asks, you carry on" as agreement.                                                          |
| D3  | What counts as delivering the message (the completion action)?                            | (a) Any committed conversation turn with the recipient at the site while `InProgress`. (b) A validated "delivers the task message" classification of the player's line to the recipient, with the recipient present. (c) A keyword match on the player's line. (d) An explicit app control. | (b). (a) lets "hello" complete it, which contradicts "through the intended world action"; (c) is wording-matching. (a) is acceptable if the owner prefers the player simply speaking to Peig to be the intended act.                   |
| D4  | What happens if the player declines, or leaves without answering?                         | (a) `Declined` is terminal; the giver may offer again later. (b) `Declined` is terminal and permanent for the proof. (c) Leaving and returning re-offers while unaccepted; no `Declined` state.                                                                                             | (c) for the proof: fewest states, repeatable. Decide whether re-offer after an explicit decline is allowed in the full game separately.                                                                                                |
| D5  | Can the player cancel, and is there a cancellation state?                                 | (a) No cancel in Milestone 6; the task stays until completed. (b) An explicit abandon request, validated like acceptance, giving `Cancelled`. (c) The giver can release it.                                                                                                                 | (a). Spec G2 asks only for assignment, progress and completion. Cancel is the owner's product question; it adds a classification and a state. Phase 6 test case P6-06 has no cancel step.                                              |
| D6  | How does dialogue learn task state?                                                       | (a) Deterministic sentences in existing fields (`knows`, `theStranger`). (b) A new dedicated input field. (c) The model infers it from the transcript.                                                                                                                                      | (a). It follows the memory and gossip pattern (C15) and needs no schema change. (b) needs an Endpoint change and a frozen-order review. (c) is the failure mode the vision warns against and is not recommended.                       |
| D7  | Does the task have any timing (deadline, "before midday", waiting for the recipient)?     | (a) None. (b) A single "recipient must be at the site" precondition only, which the schedule already gives (Peig is at the Letter Office at 10:00 and 15:00). (c) A deadline and a failure state.                                                                                           | (b). (c) adds a `Failed` state and time pressure with no spec requirement.                                                                                                                                                             |
| D8  | What happens to the desktop-era `assigned_task` path and text-overlap progression?        | (a) Keep dormant, unused by the phone path, and do not extend it. (b) Remove it with the follow-up. (c) Adapt it as the producer of the authored task.                                                                                                                                      | (a) now, (b) after #2308 passes. (c) is the "restore old behaviour" the owner ruled out. Also decide the kill switch.                                                                                                                  |
| D9  | How visible is a task to the player and the tester?                                       | (a) Tester-only: `/debug tasks` and the debug snapshot. (b) Also a line in the transcript when a state changes. (c) A task entry in the player UI.                                                                                                                                          | (a) for #2308; the product spec asks for no player task UI. (b) and (c) are product decisions.                                                                                                                                         |
| D10 | In the tiny world, is a worker-seeking referral in scope, and what is its correct answer? | (a) Out of scope for Milestone 6; add a referral grounding check to the NPC-dialogue quality gate later. (b) In scope: a Phase 6 side check that a work question gets a grounded or honest answer and no task. (c) Add a work-offering NPC to the tiny world.                               | (b), measured, not guarded. (c) conflicts with the "no extra NPCs" rule (G8) and needs an owner exception.                                                                                                                             |
| D11 | Who is the recipient of the message: is Peig named in authored data or derived?           | (a) Authored field `recipient` (3.2). (b) Keep deriving from the description text as the sheet test does today.                                                                                                                                                                             | (a). A derived recipient makes engine behaviour depend on prose.                                                                                                                                                                       |

## 7. Proposed acceptance criteria and proof methods for #2308

These are derived from this proposal; they take effect only if the owner accepts
the relevant decisions. They refine, and do not weaken, the criteria in #2308 and
P6-06 (G1 to G4, G11). References to D-numbers mean "under the recommended option".

### 7.1 Acceptance criteria

| Id    | Criterion                                                                                                                                                                                                                | Traces to |
| ----- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------- |
| AC-1  | Before any request, the ledger has no task and `/debug tasks` says so; the recipient and a third party show no sign of the task in their input.                                                                          | G11       |
| AC-2  | Asking Róisín for help at the Connolly Cottage causes the authored task to be offered, and the offer is recorded as an engine transition with a journaled cause. No model output chooses the task, its text or its site. | G1, G6    |
| AC-3  | Accepting causes exactly one `Assigned` record created from the authored definition; repeating the acceptance, replaying the journal or retrying the request creates no second record or event.                          | G2        |
| AC-4  | A declined or ignored offer, and any failed, cancelled or interrupted turn, commits no transition and leaves the ledger byte-for-byte unchanged.                                                                         | G6, G7    |
| AC-5  | Arriving at the Letter Office while `Assigned` moves the task to `InProgress`, caused by the committed arrival; arriving elsewhere does not, and waiting does not complete it.                                           | G2, G9    |
| AC-6  | Delivering Róisín's message to Peig at the Letter Office with Peig present moves the task to `Completed`. The same line to Peig without the task, or with Peig absent, completes nothing.                                | G2, G5    |
| AC-7  | A model reply that claims the task is done, or that Peig will hold the letters, changes no state. A reply that promises work in a referral creates no task.                                                              | G5, G12   |
| AC-8  | After a close and reopen at each of `Offered`, `Assigned`, `InProgress` and `Completed`, the ledger is identical and the next valid step works.                                                                          | G3        |
| AC-9  | After completion, Peig's next dialogue input carries the delivered-message sentence and Róisín's carries the player's report; before completion neither does. This is shown on the Endpoint input, not by judging prose. | G4, G10   |
| AC-10 | A new game repeats AC-1 to AC-9 with the same stages (the "repeat" of the proof shape).                                                                                                                                  | G11       |
| AC-11 | Each transition is explainable from the journal: a tester can say which turn and which world fact caused it, from `/debug tasks` and the journaled event, with no database reading.                                      | G9        |
| AC-12 | A work referral on the shipped Endpoint input, at the tiny world, either names a person present in `theVillage.people` for that person's authored work, or admits it knows none, and creates no task or offer.           | 4.1, D10  |

### 7.2 Proof methods

1. **Deterministic production-path cases.** Land P6-06 as the cases that exercise
   the shipped C boundary (`limerick-mobile-ffi/src/tests/living_world.rs`), with
   inference scripted at the Endpoint boundary as the other proofs do. Review and, if
   decisions D1 to D3 change them, update the scripted lines: the current P6-06
   lines presuppose a phrase-matched assignment and a line-to-Peig completion that
   this proposal replaces with validated classifications. Any change to a gate
   must keep it equal or stronger and state the requirement it changes (the
   repository rule on weakening gates).
2. **Authoritative-state assertions**, not prose assertions: ledger state, journaled
   event and its cause, and the Endpoint input carrying (or lacking) the task
   sentence at each stage (AC-9). Dialogue text is never the only evidence.
3. **Negative cases** as first-class tests: wrong location, recipient absent, low
   confidence, replayed request, cancelled turn, model reply claiming completion
   (AC-4, AC-6, AC-7).
4. **Save/resume** at every state, using the existing relaunch helper, plus a
   recovery test that interrupts a turn after the transition's journal write.
5. **Migration test** for a save with a pre-existing `PlayerTask` and one with none
   (3.6 items 3 and 4).
6. **Referral grounding** (AC-12): a deterministic check that the input offers the
   facts, plus one opt-in live Endpoint run against `limerick-prod` (authorised
   for live Endpoint testing in the repository's agent guide) with the transcript,
   input and ledger recorded. Dialogue grounding is reported separately from state
   correctness, as the vision requires.
7. **Simulator recording** of the whole stage sequence, labelled as a simulator
   demonstration and not as physical-iPhone validation, per the phase demo plan.
   Add the live Endpoint run to the exit demo ([#2199](https://github.com/dmooney/Rundale/issues/2199)).

### 7.3 Follow-up work this proposal implies (not started)

Scoped here only so the owner can see the shape. None of it begins until the
decisions it depends on are recorded.

1. Mod data: add `id`, `recipient`, `giver_site` and structured `afterwards` to the
   task fact; update the sheet, the loader and `world_sheet.rs` together
   (D11).
2. Engine: ledger gains authored id and site; `Offered` (and `Declined`, `Cancelled`
   if chosen) states; transitions at the movement and dialogue commit seams;
   `PlayerTaskCompleted` event; migration (3.2, 3.3, 3.6).
3. Intent Endpoint: decisions for request, accept, decline and deliver, if D1 to D3
   choose classification. Own PR; an Endpoint change is an owner-visible surface.
4. Dialogue input: task-state sentences in `knows` and `theStranger` (3.5, D6).
5. Read model: `/debug tasks` and the wire projection (3.7, D9).
6. Tests and gate: P6-06 landed with the cases in 7.2, and `LIVING_WORLD_PROOFS`
   updated as the mobile FFI `AGENTS.md` describes.
7. Decide the fate of the dormant desktop assignment path (D8).

### 7.4 What this document does not claim

- It does not claim the task works on any build. Section 2 states what the code
  does; the Phase 6 task case is pending and fails on purpose.
- It ran no live Endpoint, simulator or physical-iPhone session. The assessment of
  referral quality on the Endpoint is from code and Endpoint instructions only.
- It does not decide any product question. The recommendations in section 6 are
  proposals for the owner to accept, change or reject.
