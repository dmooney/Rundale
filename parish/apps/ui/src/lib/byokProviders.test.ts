import { describe, it, expect } from 'vitest';
import { toByokMeta } from './byokProviders';
import type { AvailableProviderInfo } from './ipc';

function info(signup_url: string | null): AvailableProviderInfo {
	return {
		id: 'evil',
		display_name: 'Evil Co',
		blurb: 'mod-supplied',
		signup_url,
		needs_base_url: false,
		keyless: false
	} as AvailableProviderInfo;
}

describe('toByokMeta signup_url scheme gate (audit security)', () => {
	it('passes through https and http URLs unchanged', () => {
		expect(toByokMeta(info('https://example.com/key')).signupUrl).toBe('https://example.com/key');
		expect(toByokMeta(info('http://example.com/key')).signupUrl).toBe('http://example.com/key');
	});

	it('drops a javascript: URL (would execute on click if bound to href)', () => {
		expect(toByokMeta(info('javascript:alert(document.cookie)')).signupUrl).toBe('');
	});

	it('drops data: and file: URLs', () => {
		expect(toByokMeta(info('data:text/html,<script>alert(1)</script>')).signupUrl).toBe('');
		expect(toByokMeta(info('file:///etc/passwd')).signupUrl).toBe('');
	});

	it('returns empty string for null/empty signup_url', () => {
		expect(toByokMeta(info(null)).signupUrl).toBe('');
		expect(toByokMeta(info('')).signupUrl).toBe('');
	});

	it('returns empty string for an unparseable URL', () => {
		expect(toByokMeta(info('http://[::bad')).signupUrl).toBe('');
	});
});
