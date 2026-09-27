import { describe, expect, it } from 'vitest';
import { getLocationIcon, ICON_PATHS } from './map-icons';
import type { LocationIcon } from './map-icons';

describe('ICON_PATHS', () => {
	const expectedKeys: LocationIcon[] = [
		'signpost',
		'beer-stein',
		'church',
		'envelope',
		'book-open',
		'waves',
		'anchor',
		'barn',
		'sparkle',
		'path',
		'storefront',
		'fire',
		'house',
		'trophy',
		'map-pin',
	];

	it('has entries for all expected icon keys', () => {
		for (const key of expectedKeys) {
			expect(ICON_PATHS).toHaveProperty(key);
		}
	});

	it('every path is a non-empty string', () => {
		for (const key of expectedKeys) {
			expect(typeof ICON_PATHS[key]).toBe('string');
			expect(ICON_PATHS[key].length).toBeGreaterThan(0);
		}
	});
});

describe('getLocationIcon', () => {
	it('returns signpost for crossroads', () => {
		expect(getLocationIcon('Crossroads')).toBe('signpost');
		expect(getLocationIcon('The Crossroads')).toBe('signpost');
	});

	it('returns beer-stein for pub', () => {
		expect(getLocationIcon('Pub')).toBe('beer-stein');
		expect(getLocationIcon('The Pub')).toBe('beer-stein');
	});

	it('returns church for church', () => {
		expect(getLocationIcon('Church')).toBe('church');
		expect(getLocationIcon("St. Mary's Church")).toBe('church');
	});

	it('returns envelope for letter office', () => {
		expect(getLocationIcon('Letter Office')).toBe('envelope');
		expect(getLocationIcon('Letteroffice')).toBe('envelope');
	});

	it('returns book-open for school', () => {
		expect(getLocationIcon('School')).toBe('book-open');
		expect(getLocationIcon('National School')).toBe('book-open');
	});

	it('returns waves for lough or shore', () => {
		expect(getLocationIcon('Lough')).toBe('waves');
		expect(getLocationIcon('Shore')).toBe('waves');
	});

	it('returns anchor for bay or harbour', () => {
		expect(getLocationIcon('Bay')).toBe('anchor');
		expect(getLocationIcon('Harbour')).toBe('anchor');
		expect(getLocationIcon('Harbor')).toBe('anchor');
	});

	it('returns barn for farm', () => {
		expect(getLocationIcon('Farm')).toBe('barn');
		expect(getLocationIcon('Farmhouse')).toBe('barn');
	});

	it('returns sparkle for fairy fort or rath', () => {
		expect(getLocationIcon('Fairy Ring')).toBe('sparkle');
		expect(getLocationIcon("O'Kelly's Fort")).toBe('sparkle');
		expect(getLocationIcon('Rathgar')).toBe('sparkle');
	});

	it('returns path for bog or road', () => {
		expect(getLocationIcon('Bog')).toBe('path');
		expect(getLocationIcon('Road')).toBe('path');
	});

	it('returns storefront for shop', () => {
		expect(getLocationIcon('Shop')).toBe('storefront');
		expect(getLocationIcon("Murphy's Shop")).toBe('storefront');
	});

	it('returns fire for kiln', () => {
		expect(getLocationIcon('Kiln')).toBe('fire');
		expect(getLocationIcon('Lime Kiln')).toBe('fire');
	});

	it('returns house for village or town', () => {
		expect(getLocationIcon('Village')).toBe('house');
		expect(getLocationIcon('Town')).toBe('house');
	});

	it('returns trophy for green or hurling', () => {
		expect(getLocationIcon('Green')).toBe('trophy');
		expect(getLocationIcon('Hurling Field')).toBe('trophy');
	});

	it('returns map-pin for unrecognised names', () => {
		expect(getLocationIcon('Unknown')).toBe('map-pin');
		expect(getLocationIcon('Some Random Place')).toBe('map-pin');
	});

	it('is case-insensitive', () => {
		expect(getLocationIcon('PUB')).toBe('beer-stein');
		expect(getLocationIcon('pub')).toBe('beer-stein');
	});
});
