# Interior Concept Candidates

Generated 2026-07-03 as a first pass for interiors that continue the
illustrated parish notebook exterior style.

## Goal

Test a default interior grammar for Rundale:

- full-screen 16:9 background plate,
- front wall removed like a notebook/dollhouse cutaway,
- same low 3/4 oblique camera and sprite/door scale as the exterior concept,
- no switch to side-scroller or top-down tile map,
- readable walkable floor and clear interaction hotspots.

## Historical Reference Anchors

- National Museum of Ireland, **Irish Country Furniture**: country-kitchen
  reconstruction with chairs, settlebed, meal bin, food cupboard, dresser,
  kitchen table, hearth furniture, cooking and eating utensils.
  <https://www.museum.ie/en-IE/museums/decorative-arts-history/exhibitions/irish-country-furniture>
- National Museum of Ireland, **Irish Country Furniture FAQs**: typical rural
  home furniture includes chairs, dresser, settlebed, mealbin/food press in the
  kitchen or hearth area.
  <https://www.museum.ie/en-ie/collections-research/art-and-industry-collections/art-industry-collections-list/furniture/irish-county-furniture-faqs>
- Barry O'Reilly, **Hearth and home: the vernacular house in Ireland from
  c. 1800**: direct-entry/lobby-entry house plans and hearth-centered domestic
  layouts.
  <https://www.jstor.org/stable/pdf/41472820.pdf>
- Buildings of Ireland, **A Living Tradition**: vernacular interiors, kitchen as
  central space, hearth-lobby and direct-entry plans.
  <https://www.buildingsofireland.ie/app/uploads/2021/12/A-Living-Tradition.pdf>
- Muckross Traditional Farms, **Traditional Furniture**: later rural reference
  for hearth, dresser, settlebed, and related furnishings. Useful as visual
  continuity, but it represents 1930s-40s farms rather than 1820.
  <https://muckross-house.ie/muckross-traditional-farms/traditional-furniture/>
- Ulster American Folk Park, **J Reilly Publican/Grocer** and **Rural Ulster**:
  useful shop/grocery analogues; Reilly's building is tied to 1820, while much
  of the documented business detail is later 19th/early 20th century.
  <https://www.ulsteramericanfolkpark.org/stories/j-reilly-publicangrocer>
  <https://www.ulsteramericanfolkpark.org/whats-on/rural-ulster>

## Candidates

| File                               | Location        | Read                                                                                                                                                |
| ---------------------------------- | --------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| `murphy-farm-hearth-cutaway-a.png` | Murphy's Farm   | Strongly readable cutaway and furniture layout; a little too polished and stone-floored.                                                            |
| `murphy-farm-hearth-cutaway-b.png` | Murphy's Farm   | Best default grammar: closer, rougher, playable, with hearth/dresser/settlebed/door scale working well.                                             |
| `connolly-shop-cutaway-a.png`      | Connolly's Shop | Clearest shop function and counter/shelf affordances; somewhat too formal and stocked for an 1820 rural cottage shop.                               |
| `connolly-shop-cutaway-b.png`      | Connolly's Shop | Best style/scale match for Connolly's: more modest domestic-shop hybrid, though the perspective is slightly closer to side elevation than Murphy B. |
| `murphy-farm-hearth-cutaway-c.png` | Murphy's Farm   | Gameplay-fit revision: wider scale, second sleeping cue, work/eating/sleeping capacity for Siobhan, Liam, and farm labour/visitors.                 |
| `connolly-shop-cutaway-c.png`      | Connolly's Shop | Gameplay-fit revision: wider scale, clear stairs to rooms above shop, shop floor/customer capacity, behind-counter work affordances.                |

Contact sheet:

- `interior-concepts-contact-sheet.png`
- `interior-concepts-c-contact-sheet.png`

Gameplay fit audit:

- `gameplay-fit-audit.md`
- `interior-gameplay-fit-scale-gauge.png`
- `interior-gameplay-fit-scale-gauge-c.png`

## Prompt Pattern

All four prompts used the same core constraints:

- Use the visible `illustrated-parish-notebook.png` and
  `illustrated-parish-scene-no-ui.png` as style, scale, linework, watercolor,
  and low-oblique camera references.
- Generate a historically grounded 1820 rural Irish interior.
- Render it as a front-wall-removed cutaway, not side-scroller, not top-down.
- Keep door height, furniture scale, and playable floor area consistent with the
  exterior concept.
- Use no UI, no text labels, no modern objects, no electric light, no brand
  packaging, no people, and no animals.
- Require every visible doorway or dark opening to contain a fitted wooden plank
  door, not a black void.

The Murphy prompts emphasized open turf hearth, crane and black pot, dresser,
settlebed, meal chest/bin, food cupboard, rough table, benches, stools, thatch
underside, tiny window, limewash, soot, and packed-earth/broken-flag floor.

The Connolly prompts emphasized a modest rural shop-front-room hybrid: rough
counter, sparse handmade shelves, scales, ledger, paper parcels, sacks of
meal/flour/oats, eggs, butter, candles, twine, coarse cloth, baskets, barrels,
and a domestic hearth corner.

## Recommendation

Use `murphy-farm-hearth-cutaway-c.png` and `connolly-shop-cutaway-c.png` as the
current gameplay-fit concept targets. They are still concept art rather than
runtime-ready plates, but the major NPC-capacity and same-sprite-scale concerns
from the A/B batch are addressed.
