const ASSET_BASE = '/rundale/illustrated-notebook-v2/';

export const PARISH_PLATE_SIZES = {
	desktop: { width: 1672, height: 941 },
	mobile: { width: 941, height: 1672 },
} as const;

export const PARISH_ASSETS = {
	sceneDesktop: `${ASSET_BASE}parish-crossroads-watercolor.png`,
	sceneMobile: `${ASSET_BASE}parish-crossroads-watercolor-mobile.png`,
	sewnPage: `${ASSET_BASE}sewn-notebook-page.png`,
} as const;

export const PARISH_ASSET_URLS = [
	PARISH_ASSETS.sceneDesktop,
	PARISH_ASSETS.sceneMobile,
	PARISH_ASSETS.sewnPage,
];
