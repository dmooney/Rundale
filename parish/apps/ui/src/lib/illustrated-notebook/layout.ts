import type {
	DepthBand,
	NotebookLayout,
	NotebookRect,
	SceneAnchor,
} from './types';

export const NOTEBOOK_ACTIONS = [
	'talk',
	'ask',
	'help',
	'observe',
	'leave',
] as const;

export function computeNotebookLayout(
	width: number,
	height: number,
): NotebookLayout {
	const mobile = width < 760;
	if (mobile) return mobileLayout(width, height);
	return desktopLayout(width, height);
}

function rect(
	x: number,
	y: number,
	width: number,
	height: number,
): NotebookRect {
	return { x, y, width, height };
}

function desktopLayout(width: number, height: number): NotebookLayout {
	const topHeight = Math.min(76, Math.max(58, height * 0.085));
	const pageWidth = Math.min(390, Math.max(318, width * 0.255));
	const pageHeight = Math.min(560, Math.max(430, height * 0.58));
	const page = rect(
		width - pageWidth - 54,
		topHeight + 18,
		pageWidth,
		pageHeight,
	);
	const nearby = rect(
		8,
		Math.max(126, topHeight + 86),
		Math.min(150, width * 0.105),
		Math.min(535, height * 0.62),
	);
	const intentWidth = Math.min(700, Math.max(500, width * 0.48));
	const intentHeight = Math.min(82, Math.max(64, height * 0.09));
	const intent = rect(
		(width - intentWidth) / 2,
		height - intentHeight - 20,
		intentWidth,
		intentHeight,
	);
	const stampSize = Math.min(92, Math.max(72, width * 0.064));
	const gap = -2;
	const totalStamps =
		NOTEBOOK_ACTIONS.length * stampSize + (NOTEBOOK_ACTIONS.length - 1) * gap;
	const stampY = intent.y - stampSize + 6;
	const stampX = (width - totalStamps) / 2;
	const stamps = NOTEBOOK_ACTIONS.map((_, i) =>
		rect(stampX + i * (stampSize + gap), stampY, stampSize, stampSize),
	);

	return {
		mode: 'desktop',
		width,
		height,
		topRibbon: rect(0, 0, width, topHeight),
		nearbyStrip: nearby,
		notebookPage: page,
		tabs: Array.from({ length: 5 }, (_, i) =>
			rect(page.x + page.width - 4, page.y + 24 + i * 86, 86, 62),
		),
		actionStamps: stamps,
		intentStrip: intent,
		mapCard: rect(10, height - 104, 102, 102),
		timeCard: rect(118, height - 104, 102, 102),
		activeIntentsCard: rect(width - 316, height - 106, 306, 104),
	};
}

function mobileLayout(width: number, height: number): NotebookLayout {
	const pad = 8;
	const topHeight = 70;
	const nearbyHeight = 104;
	const intentHeight = 88;
	const stampSize = Math.max(58, Math.min(68, (width - 24) / 5));
	const intent = rect(
		pad,
		height - intentHeight - 10,
		width - pad * 2,
		intentHeight,
	);
	const stampY = intent.y - stampSize + 6;
	const stampX = (width - stampSize * 5) / 2;
	const pageWidth = Math.min(186, width * 0.48);
	const pageHeight = Math.min(276, height * 0.34);
	const page = rect(
		width - pageWidth - 5,
		topHeight + nearbyHeight - 14,
		pageWidth,
		pageHeight,
	);

	return {
		mode: 'mobile',
		width,
		height,
		topRibbon: rect(0, 0, width, topHeight),
		nearbyStrip: rect(pad, topHeight + 4, width - pad * 2, nearbyHeight),
		notebookPage: page,
		tabs: Array.from({ length: 5 }, (_, i) =>
			rect(page.x + page.width - 3, page.y + 12 + i * 45, 48, 34),
		),
		actionStamps: NOTEBOOK_ACTIONS.map((_, i) =>
			rect(stampX + i * stampSize, stampY, stampSize, stampSize),
		),
		intentStrip: intent,
		mapCard: null,
		timeCard: null,
		activeIntentsCard: null,
	};
}

export function scaleForDepth(depth: number, bands: DepthBand[]): number {
	const clamped = Math.max(0, Math.min(1, depth));
	const band = bands.find(
		(b) => clamped >= b.min_depth && clamped <= b.max_depth,
	);
	if (band) return band.marker_scale;
	const sorted = [...bands].sort((a, b) => a.min_depth - b.min_depth);
	if (sorted.length === 0) return 0.75;
	if (clamped < sorted[0].min_depth) return sorted[0].marker_scale;
	return sorted[sorted.length - 1].marker_scale;
}

export function sortAnchorsByDepth<T extends SceneAnchor>(anchors: T[]): T[] {
	return [...anchors].sort((a, b) => a.depth - b.depth);
}

export function pointFromAnchor(
	anchor: SceneAnchor,
	width: number,
	height: number,
) {
	return {
		x: anchor.x * width,
		y: anchor.y * height,
	};
}
