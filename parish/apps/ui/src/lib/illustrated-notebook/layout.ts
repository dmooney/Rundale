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

const DESKTOP_MIN_WIDTH = 1100;
const DESKTOP_MIN_HEIGHT = 700;
const COMPACT_LANDSCAPE_MAX_HEIGHT = 500;

export function computeNotebookLayout(
	width: number,
	height: number,
): NotebookLayout {
	const mobile = width < DESKTOP_MIN_WIDTH || height < DESKTOP_MIN_HEIGHT;
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
	// Keep the centered intent clear of the two lower side-control groups.
	// At the 1100px desktop boundary this leaves a 14px gutter to the task card.
	const intentWidth = Math.min(
		700,
		Math.max(420, Math.min(width * 0.48, width - 660)),
	);
	const intentHeight = Math.min(82, Math.max(64, height * 0.09));
	const intent = rect(
		(width - intentWidth) / 2,
		height - intentHeight - 20,
		intentWidth,
		intentHeight,
	);
	const stampSize = Math.min(92, Math.max(72, width * 0.064));
	const gap = 6;
	const totalStamps =
		NOTEBOOK_ACTIONS.length * stampSize + (NOTEBOOK_ACTIONS.length - 1) * gap;
	const stampY = intent.y - stampSize - 8;
	const stampX = (width - totalStamps) / 2;
	const stamps = NOTEBOOK_ACTIONS.map((_, i) =>
		rect(stampX + i * (stampSize + gap), stampY, stampSize, stampSize),
	);
	const chronicleX = nearby.x + nearby.width + 24;
	const liveChronicle = rect(
		chronicleX,
		topHeight + 18,
		Math.max(260, page.x - chronicleX - 24),
		178,
	);
	const tabX = Math.min(width - 94, page.x + page.width - 4);

	return {
		mode: 'desktop',
		width,
		height,
		topRibbon: rect(0, 0, width, topHeight),
		nearbyStrip: nearby,
		notebookPage: page,
		tabs: Array.from({ length: 5 }, (_, i) =>
			rect(tabX, page.y + 24 + i * 86, 86, 62),
		),
		liveChronicle,
		actionStamps: stamps,
		intentStrip: intent,
		mapCard: rect(10, height - 104, 102, 102),
		timeCard: rect(118, height - 104, 102, 102),
		activeIntentsCard: rect(width - 316, height - 106, 306, 104),
	};
}

function mobileLayout(width: number, height: number): NotebookLayout {
	const pad = 8;
	const compactLandscape =
		height < COMPACT_LANDSCAPE_MAX_HEIGHT && width > height;
	const topHeight = compactLandscape ? 58 : 70;
	const nearbyHeight = compactLandscape ? 78 : 104;
	const intentHeight = compactLandscape ? 56 : 88;
	const stampGap = compactLandscape ? 4 : 4;
	const stampSize = compactLandscape
		? Math.max(40, Math.min(46, (width - 40) / 5))
		: Math.max(58, Math.min(68, (width - 24) / 5));
	const intent = rect(
		pad,
		height - intentHeight - 10,
		width - pad * 2,
		intentHeight,
	);
	const totalStamps =
		NOTEBOOK_ACTIONS.length * stampSize +
		(NOTEBOOK_ACTIONS.length - 1) * stampGap;
	const stampY = intent.y - stampSize - 6;
	const stampX = (width - totalStamps) / 2;
	const nearby = rect(pad, topHeight + 4, width - pad * 2, nearbyHeight);
	const pageY = nearby.y + nearby.height + 8;
	const pageWidth = compactLandscape
		? Math.min(186, width * 0.3)
		: Math.min(186, width * 0.48);
	const compactPageHeight = Math.max(88, stampY - pageY - 8);
	const pageHeight = compactLandscape
		? Math.min(112, compactPageHeight)
		: Math.min(276, height * 0.34);
	const page = rect(
		width - pageWidth - (compactLandscape ? 50 : 5),
		pageY,
		pageWidth,
		pageHeight,
	);
	const activeTaskGap = 8;
	const activeTask = rect(
		pad,
		pageY,
		Math.min(184, Math.max(1, page.x - pad - activeTaskGap)),
		compactLandscape ? 44 : Math.min(64, Math.max(44, height * 0.075)),
	);
	let liveChronicle: NotebookRect;
	if (compactLandscape) {
		const chronicleX = activeTask.x + activeTask.width + 8;
		liveChronicle = rect(
			chronicleX,
			pageY,
			Math.max(1, page.x - chronicleX - 8),
			Math.min(page.height, Math.max(48, stampY - pageY - 8)),
		);
	} else {
		const belowPageY = page.y + page.height + 8;
		const belowPageHeight = stampY - belowPageY - 8;
		if (belowPageHeight >= 96) {
			liveChronicle = rect(
				10,
				belowPageY,
				width - 20,
				Math.min(172, belowPageHeight),
			);
		} else {
			const chronicleY = activeTask.y + activeTask.height + 8;
			liveChronicle = rect(
				pad,
				chronicleY,
				Math.max(1, page.x - pad * 3),
				Math.min(172, Math.max(48, stampY - chronicleY - 8)),
			);
		}
	}
	const mobileTabX = Math.min(width - 50, page.x + page.width - 3);

	return {
		mode: 'mobile',
		width,
		height,
		topRibbon: rect(0, 0, width, topHeight),
		nearbyStrip: nearby,
		notebookPage: page,
		tabs: compactLandscape
			? []
			: Array.from({ length: 5 }, (_, i) =>
					rect(mobileTabX, page.y + 12 + i * 40, 48, 30),
				),
		liveChronicle,
		actionStamps: NOTEBOOK_ACTIONS.map((_, i) =>
			rect(stampX + i * (stampSize + stampGap), stampY, stampSize, stampSize),
		),
		intentStrip: intent,
		mapCard: null,
		timeCard: null,
		activeIntentsCard: activeTask,
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
