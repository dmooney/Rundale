<script>
  // v2 benchmark-of-record leaderboard table. Sortable, searchable,
  // tier/category filterable, with in-cell score bars and LMArena-style
  // CI-aware rank bands. Props: rows (BoardRow[]), categoryOrder (string[]),
  // base (site base path).
  import { brandForFamily } from '../lib/brands';
  import { fmt, fmtMoneyHour, fmtValue, fmtCi, tierClass } from '../lib/render';

  let { rows = [], categoryOrder = [], base = '' } = $props();

  const TIER_ORDER = ['free', 'budget', 'mid', 'premium'];
  const tiers = $derived(TIER_ORDER.filter((t) => rows.some((r) => r.tier === t)));

  let activeTiers = $state(new Set());
  let query = $state('');
  let focusCat = $state('');
  let sortKey = $state('overall');
  let dir = $state(-1);

  function toggleTier(t) {
    const next = new Set(activeTiers);
    next.has(t) ? next.delete(t) : next.add(t);
    activeTiers = next;
  }
  function focus(cat) {
    focusCat = cat;
    if (cat === 'value') sortBy('value_score');
    else if (cat) sortBy(`cat:${cat}`);
    else sortBy('overall');
  }
  function valueOf(r, key) {
    if (key === 'rank') return -r.rank; // lower rank = better, sort desc by default
    if (key === 'model') return r.display_name;
    if (key === 'tier') return r.tier;
    if (key.startsWith('cat:')) return r.categories[key.slice(4)]?.score ?? null;
    return r[key];
  }
  function sortBy(key) {
    if (sortKey === key) dir = -dir;
    else { sortKey = key; dir = key === 'model' ? 1 : -1; }
  }
  const arrow = (key) => (sortKey === key ? (dir === 1 ? ' ↑' : ' ↓') : '');

  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return rows.filter((r) => {
      if (activeTiers.size && !activeTiers.has(r.tier)) return false;
      if (q) {
        const hay = `${r.display_name} ${r.family ?? ''} ${r.model ?? ''} ${r.candidate}`.toLowerCase();
        if (!hay.includes(q)) return false;
      }
      return true;
    });
  });

  const sorted = $derived(
    [...filtered].sort((a, b) => {
      const av = valueOf(a, sortKey), bv = valueOf(b, sortKey);
      const an = typeof av === 'number', bn = typeof bv === 'number';
      if (!an && !bn) return String(av ?? '').localeCompare(String(bv ?? '')) * dir;
      if (!an) return 1;
      if (!bn) return -1;
      return (av - bv) * dir;
    })
  );

  // per-column max (over the filtered set) → relative bar widths + leader.
  const colMax = $derived.by(() => {
    const m = { overall: 0 };
    for (const c of categoryOrder) m[`cat:${c}`] = 0;
    for (const r of filtered) {
      if (r.overall > m.overall) m.overall = r.overall;
      for (const c of categoryOrder) {
        const s = r.categories[c]?.score;
        if (s != null && s > m[`cat:${c}`]) m[`cat:${c}`] = s;
      }
    }
    return m;
  });
  const pct = (key, v) => (typeof v === 'number' && colMax[key] ? Math.max(6, (v / colMax[key]) * 100) : 0);
  const isLeader = (key, v) => typeof v === 'number' && v === colMax[key] && v > 0;
</script>

