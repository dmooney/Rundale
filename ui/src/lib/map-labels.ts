/**
 * Label collision avoidance for the map panel.
 *
 * Resolves label positions using iterative force-directed repulsion so that
 * labels near clustered locations do not overlap. Displaced labels get leader
 * lines drawn back to their node anchors.
 */

/** A label with anchor (node center) and resolved display position. */
export interface ResolvedLabel {
	/** Node center x (anchor). */
	ax: number;
	/** Node center y (anchor). */
	ay: number;
	/** Resolved label center x after nudging. */
	cx: number;
	/** Resolved label center y after nudging. */
	cy: number;
	/** Label bounding box width. */
	w: number;
	/** Label bounding box height. */
	h: number;
}

/** Input for label resolution: node position + label dimensions. */
export interface LabelInput {
	/** Node center x. */
	nodeX: number;
	/** Node center y. */
	nodeY: number;
	/** Node radius (used for initial offset). */
	nodeR: number;
	/** Text width estimate. */
	textW: number;
	/** Text height. */
	textH: number;
}

/**
 * Resolve label positions using iterative force-directed repulsion.
 *
 * Each label starts centered below its node. Overlapping labels are pushed
 * apart over 10 iterations along the axis of least overlap. Labels are
 * clamped to stay within the given bounds (0,0)-(boundsW, boundsH).
 */
export function resolveLabels(
	inputs: LabelInput[],
	boundsW: number,
	boundsH: number
): ResolvedLabel[] {
	const padding = 2;
	const resolved: ResolvedLabel[] = inputs.map((inp) => ({
		ax: inp.nodeX,
		ay: inp.nodeY,
		cx: inp.nodeX,
		cy: inp.nodeY + inp.nodeR + 4 + inp.textH / 2,
		w: inp.textW + padding * 2,
		h: inp.textH + padding
	}));

	// Iterative repulsion — push overlapping labels apart
	for (let iter = 0; iter < 10; iter++) {
		let anyOverlap = false;
		for (let i = 0; i < resolved.length; i++) {
			for (let j = i + 1; j < resolved.length; j++) {
				const a = resolved[i];
				const b = resolved[j];

				const overlapX = (a.w + b.w) / 2 - Math.abs(a.cx - b.cx);
				const overlapY = (a.h + b.h) / 2 - Math.abs(a.cy - b.cy);

				if (overlapX <= 0 || overlapY <= 0) continue;
				anyOverlap = true;

				// Push apart along axis of least overlap
				if (overlapX < overlapY) {
					const push = overlapX / 2 + 0.5;
					if (a.cx <= b.cx) {
						a.cx -= push;
						b.cx += push;
					} else {
						a.cx += push;
						b.cx -= push;
					}
				} else {
					const push = overlapY / 2 + 0.5;
					if (a.cy <= b.cy) {
						a.cy -= push;
						b.cy += push;
					} else {
						a.cy += push;
						b.cy -= push;
					}
				}
			}
		}
		if (!anyOverlap) break;
	}

	// Clamp to bounds
	for (const label of resolved) {
		const hw = label.w / 2;
		const hh = label.h / 2;
		label.cx = Math.max(hw, Math.min(boundsW - hw, label.cx));
		label.cy = Math.max(hh, Math.min(boundsH - hh, label.cy));
	}

	return resolved;
}

/** Distance squared between two points. */
export function distSq(ax: number, ay: number, bx: number, by: number): number {
	return (ax - bx) ** 2 + (ay - by) ** 2;
}

/** Approximate text width for SVG labels (roughly 4px per char at 7px font). */
export function estimateTextWidth(name: string, maxChars = 14): number {
	const display = Math.min(name.length, maxChars);
	return display * 4;
}
