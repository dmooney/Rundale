/**
 * Label collision avoidance for the map panel.
 *
 * Uses a greedy 8-position candidate model (Imhof cartographic convention)
 * followed by iterative force refinement. Each label picks the best of 8
 * positions around its node, scored against already-placed labels and all
 * node circles, then a cleanup pass resolves any remaining overlaps.
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

const PADDING = 4;
const NODE_MARGIN = 4;
const GAP = 4;
/** Imhof preference penalty per position: NE, E, SE, S, SW, W, NW, N */
const POSITION_PREF = [0, 1, 1, 2, 4, 3, 3, 2];

/** Area of overlap between two axis-aligned rectangles (0 if none). */
function rectOverlap(
	ax: number, ay: number, aw: number, ah: number,
	bx: number, by: number, bw: number, bh: number
): number {
	const ox = (aw + bw) / 2 - Math.abs(ax - bx);
	const oy = (ah + bh) / 2 - Math.abs(ay - by);
	return ox > 0 && oy > 0 ? ox * oy : 0;
}

/** Generate 8 candidate label center positions around a node. */
function candidates(
	nx: number, ny: number, r: number, w: number, h: number
): [number, number][] {
	const g = GAP;
	return [
		[nx + r + g + w / 2, ny - r - g - h / 2],     // 0: NE
		[nx + r + g + w / 2, ny],                       // 1: E
		[nx + r + g + w / 2, ny + r + g + h / 2],      // 2: SE
		[nx,                 ny + r + g + h / 2],        // 3: S
		[nx - r - g - w / 2, ny + r + g + h / 2],      // 4: SW
		[nx - r - g - w / 2, ny],                       // 5: W
		[nx - r - g - w / 2, ny - r - g - h / 2],      // 6: NW
		[nx,                 ny - r - g - h / 2],        // 7: N
	];
}

/** An edge between two node positions, used for label-edge overlap avoidance. */
export interface EdgeLine {
	x1: number; y1: number;
	x2: number; y2: number;
}

/** Minimum distance from a point to a line segment, squared. */
function pointSegDistSq(px: number, py: number, x1: number, y1: number, x2: number, y2: number): number {
	const dx = x2 - x1, dy = y2 - y1;
	const lenSq = dx * dx + dy * dy;
	if (lenSq === 0) return (px - x1) ** 2 + (py - y1) ** 2;
	const t = Math.max(0, Math.min(1, ((px - x1) * dx + (py - y1) * dy) / lenSq));
	const projX = x1 + t * dx, projY = y1 + t * dy;
	return (px - projX) ** 2 + (py - projY) ** 2;
}

/** Check if a rectangle overlaps a line segment (approximate: test center + corners). */
function rectEdgeOverlap(cx: number, cy: number, w: number, h: number, edge: EdgeLine): boolean {
	const hw = w / 2, hh = h / 2;
	const threshold = (Math.min(hw, hh)) ** 2;
	// Check if the segment passes near the center or any corner
	if (pointSegDistSq(cx, cy, edge.x1, edge.y1, edge.x2, edge.y2) < threshold) return true;
	for (const [px, py] of [[cx - hw, cy - hh], [cx + hw, cy - hh], [cx - hw, cy + hh], [cx + hw, cy + hh]] as [number, number][]) {
		if (pointSegDistSq(px, py, edge.x1, edge.y1, edge.x2, edge.y2) < (hh * 0.5) ** 2) return true;
	}
	return false;
}

/**
 * Resolve label positions using greedy 8-position candidates + force refinement.
 */
export function resolveLabels(
	inputs: LabelInput[],
	boundsW: number,
	boundsH: number,
	edges: EdgeLine[] = []
): ResolvedLabel[] {
	if (inputs.length === 0) return [];

	const placed: ResolvedLabel[] = [];

	// Phase 1: Greedy candidate selection
	for (const inp of inputs) {
		const w = inp.textW + PADDING * 2;
		const h = inp.textH + PADDING;
		const cands = candidates(inp.nodeX, inp.nodeY, inp.nodeR, w, h);

		let bestScore = Infinity;
		let bestCx = inp.nodeX;
		let bestCy = inp.nodeY + inp.nodeR + GAP + h / 2;

		for (let ci = 0; ci < cands.length; ci++) {
			const [cx, cy] = cands[ci];
			let penalty = POSITION_PREF[ci];

			// Out-of-bounds penalty
			if (cx - w / 2 < 0 || cx + w / 2 > boundsW ||
				cy - h / 2 < 0 || cy + h / 2 > boundsH) {
				penalty += 500;
			}

			// Overlap with already-placed labels
			for (const p of placed) {
				penalty += rectOverlap(cx, cy, w, h, p.cx, p.cy, p.w, p.h) * 10;
			}

			// Overlap with ALL node circles
			for (const node of inputs) {
				const nw = node.nodeR * 2 + NODE_MARGIN * 2;
				penalty += rectOverlap(cx, cy, w, h, node.nodeX, node.nodeY, nw, nw) * 8;
			}

			// Overlap with graph edges
			for (const edge of edges) {
				if (rectEdgeOverlap(cx, cy, w, h, edge)) penalty += 5;
			}

			if (penalty < bestScore) {
				bestScore = penalty;
				bestCx = cx;
				bestCy = cy;
			}
		}

		placed.push({ ax: inp.nodeX, ay: inp.nodeY, cx: bestCx, cy: bestCy, w, h });
	}

	// Phase 2: Iterative push-apart for any remaining label-label overlaps
	for (let iter = 0; iter < 20; iter++) {
		let anyOverlap = false;
		for (let i = 0; i < placed.length; i++) {
			for (let j = i + 1; j < placed.length; j++) {
				const a = placed[i], b = placed[j];
				const ox = (a.w + b.w) / 2 - Math.abs(a.cx - b.cx);
				const oy = (a.h + b.h) / 2 - Math.abs(a.cy - b.cy);
				if (ox <= 0 || oy <= 0) continue;
				anyOverlap = true;
				if (ox < oy) {
					const push = ox / 2 + 1;
					if (a.cx <= b.cx) { a.cx -= push; b.cx += push; }
					else              { a.cx += push; b.cx -= push; }
				} else {
					const push = oy / 2 + 1;
					if (a.cy <= b.cy) { a.cy -= push; b.cy += push; }
					else              { a.cy += push; b.cy -= push; }
				}
			}
		}
		if (!anyOverlap) break;
	}

	// Clamp to bounds
	for (const label of placed) {
		const hw = label.w / 2, hh = label.h / 2;
		label.cx = Math.max(hw, Math.min(boundsW - hw, label.cx));
		label.cy = Math.max(hh, Math.min(boundsH - hh, label.cy));
	}

	return placed;
}

/** Distance squared between two points. */
export function distSq(ax: number, ay: number, bx: number, by: number): number {
	return (ax - bx) ** 2 + (ay - by) ** 2;
}

/** Approximate text width for SVG labels (roughly 0.6em per char). */
export function estimateTextWidth(name: string, maxChars = 14, fontSize = 7): number {
	const display = Math.min(name.length, maxChars);
	return display * fontSize * 0.6;
}
