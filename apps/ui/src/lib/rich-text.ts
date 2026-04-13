/**
 * Rich text segmentation for chat messages.
 *
 * Splits message content into plain and annotated segments so the chat
 * renderer can apply semantic colours to Irish vocabulary, NPC names,
 * and location names without requiring any backend markup.
 */

export type SegmentKind = 'plain' | 'irish' | 'name' | 'location';

export interface RichSegment {
	text: string;
	kind: SegmentKind;
}

interface MatchRange {
	start: number;
	end: number;
	kind: SegmentKind;
}

/**
 * Splits `content` into segments annotated with a semantic kind.
 *
 * Priority (highest first): irish > location > name.
 * Overlapping matches are resolved by priority; for equal-priority matches
 * the earlier one wins. Matching is case-insensitive and word-boundary aware.
 */
export function segmentText(
	content: string,
	irishWords: string[],
	nameWords: string[],
	locationName: string
): RichSegment[] {
	if (!content) return [];

	const ranges: MatchRange[] = [];

	const addMatches = (words: string[], kind: SegmentKind) => {
		for (const word of words) {
			if (!word) continue;
			// Escape regex special characters
			const escaped = word.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
			const re = new RegExp(`(?<![\\w\\u00C0-\\u024F])${escaped}(?![\\w\\u00C0-\\u024F])`, 'gi');
			let m: RegExpExecArray | null;
			while ((m = re.exec(content)) !== null) {
				ranges.push({ start: m.index, end: m.index + m[0].length, kind });
			}
		}
	};

	// Add in reverse priority order (lower priority first) so higher-priority
	// matches can overwrite during conflict resolution below.
	addMatches(nameWords, 'name');
	if (locationName) addMatches([locationName], 'location');
	addMatches(irishWords, 'irish');

	if (ranges.length === 0) return [{ text: content, kind: 'plain' }];

	// Sort by start position; for ties prefer higher-priority kind.
	const kindPriority: Record<SegmentKind, number> = { irish: 3, location: 2, name: 1, plain: 0 };
	ranges.sort((a, b) => a.start - b.start || kindPriority[b.kind] - kindPriority[a.kind]);

	// Resolve overlaps: keep only non-overlapping ranges (greedy left-to-right,
	// with priority breaking ties already sorted above).
	const resolved: MatchRange[] = [];
	let cursor = 0;
	for (const r of ranges) {
		if (r.start < cursor) continue; // overlaps previous — skip
		resolved.push(r);
		cursor = r.end;
	}

	// Build segment array
	const segments: RichSegment[] = [];
	let pos = 0;
	for (const r of resolved) {
		if (r.start > pos) {
			segments.push({ text: content.slice(pos, r.start), kind: 'plain' });
		}
		segments.push({ text: content.slice(r.start, r.end), kind: r.kind });
		pos = r.end;
	}
	if (pos < content.length) {
		segments.push({ text: content.slice(pos), kind: 'plain' });
	}

	return segments;
}
