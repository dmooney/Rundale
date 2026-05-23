<script>
  // Sortable dialogue-quality leaderboard. Click a header to sort; click again
  // to flip direction. Props: rows (array), base (site base path for links).
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
        <tr>
          <td><a href={`${base}/models/${r.slug}`}>{r.model_id}</a>
            {#if r.family && r.family !== 'unknown'}<span class="badge">{r.family}</span>{/if}</td>
          <td class="num"><strong>{fmt(r.overall)}</strong></td>
          {#each AXES as ax}<td class="num">{fmt(r[ax])}</td>{/each}
          <td class="num">{r.judged ?? '—'}</td>
        </tr>
      {/each}
    </tbody>
  </table>
{/if}
