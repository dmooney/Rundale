<!--
  WindGust — the entire page tilts and shifts briefly, like a gust
  rattling the words on the page. Applies a CSS animation to the
  document body so the actual content shudders.
-->
<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import type { ActiveEffect } from '$lib/effects';

	interface Props {
		effect: ActiveEffect;
	}
	let { effect }: Props = $props();

	onMount(() => {
		document.body.classList.add('wind-gust');
	});

	onDestroy(() => {
		document.body.classList.remove('wind-gust');
	});
</script>
