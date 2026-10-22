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
                     Schoolhouse
```

| Location                 | Exits                                                                    | Household | What it adds                                                                        |
| ------------------------ | ------------------------------------------------------------------------ | --------- | ----------------------------------------------------------------------------------- |
| Kilteevan Village (have) | Letter Office, Connolly Cottage, Gallagher's Forge, Darcy's, Schoolhouse | —         | The street, where households cross paths.                                           |
| Letter Office (have)     | Kilteevan Village                                                        | Hannigan  | News from beyond the parish; the gossip source.                                     |
| Connolly Cottage (have)  | Kilteevan Village                                                        | Connolly  | Smallholding.                                                                       |
| Gallagher's Forge        | Kilteevan Village                                                        | Gallagher | A workplace people come to.                                                         |
| Darcy's                  | Kilteevan Village                                                        | Darcy     | Public house and huckster's shop: salt, tobacco and candles by day, drink at night. |
| Schoolhouse              | Kilteevan Village                                                        | Brennan   | A cabin where the hedge school meets; the village's readers and writers.            |

Placement rules:

- **Homes sit where they plausibly would in a small village,** a few minutes'
  walk from the road at most. No distance band is imposed. The engine turns the
  distance between coordinates into minutes at 1.25 m/s, rounded up, with a
  1-minute floor (`meters_to_minutes` in `limerick-world`), so neighbouring homes
  all show as a 1-minute walk. The existing homes are 190 m and 263 m away
  (3 and 4 minutes). The same function caps a walk at 120 minutes, which
  understates real distances at parish scale (Kilteevan to Curraghboy is about
  16.5 km, roughly three and a half hours on foot).
- **Coordinates come from the geo tool** (`/rundale-geo-tool`). The new homes are
  fictional and placed relative to the village, with an honest `geo_source`, as
  the existing two are.
- **Five exits from the village road,** one per home. The world, not the screen,
  sets how many exits a place has; long lists wrap or scroll.
- **Short names with distinct one-word aliases:** office, cottage, forge,
  Darcy's, school. Kilteevan Village already uses "road" and "village", so no new
  place may.

Shared places that are not homes, such as a chapel, the holy well or the bog, are
not in this target. They are candidates for the next one.

## 3. Households

Names, ages and roles are provisional until the stage 1 data review.

| Household                    | People                                                                                                                        |
| ---------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| Hannigan, Letter Office      | Peig (54, letter-office keeper); her nephew, who carries letters (new)                                                        |
| Connolly, Connolly Cottage   | Mícheál (38, smallholder and drover); Róisín (31, spinner); their widowed mother (new)                                        |
| Gallagher, Gallagher's Forge | Séamus (42, blacksmith); his wife; Colm (15, apprentice); their young daughter                                                |
| Darcy, Darcy's               | Pádraig (58, publican and shopkeeper); his wife; Niamh (22, their daughter)                                                   |
| Brennan, Schoolhouse         | Aoife (29, hedge schoolmistress); her father, the old schoolmaster, who writes letters for those who cannot; her brother (17) |

That is 2 + 3 + 4 + 3 + 3 = 15 people, 12 of them new.

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
  (`limerick/testing/fixtures/mods/rundale-legacy/`). So does Aoife
  Brennan, who now heads a schoolhouse household instead of living with the
  Gallaghers. Máire Gallagher does not return, because of the probe rule.

## 4. Keeping it playable on the phone

- **No cap on how many people share a place.** Schedules follow what people in
  the village would plausibly do, not a limit set by the screen (owner,
  2026-10-18). With households of about three, most places hold one to three
  people; crowds come from gatherings, such as evenings at Darcy's. Under the
  [recipient contract](dynamic-npc-context.md) (§3.1, step 5), an unaddressed line
  with two or more people present asks "Who are you speaking to?", with one button
  per person and **All**. The player scrolls sideways through the buttons when
  there are more than fit (owner, 2026-10-18), so a crowd needs no special
  handling.
- **One schedule for every day, for now.** Sunday Mass and Saturday market need
  the chapel and the market town, and neither is in this target. Until a stage adds
  the chapel, every NPC has one schedule for all days, as the tiny world does.
- **Context size.** Each dialogue call carries every NPC and every place (the
  contract's §6 snapshot, `knownPeople` and `knownPlaces`). Fifteen people and six
  places is a modest increase on three and three. At parish scale this has to
  change, which is another reason the parish needs its own design.
- **The recipient decision** offers every person present and every introduced
  person elsewhere (contract §4.3). Stage 1 re-runs the labelled recipient cases
  with the larger roster.

## 5. Stages

**Stage 1 is the whole first target:** the three new households and their homes,
and the new members of Peig's and the Connollys' households, in one issue. The
world goes from 3 locations and 3 people to 6 locations and 15 people. Stage 1
starts after Milestone 5 closes and issue #2200 lands the regression fixture, the
Phase 6 gate and the proposal template.

Stage 1 grows households that come from the tiny world. The tiny world stays as
the separate regression fixture (#2200), so the production world may grow them
without affecting the fixture's tests.

Later stages grow toward the whole village, then the parish, sized from the
historical population (§1.1). Each gets its own proposal and issue when its time
comes.

Every stage issue states the player need, the data review of anything reused,
acceptance criteria (including the world sheet and its oracle agreeing), the
test plan, and a paced simulator walkthrough as evidence. It passes the Phase 6
gate (`just mobile-verify --phase 6`).

## Sources

- Samuel Lewis, [_A Topographical Dictionary of Ireland_, "Kilteevan"](https://libraryireland.com/topog/K/Kilteevan-Ballintobber-Roscommon.php) (1837).
- Virtual Record Treasury of Ireland, [1821 Census: Kilteevan civil parish](https://virtualtreasury.ie/item/VRTI-CEN-1821-4-43-2-10): surviving fragments for three townlands.
- [Townlands in Kilteevan](https://www.townlands.ie/roscommon/kilteevan1/): the parish's 23 townlands.
