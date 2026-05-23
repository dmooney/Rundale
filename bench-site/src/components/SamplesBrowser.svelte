<script>
  // Per-(model, slice) prompt/reply/judgement browser. Axis list is data-driven
  // so dialogue, gaeilge, and future slices reuse this without code changes.
  let { items = [], axes = [] } = $props();

  let q = $state('');
  let openId = $state(null);

  const filtered = $derived(
    items.filter((it) => {
      if (!q) return true;
      const n = q.toLowerCase();
      return (it.id ?? '').toLowerCase().includes(n)
          || (it.prompt ?? '').toLowerCase().includes(n)
          || (it.reply ?? '').toLowerCase().includes(n);
    })
  );

  const fmt = (n) => (typeof n === 'number' ? n.toFixed(1) : '—');
  const trunc = (s, n) => (s.length > n ? s.slice(0, n) + '…' : s);
  const shortAxis = (a) => (a.length > 5 ? a.slice(0, 4) : a);
</script>

<input
  type="search"
  placeholder="filter by id, prompt, or reply text…"
  bind:value={q}
  style="width:100%;padding:0.5rem;background:var(--color-input-bg);border:1px solid var(--color-border);color:var(--color-fg);border-radius:4px;font:inherit;margin-bottom:0.75rem"
/>
<p class="muted">{filtered.length} of {items.length} prompt(s)</p>

{#each filtered as it}
  <div style="margin:0.7rem 0;padding:0.7rem 0.9rem;background:var(--color-panel-bg);border:1px solid var(--color-border);border-radius:6px">
    <div style="display:flex;justify-content:space-between;align-items:baseline;gap:0.7rem;flex-wrap:wrap">
      <code>{it.id}</code>
      <div>
        {#each axes as ax}
          <span class="badge" style="margin-left:0.3rem" title={ax}>{shortAxis(ax)} {fmt(it.axes?.[ax])}</span>
        {/each}
        <span class="badge" style="margin-left:0.3rem"><strong>overall {fmt(it.overall)}</strong></span>
      </div>
    </div>
    {#if it.prompt}
      <div style="margin-top:0.5rem"><strong>Prompt:</strong> {trunc(it.prompt, 240)}</div>
    {/if}
    <div style="margin-top:0.4rem">
      <strong>Reply:</strong>
      {#if openId === it.id}
        <div style="white-space:pre-wrap;margin-top:0.2rem">{it.reply}</div>
        <button class="theme-toggle" style="margin-top:0.4rem" onclick={() => openId = null}>hide</button>
      {:else}
        {trunc(it.reply, 260)}
        <button class="theme-toggle" style="margin-left:0.4rem" onclick={() => openId = it.id}>full</button>
      {/if}
    </div>
    {#if it.reason}
      <div style="margin-top:0.4rem;font-style:italic;color:var(--color-muted)">
        <strong>Judge:</strong> {it.reason}
      </div>
    {/if}
    {#if it.english_leakage_examples && it.english_leakage_examples.length > 0}
      <div style="margin-top:0.3rem"><strong>English leakage:</strong> {it.english_leakage_examples.join(' · ')}</div>
    {/if}
    {#if it.non_latin_chars && Object.keys(it.non_latin_chars).length > 0}
      <div style="margin-top:0.4rem;color:#a33"><strong>⚠ non-Latin:</strong> {JSON.stringify(it.non_latin_chars)}</div>
    {/if}
  </div>
{/each}
