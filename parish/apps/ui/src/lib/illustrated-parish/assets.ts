const ASSET_BASE = '/rundale/illustrated-notebook-v2/';

export const PARISH_UI_ASSET_MANIFEST = `${ASSET_BASE}ui-assets.json`;

export const PARISH_PLATE_SIZES = {
	desktop: { width: 1672, height: 941 },
	mobile: { width: 941, height: 1672 },
} as const;

export const PARISH_ASSETS = {
	sceneDesktop: `${ASSET_BASE}parish-crossroads-watercolor.png`,
	sceneMobile: `${ASSET_BASE}parish-crossroads-watercolor-mobile.png`,
	sewnPage: `${ASSET_BASE}sewn-notebook-page.png`,
	topRibbon: `${ASSET_BASE}parchment-top-ribbon.png`,
	nearbyRail: `${ASSET_BASE}parchment-nearby-rail.png`,
	actionStrip: `${ASSET_BASE}parchment-action-strip.png`,
	intentStrip: `${ASSET_BASE}parchment-intent-strip.png`,
	smallCard: `${ASSET_BASE}parchment-small-card.png`,
	activeIntentsCard: `${ASSET_BASE}parchment-active-intents-card.png`,
	tab: `${ASSET_BASE}parchment-tab.png`,
	label: `${ASSET_BASE}parchment-label.png`,
	portraitFrame: `${ASSET_BASE}portrait-slot-frame.png`,
	actionIcons: {
		talk: `${ASSET_BASE}icon-talk.png`,
		ask: `${ASSET_BASE}icon-ask.png`,
		help: `${ASSET_BASE}icon-help.png`,
		observe: `${ASSET_BASE}icon-observe.png`,
		leave: `${ASSET_BASE}icon-leave.png`,
	},
	mapIcon: `${ASSET_BASE}icon-map.png`,
	timeIcon: `${ASSET_BASE}icon-time.png`,
	quillIcon: `${ASSET_BASE}icon-quill.png`,
} as const;

export const PARISH_ASSET_URLS = [
	PARISH_ASSETS.sceneDesktop,
	PARISH_ASSETS.sceneMobile,
	PARISH_ASSETS.sewnPage,
	PARISH_ASSETS.topRibbon,
	PARISH_ASSETS.nearbyRail,
	PARISH_ASSETS.actionStrip,
	PARISH_ASSETS.intentStrip,
	PARISH_ASSETS.smallCard,
	PARISH_ASSETS.activeIntentsCard,
	PARISH_ASSETS.tab,
	PARISH_ASSETS.label,
	PARISH_ASSETS.portraitFrame,
	...Object.values(PARISH_ASSETS.actionIcons),
	PARISH_ASSETS.mapIcon,
	PARISH_ASSETS.timeIcon,
	PARISH_ASSETS.quillIcon,
];
