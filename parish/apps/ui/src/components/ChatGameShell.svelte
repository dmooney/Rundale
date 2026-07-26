<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import StatusBar from './StatusBar.svelte';
	import ChatPanel from './ChatPanel.svelte';
	import MapPanel from './MapPanel.svelte';
	import Sidebar from './Sidebar.svelte';
	import InputField from './InputField.svelte';
	import SceneHeader from './SceneHeader.svelte';
	import { focailOpen, syncFocailOnViewportChange } from '../stores/game';
	import {
		activeSurface,
		closeSurface,
		openSurface,
	} from '../stores/surfaceCoordinator';

	let isMobile = $state(false);
	let mobileMediaCleanup: (() => void) | null = null;

	onMount(() => {
		if (typeof window === 'undefined' || !window.matchMedia) return;
		const query = window.matchMedia('(max-width: 768px)');
		isMobile = query.matches;
		const onChange = (event: MediaQueryListEvent) => {
			isMobile = event.matches;
			syncFocailOnViewportChange(event.matches);
		};
		query.addEventListener('change', onChange);
		mobileMediaCleanup = () => query.removeEventListener('change', onChange);
	});

	onDestroy(() => mobileMediaCleanup?.());

	function toggleMap(invoker: HTMLElement) {
		focailOpen.set(false);
		if ($activeSurface === 'map') {
			closeSurface('map');
		} else {
			void openSurface('map', invoker);
		}
	}

	function toggleSidebar() {
		if ($focailOpen) {
			focailOpen.set(false);
		} else {
			closeSurface('map', { restoreFocus: false });
			focailOpen.set(true);
		}
	}
</script>

<div class="chat-game-shell" data-testid="chat-game-shell">
	<StatusBar />

	<nav class="mobile-toolbar" aria-label="Mobile game panels">
		<button
			type="button"
			class:active={$activeSurface === 'map'}
			aria-pressed={$activeSurface === 'map'}
			aria-label="Toggle parish map"
			onclick={(event) => toggleMap(event.currentTarget as HTMLElement)}
			><img
				src="/rundale/illustrated-notebook-v2/icon-map.png"
				alt=""
				aria-hidden="true"
			/>Map</button
		>
		<button
			type="button"
			class:active={$focailOpen}
			aria-pressed={$focailOpen}
			aria-label="Toggle nearby people and language hints"
			onclick={toggleSidebar}>People & words</button
		>
	</nav>

	<main class="main-area">
		<section class="chat-column" aria-label="Parish conversation">
			{#if $focailOpen && isMobile}
				<Sidebar onclose={() => focailOpen.set(false)} />
			{:else}
				<SceneHeader />
				<ChatPanel />
				<InputField />
			{/if}
		</section>

		<aside class="context-column" aria-label="Parish context">
			<MapPanel />
			<Sidebar />
		</aside>
	</main>
</div>

<style>
	.chat-game-shell {
		display: flex;
		flex-direction: column;
		height: 100dvh;
		min-height: 0;
		overflow: hidden;
		padding-bottom: env(safe-area-inset-bottom);
		background: var(--color-bg);
	}

	.main-area {
		display: grid;
		grid-template-columns: minmax(0, 1fr) clamp(240px, 22vw, 320px);
		flex: 1;
		min-height: 0;
		overflow: hidden;
	}

	.chat-column {
		position: relative;
		display: flex;
		flex-direction: column;
		min-width: 0;
		min-height: 0;
		overflow: hidden;
	}

	.context-column {
		display: flex;
		flex-direction: column;
		min-width: 0;
		min-height: 0;
		overflow: hidden;
		border-left: 1px solid var(--color-border);
	}

	.mobile-toolbar {
		display: none;
	}

	@media (max-width: 768px) {
		.main-area {
			grid-template-columns: minmax(0, 1fr);
		}

		.context-column {
			display: none;
		}

		.mobile-toolbar {
			display: flex;
			flex: 0 0 auto;
			gap: 0.4rem;
			padding: 0.35rem max(0.6rem, env(safe-area-inset-left));
			background: var(--color-panel-bg);
			border-bottom: 1px solid var(--color-border);
			z-index: 29;
		}

		.mobile-toolbar button {
			display: inline-flex;
			align-items: center;
			gap: 0.25rem;
			padding: 0.35rem 0.65rem;
			color: var(--color-muted);
			background: transparent;
			border: 1px solid var(--color-border);
			border-radius: 0.25rem;
			font: 600 0.65rem/1.2 var(--font-display);
			letter-spacing: 0.08em;
			text-transform: uppercase;
			cursor: pointer;
		}

		.mobile-toolbar img {
			width: 1rem;
			height: 1rem;
			object-fit: contain;
		}

		.mobile-toolbar button:hover,
		.mobile-toolbar button:focus-visible,
		.mobile-toolbar button.active {
			color: var(--color-accent);
			border-color: var(--color-accent);
		}
	}
</style>
