<script>
  // Sortable dialogue-quality leaderboard. Click a header to sort; click again
  // to flip direction. Props: rows (array), base (site base path for links).
  import { brandForFamily } from '../lib/brands';

  let { rows = [], base = '' } = $props();

  const AXES = ['character', 'authenticity', 'language', 'responsiveness', 'craft'];
  let sortKey = $state('overall');
  let dir = $state(-1); // -1 desc, 1 asc

  const sorted = $derived(
    [...rows].sort((a, b) => {
      const av = a[sortKey], bv = b[sortKey];
      if (typeof av === 'number' && typeof bv === 'number') return (av - bv) * dir;
      return String(av ?? '').localeCompare(String(bv ?? '')) * dir;
    })
  );

  function sortBy(key) {
    if (sortKey === key) dir = -dir;
    else { sortKey = key; dir = key === 'model_id' ? 1 : -1; }
  }
  const arrow = (key) => (sortKey === key ? (dir === 1 ? ' ▲' : ' ▼') : '');
  const fmt = (v) => (typeof v === 'number' ? v.toFixed(2) : '—');
</script>

{#if rows.length === 0}
  <div class="empty">No judged dialogue runs yet. Run the funnel:
    <code>run --tier screen</code> → <code>drain-queue</code> → <code>ingest</code>.</div>
{:else}
  <table>
    <thead>
      <tr>
        <th onclick={() => sortBy('model_id')}>Model{arrow('model_id')}</th>
        <th class="num" onclick={() => sortBy('overall')}>Overall{arrow('overall')}</th>
        {#each AXES as ax}
          <th class="num" onclick={() => sortBy(ax)}>{ax.slice(0, 4)}{arrow(ax)}</th>
        {/each}
        <th class="num" onclick={() => sortBy('judged')}>n{arrow('judged')}</th>
      </tr>
    </thead>
    <tbody>
      {#each sorted as r}
        {@const brand = brandForFamily(r.family)}
        <tr>
          <td>
            {#if brand?.kind === 'icon'}
              <svg class="brand-icon" viewBox="0 0 24 24" width="16" height="16" role="img" aria-label={brand.title} style={`fill: #${brand.hex}`}>
                <title>{brand.title}</title>
                <path d={brand.svgPath} />
              </svg>
            {:else if brand?.kind === 'fallback'}
              <span class="brand-fallback" role="img" aria-label={brand.title} title={brand.title} style={`background:#${brand.hex}`}>{brand.initials}</span>
            {/if}
            <a href={`${base}/models/${r.slug}`} title={r.model_id}>{r.display_name ?? r.model_id}</a>
            {#if r.judge_id}<span class="badge" title="judge model" style="margin-left:0.3rem">⚖ {r.judge_id}</span>{/if}
            {#if r.bench_bugs > 0}<span class="badge" title="responses the judge flagged as bench-harness bugs (chain-of-thought leak / blank reply) and excluded from the score" style="margin-left:0.3rem;background:#fde68a;color:#92400e">🐛 {r.bench_bugs} bug{r.bench_bugs === 1 ? '' : 's'}</span>{/if}</td>
          <td class="num"><strong>{fmt(r.overall)}</strong></td>
          {#each AXES as ax}<td class="num">{fmt(r[ax])}</td>{/each}
          <td class="num">{r.judged ?? '—'}</td>
        </tr>
      {/each}
    </tbody>
  </table>
{/if}

<style>
  :global(.brand-icon), :global(.brand-fallback) {
    display: inline-block;
    vertical-align: -2px;
    margin-right: 0.4em;
    flex-shrink: 0;
  }
  :global(.brand-fallback) {
    width: 16px;
    height: 16px;
    color: #fff;
    border-radius: 999px;
    font-size: 0.62rem;
    font-weight: 600;
    line-height: 1;
    text-align: center;
    padding: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    letter-spacing: 0.02em;
  }
</style>
