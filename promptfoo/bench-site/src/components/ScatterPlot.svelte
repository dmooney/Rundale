<script>
  // Quality-vs-cost (and quality-vs-speed) scatter with a Pareto efficiency
  // frontier. Hand-rolled inline SVG — no chart lib, themes via CSS vars, and
  // scales with its viewBox. Inspired by Artificial Analysis's intelligence-vs-
  // price chart. Props: rows (BoardRow[]), base (site base for detail links).
  let { rows = [], base = '' } = $props();

  // x-axis mode: 'cost' = $/game-hour (log), 'speed' = tokens/sec (linear).
  let mode = $state('cost');

  const W = 880, H = 380;
  const M = { t: 18, r: 24, b: 48, l: 52 };
  const plotW = W - M.l - M.r;
  const plotH = H - M.t - M.b;
  // free models live in a dedicated lane left of the log axis (log can't take 0).
  const FREE_LANE = 34;

  const tierColor = (t) =>
    ({ free: 'var(--tier-free)', budget: 'var(--tier-budget)', mid: 'var(--tier-mid)', premium: 'var(--tier-premium)' })[t] ||
    'var(--muted)';

  // y domain: overall, padded to nice half-steps within [1,5].
  const yDom = $derived.by(() => {
    const vs = rows.map((r) => r.overall);
    const lo = Math.max(1, Math.floor((Math.min(...vs) - 0.25) * 2) / 2);
    const hi = Math.min(5, Math.ceil((Math.max(...vs) + 0.25) * 2) / 2);
    return lo < hi ? [lo, hi] : [1, 5];
  });
  const yPx = (v) => M.t + plotH - ((v - yDom[0]) / (yDom[1] - yDom[0])) * plotH;

  // x value per mode. cost uses log10 of positive prices; free => null (lane).
  const xVal = (r) => (mode === 'cost' ? r.usd_per_game_hour : r.tokens_per_sec);
  const paidXs = $derived(rows.map(xVal).filter((v) => typeof v === 'number' && v > 0));
  const xDom = $derived.by(() => {
    if (!paidXs.length) return [1, 10];
    if (mode === 'cost') {
      const lo = Math.log10(Math.min(...paidXs));
      const hi = Math.log10(Math.max(...paidXs));
      const pad = (hi - lo) * 0.08 || 0.3;
      return [lo - pad, hi + pad];
    }
    const hi = Math.max(...paidXs);
    return [0, hi * 1.08];
  });
  function xPx(r) {
    const v = xVal(r);
    if (mode === 'cost') {
      if (!v || v <= 0) return M.l + FREE_LANE / 2; // free lane center
      const lx = Math.log10(v);
      return M.l + FREE_LANE + ((lx - xDom[0]) / (xDom[1] - xDom[0])) * (plotW - FREE_LANE);
    }
    if (!v) return M.l;
    return M.l + (v / xDom[1]) * plotW;
  }

  // axis ticks
  const yTicks = $derived.by(() => {
    const out = [];
    for (let v = yDom[0]; v <= yDom[1] + 1e-9; v += 0.5) out.push(+v.toFixed(1));
    return out;
  });
  const xTicks = $derived.by(() => {
    if (!paidXs.length) return [];
    if (mode === 'cost') {
      const lo = Math.ceil(xDom[0]), hi = Math.floor(xDom[1]);
      const out = [];
      for (let e = lo; e <= hi; e++) out.push({ x: M.l + FREE_LANE + ((e - xDom[0]) / (xDom[1] - xDom[0])) * (plotW - FREE_LANE), label: fmtCost(Math.pow(10, e)) });
      return out;
    }
    const out = [];
    const step = niceStep(xDom[1] / 5);
    // guard: a non-positive / non-finite step would loop forever
    if (!(step > 0) || !isFinite(step)) return out;
    for (let v = 0; v <= xDom[1] && out.length < 100; v += step)
      out.push({ x: M.l + (v / xDom[1]) * plotW, label: Math.round(v) });
    return out;
  });
  function niceStep(raw) {
    const p = Math.pow(10, Math.floor(Math.log10(raw)));
    const n = raw / p;
    return (n >= 5 ? 5 : n >= 2 ? 2 : 1) * p;
  }
  function fmtCost(v) {
    if (v >= 1) return '$' + v.toFixed(0);
    if (v >= 0.01) return '$' + v.toFixed(2);
    return '$' + v.toFixed(3);
  }

  // frontier polyline: frontier rows sorted by x (free first), connected.
  const frontierPath = $derived.by(() => {
    const fr = rows
      .filter((r) => r.onFrontier)
      .slice()
      .sort((a, b) => xPx(a) - xPx(b));
    return fr.map((r) => `${xPx(r).toFixed(1)},${yPx(r.overall).toFixed(1)}`).join(' ');
  });

  let hover = $state(null); // hovered row
  const short = (r) => (r.display_name.length > 18 ? r.display_name.slice(0, 17) + '…' : r.display_name);
