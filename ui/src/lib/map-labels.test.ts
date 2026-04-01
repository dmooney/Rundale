import { describe, it, expect } from 'vitest';
import { resolveLabels, distSq, estimateTextWidth } from './map-labels';
import type { LabelInput, ResolvedLabel } from './map-labels';

function labelInput(x: number, y: number, textW = 40): LabelInput {
	return { nodeX: x, nodeY: y, nodeR: 5, textW, textH: 7 };
}

/** Check whether two resolved labels overlap. */
function overlaps(a: ResolvedLabel, b: ResolvedLabel): boolean {
	const overlapX = (a.w + b.w) / 2 - Math.abs(a.cx - b.cx);
	const overlapY = (a.h + b.h) / 2 - Math.abs(a.cy - b.cy);
	return overlapX > 0 && overlapY > 0;
}

describe('resolveLabels', () => {
	it('returns empty for empty input', () => {
		expect(resolveLabels([], 320, 240)).toEqual([]);
	});

	it('returns one label with correct anchor', () => {
		const result = resolveLabels([labelInput(100, 50)], 320, 240);
		expect(result).toHaveLength(1);
		expect(result[0].ax).toBe(100);
		expect(result[0].ay).toBe(50);
	});

	it('does not overlap when labels are far apart', () => {
		const result = resolveLabels(
			[labelInput(50, 30), labelInput(250, 200)],
			320,
			240
		);
		expect(result).toHaveLength(2);
		expect(overlaps(result[0], result[1])).toBe(false);
	});

	it('separates overlapping labels at the same position', () => {
		const result = resolveLabels(
			[labelInput(150, 100), labelInput(152, 102), labelInput(148, 101)],
			320,
			240
		);
		for (let i = 0; i < result.length; i++) {
			for (let j = i + 1; j < result.length; j++) {
				expect(overlaps(result[i], result[j])).toBe(false);
			}
		}
	});

	it('separates a cluster of five labels', () => {
		const result = resolveLabels(
			[
				labelInput(150, 100),
				labelInput(153, 103),
				labelInput(147, 98),
				labelInput(155, 101),
				labelInput(149, 105)
			],
			320,
			240
		);
		for (let i = 0; i < result.length; i++) {
			for (let j = i + 1; j < result.length; j++) {
				expect(overlaps(result[i], result[j])).toBe(false);
			}
		}
	});

	it('clamps labels within bounds', () => {
		const boundsW = 200;
		const boundsH = 100;
		const result = resolveLabels(
			[labelInput(5, 90), labelInput(195, 90)],
			boundsW,
			boundsH
		);
		for (const label of result) {
			expect(label.cx - label.w / 2).toBeGreaterThanOrEqual(0);
			expect(label.cx + label.w / 2).toBeLessThanOrEqual(boundsW);
			expect(label.cy - label.h / 2).toBeGreaterThanOrEqual(0);
			expect(label.cy + label.h / 2).toBeLessThanOrEqual(boundsH);
		}
	});

	it('preserves anchors after nudging', () => {
		const result = resolveLabels(
			[labelInput(100, 50), labelInput(102, 52)],
			320,
			240
		);
		expect(result[0].ax).toBe(100);
		expect(result[0].ay).toBe(50);
		expect(result[1].ax).toBe(102);
		expect(result[1].ay).toBe(52);
	});
});

describe('distSq', () => {
	it('computes distance squared correctly', () => {
		expect(distSq(0, 0, 3, 4)).toBeCloseTo(25);
	});

	it('returns 0 for same point', () => {
		expect(distSq(5, 5, 5, 5)).toBe(0);
	});
});

describe('estimateTextWidth', () => {
	it('estimates width proportional to character count', () => {
		expect(estimateTextWidth('Hello')).toBeCloseTo(21); // 5 * 7 * 0.6
	});

	it('caps at maxChars', () => {
		const long = 'A very long location name';
		expect(estimateTextWidth(long, 14)).toBeCloseTo(58.8); // 14 * 7 * 0.6
	});

	it('scales with font size', () => {
		expect(estimateTextWidth('Hello', 14, 11)).toBeCloseTo(33); // 5 * 11 * 0.6
	});
});
