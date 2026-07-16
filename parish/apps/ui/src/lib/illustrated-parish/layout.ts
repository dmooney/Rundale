import type { ParishLayout, ParishPoint, ParishRect } from './types';
import { PARISH_PLATE_SIZES } from './assets';

export const PARISH_ACTIONS = [
	'talk',
	'ask',
	'help',
	'observe',
	'leave',
] as const;

const PAGE_ASPECT = 440 / 620;

function rect(x: number, y: number, width: number, height: number): ParishRect {
	return { x, y, width, height };
}

function normalized(
	width: number,
	height: number,
	x: number,
	y: number,
	w: number,
	h: number,
): ParishRect {
	return rect(width * x, height * y, width * w, height * h);
}

function cells(strip: ParishRect, count: number): ParishRect[] {
	const width = strip.width / count;
	return Array.from({ length: count }, (_, index) =>
		rect(strip.x + index * width, strip.y, width, strip.height),
	);
}

export function computeParishLayout(
	width: number,
	height: number,
): ParishLayout {
	return width <= 760
		? mobileLayout(Math.max(1, width), Math.max(1, height))
		: desktopLayout(Math.max(1, width), Math.max(1, height));
}

/** Maps a normalized point on a cover-cropped scene plate into the viewport. */
export function mapPlatePointToViewport(
	viewportWidth: number,
	viewportHeight: number,
	plateWidth: number,
	plateHeight: number,
	point: ParishPoint,
): ParishPoint {
	const scale = Math.max(
		viewportWidth / Math.max(1, plateWidth),
		viewportHeight / Math.max(1, plateHeight),
	);
	const renderedWidth = plateWidth * scale;
	const renderedHeight = plateHeight * scale;
	return {
		x: (viewportWidth - renderedWidth) / 2 + point.x * renderedWidth,
		y: (viewportHeight - renderedHeight) / 2 + point.y * renderedHeight,
	};
}

/** Maps a normalized plate rectangle through the scene's cover crop. */
export function mapPlateRectToViewport(
	viewportWidth: number,
	viewportHeight: number,
	plateWidth: number,
	plateHeight: number,
	plateRect: ParishRect,
): ParishRect {
	const topLeft = mapPlatePointToViewport(
		viewportWidth,
		viewportHeight,
		plateWidth,
		plateHeight,
		{ x: plateRect.x, y: plateRect.y },
	);
	const bottomRight = mapPlatePointToViewport(
		viewportWidth,
		viewportHeight,
		plateWidth,
		plateHeight,
		{
			x: plateRect.x + plateRect.width,
			y: plateRect.y + plateRect.height,
		},
	);
	return rect(
		topLeft.x,
		topLeft.y,
		bottomRight.x - topLeft.x,
		bottomRight.y - topLeft.y,
	);
}

function desktopLayout(width: number, height: number): ParishLayout {
	let pageHeight = height * 0.575;
	let pageWidth = pageHeight * PAGE_ASPECT;
	const tabWidth = Math.min(88, Math.max(78, width * 0.06));
	const tabTuck = tabWidth * 0.4;
	const tabProtrusion = tabWidth - tabTuck;
	// The concept's 22.9% notebook width includes the protruding tabs. Keep the
	// sewn page and its tab handles inside that same overall silhouette.
	const maximumWidth = width * 0.229 - tabProtrusion;
	if (pageWidth > maximumWidth) {
		pageWidth = maximumWidth;
		pageHeight = pageWidth / PAGE_ASPECT;
	}
	const pageRight = width - Math.max(3, width * 0.003) - tabProtrusion;
	const page = rect(
		pageRight - pageWidth,
		height * 0.082,
		pageWidth,
		pageHeight,
	);
	const nearbyRail = normalized(width, height, 0, 0.215, 0.081, 0.589);
	const actionStrip = normalized(width, height, 0.316, 0.79, 0.312, 0.095);
	const tabRailHeight = Math.min(320, page.height * 0.72);
	const tabRail = rect(
		pageRight - tabTuck,
		page.y + (page.height - tabRailHeight) / 2,
		tabWidth,
		tabRailHeight,
	);
	const tabPitch = tabRail.height / 5;
	const tabHitHeight = Math.min(tabPitch, Math.max(44, tabPitch * 0.78));
	const tabHitWidth = Math.max(44, tabProtrusion);
	const tabHitX = pageRight + tabProtrusion - tabHitWidth;
	const plateRect = (x: number, y: number, w: number, h: number) =>
		mapPlateRectToViewport(
			width,
			height,
			PARISH_PLATE_SIZES.desktop.width,
			PARISH_PLATE_SIZES.desktop.height,
			rect(x, y, w, h),
		);
	const clearOfNotebook = (label: ParishRect): ParishRect => {
		const gap = Math.max(6, width * 0.006);
		const rightLimit = page.x - gap;
		if (label.x + label.width <= rightLimit) return label;
		return rect(
			Math.max(0, rightLimit - label.width),
			label.y,
			label.width,
			label.height,
		);
	};

	return {
		mode: 'desktop',
		width,
		height,
		logoCard: normalized(width, height, 0, 0, 0.225, 0.084),
		statusRibbon: normalized(width, height, 0.225, 0, 0.553, 0.061),
		compass: normalized(width, height, 0.778, 0, 0.222, 0.073),
		nearbyRail,
		moreButton: rect(
			nearbyRail.x + 7,
			nearbyRail.y + nearbyRail.height - Math.max(36, height * 0.046),
			nearbyRail.width - 14,
			Math.max(32, height * 0.043),
		),
		notebookPage: page,
		tabRail,
		tabs: Array.from({ length: 5 }, (_, index) =>
			rect(
				tabHitX,
				tabRail.y + index * tabPitch + (tabPitch - tabHitHeight) / 2,
				tabHitWidth,
				tabHitHeight,
			),
		),
		actionStrip,
		actionCells: cells(actionStrip, PARISH_ACTIONS.length),
		intentStrip: normalized(width, height, 0.237, 0.876, 0.486, 0.098),
		mapCard: normalized(width, height, 0, 0.864, 0.084, 0.136),
		timeCard: normalized(width, height, 0.084, 0.864, 0.066, 0.136),
		activeIntentsCard: normalized(width, height, 0.782, 0.863, 0.218, 0.137),
		exitLabels: [
			{
				...clearOfNotebook(plateRect(0.128, 0.119, 0.092, 0.037)),
				label: 'Chapel Lane',
			},
			{
				...clearOfNotebook(plateRect(0.64, 0.425, 0.098, 0.037)),
				label: 'Shop Road',
			},
			{
				...clearOfNotebook(plateRect(0.7, 0.565, 0.063, 0.036)),
				label: 'Bridge',
			},
		],
	};
}

