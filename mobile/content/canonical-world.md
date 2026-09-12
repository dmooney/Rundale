# Canonical mobile world sheet — Phase 3

This sheet is the independent acceptance oracle for the Phase 3 content bundle.
Only these three playable places and three interactive people belong to the
tiny world.

| Stable ID | Place | Connections | Opening presence |
| --- | --- | --- | --- |
| `kilteevan-village` | Kilteevan Village | Letter Office, Connolly Cottage | Peig Hannigan |
| `letter-office` | Letter Office | Kilteevan Village | Nobody |
| `connolly-cottage` | Connolly Cottage | Kilteevan Village | Mícheál Connolly, Róisín Connolly |

| Stable ID | Person | Home | Role | Schedule |
| --- | --- | --- | --- | --- |
| `npc-peig` | Peig Hannigan | Letter Office | letter-office keeper and village observer | Village through 08:59; Letter Office from 09:00 |
| `npc-micheal` | Mícheál Connolly | Connolly Cottage | smallholder and cattle drover | Cottage through 09:59; village from 10:00 |
| `npc-roisin` | Róisín Connolly | Connolly Cottage | spinner and household bookkeeper | Cottage through 11:59; village from 12:00 |

The game opens at 08:58. A successful action advances explicit game time,
then schedules update. Movement and scheduled travel use the authored graph;
there is no wall-clock catch-up. The three canonical relationships are Peig–
Mícheál (neighbors), Peig–Róisín (friends), and Mícheál–Róisín (family).

The machine-readable authority is `phase3-tiny-world.json`. This sheet exists
so tests and reviewers can detect accidental drift rather than deriving their
expectations from the same runtime projection.
