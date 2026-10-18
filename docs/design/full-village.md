# Proposal: the first full-village target

> Status: Proposed · Issue: [#2203](https://github.com/dmooney/Rundale/issues/2203) ·
> Milestone: Full village (spec Milestone 6)

Product spec Milestone 6 grows the canonical tiny world (three locations, three
NPCs) into a full village. The spec does not set a target, so issue #2203 asks
for one. This proposal authors no content and changes no engine or app code.
Each stage below becomes its own issue in **Full village**, stating its player
need, acceptance criteria and test plan before implementation.

## 1. Target

The owner set the direction on 2026-10-18:

- The village's size comes from its historical population, reached in stages.
- **The first target is a subset of Kilteevan village: five households of about
  three people each, all living in the village.**
- The parish comes later.

That makes the first target **five households, 15 people, and six locations**:
the village road plus one home for each household. Peig's Letter Office and the
Connollys' cottage are two of the five, so three households are new.

|            | Tiny world (today) | First target |
| ---------- | ------------------ | ------------ |
| Households | 2                  | 5            |
| People     | 3                  | 15           |
| Locations  | 3                  | 6            |

### 1.1 Historical scale

Kilteevan civil parish had 2,568 people in Samuel Lewis's _Topographical
Dictionary of Ireland_ (1837), which uses 1831 census figures. The 1821 returns
were destroyed in 1922, and the surviving fragments for Kilteevan cover only
three townlands. So about 2,500 people is the best figure for 1820, and the
population was still rising. At five to six people per household, that is
roughly 400–450 households across the parish's 23 townlands. The village was a
small cluster within that, probably a few dozen households.

Five households is therefore a slice of the village, not all of it. Three people
on average is fewer than a real 1820 household of five or six. The rest of each
family (young children, relatives away working) can exist in authored knowledge
and dialogue without being NPCs. Later targets grow toward the whole village,
then the parish. At parish scale most people cannot be hand-authored, so that
stage needs the generated-population approach sketched in
[Scalable NPC Data Design](scalable-npc-data-design.md), with its own proposal.

## 2. Map

Every household lives on or just off the village road, so the village road is the
only hub.

```text
            Letter Office      Gallagher's Forge
                       \       /
 Connolly Cottage -- Kilteevan Village -- Darcy's
                          |
                    Fallon's Cabin
```

| Location                 | Exits                                                                       | Household | What it adds                                     |
| ------------------------ | --------------------------------------------------------------------------- | --------- | ------------------------------------------------ |
| Kilteevan Village (have) | Letter Office, Connolly Cottage, Gallagher's Forge, Darcy's, Fallon's Cabin | —         | The street, where households cross paths.        |
| Letter Office (have)     | Kilteevan Village                                                           | Hannigan  | News from beyond the parish; the gossip source.  |
| Connolly Cottage (have)  | Kilteevan Village                                                           | Connolly  | Smallholding.                                    |
| Gallagher's Forge        | Kilteevan Village                                                           | Gallagher | A workplace people come to.                      |
| Darcy's                  | Kilteevan Village                                                           | Darcy     | A house that sells drink: the evening gathering. |
| Fallon's Cabin           | Kilteevan Village                                                           | Fallon    | A labouring family, the village's poorest.       |

Placement rules:

- **Homes 150–750 m from the village road,** which plays as a 2–10 minute walk.
  The engine turns the distance between coordinates into minutes at 1.25 m/s,
  rounded up, with a 1-minute floor (`meters_to_minutes` in `limerick-world`).
  So the rule is a distance: a home placed too close still shows as a 1-minute
  walk. The existing homes are 190 m and 263 m away.
- **Coordinates come from the geo tool** (`/rundale-geo-tool`). The new homes are
  fictional and placed relative to the village, with an honest `geo_source`, as
  the existing two are.
- **Five exits from the village road.** That is one more than is comfortable, but
  it stays readable: "You can go to: Letter Office, Connolly Cottage, Gallagher's
  Forge, Darcy's, Fallon's Cabin" wraps to three lines on a small iPhone. If later
  targets add more places, the street splits into two locations rather than
  growing a longer list.
- **Short names with distinct one-word aliases:** office, cottage, forge,
  Darcy's, cabin. Kilteevan Village already uses "road" and "village", so no new
  place may.

Shared places that are not homes, such as a chapel, the holy well or the bog, are
not in this target. They are candidates for the next one.

## 3. Households

Names, ages and roles are provisional until each stage's data review.

| Household                    | People                                                                                          | Stage |
| ---------------------------- | ----------------------------------------------------------------------------------------------- | ----- |
| Hannigan, Letter Office      | Peig (54, letter-office keeper); her nephew, who carries letters (new)                          | 4     |
| Connolly, Connolly Cottage   | Mícheál (38, smallholder and drover); Róisín (31, spinner); their widowed mother (new)          | 4     |
| Gallagher, Gallagher's Forge | Séamus (42, blacksmith); his wife; Colm (15, apprentice)                                        | 1     |
| Darcy, Darcy's               | Pádraig (58, keeps the house and sells drink); his wife; Niamh (22, their daughter)             | 2     |
| Fallon, Fallon's Cabin       | Donncha (26, labourer hired by the day); his wife; his mother, who keeps hens and tells stories | 3     |

That is 2 + 3 + 3 + 3 + 3 = 14 people, or 15 if one household has four. The
owner's average of three allows either.

Rules every stage checks:

- **No two people share a first name.** Each household shares its surname, which
  is intended: it exercises "Which Gallagher do you mean?".
- **No first name the recipient tests use as a probe.** The
  [recipient contract](dynamic-npc-context.md)'s acceptance matrix (§8, rows 10
  and 13) uses "ask Seán about the road" as its unknown-name case and "Mary,
  mother of God" as its exclamation case. So no Seán, Máire or Mary, and Brigid
  is unwise.
