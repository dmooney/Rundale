import { expect, installTauriMock, test } from './fixtures';
import type { Page } from '@playwright/test';
import * as fs from 'fs';
import * as path from 'path';
import { PNG } from 'pngjs';
import { fileURLToPath } from 'url';
import type { NpcInfo, WorldSnapshot } from '../src/lib/types';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const PROOF_DIR = path.resolve(
	__dirname,
	'../../../../.proofs/issue-1628-person-art',
);
const NOTEBOOK_ASSET_ROOT = '/rundale/notebook-ui/';
const FINAL_RELEASE_MANIFEST =
	'parish/apps/ui/art/notebook-person-art/approved/v1/release-manifest.json';
const FINAL_PERSON_ART_ENTRY_COUNT = 24;
const FINAL_NAMED_PERSON_ART_ENTRY_COUNT = FINAL_PERSON_ART_ENTRY_COUNT - 1;
const FINAL_PERSON_ART_IMAGE_COUNT = FINAL_PERSON_ART_ENTRY_COUNT * 2;
const BRIGID_NPC_ID = 19;
const STALE_BRIGID_COMPATIBILITY_NAME = 'Deliberately stale compatibility name';
const ID_SENTINEL_ASSETS = {
	portrait: 'people/npc-id-19-portrait-sentinel.png',
	marker: 'people/npc-id-19-marker-sentinel.png',
} as const;
const ID_SENTINEL_COLORS = {
	portrait: [251, 17, 183],
	marker: [13, 241, 47],
} as const;

interface PersonArtEntry {
	display_name: string;
	npc_id?: number | null;
	portrait: string;
	marker: string;
	approval_status: string;
	real_name?: string;
}

interface PersonArtManifest {
	release_id: string;
	release_manifest: string;
	release_manifest_sha256: string;
	approval_status: string;
	contact_sheet_html?: string | null;
	fallback: PersonArtEntry;
	people: PersonArtEntry[];
}

interface NotebookAssetManifest {
	assets: { personArt?: PersonArtManifest };
}

interface DecodedBrowserImage {
	url: string;
	ok: boolean;
	contentType: string;
	width: number;
	height: number;
	visiblePixels: number;
	nonBlankPixels: number;
	error?: string;
}

const approvedCast: NpcInfo[] = [
	{
		npc_id: BRIGID_NPC_ID,
		name: 'Brigid Ni Fhatharta',
		real_name: STALE_BRIGID_COMPATIBILITY_NAME,
		occupation: 'Midwife',
		mood: 'watchful',
		introduced: true,
		mood_emoji: '🤔',
	},
	{
		npc_id: 21,
		name: 'Sean Ruadh Kelly',
		real_name: 'Sean Ruadh Kelly',
		occupation: 'Labourer',
		mood: 'bitter',
		introduced: true,
		mood_emoji: '😒',
	},
	{
		npc_id: 22,
		name: 'Peig Hannigan',
		real_name: 'Peig Hannigan',
		occupation: 'Widow',
		mood: 'sharp',
		introduced: true,
		mood_emoji: '😤',
	},
	{
		npc_id: 4,
		name: 'Roisin Connolly',
		real_name: 'Roisin Connolly',
		occupation: 'Shopkeeper',
		mood: 'alert',
		introduced: true,
		mood_emoji: '🙂',
	},
];

const kilteevanSnapshot: WorldSnapshot = {
	location_id: 15,
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
	active_tasks: [],
};

function notebookAssetUrl(asset: string): string {
	return new URL(asset, `http://parish.invalid${NOTEBOOK_ASSET_ROOT}`).pathname;
}

async function loadPersonArtManifest(page: Page): Promise<PersonArtManifest> {
	const response = await page.request.get(
		notebookAssetUrl('asset-manifest.json'),
	);
	expect(response.ok(), 'runtime notebook asset manifest request').toBe(true);
	const manifest = (await response.json()) as NotebookAssetManifest;
	expect(manifest.assets.personArt, 'runtime person art manifest').toBeTruthy();
	return manifest.assets.personArt!;
}

function assertApprovedReleaseProvenance(personArt: PersonArtManifest): void {
	expect(personArt.approval_status, 'runtime approval status').toBe('approved');
	expect(personArt.release_id, 'runtime release ID').toMatch(/^[a-f0-9]{64}$/);
	expect(personArt.release_manifest, 'runtime release manifest').toBe(
		FINAL_RELEASE_MANIFEST,
	);
	expect(
		personArt.release_manifest_sha256,
		'runtime release manifest SHA-256',
	).toMatch(/^[a-f0-9]{64}$/);
}

