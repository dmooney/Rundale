<script lang="ts">
	let {
		url,
		alt,
		travelPending = false,
	}: {
		url: string;
		alt: string;
		travelPending?: boolean;
	} = $props();
</script>

<img
	class="scene-plate"
	class:travel-pending={travelPending}
	src={url}
	{alt}
	draggable="false"
/>

<style>
	.scene-plate {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		object-fit: cover;
		image-rendering: pixelated;
		user-select: none;
		transition: filter 0.18s ease, opacity 0.18s ease;
	}

	.scene-plate.travel-pending {
		filter: saturate(0.75) brightness(0.72);
		opacity: 0.78;
	}
</style>
