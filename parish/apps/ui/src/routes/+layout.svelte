<script lang="ts">
	import '../app.css';
	import { onMount, onDestroy } from 'svelte';
	import { captureScreen } from '$lib/screenshot';
	import {
		getGraphicalReadiness,
		onRequestScreenshot,
		reportGraphicalReady,
		reportGraphicalError,
		reportGraphicalUnready,
		saveScreenshot,
		notifyScreenshotStarted,
		notifyScreenshotCaptured,
		notifyScreenshotError
	} from '$lib/ipc';

	let { children } = $props();

	// Register the MCP-triggered screenshot listener at layout level so it
	// stays active regardless of which route is currently mounted (gameplay
	// page, editor, etc.). Without this, navigating to /editor before an
	// agent calls parish_take_screenshot causes a guaranteed 15-second timeout.
	let unlistenScreenshot: (() => void) | null = null;
	let graphicalLaunchToken: string | null = null;

	async function markGraphicalReady() {
		if (graphicalLaunchToken) return;
		try {
			const readiness = await getGraphicalReadiness();
			await reportGraphicalReady(readiness.launch_token);
			graphicalLaunchToken = readiness.launch_token;
		} catch (error) {
			console.warn('Graphical screenshot readiness failed:', error);
		}
	}

	function reportGraphicalFailure(event: Event) {
		const detail = event instanceof CustomEvent ? String(event.detail) : 'unknown renderer failure';
		console.warn('Illustrated renderer failed before graphical readiness:', detail);
		if (graphicalLaunchToken) {
			void reportGraphicalError(graphicalLaunchToken, detail).catch(() => {});
		}
	}

	onMount(async () => {
		window.addEventListener('parish:graphical-frame-ready', markGraphicalReady);
		window.addEventListener('parish:graphical-frame-failed', reportGraphicalFailure);
		if (
			(window as typeof window & { __parishGraphicalFrameReady?: boolean })
				.__parishGraphicalFrameReady
		) {
			void markGraphicalReady();
		}
		unlistenScreenshot = await onRequestScreenshot(async (payload) => {
			try {
				await notifyScreenshotStarted(payload.request_id);
				const dataUrl = await captureScreen();
				const info = await saveScreenshot(dataUrl);
				await notifyScreenshotCaptured(payload.request_id, info);
			} catch (e) {
				const msg = e instanceof Error ? e.message : String(e);
				console.warn('Agent screenshot capture failed:', msg);
				await notifyScreenshotError(payload.request_id, msg).catch(() => {});
			}
		});
		// Listener registration is the readiness boundary for the MCP protocol:
		// any later capture has a live receipt path, and the capture command still
		// rejects a blank/unpresented frame before it can report success. The Pixi
		// first-frame event above remains useful diagnostic evidence, but must not
		// race the handler registration that the bridge depends on.
		void markGraphicalReady();
	});

	onDestroy(() => {
		window.removeEventListener('parish:graphical-frame-ready', markGraphicalReady);
		window.removeEventListener('parish:graphical-frame-failed', reportGraphicalFailure);
		unlistenScreenshot?.();
		if (graphicalLaunchToken) {
			void reportGraphicalUnready(graphicalLaunchToken).catch(() => {});
		}
	});
</script>

{@render children()}
