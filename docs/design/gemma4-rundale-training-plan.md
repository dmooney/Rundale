# Post-Train Gemma 4 9B for Rundale Hiberno-English Dialogue

> Parent: [Docs Index](../index.md) | Related: [Inference Pipeline](inference-pipeline.md), [Irish English Resources](../research/Irish-English-1820s-resources.md), [ADR-005](../adr/005-ollama-local-inference.md)

## Context

Parish's inference pipeline (refreshed April 2026 in [`docs/design/inference-pipeline.md`](inference-pipeline.md)) names **Gemma 4 9B** as the recommended local Tier 1 Dialogue model on the RX 9070 16 GB baseline. But the refresh flags explicitly: *"Benchmarks don't measure 1820 Irish peasant dialogue. Build a small fixture and use the `/prove` harness before committing any model to production."*

The research doc [`docs/research/Irish-English-1820s-resources.md`](../research/Irish-English-1820s-resources.md) lays out the gap: no existing LLM — gaBERT, UCCIX, Caernarfon 3B, or any cloud model — has been trained on historical Hiberno-English. For 1820s Roscommon NPC speech (Irish-substrate grammar + English orthography + code-switching), a QLoRA on public-domain primary sources is the only practical path.

This plan QLoRA-fine-tunes `google/gemma-4-9b-it` on Joyce, Griffin, Carleton, Croker, Kickham dialogue plus reference-work-mined instruction pairs (etiquette manuals, letter-writing manuals, almanacs, period dictionaries — Talkie-1930-13B's reference-work-mining methodology). It then refines the SFT model with **iterated DPO** scored by a four-axis combined judge stack (deterministic anachronism wordlist, Talkie-1930-13B-IT loglik, a tiny ~250M-param dialect-oracle LM, and DeepSeek V4-pro coherence rubric), packages the result as `gemma4-rundale:9b` for Ollama, and wires it into the Dialogue provider category. The intended outcome: a model that meaningfully outperforms stock Gemma 4 9B on a Hiberno-English rubric and passes a new `/prove rundale-dialect` harness, with two distinct feature flags — `rundale-dialect-model` (the model + its system prompt) and `inference-rejection-sampler` (serve-time best-of-K wrapper).

## Decisions (user-confirmed)

| Decision | Choice | Rationale |
|---|---|---|
| Training host | RunPod A100-80GB primary; local ROCm RX 9070 demoted to "alternative for AMD-equipped contributors"; no MLX/Apple-Silicon retarget | Cloud + axolotl already supported; user chose RunPod over local M5 |
| Base model | `google/gemma-4-9b-it` (unchanged) | Best general instruction-following; period prior added via SFT/DPO |
| Hand-written anchor | **Removed** | Author cannot author authentic 1820s Hiberno-English |
| Data mix | **48 % literary core (13 authors) / 16 % trial/commission testimony / 12 % Joyce dialect↔standard pairs (extended with formal-contrast set) / 6 % first-person Irish memoir / 5 % travel-observer reported / 4 % folklore-oral / 3 % religious/clerical / 4 % reference-work pairs / 2 % periodicals** | Literary core dominance reduced to 48 % to make room for the highest-authenticity slice (testimony 16 %); first-person Irish, religious/clerical, periodicals add register breadth; sums to 100 %. Stage-Irish caricature is **0 %** of training mix — it's a DPO `rejected` class (see "Stage Irish" row) |
| Reference-work mining | **Added**: period etiquette manuals, letter-writing manuals, almanacs, period dictionaries (all Internet Archive). Concrete extraction recipe in §Data curation | Talkie-pattern programmatic supervision; primarily gentry/middling-farmer register |
| Literary corpus expansion | **13-author core** = original 5 (Joyce 1910, Griffin, Carleton, Croker, Kickham) + Lover, Maxwell, Lever + Maria Edgeworth (*Castle Rackrent* 1800), John & Michael Banim (*Tales by the O'Hara Family* 1st series 1825 + 2nd 1826), Anna Maria Hall (*Sketches of Irish Character* 1829, *Lights and Shadows of Irish Life* 1838) | 8-author corpus too narrow; 13 authors all rated HIGH cottier-dialogue density supports a 6/6 fully disjoint dialect-oracle / SFT split with 1 author reserved eval-only |
| Travel-observer subcorpus (NEW slice) | Arthur Young (1780), John Carr (1806), Henry David Inglis (1834), Johann Georg Kohl (1843), Asenath Nicholson (1847), Mr & Mrs S.C. Hall (1841–43) — gentry-narrator-mediated peasant speech | Provides code-switching examples (gentry observer + reported peasant dialogue) and contrast register; tagged `register: observer-reported` so it doesn't pollute the cottier mix |
| Folklore / oral-register subcorpus | Lady Wilde (*Ancient Legends* 1888), William Wilde (*Irish Popular Superstitions* 1852), Jeremiah Curtin (*Myths and Folk-lore of Ireland* 1890), Douglas Hyde (*Beside the Fire* 1890), P.W. Joyce (*Old Celtic Romances* 1879), James Hardiman (*Irish Minstrelsy* 1831) | Substrate grammar + oral idiom + supernatural register; tagged `register: oral-tale`; supplies metaphor systems and ballad meter the literary core under-represents |
| **Trial / commission / verbatim-testimony subcorpus** (HIGHEST-density addition) | Devon Commission *Digest of Evidence* 1843–45 (1125+ peasant witnesses, ~4500 pp), Whately/Poor Inquiry 1833–36 (793 pp verbatim oral testimony), Cobbett *State Trials* vols 27–33 (1798–1820 trials, esp. Captain Rock prosecutions), Society of Friends *Famine Transactions* 1852, Leadbeater *Cottage Dialogues among the Irish Peasantry* 1811, Boston Pilot "Information Wanted" emigrant ads 1831–1921 (41 185 records via Harvard Dataverse), Whyte *Famine Ship Diary* 1847 + *Ocean Plague* 1848, Bennett *Narrative of a Recent Journey* 1847, Tuke *Visit to Connaught* 1848, Nicholson *Annals of the Famine* 1851 (distinct from her 1847 travelogue already in the travel-observer slice) | Court records + parliamentary inquiries are the **single highest-authenticity** peasant-voice source — court reporters transcribed peasants in real time, not novelistic stylization. Tuke's Connaught journey brackets Roscommon directly. Tagged `register: testimony` |
| **First-person Irish-voice subcorpus** | Carleton *Autobiography* 1896 (peasant-raised novelist's own voice — distinct from his *Traits and Stories* novelistic dialect already in literary core), Joseph Holt *Memoirs* 1838 (rebel/convict autobiography), O'Connell *Correspondence* (Fitzpatrick 1888), Wolfe Tone *Memoirs* 1826, Miles Byrne *Memoirs* 1863, Mitchel *Jail Journal* 1854 + *Last Conquest of Ireland (Perhaps)* 1861 | First-person Irish writers (vs gentry-narrator novelists). Tagged `register: first-person-irish`; balances literary-core's third-person framing |
| **Religious / clerical / temperance subcorpus** | Bishop Doyle (J.K.L.) *Life & Correspondence* (1829–34 era, MacDonagh 1905 biography is the public-domain compilation), Cobbett *History of the Protestant Reformation* 1824–27 (peasants' polemical reading), 1859 Ulster Revival eyewitness accounts (Weir, witness correspondence — high direct-testimony density, Presbyterian/Ulster), Butler's Catechism (1775+, Irish editions), *Garden of the Soul* (1740 original, Dublin 1872 reprint) | Clerical pastoral register + revival witness testimony. Tagged `register: clerical` |
| **Stage Irish — REJECTED CLASS for DPO** (NOT training data) | Boucicault (*Colleen Bawn* 1860, *Arrah-na-Pogue* 1864, *Shaughraun* 1874), O'Keeffe (*The Poor Soldier* 1783, *The Wicklow Mountains* 1796), Sheridan (*St Patrick's Day* 1775, *The Rivals* — Sir Lucius O'Trigger 1775), Macklin (*The True-Born Irishman* 1762, *Love à la Mode* 1759), Tyrone Power (*Born to Good Luck* 1832), Bernard (*His Last Legs* 1839, *The Irish Attorney* 1839), Colman (*John Bull* 1803), Farquhar (*The Recruiting Officer* 1706, *Love and a Bottle* 1698) | Stage-Irish caricature ("begorrah"-flavored) is what we want the model to **avoid**. Tagged `class: stage_irish_caricature`, **excluded from SFT**. Synthesised stage-Irish responses are inserted as `rejected` examples in the DPO pair pool; the policy learns to push *away* from caricature and toward authentic substrate |
| **Formal-register contrast set** (paired contrast, not standalone training) | Lindley Murray *English Grammar* 1795 (ubiquitous prescriptive), Cobbett *Grammar of the English Language* 1818 (working-class targeted), Walker *Critical Pronouncing Dictionary* 1791 (explicit "avoid Irish peculiarities" rules), Neilson *Introduction to the Irish Language* 1808 (English↔Irish bilingual primer), Dilworth *Spelling-Book*, NE Commissioners *Books of Lessons* 1831+ | Formal English peasants were *taught-against*. Used by `build/joyce_pairs.py` to **extend** Joyce's dialect↔standard paraphrase set with prescriptive-grammar paired examples — provides a bidirectional code-switching anchor without pulling cottier output toward Standard English |
| **Periodicals (single-volume sample)** | *The Irish Penny Journal* Vol 1 (Gutenberg #55518, 1840–41) | Mixed-register weekly; period idiom; bulk-extractable in a single Gutenberg file (sidesteps the multi-issue OCR cost that deferred the *Dublin Penny Journal* / *Dublin University Magazine* / *Nation* expansion) |
| Anachronism wordlist source | **Three-source union** as the positive-attestation list — every token must appear in (a) Webster 1828, (b) Joyce 1910's vocabulary, OR (c) **Wright's English Dialect Dictionary 1898–1905** (public domain, comprehensive on regional/Hiberno-English usage); failures hit the wordlist. Plus a hand-curated blocklist for 20th–21st-century anachronisms (telephone, computer, okay, awesome, …) | Webster 1828 alone false-anachronisms British/Irish vocabulary; Joyce + Wright together patch the American-English gap; OED is paywalled and ruled out |
| `feature_tagger.py` role | Promoted from labeller to **mandatory floor gate** for cottier class (≥N substrate features per 100 tokens, drop on miss) | Transparent, fast, ungameable by general period prose |
| `class_assigner.py` role | **Evidence-based**: verb-of-saying speaker → known-class lookup OR substrate-density threshold | Curation pipeline now load-bearing without hand anchor |
| Period-axis judge | **Three components** combined: (a) deterministic anachronism wordlist (Webster 1828 ∪ Joyce 1910 ∪ Wright EDD 1898–1905 attestation + curated blocklist — see "Anachronism wordlist source" row), (b) Talkie-1930-13B-IT loglik under fixed Roscommon-1820s system prompt, (c) tiny ~250M dialect-oracle LM trained on the literary corpus (8 authors, see Stage 0 for the disjoint holdout split) | Talkie alone biases Victorian/Edwardian; tiny oracle encodes actual Roscommon-cottier prior |
| Substrate-density judge | `feature_tagger.py` deterministic floor | Already specified; promoted to gate |
| Coherence judge | **DeepSeek V4-pro via API** ($0.435 / $0.87 per MTok in/out, cache-hit input $0.003625 / MTok; 75 % discount until **2026-05-31 15:59 UTC**, then $1.74 / $3.48). Sonnet 4.6 batch + cache documented as judge-side fallback if DeepSeek calibration fails. **Note: Sonnet is also the calibration-distractor generator** (see "Calibration set" row), so the fallback path additionally requires either programmatic distractors or a third model (e.g. Gemini 2.5 Flash) to keep generation and judging models distinct | ~95 % of Sonnet 4.6 quality at ~30 % cost |
| Discount deadline | **2026-05-31**; post-deadline V4-pro cost rises ~4× (still well below Sonnet) | DPO data generation should complete before then |
| Score combination | **Borda rank-aggregate** across all four axes | Robust to scale differences between scorers |
| Candidates per scenario (DPO) | **N=8** | More preference signal at marginal cost |
| DPO iterations | **2–3 rounds**, regenerating from each round's policy | Iterated DPO outperforms single-pass |
| Serve-time rejection sampler | Local-judge subset (3 of the 4 axes — see "Serve-time judge stack" row) at K=4; gated by `config.flags.is_enabled("inference-rejection-sampler")` per CLAUDE.md rule 6, **default-on** when the wrapper ships | Catches per-turn slop the policy didn't internalize |
| Feature-flag defaults | Both `rundale-dialect-model` and `inference-rejection-sampler` ship **default-on**, in the same PR as the artifact they gate (per CLAUDE.md non-negotiable rule 6 — "Gate with `config.flags.is_enabled`, default-on, and document in PR"). The flag exists to allow disabling, not to stage rollout | Prior plan's "default-off at merge, flip later" pattern violates rule 6 |
| Calibration set | **200 synthetically generated pairs per axis**, **asymmetric**: period axis modernized by **Sonnet 4.6**, coherence axis corrupted by **Sonnet 4.6** — DeepSeek V4-pro only judges, never generates its own calibration distractors | Same-model corruption + judging is self-fulfilling; asymmetric construction breaks the circle |
| Serve-time judge stack | **Three local judges only** at K=4: anachronism wordlist + Talkie-1930-13B-IT (q4) + 250M dialect-oracle. **DeepSeek dropped at serve time** — kept for offline DPO scoring + nightly regression sweeps only | API roundtrip × K candidates kills interactive latency; local-only stack fits the budget |
| Serve-time wrapper architecture | **Background-lane critic** (`03-dialogue-quality-loops.md` §7), not inline best-of-K. Ollama serialises per instance, so K=4 inline = ~1.6 s of generation alone — unacceptable for the player-visible turn. Instead: ship the Tier 1 draft immediately (≤ 600 ms unchanged), dispatch K-1 alternates + scoring on the Background lane, silently replace the bubble if the draft loses Borda. Critic wall-clock cap **1500 ms**; abandon past that and keep the draft as-shipped | Honest accounting for Ollama's serial generation; preserves ADR-005's single-runtime stance; player never blocks on the critic |
| Pipeline automation | Single-command end-to-end orchestrator (`just train-rundale-dialect`) — provisions RunPod pod, runs all stages, calibrates judges, packages artifact, runs `/prove`, tears down pod, reports cost | User requirement: full automation; no babysitting |
| Safety rails | Hard cost cap (default $100), per-stage timeouts (12 h SFT / 12 h DPO-per-round / 8 h dialect oracle), total wall-clock cap (default **48 h**), pause-pod-on-failure with 24 h auto-destroy | Bound runaway cost; preserve failure-mode inspection without indefinite billing |
| Serving | Ollama via GGUF q4_K_M; feature-flag-gated drop-in for the Dialogue category | Unchanged from prior plan |

## Repo layout — new `training/` subproject

Kept outside the cargo workspace so it doesn't pollute `just build` / `just check`. Uses `uv` + Python 3.11.

```
training/
  pyproject.toml                    # uv project, Python 3.11
  uv.lock
  README.md                         # run instructions (mirrors §Verification below)
  .gitignore                        # data/raw/**, data/interim/**, data/processed/**, models/**, vendor/**, *.gguf
  .env.example                      # RUNPOD_API_KEY, DEEPSEEK_API_KEY, HF_TOKEN
  docker/
    Dockerfile.training             # rundale/training:latest — axolotl + bnb + transformers + trl + llama.cpp + ollama + uv
  configs/
    qlora_gemma4_9b.yaml            # axolotl SFT config
    dpo_gemma4_rundale.yaml         # axolotl DPO config (Stage 2)
    dialect_oracle_250m.yaml        # tiny-LM pretrain config (Stage 0)
    rundale_dialect_e2e.yaml        # orchestrator run config: cost caps, timeouts, judge stack, artifact destination
    modelfile.gemma4-rundale        # Ollama Modelfile template
  src/parish_train/
    ingest/                         # Gutenberg + Internet Archive fetchers
      # --- Literary core (13 authors, HIGH cottier-dialogue density) ---
      gutenberg_joyce.py            #   Joyce 1910
      ia_griffin.py                 #   Griffin 1829
      gutenberg_carleton.py         #   Carleton 1830s
      ia_croker.py                  #   Croker 1825
      gutenberg_kickham.py          #   Kickham 1879
      gutenberg_lover.py            #   Samuel Lover, Handy Andy 1842
      ia_maxwell.py                 #   William Hamilton Maxwell, Wild Sports of the West 1832
      gutenberg_lever.py            #   Charles Lever, Charles O'Malley 1841
      gutenberg_edgeworth.py        #   Maria Edgeworth, Castle Rackrent 1800 (Gutenberg #1424)
      ia_banim_1825.py              #   Banim brothers, Tales by the O'Hara Family 1st series 1825
      ia_banim_1826.py              #   Banim brothers, Tales by the O'Hara Family 2nd series 1826
      ia_hall_sketches.py           #   Anna Maria Hall, Sketches of Irish Character 1829
      ia_hall_lights.py             #   Anna Maria Hall, Lights and Shadows of Irish Life 1838
      # --- Travel-observer subcorpus (gentry-mediated peasant speech) ---
      gutenberg_young.py            #   Arthur Young, A Tour in Ireland 1776–1779 (Gutenberg #22387)
      ia_carr.py                    #   John Carr, The Stranger in Ireland 1806
      ia_inglis.py                  #   Henry David Inglis, Ireland in 1834
      ia_kohl.py                    #   Johann Georg Kohl, Travels in Ireland 1843
      ia_nicholson.py               #   Asenath Nicholson, Ireland's Welcome to the Stranger 1847
      ia_hall_scenery.py            #   Mr & Mrs S.C. Hall, Ireland: Its Scenery, Character &c. 1841–43 (3 vols)
      # --- Folklore / oral-register subcorpus ---
      gutenberg_lady_wilde.py       #   Lady Wilde, Ancient Legends of Ireland 1888 (Gutenberg #61436)
      ia_william_wilde.py           #   William Wilde, Irish Popular Superstitions 1852
      gutenberg_curtin.py           #   Jeremiah Curtin, Myths and Folk-lore of Ireland 1890 (Gutenberg #36540)
      gutenberg_hyde.py             #   Douglas Hyde, Beside the Fire 1890 (Gutenberg #60782)
      gutenberg_joyce_celtic.py     #   P.W. Joyce, Old Celtic Romances 1879 (Gutenberg #38041)
      ia_hardiman.py                #   James Hardiman, Irish Minstrelsy 1831 (2 vols)
      # --- Trial / commission / verbatim-testimony subcorpus (HIGHEST peasant-voice density) ---
      ht_devon_commission.py        #   Devon Commission Digest of Evidence 1843–45 (HathiTrust 001893729; ~4500pp)
      ia_poor_inquiry.py            #   Whately/Poor Inquiry Commission 1833–36 (IA op1245191-1001)
      ht_state_trials.py            #   Cobbett's Complete Collection of State Trials vols 27–33 (1798–1820)
      ht_friends_famine.py          #   Society of Friends Central Relief Cttee Famine Transactions 1852
      ia_leadbeater.py              #   Mary Leadbeater, Cottage Dialogues among the Irish Peasantry 1811
      dataverse_boston_pilot.py     #   Boston Pilot Information Wanted ads 1831–1921 (Harvard Dataverse DVN/UNJU3N)
      ia_whyte_diary.py             #   Robert Whyte, Famine Ship Diary 1847 + Ocean Plague 1848
      ia_bennett.py                 #   William Bennett, Narrative of a Recent Journey 1847
      ia_tuke.py                    #   James Hack Tuke, Visit to Connaught 1848 (Roscommon-adjacent)
      ia_nicholson_annals.py        #   Asenath Nicholson, Annals of the Famine 1851
      # --- First-person Irish-voice subcorpus ---
      ia_carleton_autobio.py        #   William Carleton, Autobiography 1896
      ia_holt.py                    #   Joseph Holt, Memoirs 1838
      ia_oconnell_corr.py           #   Daniel O'Connell, Correspondence (Fitzpatrick 1888)
      ia_tone.py                    #   Wolfe Tone, Memoirs 1826
      ia_byrne.py                   #   Miles Byrne, Memoirs 1863
      ia_mitchel.py                 #   John Mitchel, Jail Journal 1854 + Last Conquest of Ireland 1861
      # --- Religious / clerical / temperance subcorpus ---
      ia_doyle_jkl.py               #   Bishop Doyle (J.K.L.) Life and Correspondence (MacDonagh 1905)
      ia_cobbett_reformation.py     #   Cobbett, History of the Protestant Reformation 1824–27
      ia_ulster_revival_1859.py     #   1859 Ulster Revival eyewitness accounts (Weir et al.)
      ia_butler_catechism.py        #   James Butler II Catechism (Irish editions, 1775+)
      ia_garden_of_soul.py          #   Garden of the Soul (Challoner 1740, Dublin reprint 1872)
      # --- Periodicals (sample) ---
      gutenberg_irish_penny_journal.py  # Irish Penny Journal Vol 1 (Gutenberg #55518, 1840–41)
      # --- Stage-Irish caricature (REJECTED CLASS — fed to DPO rejected pool, not SFT) ---
      gutenberg_boucicault.py       #   Boucicault: Colleen Bawn 1860 (Gutenberg #52924), Arrah-na-Pogue, Shaughraun
      gutenberg_okeeffe.py          #   O'Keeffe: Poor Soldier 1783, Wicklow Mountains 1796
      gutenberg_sheridan.py         #   Sheridan: St Patrick's Day 1775 (Gutenberg #6707), The Rivals (#24761)
      ia_macklin.py                 #   Macklin: True-Born Irishman 1762, Love à la Mode 1759
      ia_tyrone_power_actor.py      #   Tyrone Power (the actor), Born to Good Luck 1832
      ia_bayle_bernard.py           #   William Bayle Bernard, His Last Legs / Irish Attorney 1839
      gutenberg_colman_younger.py   #   George Colman Younger, John Bull 1803 (Gutenberg #20177)
      gutenberg_farquhar.py         #   Farquhar: Recruiting Officer 1706 (#37012), Love and a Bottle 1698
      # --- Formal-register contrast set (paired contrast for joyce_pairs.py extension; NOT standalone training) ---
      ia_murray_grammar.py          #   Lindley Murray, English Grammar 1795
      ia_cobbett_grammar.py         #   Cobbett, Grammar of the English Language 1818
      ia_walker_dictionary.py       #   John Walker, Critical Pronouncing Dictionary 1791
      ia_neilson_irish.py           #   William Neilson, Introduction to the Irish Language 1808
      ia_dilworth_speller.py        #   Dilworth, Spelling-Book (1780s+ editions)
      ia_ne_commissioners_lessons.py  # NE Commissioners Books of Lessons 1831+ (Trinity Digital + IA)
      # --- Reference works (gentry/middling instructional) ---
      ia_etiquette.py               #   period etiquette manuals
      ia_letter_writing.py          #   period letter-writing manuals
      ia_almanac.py                 #   Old Moore's Almanack and similar
      ia_period_dict.py             #   period dictionaries filtered to game domain
      # --- Anachronism wordlist seeds (NOT training data) ---
      ia_webster_1828.py            #   Webster's American Dictionary 1828
      ia_wright_edd.py              #   Wright's English Dialect Dictionary 1898–1905 (Hiberno-English coverage)
      # --- Deferred (mentioned in README, not auto-ingested) ---
      # Dublin Penny Journal 1832–36, Dublin University Magazine 1833+, The Nation 1842+:
      # bulk-text extraction is per-issue OCR-heavy; tracked as a future source after baseline ships.
      # Outrage Reports / workhouse minute books: per-county digitisation status varies; deferred until
      # county heritage portals stabilise.
      common.py                     #   SHA-256 cache, fetcher harness; manifest-driven (`manifest.toml`) so each module is a thin
                                    #   wrapper around `common.fetch(source_id)` rather than per-source bespoke logic
      _MIGRATION_NOTE.md            #   With ~50 sources the per-module pattern is creaking; migration to
                                    #   `category_<name>.py` aggregator modules iterating `manifest.toml` is queued
                                    #   for the implementation PR. Naming above reserved either way.
    curate/                         # dialogue extraction, feature tagging, dedup
    build/
      instruction_pairs.py          #   literary-extracted pairs + system prompt template
      reference_pairs.py            #   reference-work-mined instruction pairs (Talkie-pattern; concrete recipe in §Data curation)
      formal_contrast_pairs.py      #   prescriptive-grammar pairs from Murray/Cobbett/Walker → extends joyce_pairs.py for code-switching anchor
      stage_irish_synth.py          #   synthesise stage-Irish caricature responses from Boucicault/Sheridan/Macklin/etc; tagged class:stage_irish_caricature for DPO rejected pool
      testimony_pairs.py            #   convert verbatim Devon/Whately/State-Trials Q&A into instruction pairs (witness Q → cottier A)
      anachronism_wordlist.py       #   union(Webster 1828, Joyce 1910 vocab, Wright EDD 1898–1905) + curated 20th–21st-c. blocklist → JSON
      split.py                      #   stratified train/val/test
    train/
      train_dialect_oracle.py       #   ~250M decoder-only LM pretrain; reads disjoint author split from configs/dialect_oracle_250m.yaml
    eval/
      judge_anachronism.py          #   deterministic three-source-union (Webster 1828 ∪ Joyce 1910 ∪ Wright EDD) + blocklist judge
      judge_talkie.py               #   Talkie-1930-13B-IT loglik judge (fixed Roscommon-1820s system prompt)
      judge_dialect_oracle.py       #   tiny-oracle loglik judge
      judge_deepseek.py             #   DeepSeek V4-pro coherence rubric (Promptfoo case) — DPO + nightly only, NOT serve-time
      judge_combined.py             #   Borda rank-aggregate; --mode {dpo|serve} selects 4-axis (DPO) or 3-axis local-only (serve)
      build_dpo_dataset.py          #   N=8 candidates per scenario → feature_tagger floor → 4-axis judge stack → (chosen, rejected) pairs;
                                    #   additionally injects stage_irish_synth.py outputs as `rejected` examples (cottier-prompt + caricature-response)
      calibrate_judges.py           #   ≥80 % direction-correct gate over synthetic pairs; halt + page on failure
      rubric.py                     #   per-feature substrate density (regression sensor)
      held_out_scenarios.py         #   60 situations × 5 classes
      ab_compare.py                 #   manual A/B markdown report
    package/                        # merge_lora + GGUF conversion + Modelfile render
    serve/
      inference_rejection_sampler.py  # K=4 best-of with 3-axis local stack (anachronism + Talkie + dialect-oracle); gated default-on by `inference-rejection-sampler`
  data/
    raw/                            # gitignored — cached downloads (SHA-256 keyed)
    interim/                        # gitignored — extracted dialogue JSONL
    processed/                      # gitignored — final train/val/test JSONL + DPO pairs JSONL
    synthetic_calibration/          # gitignored — per-run synthetic pairs (period + coherence axes)
    LICENSES.md                     # per-source public-domain attribution
  models/                           # gitignored — HF checkpoints, dialect-oracle, merged fp16, GGUF
  scripts/
    orchestrate.py                  # single Python entry point (see §Automation)
    generate_synthetic_calibration.py
    runpod_provision.py             # REST-API pod provisioning + teardown (24 h auto-destroy on failure)
    cost_monitor.py                 # RunPod billing + DeepSeek usage; halts on cap breach
    run_runpod.sh                   # thin wrapper invoked by orchestrator inside the pod
  runs/                             # gitignored — per-run state.json + stage logs (resumable checkpoints)
```

`scripts/run_local.sh` is mentioned in the README as an *alternative for AMD-equipped contributors* but is not part of the canonical artifact set.

## Data ingestion

All sources are US public domain (life+70 years expired). Attribution kept in `data/LICENSES.md`; downloads SHA-256-cached under `data/raw/<source>/` via a shared `common.py` helper so reruns are free.

**Literary core (13 authors, HIGH cottier-dialogue density):**

| Source | Format | Module | Register |
|---|---|---|---|
| Edgeworth, *Castle Rackrent* (1800) | Gutenberg #1424 | `ingest/gutenberg_edgeworth.py` | unreliable peasant-narrator (Thady) |
| Joyce, *English As We Speak It in Ireland* (1910) | Gutenberg HTML #34251, parsed w/ `selectolax` | `ingest/gutenberg_joyce.py` | mixed (cottier-heavy examples) |
| Griffin, *The Collegians* (1829) | Internet Archive plaintext (`internetarchive` PyPI) | `ingest/ia_griffin.py` | cottier (Danny Mann) + gentry (Hardress Cregan) |
| Banim brothers, *Tales by the O'Hara Family*, 1st series (1825) | Internet Archive (HathiTrust) | `ingest/ia_banim_1825.py` | cottier-focused (*Crohoore of the Billhook*, *The Fetches*) |
| Banim brothers, *Tales by the O'Hara Family*, 2nd series (1826) | Internet Archive `talesbyoharafam02banigoog` | `ingest/ia_banim_1826.py` | rural peasant + code-switching (*The Nowlans*, *Peter of the Castle*) |
| Hall (Anna Maria), *Sketches of Irish Character* (1829) | Internet Archive `sketchesofirishc00hallrich` | `ingest/ia_hall_sketches.py` | mixed (gentry observer + peasant short-form sketches) |
| Carleton, *Traits and Stories* (1830s) | Gutenberg author 2498 (multiple volumes) | `ingest/gutenberg_carleton.py` | northern peasant — Carleton was peasant-raised (highest-density single source) |
| Croker, *Fairy Legends* (1825) | Internet Archive plaintext | `ingest/ia_croker.py` | folk-tale dialogue |
| Maxwell, *Wild Sports of the West* (1832) | Internet Archive plaintext | `ingest/ia_maxwell.py` | Connacht sporting + cottier dialogue |
| Hall (Anna Maria), *Lights and Shadows of Irish Life* (1838) | Internet Archive (via Wikisource / AskAboutIreland) | `ingest/ia_hall_lights.py` | mixed cottier/gentry rural vignettes |
| Lever, *Charles O'Malley* (1841) | Gutenberg (search "Charles Lever") | `ingest/gutenberg_lever.py` | gentry + military + servant register |
| Lover, *Handy Andy* (1842) | Gutenberg (search "Samuel Lover Handy Andy") | `ingest/gutenberg_lover.py` | comic Munster servant + gentry |
| Kickham, *Knocknagow* (1879) | Gutenberg #44645 | `ingest/gutenberg_kickham.py` | cottier + middling farmer |

**Travel-observer subcorpus (gentry-mediated peasant speech, ~6 % of mix):**

| Source | Format | Module | Register |
|---|---|---|---|
| Young, *A Tour in Ireland 1776–1779* | Gutenberg #22387 | `ingest/gutenberg_young.py` | gentry observer + reported peasant; agricultural/economic context |
| Carr, *The Stranger in Ireland* (1806) | Internet Archive `strangerinirelan00carr` | `ingest/ia_carr.py` | gentry tour of S. & W. Ireland |
| Inglis, *Ireland in 1834* (2 vols) | Internet Archive `irelandin1834jou01ingl` | `ingest/ia_inglis.py` | mid-period observer + peasant interviews |
| Mr & Mrs S.C. Hall, *Ireland: Its Scenery, Character &c.* (1841–43, 3 vols) | Internet Archive `irelanditsscene00unkngoog` | `ingest/ia_hall_scenery.py` | illustrated travel; social vignettes |
| Kohl, *Travels in Ireland* (1843) | Internet Archive `travelsinirelan00kohlgoog` | `ingest/ia_kohl.py` | German visitor; pre-Famine documentation |
| Nicholson, *Ireland's Welcome to the Stranger* (1847) | Internet Archive `irelandswelcomet00nich` | `ingest/ia_nicholson.py` | American widow interviews poor; direct peasant speech recorded |

**Folklore / oral-register subcorpus (~4 % of mix):**

| Source | Format | Module | Register |
|---|---|---|---|
| Hardiman, *Irish Minstrelsy* (1831, 2 vols) | Internet Archive `irishminstrelsy00hardgoog` | `ingest/ia_hardiman.py` | song & ballad collection w/ translations |
| W. Wilde, *Irish Popular Superstitions* (1852) | Internet Archive `irishpopularsupe0000wild` | `ingest/ia_william_wilde.py` | folklore / social observation |
| Joyce, *Old Celtic Romances* (1879) | Gutenberg #38041 | `ingest/gutenberg_joyce_celtic.py` | Celtic mythology, Joyce's narrative voice |
| L. Wilde, *Ancient Legends of Ireland* (1888) | Gutenberg #61436 | `ingest/gutenberg_lady_wilde.py` | folklore frame-narratives collected from peasantry |
| Curtin, *Myths and Folk-lore of Ireland* (1890) | Gutenberg #36540 | `ingest/gutenberg_curtin.py` | West-Ireland Gaelic storytellers' English |
| Hyde, *Beside the Fire* (1890) | Gutenberg #60782 | `ingest/gutenberg_hyde.py` | Gaelic-origin stories with grammar/idiom notes |

**Reference works (gentry/middling instructional, ~4 % of mix):**

| Source | Format | Module | Register |
|---|---|---|---|
| Period etiquette manuals (e.g. *Hints on Etiquette*, 1820s–1840s) | Internet Archive plaintext | `ingest/ia_etiquette.py` | gentry / middling instructional |
| Period letter-writing manuals (e.g. *The Complete Letter-Writer*, multiple editions) | Internet Archive plaintext | `ingest/ia_letter_writing.py` | gentry / middling instructional |
| Old Moore's Almanack and contemporaneous almanacs | Internet Archive plaintext | `ingest/ia_almanac.py` | period idiom + calendar/seasonal vocabulary |
| Period dictionaries (filtered to game-domain entries) | Internet Archive / Gutenberg plaintext | `ingest/ia_period_dict.py` | lexical attestation |
| **Webster's American Dictionary of the English Language** (1828) | Internet Archive plaintext / public-domain mirrors (e.g. webstersdictionary1828.com bulk export) | `ingest/ia_webster_1828.py` | anachronism wordlist seed (positive-attestation list) |
| **Wright, *English Dialect Dictionary*** (1898–1905, 6 vols) | Internet Archive plaintext (multi-volume scans) | `ingest/ia_wright_edd.py` | anachronism wordlist seed — covers regional/Hiberno-English vocabulary Webster 1828 misses |

**Mix shape:** the 13-author literary core supplies the dominant cottier signal; travel-observer and folklore subcorpora supply ~10 % combined for register breadth; reference-work pairs supply gentry/middling instructional supervision. Webster 1828 and Wright EDD are **not** training data — they are consumed by `build/anachronism_wordlist.py` together with Joyce 1910's vocabulary to produce the deterministic period-axis judge's allow-list. Periodicals (*Dublin Penny Journal* 1832–36, *Dublin University Magazine* 1833+, *The Nation* 1842+) are deferred — bulk per-issue OCR extraction is heavier than the marginal signal warrants for v1; tracked as future sources after the baseline ships.

CORIECOR and the RIA Corpas Stairiúil are **not** automated — they require researcher contact / paid CD-ROM. Deferred: listed in README as "future sources" once baseline model is shipped.

## Data curation

- **`curate/dialogue_extractor.py`** — regex around `"…"`, `'…'`, and em-dash dialogue (Joyce/Griffin convention). Speaker attribution via verb-of-saying pattern (said/replied/cried/answered/muttered/whispered/roared/returned).
- **`curate/feature_tagger.py`** — rule-based tags mapped directly from the grammar table in the research doc. Regexes for after-perfect (`\bafter\s+\w+ing\b`), habitual `do be`, cleft `'tis\s+\w+ing`, existential `in it`, detrimental `on (me|him|her|us|ye|them)`, emphatic reduplication, and a vocab-list lookup for discourse markers (wisha/musha/arrah/yerra/wirra). **Mandatory floor gate for cottier class:** spans below a substrate-density threshold (≥N substrate features per 100 tokens, calibrated against the literary corpus) are dropped from the SFT mix and from DPO candidate scoring — not just labelled. The threshold is tuned so general 19th-century Standard English (e.g. unfiltered Carleton narration) does not pass.
- **`curate/joyce_pairs.py`** — Joyce often provides dialect→standard paraphrase (`"X" — i.e. "Y"`). Captured as paired examples. **Primary labelled supervision** (20 % of the SFT mix, see below).
- **`curate/class_assigner.py`** — **evidence-based**, not heuristic: (1) verb-of-saying speaker → `speaker_class.yaml` lookup for known characters (Danny Mann → cottier, Hardress Cregan → gentry, Father Connell → priest); (2) substrate-density threshold from `feature_tagger.py` as a fallback for unknown speakers (above-threshold → cottier; near-zero → gentry); (3) spans the assigner cannot classify with evidence are dropped, not bucketed.
- **`curate/dedupe.py`** — `datasketch` MinHashLSH at paragraph level (Jaccard 0.85).
- **`build/reference_pairs.py` extraction recipe** (concrete; the 5 % slice's quality depends on this):
  1. **Etiquette manuals** — extract every numbered rule / "Do X" / "Avoid Y" → templated as instruction pair. Example: rule "A gentleman should never address a lady to whom he has not been introduced" → `{user: "Describe how a gentleman should approach a lady he has not been formally introduced to.", assistant: "<rule, paraphrased into first-person register>"}`.
  2. **Letter-writing manuals** — every model letter becomes a pair: salutation+body+closing → `{user: "Write a <relation> letter from a <role> to a <recipient> on <topic>.", assistant: "<model letter verbatim, lightly normalised>"}`. Manuals routinely categorise letters by relation/topic — re-use those headings as the user-prompt slot.
  3. **Almanacs** — extract dated entries (saint's days, fairs, weather lore, agricultural notes) → `{user: "What does a <region> farmer say about <month> weather/fair?", assistant: "<almanac entry, attribution-stripped>"}`.
  4. **Period dictionaries** — only entries on the game-domain word-list (kinship, agriculture, religion, trade) become pairs: `{user: "Define '<word>' as a <region> farmer in 1820 would use it.", assistant: "<dictionary gloss>"}`. The remainder feeds `build/anachronism_wordlist.py` as positive attestations only.
  Each pair carries `source: reference-<manual_slug>` and `class: gentry` (etiquette, letter-writing) or `class: middling_farmer` (almanacs, dictionaries) so they don't pollute the cottier mix.
- **Volume target (13-author core + ~38 subcorpus titles):** 350–500k dialogue spans / Q&A pairs (~3–4 M tokens) post-dedup. **Escalation floor: <120k spans** triggers CORIECOR outreach before training. The bump comes mostly from the trial/commission slice — Devon Commission alone is ~4500 pp of structured Q&A, much of which converts cleanly to instruction pairs via `build/testimony_pairs.py`.

## Instruction-pair construction

JSONL schema in `data/processed/{train,val,test}.jsonl`:

```json
{
  "system": "You are an NPC in 1820s rural County Roscommon, Ireland. Speak in period-accurate Hiberno-English with Irish substrate grammar. Social class: cottier. Lean on these features when natural: after-perfect, do-be habitual, cleft sentences. Discourse markers allowed: wisha, musha, arrah.",
  "user": "A neighbour asks if you've seen the priest today.",
  "assistant": "Wisha, I am after seeing him below at the chapel, so I am — 'tis confessions he was hearing.",
  "meta": {"class": "cottier", "tags": ["after-perfect","discourse-marker:wisha","emphatic-reduplication"], "source": "literary-extracted:griffin"}
}
```

- **System-prompt template** lives once in `src/parish_train/build/instruction_pairs.py::build_system_prompt()` and is reused verbatim at inference time inside the Ollama Modelfile — single source of truth.
- **Classes:** `{cottier, small_farmer, middling_farmer, gentry, priest, schoolmaster}` (matches the research doc).
- **Mix (by row count): 75 % literary-extracted / 20 % Joyce dialect↔standard paraphrase / 5 % reference-work pairs.** No hand-written anchor; no `sample_weight` re-weighting.
- **Reference-work pairs** (`build/reference_pairs.py`) are constructed Talkie-style: snippets from etiquette / letter-writing manuals / almanacs / period dictionaries are converted into instruction pairs by templated paraphrase (e.g. "Ask a tenant farmer how to greet his landlord" → manual's documented salutation). They primarily strengthen gentry / middling-farmer register.
- **Split:** 90/5/5 stratified on `(class, primary_tag)` via scikit-learn `StratifiedShuffleSplit`.

## Training stack

**Library: axolotl** (`pip install axolotl[flash-attn]`) — declarative YAML, first-class QLoRA + Gemma chat-template support + `trl.DPOTrainer` integration. The same axolotl install drives Stage 1 SFT and Stage 2 DPO.

### Stage 0 — Tiny dialect-oracle pretrain (parallel with Stage 1)

Train a ~250M-parameter decoder-only LM (small Pythia or fresh Llama-style architecture) on the deduped 8-author literary corpus. Output: `models/dialect-oracle-250m/`. Used only as a judge in Stage 2 / Stage 3 — never served.

**Fully disjoint author split is a design requirement** (not a knob). The 13-author core enables a 6/6 disjoint split with one author **reserved eval-only** (never seen by either oracle or SFT, used for the held-out scenario set in `eval/held_out_scenarios.py`):

| Side | Authors | Used by |
|---|---|---|
| **Oracle training** (Stage 0) | Joyce 1910, Carleton, Croker, Lover, Edgeworth, Banim 1825, Hall *Sketches* 1829 | dialect-oracle pretrain |
| **SFT training** (Stage 1 + Stage 2) | Griffin, Kickham, Maxwell, Lever, Banim 1826, Hall *Lights* 1838 | actor SFT + DPO scenarios |
| **Eval-only** (held out from both) | one rotating author per run (default: Banim 1826 swapped with held-out slot — see `dialect_oracle_250m.yaml`) | `eval/held_out_scenarios.py` exclusively |

**Subcorpus assignment to oracle/SFT split:**

- **Literary core only** is split disjointly (the table above) — these 13 novelistic-prose authors are what the dialect-oracle prior is meant to encode.
- **Shared by both sides** (don't drive oracle prior, supply complementary register): travel-observer, folklore/oral, trial/commission testimony, first-person Irish memoir, religious/clerical, periodicals, reference-work pairs.
- **SFT-only, never seen by oracle**: stage-Irish caricature subcorpus (only used as DPO `rejected` examples) and the formal-contrast set (only used to extend `joyce_pairs.py` with paired examples).
- **Consumed by neither**: Webster 1828 + Wright EDD — they only feed `build/anachronism_wordlist.py`.

The split is encoded in `configs/dialect_oracle_250m.yaml` and consumed by both `train/train_dialect_oracle.py` and `build/instruction_pairs.py`.

Driver: minimal `transformers.Trainer` script in `train/train_dialect_oracle.py`. Wall-clock ~6 h on the same A100-80GB pod, parallelised with Stage 1 (the oracle's footprint is small enough to co-reside with Stage 1's QLoRA — see §Hardware fit check).

### Stage 1 — QLoRA SFT on Gemma 4 9B IT

`configs/qlora_gemma4_9b.yaml` (RunPod A100-80GB primary):

- `base_model: google/gemma-4-9b-it`
- `adapter: qlora`, `load_in_4bit: true`, `bnb_4bit_quant_type: nf4`, `bnb_4bit_compute_dtype: bfloat16`
- `lora_r: 16`, `lora_alpha: 32`, `lora_dropout: 0.05`
- `lora_target_modules: [q_proj, k_proj, v_proj, o_proj, gate_proj, up_proj, down_proj]`
- `learning_rate: 2e-4`, `lr_scheduler: cosine`, `warmup_ratio: 0.03`
- `num_epochs: 3`, `optimizer: paged_adamw_8bit`, `gradient_checkpointing: true`
- `chat_template: gemma`, `train_on_inputs: false` (mask system+user, train on assistant only)
- **A100-80GB:** `sequence_len: 4096`, `micro_batch_size: 4`, `gradient_accumulation_steps: 4`. Wall-clock 6–8 h.

Output: `models/qlora-sft-out/` (LoRA adapter on Gemma 4 9B IT).

### Stage 2 — Iterated DPO with combined judge stack

`configs/dpo_gemma4_rundale.yaml` (axolotl invokes `trl.DPOTrainer`):

1. From the SFT model, generate **N=8** candidates per held-out scenario (`eval/held_out_scenarios.py`, distinct from SFT/val/test).
2. Apply `feature_tagger.py` floor as a **hard gate** on cottier-class scenarios — candidates failing the substrate-density threshold are dropped before scoring.
3. Score the survivors on four axes via `eval/judge_combined.py --mode dpo`:
   - **`judge_anachronism`** — deterministic; counts tokens missing from `data/processed/anachronism_wordlist.json` (Webster 1828 ∪ Joyce 1910 vocab ∪ Wright EDD 1898–1905) plus blocklist hits.
   - **`judge_talkie`** — log-likelihood under Talkie-1930-13B-IT (q4) with a fixed Roscommon-1820s system prompt.
   - **`judge_dialect_oracle`** — log-likelihood under the Stage-0 ~250M oracle.
   - **`judge_deepseek`** — 1–10 rubric (in-character / mood / register / coherence) via DeepSeek V4-pro API. Promptfoo-driven for parity with the rest of the eval harness. **Used only at DPO scoring time and during nightly regression sweeps — not at serve time** (see Stage 3).
4. Aggregate via **Borda rank** across the four axes (robust to scale differences) → `(chosen, rejected)` pairs in `data/processed/dpo_round_N.jsonl`.
5. Run DPO on the SFT model. **Reference policy = the post-Stage-1 SFT adapter, frozen**, per `trl.DPOTrainer` convention (`ref_model=None` with the LoRA adapter held out via `peft`'s reference-model accessor). Not the base Gemma 4 9B IT — using base would let the policy drift back toward generic Standard English. Repeat steps 1–5 for **2–3 rounds**, regenerating candidates from each round's policy and re-pinning the reference to the *previous* round's policy (round 2 ref = round 1 output, etc.).

Output: `models/qlora-dpo-out/` (LoRA adapter ready for `peft merge`).

### Stage 3 — Inference-time rejection sampler

At serve time the dialogue provider runs a best-of-K loop using **only the three local judges** (DeepSeek is dropped — API roundtrip × K kills latency).

**Serving constraint: Ollama serialises requests per model instance.** `num_predict` controls token count, not parallel sequences; K=4 candidates with Ollama means K sequential generations of ~400 ms each → ~1.6 s for generation alone, with scoring on top. That blows any "block the player" budget. Two viable paths, with the recommended choice spelled out:

**Recommended path — Background-lane critic (no runtime change to ADR-005's Ollama stance):**

1. Tier 1 generates the **draft** as it does today. Ship the draft to the player immediately (≤ 600 ms — the existing budget from `03-dialogue-quality-loops.md` §1).
2. **In parallel**, dispatch a `CriticJob` (the §7 pattern in `03-dialogue-quality-loops.md`) that requests **K-1 alternates** from Ollama and scores draft + alternates with `judge_combined.py --mode serve`.
3. If the draft loses Borda, **silently replace it with the winner before the player can respond** (300–600 ms after draft display, well under typical reading time). If the draft wins, no UI mutation. Either way, the K-1 losers go to the nightly preference log.
4. **Hard wall-clock cap on the critic = 1500 ms.** Past that, abandon the critic for that turn and keep the draft as-shipped — the player has already started reading.

This preserves the interactive feel (no blocking on K=4), keeps Ollama as the single inference runtime per ADR-005, and still gives the model the four-axis (offline DPO) → three-axis (serve) preference signal the design needs.

**Alternate path — switch the dialogue provider to vLLM/TGI** for `n=K` continuous batching at ~250–400 ms total. **Not recommended for v1**: it requires amending ADR-005 ("Ollama for local inference"), revisiting model packaging (vLLM consumes HF safetensors, not GGUF), and re-baselining VRAM (vLLM is heavier than Ollama on the same hardware). Worth re-opening only if the Background-lane pattern shows visible flicker in playtest.

| Component (Background-lane critic) | Budget |
|---|---|
| Draft (Tier 1, unchanged) | ≤ 600 ms (player sees this) |
| K-1 = 3 alternate generations (Ollama serial, ~400 ms each) | ~1200 ms |
| Talkie q4 forward × K (~80-token outputs, resident) | ~150 ms |
| Dialect-oracle 250M forward × K | ~50 ms |
| Anachronism wordlist (K × set lookup) | <5 ms |
| Borda aggregate + bookkeeping | ~10 ms |
| **Critic total** | **~1415 ms** (under 1500 ms cap) |
| **Player-visible latency** | **≤ 600 ms** (silent replace happens during reading) |

**N=8 (DPO) vs K=4 (serve) — design choice, not a pricing accident.** Training spends more candidates per scenario because there's no latency cap and the marginal Borda signal from candidates 5–8 is worth the extra GPU minutes; serve-time halves K because every additional candidate adds ~400 ms of Ollama-serial generation that has to fit inside the 1500 ms critic cap.

**Gated by `config.flags.is_enabled("inference-rejection-sampler")`** per CLAUDE.md non-negotiable rule 6 — distinct from `rundale-dialect-model` (which controls the system prompt + model selection). **Both flags ship default-on**, in the same PR as the artifact they gate. Per rule 6, the flag exists to allow disabling, not to stage rollout — a feature merged behind a default-off flag is dead code on day one.

The sampler **wraps and does not modify** the existing JSON serving contract:

- Response shape: `NpcJsonResponse` at `parish/crates/parish-npc/src/lib.rs:216` — same schema applied to all K candidates.
- Streaming partial-JSON extraction: `extract_dialogue_from_partial_json` at `parish/crates/parish-types/src/ids.rs:229`.

Implementation: `src/parish_train/serve/inference_rejection_sampler.py` ships as a Python reference; the production path lives in a small `parish-npc` adapter that calls Ollama K times and re-uses the same scoring code via FFI / subprocess (TBD in the implementation PR).

## Hardware fit check

### Primary path — RunPod A100-80GB (Stage 2 co-residence)

Stage 2 is the worst-case footprint because the actor, frozen reference policy, and two judge models all need to be resident:

| Component | Est. |
|---|---|
| Actor (Gemma 4 9B bf16 + LoRA, post-SFT) | ~18 GB |
| LoRA grads + optimizer state | ~3 GB |
| Activations (seq 4096, mb 4, gradient-checkpointed, DPO uses 2× passes) | ~5 GB |
| Frozen reference policy (post-SFT LoRA + Gemma 4 9B base shared in q4 NF4) | ~5 GB |
| Talkie-1930-13B-IT (q4 NF4) | ~7.5 GB |
| Dialect oracle (250M bf16) | ~0.5 GB |
| Inference / serving overhead, fragmentation | ~3 GB |
| **Total** | **~42 GB of 80 GB** |

DeepSeek V4-pro is API-side and contributes no GPU footprint. Headroom is sufficient that Stage 0 (dialect-oracle pretrain) can co-reside with Stage 1 (SFT) on the same pod — a single pod runs the whole pipeline.

**Wall-clock and cost (per full run):**

- Stage 0 dialect oracle: ~6 h
- Stage 1 SFT: 6–8 h
- Stage 2 DPO: **6–10 h × 2–3 iterations** (N=8 candidates × ~300 scenarios = ~2400 generations/round, plus the DPO training step itself; per-round timeout 12 h)
- Stage 3 packaging + `/prove`: ~1 h
- **Total: ~24–38 h per run** (Stage 0 runs in parallel with Stage 1 so wall-clock is dominated by SFT + DPO + packaging)
- **RunPod cost:** A100-80GB at ~$1.89/h × ~30 h ≈ **~$45–75**
- **DeepSeek V4-pro cost** (current discount, ~40 k judge calls across DPO + nightly best-of-K eval): **~$12** before 2026-05-31 15:59 UTC ($0.435 / $0.87 per MTok in/out, cache-hit input $0.003625 / MTok); **~$48** after the discount expires ($1.74 / $3.48 per MTok). Sonnet 4.6 batch+cache fallback for the *judge* role: ~$38 for the same workload.
- **Anthropic Sonnet 4.6 cost** (always-on, calibration-distractor generator): ~$2 per run for ~400 distractor generations.

### Local alternative (RX 9070 16 GB) — not the canonical path

Maintained as an option for AMD-equipped contributors who want to iterate on data curation locally before paying for a RunPod run. Original budget retained:

| Component | Est. |
|---|---|
| Base weights (9B × 0.5 B/param NF4) | ~4.6 GB |
| LoRA adapters + grads (r=16 × 7 modules) | ~0.4 GB |
| Paged 8-bit optimizer state | ~0.6 GB |
| Activations (seq 1536, bs 1, ckpt) | ~5–7 GB |
| Kernels + fragmentation + ROCm bnb overhead | ~1.5 GB |
| **Total** | **~12–14 GB** |

Local DPO with Talkie-IT loaded simultaneously **does not fit** on 16 GB; the local path supports SFT only and must be followed by a RunPod run for Stage 2. **Biggest local risk:** `bitsandbytes-rocm` wheel availability for RDNA4 / ROCm 6.x — a local launcher script pre-flights `python -c "import bitsandbytes; print(bitsandbytes.__version__)"` and bails with a clear message pointing to the RunPod path if it fails. No MLX / Apple-Silicon path is supported.

## Evaluation

The four-judge stack triple-purposes:

1. **DPO training signal** (Stage 2) — Borda-aggregated ranks become `(chosen, rejected)` pairs.
2. **Regression sensor** — same scorers wired into `/prove rundale-dialect` and the `llm-quality-evals` harness; a leaderboard JSON tracks score-per-axis across model iterations.
3. **Inference-time best-of-K selector** (Stage 3) — gated by `inference-rejection-sampler` flag.

### Calibration gate (precondition for DPO)

Before Stage 2 starts generating candidates, every judge must pass calibration on a freshly generated synthetic set:

**Distractor / judge models are deliberately distinct** to break the same-model-corrupts-and-judges circle:

- **Period axis:** 200 spans sampled from the literary corpus, each rewritten in modern English **by Sonnet 4.6** (batch API + cache). Pair `(original=correct, modernized=incorrect)`. Each period-axis candidate judge (`judge_anachronism`, `judge_talkie`, `judge_dialect_oracle`) is scored against both halves; correct direction = judge prefers `original`. None of these judges share a model family with Sonnet, so the construction is non-circular.
- **Coherence axis:** 200 in-character NPC dialogues sampled from `parish/testing/fixtures/`, each corrupted **by Sonnet 4.6** (random mood/character/anachronism injection). Pair `(original=correct, corrupted=incorrect)`. `judge_deepseek` (DeepSeek V4-pro) is scored against both halves — distractor model and judge model are unrelated providers, so the construction is non-circular.
- **Pass criterion:** ≥80 % direction-correct on each axis. Failure → orchestrator halts and pages the user. **Fallback:** if `judge_deepseek` fails calibration, the orchestrator switches the *judge* to Sonnet 4.6 batch+cache **and** switches the *coherence-axis distractor generator* to Gemini 2.5 Flash (cheap, distinct from Sonnet) so the new pairing is again non-circular. If that re-calibration also fails the run is aborted.
- **Per-run, not pre-cached.** Calibration always reflects the current judge state (e.g. an updated Talkie checkpoint, a re-trained dialect oracle). See §Automation for the generation procedure.
- **Cost note.** Sonnet 4.6 batch+cache for ~400 distractor generations is ~$2 per run; offsets the DeepSeek-only construction by a few dollars and is the price of non-circularity.

### Static evaluation (unchanged from prior plan, now complementary to the four-judge stack)

- **Held-out scenario set** (`eval/held_out_scenarios.py`): 60 hand-written situations × 5 classes = 300 prompts, never seen at training.
- **Automated rubric** (`eval/rubric.py`): per generation, counts feature occurrences per 100 tokens for after-perfect, habitual `do be`, cleft `'tis…`, existential `in it`, detrimental `on me`, discourse markers, echo-verb answers instead of yes/no, emphatic reduplication. Plus an **anachronism block-list** (ok, okay, hi, hey, guys, awesome, cool) — any hit fails the example.
- **Social-register check:** cottier outputs ≥1 phonetic spelling / 50 tokens; gentry outputs ≤0.1 / 50 tokens.
- **`/prove rundale-dialect`** (CLAUDE.md rule 4): new harness script `mods/rundale/scripts/prove_rundale_dialect.toml` switches provider to `gemma4-rundale:9b`, walks into a cottage, speaks with a cottier and a priest, and asserts both the rubric and the four-judge stack pass on the JSON output.
- **Manual A/B** (`eval/ab_compare.py`): same 30 prompts to base `gemma4:9b-it` and candidate `gemma4-rundale:9b`, two-column markdown at `eval/reports/ab_<date>.md` for human review.

**Success bar to merge:** rubric ≥1 substrate feature / 30 tokens for cottier class, ≤0.05 anachronism rate across all classes, four-judge stack shows the candidate Borda-beats stock Gemma 4 9B on ≥70 % of held-out scenarios, and a green `/prove rundale-dialect`.

## Packaging for Ollama

1. `package/merge_lora.py` — load base in fp16, `PeftModel.from_pretrained`, `merge_and_unload()`, save to `models/merged-fp16/`.
2. `package/to_gguf.sh` — clones `llama.cpp` into `training/vendor/llama.cpp` (gitignored), runs `convert_hf_to_gguf.py models/merged-fp16 --outfile models/gemma4-rundale-f16.gguf`, then `llama-quantize models/gemma4-rundale-f16.gguf models/gemma4-rundale-q4_K_M.gguf q4_K_M`.
3. `configs/modelfile.gemma4-rundale`:
   ```
   FROM ./models/gemma4-rundale-q4_K_M.gguf
   TEMPLATE """<start_of_turn>user
   {{ .System }}
   {{ .Prompt }}<end_of_turn>
   <start_of_turn>model
   """
   PARAMETER temperature 0.85
   PARAMETER top_p 0.9
   PARAMETER repeat_penalty 1.08
   PARAMETER stop "<end_of_turn>"
   PARAMETER stop "<start_of_turn>"
   SYSTEM """You are an NPC in 1820s rural County Roscommon, Ireland. Speak in period-accurate Hiberno-English with Irish substrate grammar."""
   ```
4. `ollama create gemma4-rundale:9b -f training/configs/modelfile.gemma4-rundale`.

The dialect oracle is **not** Ollama-served — it is a judge-only artifact and stays under `models/dialect-oracle-250m/` for use by `judge_dialect_oracle.py`.

## Parish wiring

- **`parish.example.toml`** — append a commented opt-in example under the existing provider block:
  ```toml
  # [provider.dialogue]
  # name = "ollama"
  # base_url = "http://localhost:11434"
  # model = "gemma4-rundale:9b"   # see training/README.md to build this
  ```
- **Two distinct feature flags, both default-on** (per CLAUDE.md rule 6 — "Gate with `config.flags.is_enabled`, default-on, and document in PR"):
  - `rundale-dialect-model` — gates the Rundale-specific system-prompt injection. **Ships default-on in the same PR as the model artifact + the dialect system-prompt assembly site.** Controls only the dialect system prompt — if disabled, the engine uses the generic Dialogue prompt regardless of which model is wired up, so users who point `[provider.dialogue]` at stock `gemma4:9b` are unaffected. Disable knob exists for users who don't want Hiberno-English defaults (e.g. someone running a non-Rundale mod).
  - `inference-rejection-sampler` — gates the serve-time best-of-K wrapper (Stage 3). Independent of `rundale-dialect-model`. **Ships default-on in the same PR as the wrapper implementation + a passing best-of-K evaluation pass + a measured latency profile under 600 ms.** No "merge dead, flip later" sequence — if the wrapper isn't ready to ship default-on, it doesn't merge.
  - **Eval gating belongs in CI**, not in flag-flip choreography. `cargo test` (architecture-fitness) plus `/prove rundale-dialect` plus the four-judge regression sensor are the gates; if those pass, the flag ships on. If they don't, the PR doesn't merge — the wrapper is not flag-hidden behind a default-off in either case.
- **Doc follow-ups** (same PR as the feature flag):
  - [`docs/design/inference-pipeline.md`](inference-pipeline.md) — add `gemma4-rundale:9b` as an optional Dialogue pick under "Recommended Models (April 2026)".
  - [`docs/adr/005-ollama-local-inference.md`](../adr/005-ollama-local-inference.md) — append a "Specialist models" subsection pointing at the new ADR.
  - [`docs/research/Irish-English-1820s-resources.md`](../research/Irish-English-1820s-resources.md) — append outcome notes (data volumes, rubric scores, A/B findings) under the existing "For Fine-Tuning" section.
  - [`docs/plans/llm-quality-evals.md`](../plans/llm-quality-evals.md) — note four-judge harness as regression sensor + synthetic-calibration approach.
  - [`docs/design/ai-techniques/03-dialogue-quality-loops.md`](ai-techniques/03-dialogue-quality-loops.md) — append inference-time rejection-sampler subsection.
  - **New ADR** `docs/adr/0NN-rundale-dialect-model.md` — documents the QLoRA + DPO decision, dataset provenance, judge stack rationale, eval results, and serving path.

## Critical files to create / modify

**Create:**
- `training/pyproject.toml`, `training/README.md`, `training/.gitignore`, `training/.env.example`
- `training/docker/Dockerfile.training`
- `training/configs/qlora_gemma4_9b.yaml`
- `training/configs/dpo_gemma4_rundale.yaml`
- `training/configs/dialect_oracle_250m.yaml`
- `training/configs/rundale_dialect_e2e.yaml`
- `training/configs/modelfile.gemma4-rundale`
- `training/src/parish_train/ingest/{` literary core: `gutenberg_joyce,ia_griffin,gutenberg_carleton,ia_croker,gutenberg_kickham,gutenberg_lover,ia_maxwell,gutenberg_lever,gutenberg_edgeworth,ia_banim_1825,ia_banim_1826,ia_hall_sketches,ia_hall_lights`; travel-observer: `gutenberg_young,ia_carr,ia_inglis,ia_kohl,ia_nicholson,ia_hall_scenery`; folklore: `gutenberg_lady_wilde,ia_william_wilde,gutenberg_curtin,gutenberg_hyde,gutenberg_joyce_celtic,ia_hardiman`; trial/commission/testimony: `ht_devon_commission,ia_poor_inquiry,ht_state_trials,ht_friends_famine,ia_leadbeater,dataverse_boston_pilot,ia_whyte_diary,ia_bennett,ia_tuke,ia_nicholson_annals`; first-person Irish: `ia_carleton_autobio,ia_holt,ia_oconnell_corr,ia_tone,ia_byrne,ia_mitchel`; religious/clerical: `ia_doyle_jkl,ia_cobbett_reformation,ia_ulster_revival_1859,ia_butler_catechism,ia_garden_of_soul`; periodicals: `gutenberg_irish_penny_journal`; stage-Irish (rejected class): `gutenberg_boucicault,gutenberg_okeeffe,gutenberg_sheridan,ia_macklin,ia_tyrone_power_actor,ia_bayle_bernard,gutenberg_colman_younger,gutenberg_farquhar`; formal-contrast: `ia_murray_grammar,ia_cobbett_grammar,ia_walker_dictionary,ia_neilson_irish,ia_dilworth_speller,ia_ne_commissioners_lessons`; reference works: `ia_etiquette,ia_letter_writing,ia_almanac,ia_period_dict`; wordlist seeds: `ia_webster_1828,ia_wright_edd`; harness: `common}.py` (~50 modules; manifest-driven aggregation queued for impl PR — see `_MIGRATION_NOTE.md` in §Repo layout)
- `training/src/parish_train/curate/{dialogue_extractor,feature_tagger,joyce_pairs,class_assigner,dedupe}.py`
- `training/src/parish_train/build/{instruction_pairs,reference_pairs,formal_contrast_pairs,stage_irish_synth,testimony_pairs,anachronism_wordlist,split}.py`
- `training/src/parish_train/train/train_dialect_oracle.py`
- `training/src/parish_train/eval/{judge_anachronism,judge_talkie,judge_dialect_oracle,judge_deepseek,judge_combined,build_dpo_dataset,calibrate_judges,rubric,held_out_scenarios,ab_compare}.py`
- `training/src/parish_train/package/{merge_lora,build_modelfile}.py` + `to_gguf.sh`
- `training/src/parish_train/serve/inference_rejection_sampler.py`
- `training/scripts/{orchestrate,generate_synthetic_calibration,runpod_provision,cost_monitor,render_cards}.py` + `run_runpod.sh`
- `training/cards/{model_card_lora,model_card_gguf,model_card_oracle,dataset_card,dataset_card_raw,org_README}.md.j2` (Jinja templates)
- `training/data/manifest.toml` (URL + SHA-256 + license per source)
- `training/data/LICENSES.md`
- `training/LICENSES/GEMMA-4.txt` (verbatim Gemma 4 license + use-policy)
- `mods/rundale/scripts/prove_rundale_dialect.toml` (new `/prove` harness script — **must land in the same PR as the orchestrator** so Stage 5 has a target to invoke)
- `docs/adr/0NN-rundale-dialect-model.md`

**Modify:**
- `parish.example.toml` — add commented `[provider.dialogue]` example
- `docs/design/inference-pipeline.md` — add `gemma4-rundale:9b` to Dialogue recommendations
- `docs/adr/005-ollama-local-inference.md` — Specialist models subsection
- `docs/research/Irish-English-1820s-resources.md` — append outcome notes
- `parish-core` (wherever Dialogue system-prompt is assembled) — add the `rundale-dialect-model` flag check
- `parish-npc` Ollama call site — add the `inference-rejection-sampler` flag-gated wrapper
- `justfile` (top-level) — add the `train-rundale-dialect` recipe

## Verification — end-to-end

The canonical path is a single command:

```sh
cd /Users/dmooney/talkie
just train-rundale-dialect
# orchestrator: provisions RunPod pod → ingest → calibrate → SFT + dialect-oracle (parallel)
#               → calibration gate → iterated DPO → merge → GGUF q4_K_M → Ollama image
#               → /prove rundale-dialect → reports cost → tears down pod (or pauses on failure)
```

The orchestrator (see §Automation) checkpoints state per stage; a re-run resumes from the last good checkpoint.

### Manual / debugging path (single-stage invocation)

When iterating on a single stage, the orchestrator's per-stage targets can be invoked directly inside an existing pod:

```sh
# 0. one-time pod-side setup
cd /workspace/training && uv sync

# 1. ingest — ~50 sources across 9 register-tagged subcorpora.
#    Once the per-source modules are migrated to manifest-driven category fetchers
#    (see _MIGRATION_NOTE.md in src/parish_train/ingest/), this collapses to a single command:
#
#      uv run python -m parish_train.ingest                  # iterates manifest.toml
#
#    Until then, fetchers are invoked per-module within each category. Category groupings:
#      Literary core (13):     gutenberg_edgeworth, gutenberg_joyce, ia_griffin, ia_banim_1825,
#                              ia_banim_1826, ia_hall_sketches, gutenberg_carleton, ia_croker,
#                              ia_maxwell, ia_hall_lights, gutenberg_lever, gutenberg_lover, gutenberg_kickham
#      Travel-observer (6):    gutenberg_young, ia_carr, ia_inglis, ia_hall_scenery, ia_kohl, ia_nicholson
#      Folklore / oral (6):    ia_hardiman, ia_william_wilde, gutenberg_joyce_celtic,
#                              gutenberg_lady_wilde, gutenberg_curtin, gutenberg_hyde
#      Trial / commission (10):ht_devon_commission, ia_poor_inquiry, ht_state_trials, ht_friends_famine,
#                              ia_leadbeater, dataverse_boston_pilot, ia_whyte_diary, ia_bennett, ia_tuke,
#                              ia_nicholson_annals
#      First-person Irish (6): ia_carleton_autobio, ia_holt, ia_oconnell_corr, ia_tone, ia_byrne, ia_mitchel
#      Religious / clerical (5): ia_doyle_jkl, ia_cobbett_reformation, ia_ulster_revival_1859,
#                                ia_butler_catechism, ia_garden_of_soul
#      Periodicals (1):        gutenberg_irish_penny_journal
#      Stage-Irish rejected (8):gutenberg_boucicault, gutenberg_okeeffe, gutenberg_sheridan,
#                               ia_macklin, ia_tyrone_power_actor, ia_bayle_bernard,
#                               gutenberg_colman_younger, gutenberg_farquhar
#      Formal-contrast (6):    ia_murray_grammar, ia_cobbett_grammar, ia_walker_dictionary,
#                              ia_neilson_irish, ia_dilworth_speller, ia_ne_commissioners_lessons
#      Reference works (4):    ia_etiquette, ia_letter_writing, ia_almanac, ia_period_dict
#      Wordlist seeds (2):     ia_webster_1828, ia_wright_edd

# Curate + build (independent of which fetcher path was used):
uv run python -m parish_train.curate.dialogue_extractor
uv run python -m parish_train.curate.feature_tagger
uv run python -m parish_train.curate.dedupe
uv run python -m parish_train.build.instruction_pairs
uv run python -m parish_train.build.testimony_pairs           # Devon/Whately/State-Trials Q&A → instruction pairs
uv run python -m parish_train.build.reference_pairs           # etiquette/letter/almanac/dict → instruction pairs
uv run python -m parish_train.build.formal_contrast_pairs     # Murray/Cobbett/Walker → extends joyce_pairs.py
uv run python -m parish_train.build.stage_irish_synth         # synthesises stage-Irish caricature responses for DPO rejected pool
uv run python -m parish_train.build.anachronism_wordlist      # union(Webster 1828, Joyce 1910, Wright EDD) + blocklist → JSON
uv run python -m parish_train.build.split

# 2a. dialect oracle (parallel with 2b)
uv run python -m parish_train.train.train_dialect_oracle

# 2b. SFT
axolotl train configs/qlora_gemma4_9b.yaml

# 3. calibrate judges (halts on failure)
uv run python -m parish_train.eval.calibrate_judges

# 4. iterated DPO (2-3 rounds)
uv run python -m parish_train.eval.build_dpo_dataset --round 1
axolotl train configs/dpo_gemma4_rundale.yaml --round 1
uv run python -m parish_train.eval.build_dpo_dataset --round 2
axolotl train configs/dpo_gemma4_rundale.yaml --round 2

# 5. package
uv run python -m parish_train.package.merge_lora
bash src/parish_train/package/to_gguf.sh
ollama create gemma4-rundale:9b -f configs/modelfile.gemma4-rundale

# 6. wire + prove
cd /Users/dmooney/talkie
cp parish.example.toml parish.toml        # uncomment [provider.dialogue]
just check
/prove rundale-dialect
```

**Green bar to merge:**

1. `just check` passes.
2. `eval/rubric.py` reports ≥1 substrate feature / 30 tokens on the cottier slice and ≤0.05 anachronism rate overall.
3. Four-judge stack: candidate Borda-beats stock `gemma4:9b-it` on ≥70 % of held-out scenarios.
4. `/prove rundale-dialect` passes.
5. Manual A/B report shows the fine-tune is clearly more period-appropriate than stock `gemma4:9b-it` on ≥70 % of 30 paired prompts.

## Distribution & model cards

Three artifacts ship to public registries; one stays internal. Storage table:

| Artifact | Where | Card / README |
|---|---|---|
| Manifest + SHA-256 + `LICENSES.md` (URL+hash for raw corpora) | Git (`training/data/manifest.toml`) | inline doc |
| **Raw corpora snapshot** (deduped book/manual plaintext, pre-curation) | **HuggingFace Datasets** `rundale/dialect-corpus-raw-v{N}` | **raw-dataset card** (`cards/dataset_card_raw.md.j2`) |
| Local raw cache (`data/raw/`) | gitignored — populated from manifest (Gutenberg/IA primary) OR from `rundale/dialect-corpus-raw-v{N}` if upstream rotted | n/a |
| **Processed instruction-pair dataset** (train/val/test JSONL + DPO pairs + register tags) | **HuggingFace Datasets** `rundale/dialect-corpus-v{N}` | **dataset card** (`cards/dataset_card.md.j2`) |
| Synthetic calibration pairs | gitignored, regenerated each run | n/a |
| Dialect-oracle 250M | **HuggingFace Hub** `rundale/dialect-oracle-250m` | **model card** (`cards/model_card_oracle.md.j2`) — flagged "JUDGE ONLY, NOT FOR GENERATION" |
| QLoRA adapter (post-DPO) | **HuggingFace Hub** `rundale/gemma4-rundale-9b-lora` | **model card** (`cards/model_card_lora.md.j2`) |
| GGUF q4_K_M | **Ollama Registry** `rundale/gemma4-rundale:9b` + mirror to `rundale/gemma4-rundale-9b-gguf` on HF Hub | **model card** (`cards/model_card_gguf.md.j2`) |
| GGUF fp16 (intermediate) | not shipped — regenerable from adapter + base | n/a |
| Run logs + `state.json` | gitignored; optional S3 archive | n/a |

**Three-tier reproducibility chain.** Anyone re-deriving the model can pick the cheapest available step:

1. **Re-derive from upstream**: pull manifest from Git → fetch Gutenberg/IA per URL+SHA → curate → train. Cheapest source-of-truth path; depends on upstream URLs not rotting.
2. **Re-derive from raw snapshot**: pull `rundale/dialect-corpus-raw-v{N}` from HF Datasets → curate → train. Used when an upstream URL has 404'd or an Internet Archive identifier moved; the SHA-256 in `manifest.toml` lets the orchestrator detect the rot and silently fall back to the HF mirror without changing the curation step.
3. **Skip preprocessing**: pull `rundale/dialect-corpus-v{N}` directly → train. Trusts the published curation (regex + tagger + dedup + class assigner versions) and skips ~30 min of CPU work.

### Card content (mandatory sections)

Every card is rendered by `scripts/render_cards.py` from a template + the run's actual eval metrics, so numbers are never stale. Each model card MUST include:

1. **Base model + license inheritance** — Gemma 4's license + acceptable-use policy is replicated verbatim into the LoRA-adapter and GGUF cards. Cards link to the base model card on HF Hub.
2. **Intended use** — "NPC dialogue generation for the Rundale game (1820s rural County Roscommon, Ireland). Not validated for production use outside this game's dialogue context."
3. **Out-of-scope** — historical research, automated translation, non-Rundale game integration without separate eval.
4. **Training data** — corpus composition (13 literary + 6 travel + 6 folklore + 4 reference categories), mix percentages, attribution to each public-domain source. Links the HF dataset card.
5. **Training procedure** — QLoRA SFT → 2–3 rounds iterated DPO with four-axis Borda judge stack. Hyperparameters from `qlora_gemma4_9b.yaml` and `dpo_gemma4_rundale.yaml` embedded verbatim.
6. **Evaluation** — actual rubric scores (substrate-feature density / 30 tokens, anachronism rate), four-judge Borda head-to-head vs stock `gemma4:9b-it`, A/B win rate. **Numbers populated from `runs/{run_id}/eval_results.json`, not handwritten.**
7. **Limitations & biases** — modern base model's Standard English prior pulls outputs toward register-mixed period-flavored prose rather than fully substrate-native dialect; gentry register stronger than cottier (corpus skew); reproduces 1820s social hierarchies in dialogue (period-accurate, may surprise modern readers); not validated outside the Rundale gameplay context.
8. **Recommendations** — gate behind `rundale-dialect-model` flag; don't use as a general-purpose 1820s English generator.
9. **Citation** — BibTeX block referencing the training plan + commit SHA + run_id.
10. **Contact** — repository issue tracker.

### Dataset card additionally includes

- Per-source attribution table (URL + Internet Archive ID / Gutenberg #, public-domain status, register tag).
- Split definitions: which authors are oracle-side, SFT-side, eval-only (per `dialect_oracle_250m.yaml`).
- Pre-processing description: dialogue extraction regex set, feature-tagger thresholds, MinHashLSH dedup parameters, `class_assigner` rules.
- Known biases: Joyce 1910 retrospective (recalled childhood dialogue, not contemporaneous), Wright EDD post-dates the game setting (1898–1905), travel observers filter peasant speech through gentry framing.
- Distractor-generation models (Sonnet 4.6 for calibration) noted as a third-party API touchpoint — outputs not redistributed in the dataset, only used to score judge calibration.

### Dialect-oracle card additionally includes

- **JUDGE-ONLY warning at the top.** This model is a 250M-param scoring tool, not an instruction-following dialogue generator. Generating from it directly produces incoherent period-flavoured text.
- Author-level holdout split documented (which 7 authors trained on, which 6 held out).

### Stage 4 (orchestrator) — expanded

The orchestrator's Stage 4 (artifact upload) now does:

1. Render cards from templates: `scripts/render_cards.py` reads `runs/{run_id}/eval_results.json` + `runs/{run_id}/cost_summary.json` and produces filled cards.
2. Push the dataset to HF Datasets (`huggingface_hub` Python client) with the rendered card + revision tag = run_id.
3. Push the dialect oracle and the LoRA adapter to HF Hub, each with its rendered card.
4. Push the GGUF q4_K_M to HF Hub (mirror) and to Ollama Registry (`ollama push`).
5. Validate every push with a sanity load: `huggingface_hub.snapshot_download` round-trips and pulls each card; `ollama pull` round-trips the GGUF.
6. Surface URLs in the run-summary report.

### License & terms-of-service notes

- **Gemma 4 license** (Apache-2.0-flavoured with use-policy addendum): LoRA adapter and GGUF inherit. License + use-policy text is committed at `training/LICENSES/GEMMA-4.txt` and copied into every model card.
- **Public-domain training corpora** carry no redistribution restriction; per-source attribution is in `training/data/LICENSES.md` and the dataset card.
- **API distillation policy:** DeepSeek V4-pro and Sonnet 4.6 outputs are used as DPO scoring signal and as calibration distractors — not redistributed. Each provider's terms-of-service permits using outputs for training derivative models without redistribution of provider outputs themselves; the orchestrator does not write provider responses to the published dataset, only `(chosen, rejected)` pair *indices* derived from them.
- **`HF_TOKEN`** in `.env.example` must have **write access to the `rundale` HF org**. One-time org setup: create org `rundale` on huggingface.co, add the token user as a member with write permission. Orchestrator fails fast if push 401s with an explicit message naming this requirement.

## Methodology lineage

This plan ports two patterns from **Talkie-1930-13B** (Radford et al., April 2026):

1. **Reference-work-mined instruction pairs** — Talkie demonstrated that period instructional material (etiquette manuals, letter-writing manuals, almanacs, period dictionaries) yields high-signal supervision when programmatically converted into instruction pairs. The reference-pair sources in §Data ingestion adopt this pattern wholesale.
2. **Model-as-judge for DPO** — Talkie used itself as the period-fluency judge for its own preference data. Rundale adopts this *partially*: Talkie-1930-13B-IT is **one of three** period-axis judges, not the sole judge.

The reason for the partial adoption is important: Talkie's training distribution centres on **pre-1931 publishing**, which is dominated by Victorian / Edwardian Standard English (mass-market periodicals, Gutenberg's heaviest-represented decades). 1820s Roscommon **cottier** Hiberno-English — Irish-substrate grammar with phonetic spelling and code-switching — is a tiny minority within that prior, and Talkie's log-likelihood will systematically prefer cleaner, more standard period prose over substrate-marked dialect. The deterministic anachronism wordlist anchors the lower bound on period-correctness, the tiny ~250M dialect oracle (trained on the literary corpus Talkie under-weights, with a fully disjoint author split — see Stage 0) supplies the cottier-specific prior, and Talkie picks up the rest. Borda-aggregating the three keeps any one judge from dominating.

The base model remains Gemma 4 9B IT — the period prior is added via SFT and DPO, not by adopting Talkie as the policy.

## Automation

A single command (`just train-rundale-dialect`) drives the full pipeline. Implementation in `training/scripts/orchestrate.py`.

### Orchestrator responsibilities

1. **Read run config** from `training/configs/rundale_dialect_e2e.yaml` (model, corpus paths, judge stack, cost caps, artifact destination, page channel).
2. **Provision RunPod A100-80GB pod** via REST API using `RUNPOD_API_KEY`. Pre-baked image (`rundale/training:latest`, built from `training/docker/Dockerfile.training`) ships with axolotl + bitsandbytes + transformers + trl + llama.cpp + ollama + uv.
3. **Drive each stage** as a subprocess on the pod:
   - **Stage −1**: ingest all corpora (Gutenberg + Internet Archive sources)
   - **Stage 0a**: synthetic-calibration generation (see below)
   - **Stage 0b**: dialect-oracle pretraining on author-level holdout (parallel with Stage 1)
   - **Stage 1**: Gemma 4 9B QLoRA SFT
   - **Stage 1.5**: judge calibration gate (≥80 % direction-correct on synthetic calibration set; halt if any judge fails; auto-fallback DeepSeek → Sonnet 4.6 with Gemini 2.5 Flash distractor regeneration if coherence axis fails — see §Evaluation)
   - **Stage 2**: iterated DPO (2–3 rounds) with combined four-axis judge stack (Borda over anachronism + Talkie + dialect-oracle + DeepSeek)
   - **Stage 3**: peft merge → fp16 GGUF → q4_K_M GGUF → Ollama image build
   - **Stage 4**: render model + dataset cards from `cards/*.md.j2` templates with run-specific eval metrics + cost data; push to HF Hub (org `rundale`: raw-corpus dataset, processed dataset, dialect-oracle, LoRA adapter, GGUF mirror) + Ollama registry (`rundale/gemma4-rundale:9b`); round-trip-validate each push
   - **Stage 5**: run `/prove rundale-dialect` against the new artifact; capture pass/fail
4. **Checkpoint** state to `runs/{run_id}/state.json` after each stage. On re-run, resume from last good checkpoint — a Stage-2 retry must not re-run Stages −1, 0, or 1.
5. **Stream logs** to `runs/{run_id}/stage_{n}_{name}.log`; tail to user terminal in real time.
6. **Cost tracking**: RunPod billing API (GPU-hours) + DeepSeek usage API (tokens) + Anthropic usage API (Sonnet calibration tokens) + Gemini usage API (only on calibration fallback). Hard cap per run (default **$100**). On breach: halt and page.
7. **Per-stage timeouts**: default 12 h SFT, **12 h DPO/iteration** (sized for 4800–7200 candidate generations × 2–3 rounds at N=8 × ~300 held-out scenarios), 8 h dialect oracle. **Total wall-clock cap default 48 h** (raised from 36 h to absorb the DPO bump without forcing a manual override on every full run).
8. **Teardown policy**: auto-destroy pod on full success; **pause pod on any failure** for inspection, with a **24-hour auto-destroy** to bound runaway cost.

### Synthetic-calibration generator

`training/scripts/generate_synthetic_calibration.py` runs as part of every full pipeline invocation (not pre-cached) so calibration always reflects the current judge state.

- **Period-axis pairs**: sample 200 spans from the literary corpus → **Sonnet 4.6** (batch + cache) rewrites each in modern English → pair `(original=correct, modernized=incorrect)`. The three period-axis judges (`judge_anachronism`, `judge_talkie`, `judge_dialect_oracle`) score both halves; correct direction = judge prefers `original`. None of those judges are Anthropic models, so the construction is non-circular.
- **Coherence-axis pairs**: sample 200 in-character NPC dialogues from `parish/testing/fixtures/` → **Sonnet 4.6** corrupts each (random mood / character / anachronism injection) → pair `(original=correct, corrupted=incorrect)`. `judge_deepseek` (DeepSeek V4-pro) scores both halves — distractor model and judge model are unrelated providers, so the construction is non-circular.
- **Fallback path** (coherence-axis calibration failure): orchestrator switches the *judge* to Sonnet 4.6 batch+cache **and** switches the *coherence-axis distractor generator* to Gemini 2.5 Flash, preserving the distinct-model invariant.
- Pass criterion = ≥80 % direction-correct on each axis.
- Cost: ~$2 Sonnet (batch+cache) + ~$1 DeepSeek + ~10 min pod time per run; fallback path adds ~$1 Gemini.

### User-facing surface

- **`just train-rundale-dialect`** (top-level justfile): `uv run --project training python training/scripts/orchestrate.py`.
- **`training/.env.example`** lists required secrets: `RUNPOD_API_KEY`, `DEEPSEEK_API_KEY`, `ANTHROPIC_API_KEY` (Sonnet 4.6 — calibration distractors + judge fallback), `HF_TOKEN`. Optional: `GEMINI_API_KEY` (only used on coherence-axis calibration failure fallback). Orchestrator fails fast on a missing required secret with an explicit env-var name.
- **Returns**: success (artifact location + cost summary) or failure (paused-pod inspection URL + log paths + reason).

### Safety rails

All configured in `training/configs/rundale_dialect_e2e.yaml`:

- Hard **cost cap** (default $100), per-stage **timeouts** (12 h SFT, 12 h DPO/round, 8 h dialect oracle), total **wall-clock cap** (default **48 h**).
- **On-breach behaviour**: halt → page → pause pod → 24 h auto-destroy.
- **Page channel** configurable: stdout / Slack webhook / email.

### Irreducibly manual

- One-time secret provisioning (`RUNPOD_API_KEY`, `DEEPSEEK_API_KEY`, `ANTHROPIC_API_KEY`, `HF_TOKEN`; `GEMINI_API_KEY` if you want the calibration-failure fallback to work without manual intervention).
- Reviewing the `/prove rundale-dialect` verdict if it fails — orchestrator surfaces the failure but does not auto-modify code in response.
- DeepSeek discount renewal post-2026-05-31 — orchestrator surfaces post-deadline cost as a warning before each run starts.