{#if rows.length === 0}
  <div class="empty">
    <strong>No funded benchmark run committed yet.</strong>
    <p class="muted">Live against the v2 schema; populates when the first
      <code>promptfoo/leaderboard/leaderboard.jsonl</code> run lands
      (see <code>promptfoo/RESUME.md</code>).</p>
  </div>
{:else}
  <div class="controls">
    <div class="grp">
      <input class="search" type="search" placeholder="Search model / family…" bind:value={query} />
    </div>
    <div class="grp">
      <span class="lbl">Tier</span>
      {#each tiers as t}
        <button class="chip {tierClass(t)}" class:active={activeTiers.has(t)} onclick={() => toggleTier(t)}>{t}</button>
      {/each}
      {#if activeTiers.size}<button class="chip clear" onclick={() => (activeTiers = new Set())}>clear</button>{/if}
    </div>
    <div class="grp">
      <span class="lbl">Best for</span>
      <select bind:value={focusCat} onchange={(e) => focus(e.currentTarget.value)}>
        <option value="">overall</option>
        {#each categoryOrder as c}<option value={c}>{c}</option>{/each}
        <option value="value">value</option>
      </select>
      {#if focusCat && sorted.length}<span class="muted lead">→ {sorted[0].display_name}</span>{/if}
    </div>
    <div class="grp count"><span class="muted">{sorted.length} / {rows.length}</span></div>
  </div>

  <div class="table-scroll">
    <table>
      <thead>
        <tr>
          <th class="num" onclick={() => sortBy('rank')} title="CI-aware rank: rises by one for every model whose lower CI exceeds this model's upper CI. Tied = overlapping CIs.">#{arrow('rank')}</th>
          <th onclick={() => sortBy('model')}>Model{arrow('model')}</th>
          <th class="num" onclick={() => sortBy('overall')}>Overall{arrow('overall')}</th>
          {#each categoryOrder as c}
            <th class="num" onclick={() => sortBy(`cat:${c}`)}>{c.slice(0, 4)}{arrow(`cat:${c}`)}</th>
          {/each}
          <th class="num" onclick={() => sortBy('usd_per_game_hour')} title="USD per hour of normal-play gameplay">$/hr{arrow('usd_per_game_hour')}</th>
          <th class="num" onclick={() => sortBy('value_score')} title="overall ÷ $/game-hour">Value{arrow('value_score')}</th>
          <th class="num" onclick={() => sortBy('latency_p50_ms')}>p50{arrow('latency_p50_ms')}</th>
          <th class="num" onclick={() => sortBy('latency_p95_ms')}>p95{arrow('latency_p95_ms')}</th>
        </tr>
      </thead>
      <tbody>
        {#each sorted as r}
          {@const brand = brandForFamily(r.family)}
          <tr>
            <td class="num"><span class="rank" class:tie={r.tied}>{r.rank}{r.tied ? '⁼' : ''}</span></td>
            <td>
              <span class="model-cell">
                {#if brand?.kind === 'icon'}
                  <svg class="brand-icon" class:brand-dark={brand.lowLum} viewBox="0 0 24 24" width="18" height="18" role="img" aria-label={brand.title} style={`fill: #${brand.hex}`}><title>{brand.title}</title><path d={brand.svgPath} /></svg>
                {:else if brand?.kind === 'fallback'}
                  <span class="brand-fallback" role="img" aria-label={brand.title} title={brand.title} style={`background:#${brand.hex}`}>{brand.initials}</span>
                {/if}
                <a href={`${base}/models/${r.slug}`} title={r.candidate}>{r.display_name}</a>
                {#if r.tier}<span class="badge {tierClass(r.tier)}">{r.tier}</span>{/if}
                {#if r.onFrontier}<span class="frontier-dot" title="on the quality/cost efficiency frontier">◆</span>{/if}
              </span>
            </td>
            <td class="num" class:leader={isLeader('overall', r.overall)}>
              <span class="scorecell"><span class="bar"><span style={`width:${pct('overall', r.overall)}%`}></span></span><span class="n"><strong>{fmt(r.overall)}</strong></span></span>
              <span class="ci">{fmtCi(r.overall_ci95)}</span>
            </td>
            {#each categoryOrder as c}
              {@const cs = r.categories[c]}
              <td class="num" class:leader={cs && isLeader(`cat:${c}`, cs.score)} title={cs ? `n=${cs.n}, CI ${fmtCi(cs.ci95)}` : 'not evaluated'}>
                {#if cs}<span class="scorecell"><span class="bar"><span style={`width:${pct(`cat:${c}`, cs.score)}%`}></span></span><span class="n">{fmt(cs.score)}</span></span>{:else}<span class="muted">—</span>{/if}
              </td>
            {/each}
            <td class="num">{fmtMoneyHour(r.usd_per_game_hour)}</td>
            <td class="num">{fmtValue(r.value_score)}</td>
            <td class="num">{r.latency_p50_ms ?? '—'}</td>
            <td class="num">{r.latency_p95_ms ?? '—'}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}

<style>
  .controls .count { margin-left: auto; }
  .lead { font-size: 0.8rem; }
  .frontier-dot { color: var(--accent); font-size: 0.7rem; cursor: help; }
  .badge { margin-left: 0.1rem; }
</style>
