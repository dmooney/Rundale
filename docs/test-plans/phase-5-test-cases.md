# Phase 5 living-world test cases

Status: implementation and simulator plan. Physical results must be recorded in
`mobile/acceptance.md`; this document is not evidence that a case passed.

## Preconditions

- Use the canonical three-location/three-NPC content bundle and a known fresh
  Phase 5 fixture unless the case explicitly starts from an older save.
- Enable internal diagnostics through `RUNDALE_INTERNAL_DIAGNOSTICS`. Public
  builds must leave that build setting disabled.
- Treat `/debug memory <npc>`, `/debug knowledge <npc>`, `/debug tasks`, and
  `/debug world` as read-only. Confirm their state revision does not change.
- Treat `/setup` as an internal mutation path. Reset must be exactly
  `/setup reset CONFIRM`; setup records must show `diagnostic_override`.

## P5-1 — Player claim memory

Tell Peig, “I grew up in Athleague.” Accept a validated v2 response containing
one exact-evidence memory proposal. Peig’s typed ledger must contain one
`player_claim` with request/event/location/time/revision provenance. Mícheál and
Róisín must not contain it. Relaunch and repeat all assertions. A mismatched,
late, canceled, malformed, or duplicate proposal must not change the ledger or
revision.

## P5-2 — Single gossip propagation

Before contact, Mícheál alone has `fact-micheal-stock`. During the authored
morning cottage contact, advance normal play with `/wait 1`. Róisín gains one
`heard_from_npc` record derived from Mícheál’s source record. Repeated waits do
not duplicate it; Peig never gains it. Relaunch before and after acquisition.

## P5-3 — Sealed-letter task

At the Letter Office, a validated Peig response may propose only
`task-deliver-peig-letter`. Assert `assigned`; explicitly take the letter and
assert `in_progress`; relaunch; give it to present Róisín at Connolly Cottage
and assert `completed`. Wrong speaker, place, target, task ID, order, canceled
response, or absent Róisín produces no transition. Later Peig grounding includes
the completed state.

## P5-4 — Weather decision

From 09:59 with Clear weather, `/wait 1` puts Mícheál in Kilteevan Village with
cause `schedule`. From the same fixture in Heavy Rain, `/wait 1` keeps him at
Connolly Cottage with cause `weather_override`. Assert both the scene/presence
behavior and typed decision record after relaunch.

## P5-5 — Peig schedule and availability

From 08:58 run `/wait 2`. `/people`, conversation target availability, and
`/debug world` must place Peig at the Letter Office. Relaunch there and repeat
the presence and conversation assertions.

## Required execution

Run the targeted Rust/SQLite/FFI/Swift tests, Endpoint v1/v2 schema fixtures,
the Phase 5 native UI suite, `./verify --phase 5`, `./verify --phase all`, the
production-path gameplay proof, `just check`, and `just verify`. Run the opt-in
live Endpoint memory/task proof only with configured credentials and strict
cost/output bounds. Finally repeat P5-1 through P5-5 on one signed Internal Beta
iPhone with force-quit/resume and cancellation; keep the separate Phase 4
small-screen/two-device matrix deferred and unpassed.
