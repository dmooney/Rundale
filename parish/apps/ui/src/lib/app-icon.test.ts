import { beforeEach, describe, expect, it } from 'vitest';
import { applyAppIcon } from './app-icon';

describe('applyAppIcon', () => {
	beforeEach(() => {
		document.head.innerHTML = '';
	});

	it('creates favicon and apple-touch links for the active mod icon', () => {
		applyAppIcon('/api/app-icon.png', '/api/favicon.png');

		const favicon =
			document.head.querySelector<HTMLLinkElement>('link[rel="icon"]');
		const appleTouch = document.head.querySelector<HTMLLinkElement>(
			'link[rel="apple-touch-icon"]',
		);

		expect(favicon?.getAttribute('href')).toBe('/api/favicon.png');
		expect(favicon?.getAttribute('type')).toBe('image/png');
		expect(appleTouch?.getAttribute('href')).toBe('/api/app-icon.png');
	});

	it('leaves the engine icon alone when no mod icon is provided', () => {
		applyAppIcon(null);

		expect(document.head.querySelector('link[rel="icon"]')).toBeNull();
		expect(
			document.head.querySelector('link[rel="apple-touch-icon"]'),
		).toBeNull();
	});
});
