Evidence type: test-output + svelte-check
Verdict: sufficient
Technical debt: clear

Both extractions are pure relocation/refactoring with zero behaviour change:
- MapTooltip.svelte consolidates 13-line conditional template + now-unified CSS that was previously written identically in two places.
- tileSync.ts eliminates a 3-line $effect block that was literally copy-pasted between components. The store subscribe approach is simpler (no mounted flag needed) and provides proper lifecycle cleanup. Both components' public API is unchanged. All 379 existing tests pass; svelte-check reports no new errors.
