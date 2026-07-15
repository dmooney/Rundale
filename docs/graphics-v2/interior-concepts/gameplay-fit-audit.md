# Interior Gameplay Fit Audit

Generated 2026-07-03 after the first interior concept batch.

## Data Checked

- `mods/rundale/world.json`
  - Location 9: Murphy's Farm
  - Location 13: Connolly's Shop
- `mods/rundale/npcs.json`
  - NPC 2: Siobhan Murphy
  - NPC 4: Roisin Connolly
  - NPC 18: Liam Murphy
  - recurring scheduled visitors/workers at locations 9 and 13
- Exterior scale references:
  - `pipeline-experiments/idea-bx-e2-murphy-farm-bounded-roof-boundary-fix.png`
  - `pipeline-experiments/idea-bv-e2-grove-bv-e1-bu-style-tighten.png`
  - Cycle BS door-height calibration notes

## Scale Finding

The initial A/B interiors are good visual concepts, but they are probably too
close for direct gameplay use with the same NPC sprite scale as the exterior
plates. The interior doors, counters, beds, and tables read larger than the
accepted exterior door-height target.

Scale audit sheet:

- `interior-gameplay-fit-scale-gauge.png`
- `interior-gameplay-fit-scale-gauge-c.png`

The C pass was generated with an explicit constant sprite/door gauge standard:

- use the same plate size as exterior plates, `1672x941`,
- use one fixed NPC sprite height for exterior and interior tests,
- door height should read close to the established exterior/notebook door
  standard, not as a close-up facade-study door,
- counters should sit around waist/chest height on the same sprite,
- beds should fit a human without implying a different sprite scale,
- no furniture or room element should require per-location sprite scaling.

## Murphy's Farm Requirements

Core residents/workers:

- Siobhan Murphy, 45, farmer, home/workplace 9.
- Liam Murphy, 16, farm boy, home/workplace 9.

Biographic/schedule implications:

- Siobhan runs the farm after her husband's death; she is practical,
  no-nonsense, and does heavy farm management.
- Liam has taken on much of the heavy farm work and is good with cattle and
  horses.
- Both sleep at the farmhouse.
- Both eat breakfast/dinner/supper there.
- Winter schedules explicitly put Siobhan indoors for spinning wool, mending
  clothes, and repairing tools.
- Siobhan churns butter and prepares butter/eggs for market.
- Sean Ruadh Kelly regularly labours at Murphy's Farm and is paid partly in
  dinner, so the eating area must support at least three people.
- Colm Gallagher visits Liam at the farm.
- Brigid Ni Fhatharta and Martin Concannon visit as mobile professional/social
  traffic.

Minimum art requirements:

- Two credible sleeping provisions, or one visible bed plus clear offscreen
  sleeping access. The initial A/B candidates showed one obvious bed/settlebed
  only.
- Hearth cooking zone with pot/crane/kettle and fire-banking affordance.
- Table/bench seating for at least three, ideally four.
- Butter/egg/churn/market-prep storage: churn, crocks, baskets, egg/butter
  containers, shelf or chest.
- Indoor winter work zone: stool/bench, mending basket, wool/spinning or tool
  repair hints.
- Farm threshold/tools: boots, bucket, broom, rope, turf basket, tool chest, but
  not so much clutter that sprites cannot walk.
- Walkable floor lane from threshold to hearth, table, bed, and side/back door.

Candidate notes:

- `murphy-farm-hearth-cutaway-a.png`: strong cutaway readability and table
  layout; too polished and stone-floored; one visible bed; likely too close.
- `murphy-farm-hearth-cutaway-b.png`: best current interior grammar; rougher,
  clearer, better hearth/furniture balance; still needs a second sleeping
  provision or offscreen sleeping cue, and likely needs a 20-30% wider/less
  close scale pass.
- `murphy-farm-hearth-cutaway-c.png`: resolves the main gameplay concerns. It
  adds a second sleeping provision/loft-room cue with ladder and curtain, keeps
  a visible settlebed, seats at least three/four around the table, includes
  hearth, dresser, churn/market-prep storage, egg/butter baskets, tool/mending
  work surfaces, and has clear walkable lanes. The same 85px NPC gauge reads
  plausibly at the threshold, table, sleeping area, and side door.