function mobileLayout(width: number, height: number): ParishLayout {
	let pageWidth = width * 0.44;
	let pageHeight = pageWidth / PAGE_ASPECT;
	const maximumHeight = height * 0.326;
	if (pageHeight > maximumHeight) {
		pageHeight = maximumHeight;
		pageWidth = pageHeight * PAGE_ASPECT;
	}
	const tabWidth = Math.min(58, Math.max(52, width * 0.14));
	const tabTuck = tabWidth * 0.48;
	const tabProtrusion = tabWidth - tabTuck;
	const pageRight = width - Math.max(2, width * 0.005) - tabProtrusion;
	const page = rect(
		pageRight - pageWidth,
		height * 0.19,
		pageWidth,
		pageHeight,
	);
	const nearbyRail = normalized(width, height, 0.015, 0.086, 0.97, 0.102);
	const actionStrip = normalized(width, height, 0.015, 0.67, 0.97, 0.085);
	const tabRailHeight = Math.min(228, page.height * 0.9);
	const tabRail = rect(
		pageRight - tabTuck,
		page.y + (page.height - tabRailHeight) / 2,
		tabWidth,
		tabRailHeight,
	);
	const tabPitch = tabRail.height / 5;
	const tabHitHeight = Math.max(42, Math.min(46, tabPitch));
	const tabHitWidth = Math.max(44, tabProtrusion);
	const tabHitX = pageRight + tabProtrusion - tabHitWidth;

	return {
		mode: 'mobile',
		width,
		height,
		logoCard: normalized(width, height, 0.015, 0.01, 0.3, 0.064),
		statusRibbon: normalized(width, height, 0.315, 0.01, 0.67, 0.064),
		compass: normalized(width, height, 0.89, 0.014, 0.08, 0.052),
		nearbyRail,
		moreButton: rect(
			nearbyRail.x + nearbyRail.width - Math.max(52, width * 0.16),
			nearbyRail.y + 5,
			Math.max(48, width * 0.15),
			nearbyRail.height - 10,
		),
		notebookPage: page,
		tabRail,
		tabs: Array.from({ length: 5 }, (_, index) =>
			rect(
				tabHitX,
				tabRail.y + index * tabPitch + (tabPitch - tabHitHeight) / 2,
				tabHitWidth,
				tabHitHeight,
			),
		),
		actionStrip,
		actionCells: cells(actionStrip, PARISH_ACTIONS.length),
		intentStrip: normalized(width, height, 0.015, 0.765, 0.97, 0.092),
		mapCard: normalized(width, height, 0.015, 0.872, 0.21, 0.113),
		timeCard: normalized(width, height, 0.235, 0.872, 0.21, 0.113),
		activeIntentsCard: normalized(width, height, 0.455, 0.872, 0.53, 0.113),
		exitLabels: [],
	};
}
