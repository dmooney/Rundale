import { describe, it, expect } from 'vitest';
import { layoutTree, NODE_W, NODE_H, GAP_X, GAP_Y } from './dag';
import type { SaveBranchDisplay } from '$lib/types';

function branch(name: string, parent: string | null, id = 0): SaveBranchDisplay {
	return {
		name,
		id,
		parent_name: parent,
		snapshot_count: 1,
		latest_location: null,
		latest_game_date: null,
		snapshots: []
	};
}

describe('layoutTree', () => {
	it('returns empty layout for empty input', () => {
		const result = layoutTree([]);
		expect(result.nodes).toHaveLength(0);
		expect(result.edges).toHaveLength(0);
		expect(result.width).toBe(0);
		expect(result.height).toBe(0);
	});

	it('lays out a single root node', () => {
		const result = layoutTree([branch('main', null, 1)]);
		expect(result.nodes).toHaveLength(1);
		expect(result.edges).toHaveLength(0);
		expect(result.nodes[0].branch.name).toBe('main');
		// Single leaf: x = 0 + PAD (40)
		expect(result.nodes[0].x).toBe(40);
		// depth 0, maxDepth 0: y = (0 - 0) * (NODE_H + GAP_Y) = 0 + PAD
		expect(result.nodes[0].y).toBe(40);
	});

	it('lays out a linear chain: root → child → grandchild', () => {
		const branches = [
			branch('main', null, 1),
			branch('dev', 'main', 2),
			branch('feature', 'dev', 3)
		];
		const result = layoutTree(branches);
		expect(result.nodes).toHaveLength(3);
		// Linear chain: 2 edges (main→dev, dev→feature)
		expect(result.edges).toHaveLength(2);

		const byName = Object.fromEntries(result.nodes.map(n => [n.branch.name, n]));

		// All nodes share the same x (single leaf column)
		expect(byName.main.x).toBe(byName.dev.x);
		expect(byName.dev.x).toBe(byName.feature.x);

		// Inverted depth: grandchild (leaf) at top (smallest y),
		// root at bottom (largest y)
		expect(byName.feature.y).toBeLessThan(byName.dev.y);
		expect(byName.dev.y).toBeLessThan(byName.main.y);
	});

	it('lays out a 3-branch fan: root with two children', () => {
		// root
		//  ├─ left
		//  └─ right
		const branches = [
			branch('root', null, 1),
			branch('left', 'root', 2),
			branch('right', 'root', 3)
		];
		const result = layoutTree(branches);
		expect(result.nodes).toHaveLength(3);
		// Two edges: root→left, root→right
		expect(result.edges).toHaveLength(2);

		const byName = Object.fromEntries(result.nodes.map(n => [n.branch.name, n]));

		// Two leaf slots: left gets slot 0, right gets slot 1
		// left.x = 0 * (NODE_W + GAP_X) + PAD = 40
		// right.x = 1 * (NODE_W + GAP_X) + PAD = NODE_W + GAP_X + 40
		expect(byName.left.x).toBe(40);
		expect(byName.right.x).toBe(NODE_W + GAP_X + 40);

		// root is centered between left and right
		const expectedRootX = (byName.left.x + byName.right.x) / 2;
		expect(byName.root.x).toBeCloseTo(expectedRootX);

		// root is deeper (higher y) than its leaves
		expect(byName.root.y).toBeGreaterThan(byName.left.y);
		expect(byName.root.y).toBeGreaterThan(byName.right.y);

		// Siblings share the same depth → same y
		expect(byName.left.y).toBe(byName.right.y);
	});

	it('edge count equals total non-root nodes', () => {
		// 1 root + 3 children: 3 edges
		const branches = [
			branch('root', null, 1),
			branch('a', 'root', 2),
			branch('b', 'root', 3),
			branch('c', 'root', 4)
		];
		const result = layoutTree(branches);
		expect(result.edges).toHaveLength(3);
	});

	it('container dimensions are large enough to contain all nodes', () => {
		const branches = [
			branch('root', null, 1),
			branch('left', 'root', 2),
			branch('right', 'root', 3)
		];
		const result = layoutTree(branches);
		for (const node of result.nodes) {
			expect(node.x + NODE_W).toBeLessThanOrEqual(result.width);
			expect(node.y + NODE_H).toBeLessThanOrEqual(result.height);
		}
	});

	it('edge connects parent top-center to child bottom-center', () => {
		const branches = [
			branch('root', null, 1),
			branch('child', 'root', 2)
		];
		const result = layoutTree(branches);
		expect(result.edges).toHaveLength(1);
		const [edge] = result.edges;

		const byName = Object.fromEntries(result.nodes.map(n => [n.branch.name, n]));
		// x1 = root.x + NODE_W/2, y1 = root.y (top of root)
		expect(edge.x1).toBeCloseTo(byName.root.x + NODE_W / 2);
		expect(edge.y1).toBe(byName.root.y);
		// x2 = child.x + NODE_W/2, y2 = child.y + NODE_H (bottom of child)
		expect(edge.x2).toBeCloseTo(byName.child.x + NODE_W / 2);
		expect(edge.y2).toBe(byName.child.y + NODE_H);
	});

	it('handles disconnected nodes (no parent match) as separate roots', () => {
		// Two branches with no parent_name — treated as two separate roots
		const branches = [
			branch('alpha', null, 1),
			branch('beta', null, 2)
		];
		const result = layoutTree(branches);
		expect(result.nodes).toHaveLength(2);
		expect(result.edges).toHaveLength(0);
		// Each is a leaf; should be placed in adjacent columns
		const byName = Object.fromEntries(result.nodes.map(n => [n.branch.name, n]));
		expect(byName.alpha.x).not.toBe(byName.beta.x);
	});
});
