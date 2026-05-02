import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import { tick } from 'svelte';

/** Flush all pending microtasks + Svelte ticks. */
async function flush() {
	await Promise.resolve();
	await tick();
	await Promise.resolve();
	await tick();
}
import { savePickerVisible, saveFiles, currentSaveState } from '../stores/save';
import SavePicker from './SavePicker.svelte';
import type { SaveBranchDisplay, SaveFileInfo, SaveState } from '$lib/types';

// jsdom does not implement Element.scrollIntoView; stub it to avoid unhandled rejections.
Element.prototype.scrollIntoView = vi.fn();

// ── IPC mock ────────────────────────────────────────────────────────────────
// All IPC calls are mocked; test-specific overrides via mockReturnValue/mockResolvedValue.

const mockDiscoverSaveFiles = vi.fn<() => Promise<SaveFileInfo[]>>(() => Promise.resolve([]));
const mockGetSaveState = vi.fn<() => Promise<SaveState | null>>(() => Promise.resolve(null));
const mockLoadBranch = vi.fn<(filePath: string, branchId: number) => Promise<void>>(() => Promise.resolve());
const mockCreateBranch = vi.fn<(name: string, parentBranchId: number) => Promise<string>>(() => Promise.resolve('new-id'));
const mockGetWorldSnapshot = vi.fn(() => Promise.resolve({}));
const mockGetMap = vi.fn(() => Promise.resolve({}));
const mockGetNpcsHere = vi.fn(() => Promise.resolve([]));
const mockSaveGame = vi.fn(() => Promise.resolve('ok'));
const mockNewSaveFile = vi.fn(() => Promise.resolve());
const mockNewGame = vi.fn(() => Promise.resolve());

vi.mock('$lib/ipc', () => ({
	discoverSaveFiles: () => mockDiscoverSaveFiles(),
	getSaveState: () => mockGetSaveState(),
	loadBranch: (filePath: string, branchId: number) => mockLoadBranch(filePath, branchId),
	createBranch: (name: string, parentBranchId: number) => mockCreateBranch(name, parentBranchId),
	getWorldSnapshot: () => mockGetWorldSnapshot(),
	getMap: () => mockGetMap(),
	getNpcsHere: () => mockGetNpcsHere(),
	saveGame: () => mockSaveGame(),
	newSaveFile: () => mockNewSaveFile(),
	newGame: () => mockNewGame()
}));

// ── Helpers ─────────────────────────────────────────────────────────────────

function makeBranch(name: string, parent: string | null, id: number): SaveBranchDisplay {
	return {
		name,
		id,
		parent_name: parent,
		snapshot_count: 1,
		latest_location: 'Kilteevan',
		latest_game_date: 'March 1820',
		snapshots: []
	};
}

function makeFile(filename: string, branches: SaveBranchDisplay[]): SaveFileInfo {
	return {
		path: `/saves/${filename}`,
		filename,
		file_size: '12 KB',
		branches,
		locked: false
	};
}

const SAVE_STATE: SaveState = {
	filename: 'parish.db',
	branch_id: 1,
	branch_name: 'main'
};

/**
 * Mount the picker with pre-seeded IPC mocks so the refreshSaves() side-effect
 * (triggered by visibility) writes the right data into the stores.
 */
async function mountWithFile(file: SaveFileInfo, saveState: SaveState | null = SAVE_STATE) {
	mockDiscoverSaveFiles.mockResolvedValue([file]);
	mockGetSaveState.mockResolvedValue(saveState);

	savePickerVisible.set(false);
	saveFiles.set([]);
	currentSaveState.set(null);

	const result = render(SavePicker);

	// Trigger the visibility reactive block which calls refreshSaves() (async IPC)
	savePickerVisible.set(true);
	// Flush microtasks + Svelte ticks so async IPC resolves and stores update
	await flush();
	return result;
}

// ── Reset mocks between tests ────────────────────────────────────────────────

