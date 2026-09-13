function upsertIconLink(rel: string): HTMLLinkElement {
	const selector = `link[rel="${rel}"]`;
	const existing = document.head.querySelector<HTMLLinkElement>(selector);
	if (existing) return existing;

	const link = document.createElement('link');
	link.rel = rel;
	document.head.appendChild(link);
	return link;
}

export function applyAppIcon(
	appIconHref: string | null | undefined,
	faviconHref: string | null | undefined = appIconHref,
): void {
	if (typeof document === 'undefined' || !appIconHref) return;

	const favicon = upsertIconLink('icon');
	favicon.type = 'image/png';
	favicon.href = faviconHref ?? appIconHref;

	const appleTouch = upsertIconLink('apple-touch-icon');
	appleTouch.href = appIconHref;
}