</script>

<div class="sc-controls">
  <button class="chip" class:active={mode === 'cost'} onclick={() => (mode = 'cost')}>Quality vs cost</button>
  <button class="chip" class:active={mode === 'speed'} onclick={() => (mode = 'speed')}>Quality vs speed</button>
  <span class="muted sc-help">Dashed line = efficiency frontier (best quality at each cost). Dot colour = cost tier.</span>
</div>

<div class="scatter-wrap">
  <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="xMidYMid meet" role="img" aria-label="quality versus cost scatter">
    <!-- grid + y axis -->
    {#each yTicks as t}
      <line class="sc-grid" x1={M.l} y1={yPx(t)} x2={W - M.r} y2={yPx(t)} />
      <text class="sc-tick" x={M.l - 8} y={yPx(t) + 4} text-anchor="end">{t.toFixed(1)}</text>
    {/each}
    <line class="sc-axis" x1={M.l} y1={M.t} x2={M.l} y2={M.t + plotH} />
    <line class="sc-axis" x1={M.l} y1={M.t + plotH} x2={W - M.r} y2={M.t + plotH} />

    <!-- free lane divider (cost mode) -->
    {#if mode === 'cost' && paidXs.length}
      <line class="sc-grid" x1={M.l + FREE_LANE} y1={M.t} x2={M.l + FREE_LANE} y2={M.t + plotH} />
      <text class="sc-tick" x={M.l + FREE_LANE / 2} y={M.t + plotH + 16} text-anchor="middle">free</text>
    {/if}

    <!-- x ticks -->
    {#each xTicks as t}
      <text class="sc-tick" x={t.x} y={M.t + plotH + 16} text-anchor="middle">{t.label}</text>
    {/each}

    <!-- axis labels -->
    <text class="sc-axis-label" x={M.l + plotW / 2} y={H - 6} text-anchor="middle">
      {mode === 'cost' ? '$ / game-hour (log)' : 'throughput (tokens/sec)'}
    </text>
    <text class="sc-axis-label" transform={`translate(14 ${M.t + plotH / 2}) rotate(-90)`} text-anchor="middle">overall quality (1–5)</text>

    <!-- frontier -->
    {#if frontierPath}<polyline class="sc-frontier" points={frontierPath} />{/if}

    <!-- dots -->
    {#each rows as r}
      <circle
        class="sc-dot"
        class:frontier={r.onFrontier}
        cx={xPx(r)}
        cy={yPx(r.overall)}
        r={r.onFrontier ? 6 : 4.5}
        fill={tierColor(r.tier)}
        onmouseenter={() => (hover = r)}
        onmouseleave={() => (hover = null)}
        role="button"
        tabindex="-1"
        aria-label={r.display_name}
      />
      {#if r.onFrontier && hover !== r}
        <text class="sc-label" x={xPx(r)} y={yPx(r.overall) + 17} text-anchor="middle">{short(r)}</text>
      {/if}
    {/each}

    <!-- tooltip -->
    {#if hover}
      {@const tx = Math.min(xPx(hover) + 10, W - 168)}
      {@const ty = Math.max(yPx(hover.overall) - 52, M.t)}
      <g transform={`translate(${tx} ${ty})`} pointer-events="none">
        <rect width="158" height="48" rx="7" fill="var(--surface)" stroke="var(--border-strong)" />
        <text x="8" y="17" fill="var(--fg)" font-size="11.5" font-weight="600">{short(hover)}</text>
        <text x="8" y="32" fill="var(--muted)" font-size="10.5" font-family="var(--mono)">
          {hover.overall.toFixed(2)} · {hover.usd_per_game_hour > 0 ? fmtCost(hover.usd_per_game_hour) + '/hr' : 'free'}
        </text>
        <text x="8" y="44" fill="var(--muted)" font-size="10.5" font-family="var(--mono)">
          {hover.tokens_per_sec ? hover.tokens_per_sec.toFixed(0) + ' tok/s' : ''} {hover.value_score != null ? '· value ' + hover.value_score.toFixed(1) : ''}
        </text>
      </g>
    {/if}
  </svg>
</div>

<style>
  .sc-controls { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; margin-bottom: 0.6rem; }
  .sc-help { font-size: 0.78rem; margin-left: 0.3rem; }
  .scatter-wrap svg { width: 100%; height: clamp(300px, 38vw, 440px); }
  .sc-dot { transition: r 0.08s ease; }
  .sc-dot:hover { r: 7; }
</style>