function solidPng(color: readonly [number, number, number]): Buffer {
	const image = new PNG({ width: 32, height: 32 });
	for (let index = 0; index < image.data.length; index += 4) {
		image.data[index] = color[0];
		image.data[index + 1] = color[1];
		image.data[index + 2] = color[2];
		image.data[index + 3] = 255;
	}
	return PNG.sync.write(image);
}

async function installNumericIdentitySentinel(page: Page): Promise<void> {
	for (const [kind, asset] of Object.entries(ID_SENTINEL_ASSETS) as Array<
		[keyof typeof ID_SENTINEL_ASSETS, string]
	>) {
		await page.route(`**${notebookAssetUrl(asset)}`, (route) =>
			route.fulfill({
				contentType: 'image/png',
				body: solidPng(ID_SENTINEL_COLORS[kind]),
			}),
		);
	}
	await page.route(
		'**/rundale/notebook-ui/asset-manifest.json',
		async (route) => {
			const response = await route.fetch();
			const manifest = (await response.json()) as NotebookAssetManifest;
			const brigid = manifest.assets.personArt?.people.find(
				(entry) => entry.npc_id === BRIGID_NPC_ID,
			);
			if (!brigid) {
				throw new Error(`missing runtime person art for NPC ${BRIGID_NPC_ID}`);
			}
			brigid.portrait = ID_SENTINEL_ASSETS.portrait;
			brigid.marker = ID_SENTINEL_ASSETS.marker;
			await route.fulfill({
				response,
				contentType: 'application/json',
				body: JSON.stringify(manifest),
			});
		},
	);
}

async function decodeBrowserImages(
	page: Page,
	assetUrls: string[],
): Promise<DecodedBrowserImage[]> {
	return page.evaluate(async (urls) => {
		return Promise.all(
			urls.map(async (url) => {
				try {
					const response = await fetch(url);
					const contentType = response.headers.get('content-type') ?? '';
					if (!response.ok) {
						return {
							url,
							ok: false,
							contentType,
							width: 0,
							height: 0,
							visiblePixels: 0,
							nonBlankPixels: 0,
							error: `HTTP ${response.status}`,
						};
					}

					const image = await createImageBitmap(await response.blob());
					const canvas = document.createElement('canvas');
					canvas.width = image.width;
					canvas.height = image.height;
					const context = canvas.getContext('2d', {
						willReadFrequently: true,
					});
					if (!context) throw new Error('could not read decoded image pixels');
					context.drawImage(image, 0, 0);
					const pixels = context.getImageData(
						0,
						0,
						canvas.width,
						canvas.height,
					).data;
					let visiblePixels = 0;
					let nonBlankPixels = 0;
					for (let index = 0; index < pixels.length; index += 4) {
						const alpha = pixels[index + 3];
						if (alpha < 16) continue;
						visiblePixels += 1;
						if (
							pixels[index] < 245 ||
							pixels[index + 1] < 245 ||
							pixels[index + 2] < 245
						) {
							nonBlankPixels += 1;
						}
					}
					image.close();
					return {
						url,
						ok: true,
						contentType,
						width: canvas.width,
						height: canvas.height,
						visiblePixels,
						nonBlankPixels,
					};
				} catch (error) {
					return {
						url,
						ok: false,
						contentType: '',
						width: 0,
						height: 0,
						visiblePixels: 0,
						nonBlankPixels: 0,
						error: error instanceof Error ? error.message : String(error),
					};
				}
			}),
		);
	}, assetUrls);
}

function assertDecodedNonBlank(images: DecodedBrowserImage[]): void {
	for (const image of images) {
		expect(
			image.ok,
			`${image.url} must return and decode: ${image.error}`,
		).toBe(true);
		expect(image.contentType, `${image.url} content type`).toContain('image/');
		expect(image.width, `${image.url} decoded width`).toBeGreaterThan(1);
		expect(image.height, `${image.url} decoded height`).toBeGreaterThan(1);
		expect(image.visiblePixels, `${image.url} visible pixels`).toBeGreaterThan(
			24,
		);
		expect(
			image.nonBlankPixels,
			`${image.url} nonblank pixels`,
		).toBeGreaterThan(24);
	}
}

