<script lang="ts">
	import { worldState } from '../stores/game';
</script>

{#if $worldState}
	<figure class="scene-header" data-testid="scene-header">
		<picture>
			<source
				media="(max-width: 768px)"
				srcset="/rundale/illustrated-notebook-v2/parish-crossroads-watercolor-mobile.png"
			/>
			<img
				src="/rundale/illustrated-notebook-v2/parish-crossroads-watercolor.png"
				alt=""
				aria-hidden="true"
			/>
		</picture>
		<figcaption>
			<strong>{$worldState.location_name}</strong>
			<span>{$worldState.location_description}</span>
		</figcaption>
	</figure>
{/if}

<style>
	.scene-header {
		position: relative;
		flex: 0 0 clamp(88px, 14vh, 132px);
		min-height: 0;
		margin: 0;
		overflow: hidden;
		border-bottom: 1px solid var(--color-border);
		background: var(--color-panel-bg);
	}

	picture,
	img {
		display: block;
		width: 100%;
		height: 100%;
	}

	img {
		object-fit: cover;
		object-position: center 48%;
		filter: saturate(0.82) contrast(0.94);
	}

	.scene-header::after {
		position: absolute;
		inset: 0;
		content: '';
		background:
			linear-gradient(90deg, rgba(17, 18, 16, 0.78), transparent 68%),
			linear-gradient(0deg, rgba(17, 18, 16, 0.35), transparent 65%);
		pointer-events: none;
	}

	figcaption {
		position: absolute;
		z-index: 1;
		left: clamp(0.8rem, 3vw, 2rem);
		bottom: 0.7rem;
		display: grid;
		gap: 0.2rem;
		width: min(44rem, calc(100% - 2rem));
		color: #fffaf0;
		text-shadow: 0 1px 4px rgba(0, 0, 0, 0.85);
	}

	figcaption strong {
		font: 600 clamp(0.85rem, 1.5vw, 1.15rem) / 1.2 var(--font-display);
		letter-spacing: 0.05em;
	}

	figcaption span {
		overflow: hidden;
		font-size: clamp(0.68rem, 1.1vw, 0.82rem);
		line-height: 1.35;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	@media (max-width: 768px) {
		.scene-header {
			flex-basis: 82px;
		}

		figcaption {
			left: 0.7rem;
			bottom: 0.45rem;
		}
	}
</style>
