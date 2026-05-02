/**
 * Pure DAG layout for the save-picker branch tree.
 *
 * No Svelte or DOM dependencies — safe to unit-test in Node/jsdom.
 */

import type { SaveBranchDisplay } from '$lib/types';

export const NODE_W = 160;
export const NODE_H = 70;
export const GAP_X = 24;
export const GAP_Y = 44;

export interface TreeNode {
	branch: SaveBranchDisplay;
	children: TreeNode[];
	x: number;
	y: number;
	span: number; // number of leaf-slots this subtree occupies
}

export interface LayoutResult {
	nodes: TreeNode[];
	width: number;
	height: number;
	edges: { x1: number; y1: number; x2: number; y2: number }[];
}

export function layoutTree(branches: SaveBranchDisplay[]): LayoutResult {
	if (branches.length === 0) return { nodes: [], width: 0, height: 0, edges: [] };

	// Build tree structure
	function buildNode(branch: SaveBranchDisplay): TreeNode {
		const children = branches
			.filter(b => b.parent_name === branch.name)
			.map(b => buildNode(b));
		return { branch, children, x: 0, y: 0, span: 0 };
	}

	const roots = branches
		.filter(b => b.parent_name === null)
		.map(b => buildNode(b));

	// If somehow no roots, treat all as roots
	const tree = roots.length > 0 ? roots : branches.map(b => buildNode(b));

	// Compute spans (how many leaf slots each subtree needs)
	function computeSpan(node: TreeNode): number {
		if (node.children.length === 0) {
			node.span = 1;
		} else {
			node.span = node.children.reduce((sum, c) => sum + computeSpan(c), 0);
		}
		return node.span;
	}

	for (const root of tree) {
		computeSpan(root);
	}

	// Compute depth (for y positioning)
	let maxDepth = 0;
	function computeDepth(node: TreeNode, depth: number) {
		if (depth > maxDepth) maxDepth = depth;
		for (const child of node.children) {
			computeDepth(child, depth + 1);
		}
	}
	for (const root of tree) {
		computeDepth(root, 0);
	}

	// Assign x positions: each leaf gets a slot, parents center over children
	let leafSlot = 0;
	function assignX(node: TreeNode) {
		if (node.children.length === 0) {
			node.x = leafSlot * (NODE_W + GAP_X);
			leafSlot++;
		} else {
			for (const child of node.children) {
				assignX(child);
			}
			// Center parent over children
			const firstChild = node.children[0];
			const lastChild = node.children[node.children.length - 1];
			node.x = (firstChild.x + lastChild.x) / 2;
		}
	}

	for (const root of tree) {
		assignX(root);
	}

	// Assign y positions: root at bottom (maxDepth), leaves at top (0)
	// Inverted: depth 0 is at the bottom of the container
	function assignY(node: TreeNode, depth: number) {
		node.y = (maxDepth - depth) * (NODE_H + GAP_Y);
		for (const child of node.children) {
			assignY(child, depth + 1);
		}
	}
	for (const root of tree) {
		assignY(root, 0);
	}

	// Collect all nodes flat
	const allNodes: TreeNode[] = [];
	function collectNodes(node: TreeNode) {
		allNodes.push(node);
		for (const child of node.children) {
			collectNodes(child);
		}
	}
	for (const root of tree) {
		collectNodes(root);
	}

	// Compute container size with padding for badges and breathing room
	const PAD = 40;
	let maxX = 0;
	for (const n of allNodes) {
		if (n.x + NODE_W > maxX) maxX = n.x + NODE_W;
	}
	// Offset all nodes to add padding
	for (const n of allNodes) {
		n.x += PAD;
		n.y += PAD;
	}
	const width = maxX + PAD * 2;
	const height = (maxDepth + 1) * (NODE_H + GAP_Y) - GAP_Y + PAD * 2;

	// Compute edges AFTER offset (parent top-center → child bottom-center)
	const edges: { x1: number; y1: number; x2: number; y2: number }[] = [];
	function collectEdges(node: TreeNode) {
		for (const child of node.children) {
			edges.push({
				x1: node.x + NODE_W / 2,
				y1: node.y,
				x2: child.x + NODE_W / 2,
				y2: child.y + NODE_H
			});
			collectEdges(child);
		}
	}
	for (const root of tree) {
		collectEdges(root);
	}

	return { nodes: allNodes, width, height, edges };
}
