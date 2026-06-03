# Harness shadow — initial divergence ledger

Differential run of the `GameTestHarness` corpus against the real `parish_core::game_loop` (`PARISH_HARNESS_SHADOW=1`). Each record is an input where the legacy router's player-visible (`text-log`) output and the real loop's output differ after normalization. This is a **measurement** (non-gating): the go/no-go signal for draining the legacy path (#1159).

**Total divergences: 1223**

## By case

| Case | Divergences |
| --- | --- |
| `engine-integration` | 1184 |
| `engine-unit` | 39 |

## Most frequently diverging inputs

| Input | Count |
| --- | --- |
| `go to crossroads` | 309 |
| `look` | 209 |
| `go to pub` | 191 |
| `go to kilteevan` | 98 |
| `/debug tiers` | 34 |
| `go to church` | 31 |
| `go to murphy's farm` | 30 |
| `look around` | 18 |
| `go to fairy fort` | 17 |
| `l` | 16 |
| `/debug here` | 16 |
| `go to hodson bay` | 15 |
| `go to lough ree` | 14 |
| `go to letter office` | 14 |
| `go to connolly's shop` | 13 |

## Examples (legacy `old` vs real-loop `new`)

### `go to pub`  _(case: engine-unit)_

- **old:** `[["text-log", "\"The name's Padraig. I'm the Publican,\" they say, extending a hand."], ["text-log", "\"You must be new to the parish. I'm …`
- **new:** `[["text-log", "You walk along a short lane past a row of cottages. (1 minute on foot)"], ["text-log", "The warm interior of Darcy's Pub. Tu…`

### `go to crossroads`  _(case: engine-unit)_

- **old:** `[["text-log", "You walk along the road north past low fields to the crossroads. (13 minutes on foot)"], ["text-log", "A quiet crossroads wh…`
- **new:** `[["text-log", "You walk along the road north past low fields to the crossroads. (13 minutes on foot)"], ["text-log", "You spot someone on t…`

### `I saw Padraig and Niamh by the road.`  _(case: engine-unit)_

- **old:** `[["text-log", "Nothing happens."]]`
- **new:** `[["text-log", "Only the sound of a distant crow."]]`

### `hello`  _(case: engine-unit)_

- **old:** `[["text-log", "Padraig Darcy: Only one response"], ["text-log", "Niamh Darcy 😊"], ["text-log", "Padraig Darcy 😊"]]`
- **new:** `[["text-log", "There's someone here, but the LLM is not configured — set a provider with /provider."]]`

### `Good morning, Padraig and good day, Niamh.`  _(case: engine-unit)_

- **old:** `[["text-log", "Padraig Darcy: A fair morning to ye from Padraig."], ["text-log", "Niamh Darcy: And a good day back to ye from Niamh."], ["t…`
- **new:** `[["text-log", "There's someone here, but the LLM is not configured — set a provider with /provider."]]`

### `hello again`  _(case: engine-unit)_

- **old:** `[["text-log", "Niamh Darcy 😊"]]`
- **new:** `[["text-log", "There's someone here, but the LLM is not configured — set a provider with /provider."]]`

### `hello there`  _(case: engine-unit)_

- **old:** `[["text-log", "Padraig Darcy: Ah, good morning to ye!"], ["text-log", "Padraig Darcy 😊"]]`
- **new:** `[["text-log", "There's someone here, but the LLM is not configured — set a provider with /provider."]]`

### `how are you`  _(case: engine-unit)_

- **old:** `[["text-log", "Padraig Darcy: Second response"]]`
- **new:** `[["text-log", "There's someone here, but the LLM is not configured — set a provider with /provider."]]`

### `Good morning`  _(case: engine-unit)_

- **old:** `[["text-log", "Sean Ruadh Kelly: Good morning to ye!"]]`
- **new:** `[["text-log", "There's someone here, but the LLM is not configured — set a provider with /provider."]]`

### `Hello there`  _(case: engine-unit)_

- **old:** `[["text-log", "Sean Ruadh Kelly: Dia dhuit, a chara!"], ["text-log", "Sean Ruadh Kelly 😊"], ["text-log", "Mick Flanagan 😊"], ["text-log", "…`
- **new:** `[["text-log", "There's someone here, but the LLM is not configured — set a provider with /provider."]]`

### `The landlord is after us all for rent.`  _(case: engine-unit)_

- **old:** `[["text-log", "Brigid Ni Fhatharta 😠"], ["text-log", "Mick Flanagan 😠"], ["text-log", "Aoife Brennan 😠"], ["text-log", "Peig Hannigan 😠"]]`
- **new:** `[["text-log", "There's someone here, but the LLM is not configured — set a provider with /provider."]]`

### `look`  _(case: engine-unit)_

- **old:** `[["text-log", "The small village of Kilteevan — a handful of whitewashed cottages clustered around a well and an old stone bridge over a sh…`
- **new:** `[["text-log", "The small village of Kilteevan — a handful of whitewashed cottages clustered around a well and an old stone bridge over a sh…`

### `look around`  _(case: engine-unit)_

- **old:** `[["text-log", "The small village of Kilteevan — a handful of whitewashed cottages clustered around a well and an old stone bridge over a sh…`
- **new:** `[["text-log", "The small village of Kilteevan — a handful of whitewashed cottages clustered around a well and an old stone bridge over a sh…`

### `go to kilteevan`  _(case: engine-unit)_

- **old:** `[["text-log", "a small, sharp-eyed old woman wrapped in a shawl glances over, then goes back to what they were doing."], ["text-log", "You …`
- **new:** `[["text-log", "You walk along the Kilteevan road heading south past low fields. (13 minutes on foot)"], ["text-log", "The small village of …`

### `walk to crossroads`  _(case: engine-unit)_

- **old:** `[["text-log", "You walk along the road north past low fields to the crossroads. (13 minutes on foot)"], ["text-log", "A quiet crossroads wh…`
- **new:** `[["text-log", "You walk along the road north past low fields to the crossroads. (13 minutes on foot)"], ["text-log", "A quiet crossroads wh…`

### `stroll to kilteevan`  _(case: engine-unit)_

- **old:** `[["text-log", "\"Peig,\" they say simply, with a nod."], ["text-log", "You walk along the Kilteevan road heading south past low fields. (13…`
- **new:** `[["text-log", "You walk along the Kilteevan road heading south past low fields. (13 minutes on foot)"], ["text-log", "A farmer nods to you …`

### `head to crossroads`  _(case: engine-unit)_

- **old:** `[["text-log", "You walk along the road north past low fields to the crossroads. (13 minutes on foot)"], ["text-log", "A quiet crossroads wh…`
- **new:** `[["text-log", "You walk along the road north past low fields to the crossroads. (13 minutes on foot)"], ["text-log", "A farmer nods to you …`

### `/save`  _(case: engine-unit)_

- **old:** `[["text-log", "Game saved."]]`
- **new:** `[["text-log", "Persistence not available."]]`

### `/branches`  _(case: engine-unit)_

- **old:** `[["text-log", "Save branches:"], ["text-log", "main * (created 3 Jun 1:37 AM)"]]`
- **new:** `[["text-log", "Persistence not available."]]`

### `The landlord's agent is demanding the rent this week.`  _(case: engine-unit)_

- **old:** `[["text-log", "Peig Hannigan 😠"], ["text-log", "Mick Flanagan 😠"], ["text-log", "Brigid Ni Fhatharta 😠"], ["text-log", "Sean Ruadh Kelly 😠"…`
- **new:** `[["text-log", "There's someone here, but the LLM is not configured — set a provider with /provider."]]`

