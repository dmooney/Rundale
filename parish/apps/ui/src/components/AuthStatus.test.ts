import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render } from '@testing-library/svelte';
import AuthStatus from './AuthStatus.svelte';

beforeEach(() => {
	vi.restoreAllMocks();
});

describe('AuthStatus', () => {
	it('shows nothing when oauth is not enabled', async () => {
		vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce(
			new Response(JSON.stringify({ oauth_enabled: false, logged_in: false })),
		);
		const { container } = render(AuthStatus);
		// Wait for the onMount fetch
		await vi.waitFor(() => {
			expect(container.querySelector('.auth-indicator')).toBeNull();
			expect(container.querySelector('.auth-link')).toBeNull();
		});
	});

	it('shows a login link when oauth is enabled but not logged in', async () => {
		vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce(
			new Response(
				JSON.stringify({
					oauth_enabled: true,
					logged_in: false,
					provider: 'google',
				}),
			),
		);
		const { container } = render(AuthStatus);
		await vi.waitFor(() => {
			const link = container.querySelector('.auth-link');
			expect(link).toBeTruthy();
			expect(link!.textContent).toMatch(/Login/);
		});
	});

	it('shows display name and sign out when logged in', async () => {
		vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce(
			new Response(
				JSON.stringify({
					oauth_enabled: true,
					logged_in: true,
					display_name: 'TestUser',
					provider: 'google',
				}),
			),
		);
		const { container } = render(AuthStatus);
		await vi.waitFor(() => {
			const indicator = container.querySelector('.auth-indicator');
			expect(indicator).toBeTruthy();
			expect(indicator!.textContent).toMatch(/TestUser/);
			expect(container.querySelectorAll('.auth-link').length).toBe(1);
		});
	});

	it('does not fetch in Tauri environment', async () => {
		const fetchSpy = vi.spyOn(globalThis, 'fetch');
		(window as any).__TAURI_INTERNALS__ = {};
		render(AuthStatus);
		await vi.waitFor(() => {
			expect(fetchSpy).not.toHaveBeenCalled();
		});
		delete (window as any).__TAURI_INTERNALS__;
	});
});
