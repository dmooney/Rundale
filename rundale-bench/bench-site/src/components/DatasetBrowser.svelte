<script>
  // Filterable dataset record list. Props: records (array), slice (string).
  let { records = [], slice = '' } = $props();

  let q = $state('');
  let openId = $state(null);

  const promptText = (r) => r.prompt ?? r.system_template ?? JSON.stringify(r);

  const filtered = $derived(
    records.filter((r) => {
      if (!q) return true;
      const needle = q.toLowerCase();
      return (r.id ?? '').toLowerCase().includes(needle)
          || promptText(r).toLowerCase().includes(needle);
    })
  );

  const trunc = (s, n) => (s.length > n ? s.slice(0, n) + '…' : s);
</script>

<input
  type="search"
  placeholder={`filter ${slice} by id or prompt text…`}
  bind:value={q}
  style="width:100%;padding:0.5rem;background:var(--color-input-bg);border:1px solid var(--color-border);color:var(--color-fg);border-radius:4px;font:inherit;margin-bottom:0.75rem"
/>
<p class="muted">{filtered.length} of {records.length} record(s)</p>

<table>
  <thead>
    <tr><th style="width:11rem">ID</th><th>Prompt</th>{#if records[0]?.tier}<th style="width:6rem">tier</th>{/if}</tr>
  </thead>
  <tbody>
    {#each filtered as r}
      <tr style="cursor:pointer" onclick={() => openId = openId === r.id ? null : r.id}>
        <td><code>{r.id}</code></td>
        <td>{openId === r.id ? promptText(r) : trunc(promptText(r), 120)}</td>
        {#if records[0]?.tier}<td><span class="badge">{r.tier ?? '—'}</span></td>{/if}
      </tr>
      {#if openId === r.id}
        <tr>
          <td colspan="3" style="background:var(--color-input-bg);font-family:ui-monospace,Menlo,monospace;font-size:0.85rem;white-space:pre-wrap">{JSON.stringify(r, null, 2)}</td>
        </tr>
      {/if}
    {/each}
  </tbody>
</table>
