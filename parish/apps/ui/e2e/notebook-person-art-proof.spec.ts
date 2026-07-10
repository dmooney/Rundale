import { expect, installTauriMock, test } from './fixtures';
import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';
import type { NpcInfo, WorldSnapshot } from '../src/lib/types';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const PROOF_DIR = path.resolve(
	__dirname,
	'../../../../.proofs/issue-1628-person-art',
);

const approvedCast: NpcInfo[] = [
	{
		name: 'Brigid Ni Fhatharta',
		real_name: 'Brigid Ni Fhatharta',
		occupation: 'Midwife',
		mood: 'watchful',
		introduced: true,
		mood_emoji: '🤔',
	},
	{
		name: 'Sean Ruadh Kelly',
		real_name: 'Sean Ruadh Kelly',
		occupation: 'Labourer',
		mood: 'bitter',
		introduced: true,
		mood_emoji: '😒',
	},
	{
		name: 'Peig Hannigan',
		real_name: 'Peig Hannigan',
		occupation: 'Widow',
		mood: 'sharp',
		introduced: true,
		mood_emoji: '😤',
	},
	{
		name: 'Roisin Connolly',
		real_name: 'Roisin Connolly',
		occupation: 'Shopkeeper',
		mood: 'alert',
		introduced: true,
		mood_emoji: '🙂',
	},
];

const kilteevanSnapshot: WorldSnapshot = {
	location_name: 'Kilteevan Village',
	location_description:
		'The crossroads at Kilteevan are damp after rain, with cottages, low walls, and neighbours moving through the morning.',
	time_label: 'Morning',
	hour: 8,
	minute: 0,
	weather: 'Clear',
	season: 'Spring',
	festival: null,
	paused: false,
	inference_paused: false,
	game_epoch_ms: Date.UTC(1820, 2, 23, 8, 0, 0),
	speed_factor: 0,
	name_hints: [],
	day_of_week: 'Monday',
};

async function setupProofPage(page: import('@playwright/test').Page) {
	await installTauriMock(page, 'morning', {
		npcs: approvedCast,
		snapshot: kilteevanSnapshot,
	});
	await page.goto('/');
	await page.waitForLoadState('networkidle');
	await expect(
		page.locator('[data-testid="illustrated-notebook-game"]'),
	).toBeVisible();
	await expect(
		page.locator('[data-testid="illustrated-notebook-pixi-host"] canvas'),
	).toBeVisible();
	await expect(
		page.getByRole('button', {
			name: 'Select nearby person Brigid Ni Fhatharta',
		}),
	).toBeVisible();
}

test.describe('issue 1628 notebook person art proof', () => {
	test.beforeAll(() => {
		fs.mkdirSync(PROOF_DIR, { recursive: true });
	});

	test('desktop approved person art reads in the first viewport', async ({
		page,
	}) => {
		await page.setViewportSize({ width: 1440, height: 900 });
		await setupProofPage(page);

		await page.screenshot({
			path: path.join(PROOF_DIR, 'desktop.png'),
			fullPage: false,
		});
	});

	test('mobile approved person art reads in the first viewport', async ({
		page,
	}) => {
		await page.setViewportSize({ width: 390, height: 844 });
		await setupProofPage(page);

		await page.screenshot({
			path: path.join(PROOF_DIR, 'mobile.png'),
			fullPage: false,
		});
	});
});