async function installNearAndFarSceneFixture(page: Page): Promise<void> {
	await page.route(
		'**/rundale/notebook-ui/visual-scenes.json',
		async (route) => {
			const response = await route.fetch();
			const scenes = (await response.json()) as {
				scenes?: Array<{
					anchors?: { npcs?: Array<Record<string, unknown>> };
				}>;
			};
			const npcAnchors = scenes.scenes?.[0]?.anchors?.npcs;
			if (npcAnchors && npcAnchors.length >= 2) {
				npcAnchors[0] = {
					...npcAnchors[0],
					x: 0.22,
					y: 0.3,
					depth: 0.2,
				};
				npcAnchors[1] = {
					...npcAnchors[1],
					x: 0.52,
					y: 0.66,
					depth: 0.84,
				};
			}
			await route.fulfill({
				response,
				contentType: 'application/json',
				body: JSON.stringify(scenes),
			});
		},
	);
}

async function setupProofPage(page: Page) {
	const browserErrors: string[] = [];
	page.on('pageerror', (error) =>
		browserErrors.push(error.stack ?? error.message),
	);
	page.on('console', (message) => {
		if (message.type() === 'error') browserErrors.push(message.text());
	});
	await installTauriMock(page, 'morning', {
		npcs: approvedCast,
		snapshot: kilteevanSnapshot,
	});
	await installNearAndFarSceneFixture(page);
	await page.goto('/');
	await page.waitForLoadState('networkidle');
	try {
		await expect(
			page.locator('[data-testid="illustrated-notebook-game"]'),
		).toBeVisible();
	} catch (error) {
		throw new Error(
			`Illustrated notebook did not mount. Browser errors:\n${browserErrors.join('\n') || '(none captured)'}`,
			{ cause: error },
		);
	}
	await expect(
		page.locator('[data-testid="illustrated-notebook-pixi-host"] canvas'),
	).toBeVisible();
	await expect(
		page.getByRole('button', {
			name: 'Select nearby person Brigid Ni Fhatharta',
		}),
	).toBeVisible();
}

async function assertCanvasTargetIsPainted(
	page: Page,
	targetName: string,
	requireUnclipped = false,
): Promise<void> {
	const canvas = page.locator(
		'[data-testid="illustrated-notebook-pixi-host"] canvas',
	);
	const target = page.getByRole('button', { name: targetName });
	await expect(target).toBeVisible();

	const [canvasBox, targetBox] = await Promise.all([
		canvas.boundingBox(),
		target.boundingBox(),
	]);
	if (!canvasBox || !targetBox) {
		throw new Error(`could not read canvas bounds for ${targetName}`);
	}

	if (requireUnclipped) {
		expect(targetBox.x, `${targetName} left clipping`).toBeGreaterThanOrEqual(
			canvasBox.x - 1,
		);
		expect(targetBox.y, `${targetName} top clipping`).toBeGreaterThanOrEqual(
			canvasBox.y - 1,
		);
		expect(
			targetBox.x + targetBox.width,
			`${targetName} right clipping`,
		).toBeLessThanOrEqual(canvasBox.x + canvasBox.width + 1);
		expect(
			targetBox.y + targetBox.height,
			`${targetName} bottom clipping`,
		).toBeLessThanOrEqual(canvasBox.y + canvasBox.height + 1);
	}

	const screenshot = PNG.sync.read(await canvas.screenshot());
	const scaleX = screenshot.width / canvasBox.width;
	const scaleY = screenshot.height / canvasBox.height;
	const left = Math.max(0, Math.floor((targetBox.x - canvasBox.x) * scaleX));
	const top = Math.max(0, Math.floor((targetBox.y - canvasBox.y) * scaleY));
	const right = Math.min(
		screenshot.width,
		Math.ceil((targetBox.x + targetBox.width - canvasBox.x) * scaleX),
	);
	const bottom = Math.min(
		screenshot.height,
		Math.ceil((targetBox.y + targetBox.height - canvasBox.y) * scaleY),
	);
	let nonBlankPixels = 0;
	for (let y = top; y < bottom; y += 1) {
		for (let x = left; x < right; x += 1) {
			const index = (y * screenshot.width + x) * 4;
			if (
				screenshot.data[index + 3] >= 16 &&
				(screenshot.data[index] < 245 ||
					screenshot.data[index + 1] < 245 ||
					screenshot.data[index + 2] < 245)
			) {
				nonBlankPixels += 1;
			}
		}
	}
	const regionPixels = Math.max(0, right - left) * Math.max(0, bottom - top);
	expect(regionPixels, `${targetName} canvas region`).toBeGreaterThan(0);
	expect(
		nonBlankPixels,
		`${targetName} canvas region must contain rendered pixels`,
	).toBeGreaterThan(Math.max(24, Math.floor(regionPixels * 0.01)));
}

