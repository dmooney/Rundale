<script lang="ts">
	import { openNotebookOverlay } from '../stores/notebookOverlay';

	let {
		kind,
		label,
		detail,
	}: { kind: string; label: string; detail: unknown } = $props();

	function fileBug(e: MouseEvent) {
		// Don't trigger the surrounding row's own click handler (e.g. inference
		// log-row selection).
		e.stopPropagation();
		void openNotebookOverlay('bug', e.currentTarget as HTMLButtonElement, {
			kind,
			label,
			detail,
		});
	}
</script>

<button
	type="button"
	class="bug-chip"
	aria-label="Report a bug about this record"
	title="Report a bug about this record"
	onclick={fileBug}>🐛</button
>

<style>
	.bug-chip {
		background: none;
		border: none;
		cursor: pointer;
		opacity: 0.45;
		font-size: 0.7rem;
		padding: 0 0.2rem;
		line-height: 1;
		transition: opacity 0.15s;
	}
	.bug-chip:hover,
	.bug-chip:focus-visible {
		opacity: 1;
	}
</style>