beforeEach(() => {
	vi.clearAllMocks();
	// Default: empty saves, null state
	mockDiscoverSaveFiles.mockResolvedValue([]);
	mockGetSaveState.mockResolvedValue(null);
	savePickerVisible.set(false);
	saveFiles.set([]);
	currentSaveState.set(null);
});

// ── Test cases ───────────────────────────────────────────────────────────────

describe('SavePicker', () => {
	it('renders nothing when savePickerVisible is false', () => {
		const { container } = render(SavePicker);
		expect(container.querySelector('[role="dialog"]')).toBeNull();
	});

	describe('empty branch list', () => {
		it('shows "No save file found" when picker is visible but no save files exist', async () => {
			savePickerVisible.set(true);
			const { getByText } = render(SavePicker);
			await tick();
			await tick();
			expect(getByText('No save file found.')).toBeTruthy();
		});

		it('renders the modal with correct aria label', async () => {
			savePickerVisible.set(true);
			const { container } = render(SavePicker);
			await tick();
			const dialog = container.querySelector('[role="dialog"]');
			expect(dialog).toBeTruthy();
			expect(dialog?.getAttribute('aria-label')).toBe('The Parish Ledger');
		});
	});

	describe('3-branch DAG rendering', () => {
		it('renders 3 dag-node elements for root + 2 children', async () => {
			const branches = [
				makeBranch('main', null, 1),
				makeBranch('left', 'main', 2),
				makeBranch('right', 'main', 3)
			];
			const { container } = await mountWithFile(makeFile('parish.db', branches));
			const nodes = container.querySelectorAll('.dag-node');
			expect(nodes).toHaveLength(3);
		});

		it('renders 2 edge paths in the SVG for a 2-edge DAG', async () => {
			const branches = [
				makeBranch('main', null, 1),
				makeBranch('left', 'main', 2),
				makeBranch('right', 'main', 3)
			];
			const { container } = await mountWithFile(makeFile('parish.db', branches));
			const edges = container.querySelectorAll('.dag-edges path');
			expect(edges).toHaveLength(2);
		});

		it('marks the current branch with dag-current class', async () => {
			const branches = [
				makeBranch('main', null, 1),
				makeBranch('left', 'main', 2),
				makeBranch('right', 'main', 3)
			];
			const { container } = await mountWithFile(makeFile('parish.db', branches));
			const currentNode = container.querySelector('.dag-current');
			expect(currentNode).toBeTruthy();
			expect(currentNode?.querySelector('.node-name')?.textContent).toBe('main');
		});

		it('renders branch names as node text', async () => {
			const branches = [
				makeBranch('main', null, 1),
				makeBranch('left', 'main', 2),
				makeBranch('right', 'main', 3)
			];
			const { getByText } = await mountWithFile(makeFile('parish.db', branches));
			expect(getByText('main')).toBeTruthy();
			expect(getByText('left')).toBeTruthy();
			expect(getByText('right')).toBeTruthy();
		});

		it('renders "You are here" badge on current branch node', async () => {
			const branches = [makeBranch('main', null, 1)];
			const { getByText } = await mountWithFile(makeFile('parish.db', branches));
			expect(getByText('You are here')).toBeTruthy();
		});
	});

	describe('load branch on click', () => {
		it('calls loadBranch IPC when a node body button is clicked', async () => {
			const branches = [makeBranch('main', null, 1)];
			const { container } = await mountWithFile(makeFile('parish.db', branches));

			const nodeBtn = container.querySelector('.node-body') as HTMLButtonElement;
			expect(nodeBtn).toBeTruthy();
			await fireEvent.click(nodeBtn);
			// Allow async IPC handler to start
			await tick();

			expect(mockLoadBranch).toHaveBeenCalledOnce();
			expect(mockLoadBranch).toHaveBeenCalledWith('/saves/parish.db', 1);
		});
	});

	describe('fork branch flow', () => {
		it('shows phantom node after clicking "Branch From Here"', async () => {
			const branches = [
				makeBranch('main', null, 1),
				makeBranch('dev', 'main', 2)
			];
			const { container } = await mountWithFile(makeFile('parish.db', branches));

			// Click "Branch From Here" on the 'dev' node
			const devNode = Array.from(container.querySelectorAll('.dag-node')).find(
				n => n.querySelector('.node-name')?.textContent === 'dev'
			);
			expect(devNode).toBeTruthy();
			const branchBtn = devNode!.querySelector('.node-branch-btn') as HTMLButtonElement;
			await fireEvent.click(branchBtn);
			await tick();

			// Phantom node should appear
			expect(container.querySelector('.dag-phantom')).toBeTruthy();
		});

		it('calls createBranch IPC with parent branch id when Create is clicked', async () => {
			const branches = [
				makeBranch('main', null, 1),
				makeBranch('dev', 'main', 2)
			];
			// createBranch triggers refreshSaves; provide stable mock data
			mockDiscoverSaveFiles.mockResolvedValue([makeFile('parish.db', branches)]);
			mockGetSaveState.mockResolvedValue(SAVE_STATE);
			const { container } = await mountWithFile(makeFile('parish.db', branches));

			// Open phantom node on 'dev'
			const devNode = Array.from(container.querySelectorAll('.dag-node')).find(
				n => n.querySelector('.node-name')?.textContent === 'dev'
			);
			await fireEvent.click(devNode!.querySelector('.node-branch-btn')!);
			await tick();

			// Click Create
			const phantom = container.querySelector('.dag-phantom')!;
			const createBtn = Array.from(phantom.querySelectorAll('.phantom-btn')).find(
				b => b.textContent?.trim() === 'Create'
			) as HTMLButtonElement;
			expect(createBtn).toBeTruthy();
			await fireEvent.click(createBtn);
			await tick();

			expect(mockCreateBranch).toHaveBeenCalledOnce();
			// Second argument must be the parent branch id (2 = 'dev')
			expect(mockCreateBranch.mock.calls[0][1]).toBe(2);
			// First argument must be a non-empty string name
			expect(typeof mockCreateBranch.mock.calls[0][0]).toBe('string');
			expect((mockCreateBranch.mock.calls[0][0] as string).length).toBeGreaterThan(0);
		});

		it('Cancel button dismisses the phantom node', async () => {
			const branches = [makeBranch('main', null, 1)];
			const { container } = await mountWithFile(makeFile('parish.db', branches));

			await fireEvent.click(container.querySelector('.node-branch-btn')!);
			await tick();
			expect(container.querySelector('.dag-phantom')).toBeTruthy();

			const cancelBtn = Array.from(
				container.querySelectorAll('.phantom-btn')
			).find(b => b.textContent?.trim() === 'Cancel') as HTMLButtonElement;
			await fireEvent.click(cancelBtn);
			await tick();

			expect(container.querySelector('.dag-phantom')).toBeNull();
		});
	});

	describe('close button', () => {
		it('hides modal when Close is clicked', async () => {
			savePickerVisible.set(true);
			const { container, getByText } = render(SavePicker);
			await tick();

			const closeBtn = getByText('Close') as HTMLButtonElement;
			await fireEvent.click(closeBtn);
			await tick();

			expect(container.querySelector('[role="dialog"]')).toBeNull();
		});
	});

	describe('Ledgers view', () => {
		it('switches to ledger list when Ledgers footer button is clicked', async () => {
			const file = makeFile('parish.db', [makeBranch('main', null, 1)]);
			const { container, getByText } = await mountWithFile(file);

			await fireEvent.click(getByText('Ledgers'));
			await tick();

			// Modal title changes to "Ledgers"
			expect(container.querySelector('.modal-title')?.textContent?.trim()).toBe('Ledgers');
		});

		it('shows Back button in ledger view', async () => {
			const file = makeFile('parish.db', [makeBranch('main', null, 1)]);
			const { container, getByText } = await mountWithFile(file);

			await fireEvent.click(getByText('Ledgers'));
			await tick();

			expect(getByText('← Back')).toBeTruthy();
		});
	});
});