async function assertCanvasTargetContainsColor(
	page: Page,
	targetName: string,
	color: readonly [number, number, number],
): Promise<void> {
	const canvas = page.locator(
		'[data-testid="illustrated-notebook-pixi-host"] canvas',
	);
	const target = page.getByRole('button', { name: targetName });
	const [canvasBox, targetBox] = await Promise.all([
		canvas.boundingBox(),
		target.boundingBox(),
	]);
	if (!canvasBox || !targetBox) {
		throw new Error(`could not read canvas bounds for ${targetName}`);
	}

	const screenshot = PNG.sync.read(await canvas.screenshot());
	const scaleX = screenshot.width / canvasBox.width;
	const scaleY = screenshot.height / canvasBox.height;
	const left = Math.max(0, Math.floor((targetBox.x - canvasBox.x) * scaleX));
	const top = Math.max(0, Math.floor((targetBox.y - canvasBox.y) * scaleY));
	const right = Math.min(
		screenshot.width,
		Math.ceil((targetBox.x + targetBox.width - canvasBox.x) * scaleX),
	);
	const bottom = Math.min(
		screenshot.height,
		Math.ceil((targetBox.y + targetBox.height - canvasBox.y) * scaleY),
	);
	let matchingPixels = 0;
	for (let y = top; y < bottom; y += 1) {
		for (let x = left; x < right; x += 1) {
			const index = (y * screenshot.width + x) * 4;
			if (
				screenshot.data[index + 3] >= 240 &&
				Math.abs(screenshot.data[index] - color[0]) <= 8 &&
				Math.abs(screenshot.data[index + 1] - color[1]) <= 8 &&
				Math.abs(screenshot.data[index + 2] - color[2]) <= 8
			) {
				matchingPixels += 1;
			}
		}
	}
	const regionPixels = Math.max(0, right - left) * Math.max(0, bottom - top);
	expect(regionPixels, `${targetName} canvas region`).toBeGreaterThan(0);
	expect(
		matchingPixels,
		`${targetName} must render its numeric-ID sentinel color`,
	).toBeGreaterThan(Math.max(16, Math.floor(regionPixels * 0.01)));
}

async function assertCanvasContainsColor(
	page: Page,
	label: string,
	color: readonly [number, number, number],
	tolerance = 8,
): Promise<void> {
	const canvas = page.locator(
		'[data-testid="illustrated-notebook-pixi-host"] canvas',
	);
	const screenshot = PNG.sync.read(await canvas.screenshot());
	let matchingPixels = 0;
	for (let index = 0; index < screenshot.data.length; index += 4) {
		if (
			screenshot.data[index + 3] >= 240 &&
			Math.abs(screenshot.data[index] - color[0]) <= tolerance &&
			Math.abs(screenshot.data[index + 1] - color[1]) <= tolerance &&
			Math.abs(screenshot.data[index + 2] - color[2]) <= tolerance
		) {
			matchingPixels += 1;
		}
	}
	expect(matchingPixels, `${label} sentinel color`).toBeGreaterThan(64);
}

