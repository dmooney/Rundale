import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render } from '@testing-library/svelte';
import AuthStatus from './AuthStatus.svelte';
import { getAuthStatus } from '$lib/ipc';

// AuthStatus now routes through the IPC seam (getAuthStatus) instead of a raw
// fetch('/api/auth/status'); see audit finding M7. Tauri-vs-web forking and the
// fetch itself are covered in ipc.test.ts; here we mock the binding and assert
// the rendered output for each status shape.
vi.mock('$lib/ipc', () => ({
	getAuthStatus: vi.fn(),
}));

const mockGetAuthStatus = vi.mocked(getAuthStatus);

beforeEach(() => {
	mockGetAuthStatus.mockReset();
});

describe('AuthStatus', () => {
	it('shows nothing when oauth is not enabled', async () => {
		mockGetAuthStatus.mockResolvedValueOnce({
			oauth_enabled: false,
			logged_in: false,
		});
		const { container } = render(AuthStatus);
		await vi.waitFor(() => {
			expect(container.querySelector('.auth-indicator')).toBeNull();
			expect(container.querySelector('.auth-link')).toBeNull();
		});
	});

	it('shows nothing when getAuthStatus returns null (Tauri / failure)', async () => {
		mockGetAuthStatus.mockResolvedValueOnce(null);
		const { container } = render(AuthStatus);
		await vi.waitFor(() => {
			expect(mockGetAuthStatus).toHaveBeenCalled();
			expect(container.querySelector('.auth-indicator')).toBeNull();
			expect(container.querySelector('.auth-link')).toBeNull();
		});
	});

	it('shows a login link when oauth is enabled but not logged in', async () => {
		mockGetAuthStatus.mockResolvedValueOnce({
			oauth_enabled: true,
			logged_in: false,
			provider: 'google',
		});
		const { container } = render(AuthStatus);
		await vi.waitFor(() => {
			const link = container.querySelector('.auth-link');
			expect(link).toBeTruthy();
			expect(link!.textContent).toMatch(/Login/);
		});
	});

	it('shows display name and sign out when logged in', async () => {
		mockGetAuthStatus.mockResolvedValueOnce({
			oauth_enabled: true,
			logged_in: true,
			display_name: 'TestUser',
			provider: 'google',
		});
		const { container } = render(AuthStatus);
		await vi.waitFor(() => {
			const indicator = container.querySelector('.auth-indicator');
			expect(indicator).toBeTruthy();
			expect(indicator!.textContent).toMatch(/TestUser/);
			expect(container.querySelectorAll('.auth-link').length).toBe(1);
		});
	});
});
