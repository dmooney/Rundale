# NPC dialogue prompt lab

The prompt lab of the [NPC dialogue prompt requirements](../../docs/design/npc-dialogue-prompt.md#6-prompt-lab)
(§6, issue 2294). It writes NPC dialogue prompts by hand for the canonical
tiny world, runs them against the production model, and improves them with an
automated research loop until the scores level off. Then the owner reads the
held-out replies beside the baseline's and freezes the prompt.

It calls models directly and is not the game. It is separate from the
rundale-bench leaderboard in the parent directory.

## What is here

| Path                            | What it holds                                                                                                                               |
| ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| `content/common-knowledge.json` | §4.1 common knowledge as fact-checked, plus each character's shaded note (§8 question 2)                                                    |
| `content/characters.json`       | §4.2 character content from `mods/rundale/npcs.json` and three to five hand-written backstory lines each                                    |
| `content/village.json`          | §4.3 places, looks, stories, ways, people and the feast calendar                                                                            |
| `cases/cases.json`              | 20 training and 8 held-out moments, each tagged with the requirements it tests and an expectation only the judges see                       |
| `prompts/brief/`                | the starting brief (§6.1.2): actor-style instructions, JSON layout, shared common knowledge                                                 |
| `prompts/brief-noflag/`         | the brief without the anachronism flag (§6.1.3), for the B7 cases                                                                           |
| `promptlab/render.py`           | renders the v1 baseline (the Endpoint's v1 instructions with the v1 input the engine builds) and the brief, in the production request shape |
| `promptlab/research.py`         | the loop, the held-out review and the write-up                                                                                              |
| `config.json`                   | models, prices, judges, stop rules and the spend cap                                                                                        |
| `promptfooconfig.yaml`          | the same suite as a promptfoo eval, for the viewer and one-off comparisons                                                                  |

Every request has the production shape (§5): the instructions as the system
instruction, one user part `Endpoint input values:\n<compact JSON>`, and the
v1 output schema as the response schema, as
`endpoints/packages/providers/src/google/adapter.ts` sends them. The JSON keys
are sorted at every level, because the phone re-encodes each invocation with
sorted keys (`mobile/RundaleBridge/Sources/RundaleBridge/LimerickRuntime.swift`,
`.sortedKeys`) and the Endpoint passes the values through unchanged. So a
brief's key names decide the order the model reads its sections: they must sort
the static sections first, then the moment, then the player's line, so a
provider's prompt cache can reuse the static prefix. If #2299 wants an order
that alphabetical keys cannot give, the bridge has to stop sorting first.

The brief's values carry only §4 content. A production invocation also carries
the v1 envelope fields (`contractVersion`, `sessionID`, `attemptID` and so on),
which the baseline reproduces and the brief leaves to #2299.

## The loop

```sh
just -f promptfoo/prompt-lab/justfile estimate        # rough cost of a round
just -f promptfoo/prompt-lab/justfile loop            # run or resume
just -f promptfoo/prompt-lab/justfile heldout <run>   # owner review
just -f promptfoo/prompt-lab/justfile report <run> docs/test-plans/results/<date>-prompt-lab.md
```

1. Round 0 scores the v1 baseline once, for reference, then the starting brief.
2. Each round, every training case runs on both Gemini models
   (`gemini-3.5-flash-lite` and `gemini-3.5-flash`), `samples_per_case` times.
3. Two judges from other families score each reply 0 to 100 against the
   requirements its case is tagged with: Claude Sonnet through the `claude`
   CLI and a GPT model through the `codex` CLI. The row score is their mean;
   a split over 20 points is flagged. A row only one judge scored is left out,
   never scored by the other alone, so every round measures the same thing; if
   more than 5% of a round's replies lack a judge (a subscription limit, say),
   the round is discarded and the run pauses, and resuming reruns it. A reply that is not valid output-schema
   JSON scores 0. Judges score against the true world (`judge_reference`), not
   the prompt the model saw, so a prompt that leaves a fact out is marked down.
4. The rewriter (Claude Opus through the `claude` CLI) reads the best prompt so
   far, the scores by requirement, the weakest replies with the judges' notes,
   and every earlier round's change, and proposes a new variant.
5. It stops after three rounds in a row that each gain under 2 points on the
   best score before them, at $25 of model spend less a $2 reserve for the
   held-out run, or at `max_rounds`. The best prompt is kept, not the last.

The training score is the mean judge score over every training reply on both
models. Resuming a run continues from its last completed round.

**What the loop may change.** A variant holds only presentation: the
instructions, `layout` (`json` keeps structured values; `prose` renders each
section as sentences inside the same JSON envelope), `common_knowledge`
(`shared` or `shaded`), the key names (and with them the order of the static
sections), and
the wording of the moment's cues (the stranger, how the character came to
answer, the anachronism notice). Facts come from `content/` through the
renderer, so a rewrite cannot change the world. `promptlab/variant.py` also
refuses any rewrite whose editable text adds a proper noun from the content or
a number, and keeps the two experiment switches, `backstory` and
`anachronism_flag`, as the starting variant set them. The prose layout's
sentence templates are code, not variant text, so the loop cannot reword them.

**Held-out.** The loop loads training cases only. `heldout` runs the best
prompt and the baseline on the held-out cases and writes `side-by-side.md`
for the owner.

**Spend.** Gemini spend is metered from each response's usage at the prices in
`config.json`; a model without a price is refused. The CLI judges and the
rewriter run on the owner's subscriptions and are not metered. Without the
`codex` CLI the GPT judge falls back to the OpenAI API (`gpt-5-mini`), which is
billed and counted against the cap. Spend is saved after every round and on any
interrupt. One-off runs (`lab.py round`, `just noflag`, `just eval`) are
outside a run's ledger: each prints or records its own spend, and the write-up
reports them separately from the loop's total.

## The §8 questions

- **JSON or prose:** score `brief` and a copy with `"layout": "prose"` (or let
  the loop switch it, and read which layout the best round uses).
- **Shaded or shared common knowledge:** the same, with `common_knowledge`.
- **Does backstory earn a place:** score the best prompt with
  `"backstory": false` as a new directory under `prompts/` and compare.
- **Flag or standing guidance for B7:** `just -f promptfoo/prompt-lab/justfile noflag`.

Use `lab.py round <variant> [--tags B7]` for these one-off comparisons. Even
with `samples_per_case` at 2, a difference of a few points on a handful of
cases is within noise; raise it before deciding a question.

## Requirements to run

- `GEMINI_API_KEY` (or `GOOGLE_API_KEY`) for a Gemini API key.
- The `claude` CLI, signed in, for the Claude judge and the rewriter.
- The `codex` CLI, signed in, for the GPT judge (otherwise `OPENAI_API_KEY`).

`--stand-in` (and `just smoke`) swaps Gemini for an OpenAI model to prove the
plumbing where no Gemini key is available. Stand-in runs are marked
`plumbing_only` and their scores are not research results.

Raw output goes to `output/`, which git ignores. The write-up goes in
`docs/test-plans/results/`.