test.describe('issue 1628 notebook person art proof', () => {
	test.beforeAll(() => {
		fs.mkdirSync(PROOF_DIR, { recursive: true });
	});

	test('resolves NPC art by numeric ID when the compatibility name is stale', async ({
		page,
	}) => {
		const brigid = approvedCast.find((npc) => npc.npc_id === BRIGID_NPC_ID);
		expect(brigid?.real_name, 'Brigid fixture compatibility name').toBe(
			STALE_BRIGID_COMPATIBILITY_NAME,
		);
		await installNumericIdentitySentinel(page);
		await setupProofPage(page);

		await assertCanvasContainsColor(
			page,
			'Brigid numeric-ID portrait',
			ID_SENTINEL_COLORS.portrait,
			// The fresh WebGL raster pipeline color-manages the solid sentinel;
			// retain a saturated-magenta proof while allowing that bounded transform.
			48,
		);
		await assertCanvasTargetContainsColor(
			page,
			'Select marker for Brigid Ni Fhatharta',
			ID_SENTINEL_COLORS.marker,
		);
	});

	test('desktop renders approved portraits and unclipped far and near markers', async ({
		page,
	}) => {
		await page.setViewportSize({ width: 1440, height: 900 });
		await setupProofPage(page);

		await assertCanvasTargetIsPainted(
			page,
			'Select nearby person Brigid Ni Fhatharta',
		);
		await assertCanvasTargetIsPainted(
			page,
			'Select marker for Brigid Ni Fhatharta',
			true,
		);
		await assertCanvasTargetIsPainted(
			page,
			'Select marker for Sean Ruadh Kelly',
			true,
		);

		await page.screenshot({
			path: path.join(PROOF_DIR, 'desktop.png'),
			fullPage: false,
		});
	});

	test('mobile renders approved person art in the first viewport', async ({
		page,
	}) => {
		await page.setViewportSize({ width: 390, height: 844 });
		await setupProofPage(page);

		await assertCanvasTargetIsPainted(
			page,
			'Select nearby person Brigid Ni Fhatharta',
		);
		await assertCanvasTargetIsPainted(
			page,
			'Select marker for Sean Ruadh Kelly',
			true,
		);

		await page.screenshot({
			path: path.join(PROOF_DIR, 'mobile.png'),
			fullPage: false,
		});
	});

	test('final contact sheet has 24 approved entries and 48 decoded images', async ({
		page,
	}) => {
		const personArt = await loadPersonArtManifest(page);
		assertApprovedReleaseProvenance(personArt);
		expect(personArt.people, 'named approved roster').toHaveLength(
			FINAL_NAMED_PERSON_ART_ENTRY_COUNT,
		);
		expect(
			personArt.people
				.map((entry) => entry.npc_id)
				.toSorted((left, right) => Number(left) - Number(right)),
			'named roster has exactly NPC IDs 1 through 23',
		).toEqual(
			Array.from(
				{ length: FINAL_NAMED_PERSON_ART_ENTRY_COUNT },
				(_, index) => index + 1,
			),
		);
		expect(personArt.fallback.npc_id, 'fallback has no numeric NPC ID').toBe(
			null,
		);
		const entries = [...personArt.people, personArt.fallback];
		expect(entries, 'named entries plus fallback').toHaveLength(
			FINAL_PERSON_ART_ENTRY_COUNT,
		);
		expect(
			entries.every((entry) => entry.approval_status === 'approved'),
			'every final contact-sheet entry is approved',
		).toBe(true);
		const assetUrls = entries.flatMap((entry) => [
			notebookAssetUrl(entry.portrait),
			notebookAssetUrl(entry.marker),
		]);
		expect(assetUrls, 'portrait and marker requests').toHaveLength(
			FINAL_PERSON_ART_IMAGE_COUNT,
		);
		expect(new Set(assetUrls).size, 'each final art request is unique').toBe(
			FINAL_PERSON_ART_IMAGE_COUNT,
		);
		const contactSheetPath = notebookAssetUrl(
			personArt.contact_sheet_html ?? 'person-art-contact-sheet.html',
		);
		const contactSheetResponse = await page.goto(contactSheetPath);
		expect(contactSheetResponse?.ok(), 'contact sheet request').toBe(true);
		assertDecodedNonBlank(await decodeBrowserImages(page, assetUrls));
		await expect(page.locator('.sheet figure')).toHaveCount(
			FINAL_PERSON_ART_ENTRY_COUNT,
		);
		await expect(page.locator('.sheet img')).toHaveCount(
			FINAL_PERSON_ART_IMAGE_COUNT,
		);

		const contactImages = await page
			.locator('.sheet img')
			.evaluateAll(async (images) =>
				Promise.all(
					images.map(async (element) => {
						const image = element as HTMLImageElement;
						await image.decode();
						const canvas = document.createElement('canvas');
						canvas.width = image.naturalWidth;
						canvas.height = image.naturalHeight;
						const context = canvas.getContext('2d', {
							willReadFrequently: true,
						});
						if (!context) throw new Error(`could not read ${image.src}`);
						context.drawImage(image, 0, 0);
						const pixels = context.getImageData(
							0,
							0,
							canvas.width,
							canvas.height,
						).data;
						let nonBlankPixels = 0;
						for (let index = 0; index < pixels.length; index += 4) {
							if (
								pixels[index + 3] >= 16 &&
								(pixels[index] < 245 ||
									pixels[index + 1] < 245 ||
									pixels[index + 2] < 245)
							) {
								nonBlankPixels += 1;
							}
						}
						return {
							src: image.src,
							width: image.naturalWidth,
							height: image.naturalHeight,
							nonBlankPixels,
						};
					}),
				),
			);
		for (const image of contactImages) {
			expect(image.width, `${image.src} contact width`).toBeGreaterThan(1);
			expect(image.height, `${image.src} contact height`).toBeGreaterThan(1);
			expect(
				image.nonBlankPixels,
				`${image.src} contact sheet pixels`,
			).toBeGreaterThan(24);
		}

		await page.screenshot({
			path: path.join(PROOF_DIR, 'contact-sheet.png'),
			fullPage: true,
		});
	});
});
