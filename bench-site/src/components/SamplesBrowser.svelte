<script>
  // Per-model prompt/reply browser with inline judge scores.
  let { items = [] } = $props();

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

  const AXES = ['character', 'authenticity', 'language', 'responsiveness', 'craft'];
  const fmt = (n) => (typeof n === 'number' ? n.toFixed(1) : '—');
  const trunc = (s, n) => (s.length > n ? s.slice(0, n) + '…' : s);
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
    <div style="display:flex;justify-content:space-between;align-items:baseline;gap:0.7rem">
      <code>{it.id}</code>
      <div>
        {#each AXES as ax}
          <span class="badge" style="margin-left:0.3rem" title={ax}>{ax.slice(0,4)} {fmt(it.axes?.[ax])}</span>
        {/each}
        <span class="badge" style="margin-left:0.3rem"><strong>overall {fmt(it.overall)}</strong></span>
      </div>
    </div>
    <div style="margin-top:0.5rem">
      <strong>Prompt:</strong> {trunc(it.prompt, 220)}
    </div>
    <div style="margin-top:0.4rem">
      <strong>Reply:</strong>
      {#if openId === it.id}
        <div style="white-space:pre-wrap">{it.reply}</div>
        <button class="theme-toggle" style="margin-top:0.4rem" onclick={() => openId = null}>hide</button>
      {:else}
        {trunc(it.reply, 240)}
        <button class="theme-toggle" style="margin-left:0.4rem" onclick={() => openId = it.id}>full</button>
      {/if}
    </div>
    {#if it.non_latin_chars && Object.keys(it.non_latin_chars).length > 0}
      <div style="margin-top:0.4rem;color:#a33"><strong>⚠ non-Latin:</strong> {JSON.stringify(it.non_latin_chars)}</div>
    {/if}
  </div>
{/each}
