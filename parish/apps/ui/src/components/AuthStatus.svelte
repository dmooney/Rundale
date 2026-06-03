<script lang="ts">
	import { onMount } from 'svelte';
	import { getAuthStatus } from '$lib/ipc';
	import type { AuthStatus } from '$lib/types';

	let status = $state<AuthStatus | null>(null);

	const providerName = $derived(
		status?.provider
			? status.provider.charAt(0).toUpperCase() + status.provider.slice(1)
			: 'Google'
	);

	onMount(async () => {
		// getAuthStatus routes through the IPC seam: returns null in Tauri (no
		// /api server) and on any failure — the auth UI is optional chrome.
		status = await getAuthStatus();
	});
</script>

{#if status?.oauth_enabled}
	<span class="sep">·</span>
	{#if status.logged_in}
		<span class="auth-indicator" title="Signed in with {providerName} — your saves are synced">
			✓ {status.display_name ?? providerName}
		</span>
		<span class="sep">·</span>
		<!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- /auth/logout is a server-side OAuth endpoint, not a SvelteKit route -->
		<a href="/auth/logout" class="auth-link">Sign out</a>
	{:else}
		<!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- /auth/login/{provider} is a server-side OAuth endpoint, not a SvelteKit route -->
		<a href="/auth/login/{status.provider ?? 'google'}" class="auth-link">Login with {providerName}</a>
	{/if}
{/if}

<style>
	.auth-indicator {
		color: var(--color-muted);
		font-size: 0.6rem;
		letter-spacing: 0.07em;
		white-space: nowrap;
	}

	.auth-link {
		color: var(--color-muted);
		font-size: 0.6rem;
		letter-spacing: 0.07em;
		text-decoration: none;
		border-bottom: 1px solid var(--color-border);
		transition: color 0.2s, border-color 0.2s;
		white-space: nowrap;
	}

	.auth-link:hover {
		color: var(--color-accent);
		border-color: var(--color-accent);
	}

	.sep {
		color: var(--color-border);
		font-size: 0.7rem;
		letter-spacing: 0;
		opacity: 0.8;
	}
</style>
