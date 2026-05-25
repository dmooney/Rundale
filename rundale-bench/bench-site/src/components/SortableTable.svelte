<script>
  // Generic sortable table with pre-rendered HTML cells.
  //
  // Astro's `client:load` boundary JSON-serializes props, so column
  // `render`/`value` functions get stripped. We move all formatting into
  // the .astro frontmatter and pass two parallel maps per row instead:
  //
  //   columns:   [{ key, header, num? }]
  //   rows:      [{ sort: {<key>: any}, html: {<key>: string} }]
  //   initialSort?: { key, dir: 1 | -1 }
  //
  // `sort[key]` is the value used to order rows (number / string / null).
  // `html[key]` is the trusted HTML rendered into the <td> via `{@html}`.
  // Missing values (null / undefined / NaN) always sink to the bottom
  // regardless of direction, so "no data" never wins.
  let { columns = [], rows = [], initialSort = null } = $props();

  function defaultSort() {
    if (initialSort) return initialSort;
    const firstNum = columns.find((c) => c.num);
    if (firstNum) return { key: firstNum.key, dir: -1 };
    return { key: columns[0]?.key, dir: 1 };
  }

  let state = $state(defaultSort());

  function sortValue(row, key) {
    return row?.sort?.[key];
  }

  function cellHtml(row, key) {
    const v = row?.html?.[key];
    return v === undefined || v === null ? '—' : String(v);
  }

  const sorted = $derived.by(() => {
    const key = state.key;
    if (!key) return rows;
    const dir = state.dir;
    return [...rows].sort((a, b) => {
      const av = sortValue(a, key);
      const bv = sortValue(b, key);
      const aMissing = av === null || av === undefined || (typeof av === 'number' && Number.isNaN(av));
      const bMissing = bv === null || bv === undefined || (typeof bv === 'number' && Number.isNaN(bv));
      if (aMissing && bMissing) return 0;
      if (aMissing) return 1;
      if (bMissing) return -1;
      if (typeof av === 'number' && typeof bv === 'number') return (av - bv) * dir;
      return String(av).localeCompare(String(bv)) * dir;
    });
  });

  function sortBy(key) {
    if (state.key === key) {
      state = { key, dir: -state.dir };
    } else {
      const col = columns.find((c) => c.key === key);
      state = { key, dir: col?.num ? -1 : 1 };
    }
  }

  const arrow = (key) => (state.key === key ? (state.dir === 1 ? ' ▲' : ' ▼') : '');
</script>

<div class="table-scroll">
  <table>
    <thead>
      <tr>
        {#each columns as col (col.key)}
          <th class={col.num ? 'num sortable' : 'sortable'} onclick={() => sortBy(col.key)} title="Click to sort">
            {col.header}{arrow(col.key)}
          </th>
        {/each}
      </tr>
    </thead>
    <tbody>
      {#each sorted as row, i (i)}
        <tr>
          {#each columns as col (col.key)}
            <td class={col.num ? 'num' : ''}>{@html cellHtml(row, col.key)}</td>
          {/each}
        </tr>
      {/each}
    </tbody>
  </table>
</div>

<style>
  .sortable { cursor: pointer; user-select: none; white-space: nowrap; }
  .sortable:hover { background: var(--gray-100, #f3f4f6); }
</style>
