<script lang="ts">
	import { palette } from '../../stores/theme';

	/** Derive a CSS colour string from the current palette accent. */
	const overlayColor = $derived($palette.accent);
</script>

<!-- Full-cover div with multiply blend, tinted from the palette accent colour.
     This gives a dawn / dusk / night mood tint over the scene plate. -->
<div
	class="scene-overlay"
	style="background-color: {overlayColor};"
	aria-hidden="true"
></div>

<style>
	.scene-overlay {
		position: absolute;
		inset: 0;
		mix-blend-mode: multiply;
		opacity: 0.25;
		pointer-events: none;
	}
</style>