## Connolly's Shop Requirements

Core resident/worker:

- Roisin Connolly, 38, shopkeeper, home/workplace 13.

Biographic/schedule implications:

- Roisin runs Connolly's Shop with her mother. The mother is not currently a
  full NPC record, but the biography and Sunday schedule require space for her.
- Roisin sleeps above the shop.
- Sunday dinner is with her mother above the shop.
- Roisin eats quick dinners behind the counter, minds the shop, weighs flour,
  measures cloth, tallies debts, writes orders, and does accounts by candlelight.
- Market day brings farmers selling butter/eggs and buying supplies.

Recurring shop traffic:

- Siobhan Murphy sells butter/eggs and buys supplies.
- Maire Gallagher shops and exchanges news.
- Peig Hannigan gathers intelligence.
- Niamh Darcy browses and chats.
- Cormac and Nora Duffy discuss prices/supplies.
- Kathleen Walsh shops with Ciaran.
- Brigid Ni Fhatharta trades herbs for supplies.
- Una Malone sells cloth and buys supplies.

Minimum art requirements:

- Public shop floor with enough standing room for Roisin plus two to four
  customers without blocking the door.
- Counter plus clear behind-counter work lane.
- Scales, ledger/account book, measuring cloth area, shelves, sacks, baskets,
  butter/egg receiving space, parcels, and candle/meal/twine/cloth stock.
- A small behind-counter meal spot or stool for quick dinner.
- Clear access to the rooms above the shop: stairs, ladder, hatch, or a visible
  side/back door that reads as living-quarter access.
- Upstairs sleeping/eating does not need to be fully visible on this plate, but
  the plate must imply it.
- Door/opening discipline: no dark voids; all visible doorway openings need
  fitted plank doors or a readable stair/hatch.

Candidate notes:

- `connolly-shop-cutaway-a.png`: best for shop function and customer space;
  too formal/stocked and later-general-store feeling; missing explicit upstairs
  access; likely too close.
- `connolly-shop-cutaway-b.png`: best style match and modest domestic-shop
  feel; still missing explicit above-shop access; counter/workflow is good, but
  it leans toward a flatter side-elevation cutaway and needs more low-oblique
  consistency with the exterior plates.
- `connolly-shop-cutaway-c.png`: resolves the main gameplay concerns. It adds
  clear stairs to the rooms above the shop, keeps a broad public customer floor,
  gives Roisin a behind-counter work lane, includes scales, ledger, shelves,
  sacks, egg/butter receiving baskets, parcels, and goods without becoming a
  grand Victorian shop. The same 85px NPC gauge reads plausibly at the
  threshold/customer area, counter, stairs, and side door.

## Recommendation

Treat the A/B batch as style exploration and the C batch as gameplay-fit
concept targets.

The C candidates are acceptable for concept direction because they address:

- enough sleep/eat/work affordances for the resident NPCs,
- enough standing/walkable space for expected visitors,
- explicit access to offscreen/upper living space where schedules require it,
- same-plate same-sprite scale according to the 85px gauge audit,
- no missing-door/dark-void failures on visible person-sized openings.

If these become production plates, the next step is not another open-ended art
iteration. The next step is to derive masks/sockets:

- walkable floor masks,
- blocked/occlusion masks for walls, counters, beds, stairs, and shelves,
- door/interior-transition sockets,
- bed/table/counter/hearth/workstation interaction zones,
- NPC spawn/standing points for residents and visitors.

Previous recommendation, now satisfied by C:

1. Murphy's Farm C:

   - based on `murphy-farm-hearth-cutaway-b.png`,
   - 20-30% wider scale,
   - two sleeping provisions or clear loft/side-room sleeping cue,
   - table for three/four,
   - churn/butter/egg/mending/tool repair stations,
   - constant NPC sprite/door-height gauge.

2. Connolly's Shop C:
   - merge `connolly-shop-cutaway-b.png` modest style with
     `connolly-shop-cutaway-a.png` counter/shop readability,
   - 20-30% wider scale,
   - clear stairs/ladder/hatch/door to above-shop living quarters,
   - enough shop floor for market-day crowding,
   - behind-counter meal/accounting spot,
   - constant NPC sprite/door-height gauge.

Both should be rejected if they require a different sprite scale than the
exterior plates.