- **People are fictional,** as
  [ADR 009](../adr/009-real-geography-fictional-people.md) requires. Real families
  in the parish records are not used.
- **Reused pre-reset characters are reviewed, not copied.** Séamus and Colm
  Gallagher and Pádraig and Niamh Darcy come from the pre-reset world
  (`limerick/testing/fixtures/mods/rundale-legacy/`). Máire Gallagher does not,
  because of the probe rule. Donncha replaces the pre-reset labourer Sean Ruadh
  Kelly for the same reason.

## 4. Keeping it playable on the phone

- **At most four people in one place** at every hour the world sheet lists. A
  whole household at home is at most three or four, so this mainly constrains the
  village road in the afternoon and Darcy's in the evening, where visitors
  join the Darcys. Stages stagger schedules to keep to it. Under the
  [recipient contract](dynamic-npc-context.md) (§3.1, step 5), an unaddressed line
  with two or more people present asks "Who are you speaking to?". With four
  people that is five buttons, four people and **All**.
- **One schedule for every day.** An 1820 Sunday would put the whole village at
  Mass at once. The tiny world uses one schedule for all days, and this target
  does too. Sundays and market days wait for a later proposal.
- **Context size.** Each dialogue call carries every NPC and every place (the
  contract's §6 snapshot, `knownPeople` and `knownPlaces`). Fifteen people and six
  places is a modest increase on three and three. At parish scale this has to
  change, which is another reason the parish needs its own design.
- **The recipient decision** offers every person present and every introduced
  person elsewhere (contract §4.3). Stage 3 re-runs the labelled recipient cases
  with the larger roster.

## 5. Stages

One household per stage. Each stage adds one location and at most three people,
leaves a playable world that agrees with the world sheet, and passes the Phase 6
gate (`just mobile-verify --phase 6`). Stages start after Milestone 5 closes and
#2200 lands the regression fixture, the gate and the proposal template.

| Stage | Adds                                                    | World after               | What it tests first                                                 |
| ----- | ------------------------------------------------------- | ------------------------- | ------------------------------------------------------------------- |
| 1     | The Gallaghers and their forge                          | 4 locations, 6 people     | A third household; a family of three at home together               |
| 2     | The Darcys and their house                              | 5 locations, 9 people     | An evening gathering at the four-person cap                         |
| 3     | The Fallons and their cabin                             | 6 locations, 12 people    | Five exits on the village road; the recipient re-run                |
| 4     | A third member for the Hannigan and Connolly households | 6 locations, 14–15 people | Growing an existing household without breaking its tested behaviour |

Stage 4 changes households that come from the tiny world. The tiny world stays
as the separate regression fixture (#2200), so the production world may grow
them without affecting the fixture's tests.

Each stage issue states the player need, the data review of anything reused,
acceptance criteria (the world sheet and its oracle agree, exits and presence
within the limits above, walks in the 2–10 minute band), the test plan, and a
paced simulator walkthrough as evidence.

## Sources

- Samuel Lewis, [_A Topographical Dictionary of Ireland_, "Kilteevan"](https://libraryireland.com/topog/K/Kilteevan-Ballintobber-Roscommon.php) (1837).
- Virtual Record Treasury of Ireland, [1821 Census: Kilteevan civil parish](https://virtualtreasury.ie/item/VRTI-CEN-1821-4-43-2-10): surviving fragments for three townlands.
- [Townlands in Kilteevan](https://www.townlands.ie/roscommon/kilteevan1/): the parish's 23 townlands.
