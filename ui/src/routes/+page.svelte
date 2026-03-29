<script lang="ts">
	import { onMount } from 'svelte';
	import { get } from 'svelte/store';
	import StatusBar from '../components/StatusBar.svelte';
	import ChatPanel from '../components/ChatPanel.svelte';
	import MapPanel from '../components/MapPanel.svelte';
	import Sidebar from '../components/Sidebar.svelte';
	import InputField from '../components/InputField.svelte';
	import DebugPanel from '../components/DebugPanel.svelte';

	import { worldState, mapData, npcsHere, textLog, streamingActive, loadingSpinner, loadingPhrase, loadingColor, languageHints, uiConfig } from '../stores/game';
	import { debugVisible, debugSnapshot } from '../stores/debug';
	import { palette } from '../stores/theme';
	import {
		getWorldSnapshot,
		getMap,
		getNpcsHere,
		getUiConfig,
		getTheme,
		getDebugSnapshot,
		onWorldUpdate,
		onStreamToken,
		onStreamEnd,
		onTextLog,
		onLoading,
		onThemeUpdate,
		onDebugUpdate
	} from '$lib/ipc';

	// F12 toggle for debug panel
	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'F12') {
			e.preventDefault();
			debugVisible.update((v) => !v);
			// Fetch initial snapshot when opening
			if (!$debugVisible) {
				getDebugSnapshot()
					.then((s) => debugSnapshot.set(s))
					.catch(() => {});
			}
		}
	}

	onMount(async () => {
		// Initial data fetch (theme first to avoid color flash)
		try {
			const [snap, map, npcs, theme] = await Promise.all([
				getWorldSnapshot(),
				getMap(),
				getNpcsHere(),
				getTheme()
			]);
			worldState.set(snap);
			mapData.set(map);
			npcsHere.set(npcs);
			palette.apply(theme);
			// Show initial location description in the chat panel
			if (snap.location_description) {
				textLog.update((log) => [
					...log,
					{ source: 'system', content: snap.location_description }
				]);
			}
		} catch (e) {
			console.warn('Initial fetch failed (expected in browser dev):', e);
		}

		// Fetch UI config from mod and show splash text
		try {
			const cfg = await getUiConfig();
			uiConfig.set(cfg);
			if (cfg.splash_text) {
				textLog.update((log) => [
					{ source: 'system', content: cfg.splash_text },
					...log
				]);
			}
		} catch (_) {}

		// Subscribe to events
		// Fetch initial debug snapshot
		try {
			const debugSnap = await getDebugSnapshot();
			debugSnapshot.set(debugSnap);
		} catch (_) {}

		const unlisten = await Promise.all([
			onWorldUpdate(async (snap) => {
				worldState.set(snap);
				try {
					const [map, npcs] = await Promise.all([getMap(), getNpcsHere()]);
					mapData.set(map);
					npcsHere.set(npcs);
				} catch (_) {}
			}),

			onTextLog((payload) => {
				textLog.update((log) => [
					...log,
					{ source: payload.source, content: payload.content }
				]);
			}),

			onStreamToken((payload) => {
				textLog.update((log) => {
					if (log.length > 0 && log[log.length - 1].streaming) {
						const last = log[log.length - 1];
						return [
							...log.slice(0, -1),
							{ ...last, content: last.content + payload.token }
						];
					}
					// Start new streaming entry
					return [...log, { source: 'NPC', content: payload.token, streaming: true }];
				});
			}),

			onStreamEnd((payload) => {
				// Finalize the streaming entry
				textLog.update((log) => {
					if (log.length > 0 && log[log.length - 1].streaming) {
						const last = log[log.length - 1];
						return [...log.slice(0, -1), { ...last, streaming: false }];
					}
					return log;
				});
				languageHints.set(payload.hints);
				streamingActive.set(false);
			}),

			onLoading((payload) => {
				const wasActive = get(streamingActive);
				streamingActive.set(payload.active);
				if (payload.active) {
					// Update animated loading phrase and spinner
					if (payload.spinner) loadingSpinner.set(payload.spinner);
					if (payload.phrase) loadingPhrase.set(payload.phrase);
					if (payload.color) loadingColor.set(payload.color);
					// Only clean up stale streaming entries on the *first*
					// loading event (transition from inactive → active), not
					// on every animation tick, to avoid erasing in-progress text.
					if (!wasActive) {
						textLog.update((log) => {
							if (log.length > 0 && log[log.length - 1].streaming) {
								return log.slice(0, -1);
							}
							return log;
						});
					}
				}
			}),

			onThemeUpdate((p) => {
				palette.apply(p);
			}),

			onDebugUpdate((snap) => {
				debugSnapshot.set(snap);
			})
		]);

		return () => {
			unlisten.forEach((fn) => fn());
		};
	});
</script>

<svelte:window on:keydown={handleKeydown} />

<div class="app-shell" class:debug-open={$debugVisible}>
	<StatusBar />
	<div class="main-area">
		<div class="chat-col">
			<ChatPanel />
			<InputField />
		</div>
		<div class="right-col">
			<MapPanel />
			<Sidebar />
		</div>
	</div>
</div>

<DebugPanel />

<style>
	.app-shell {
		display: flex;
		flex-direction: column;
		height: 100vh;
		overflow: hidden;
		transition: height 0.15s ease;
	}

	.app-shell.debug-open {
		height: 60vh;
	}

	.main-area {
		flex: 1;
		display: grid;
		grid-template-columns: 1fr 220px;
		overflow: hidden;
	}

	.chat-col {
		display: flex;
		flex-direction: column;
		min-height: 0;
		overflow: hidden;
	}

	.right-col {
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}
</style>
