import { beforeEach, describe, it, expect, vi } from 'vitest';
import { render, waitFor } from '@testing-library/svelte';
import SaveInspector from './SaveInspector.svelte';
import type {
	SaveFileSummary,
	BranchSummary,
	SnapshotDetail,
} from '$lib/editor-types';

// ── Mocks ─────────────────────────────────────────────────────────────────────

const mocks = vi.hoisted(() => ({
	editorListSaves: vi.fn<() => Promise<SaveFileSummary[]>>(),
	editorListBranches: vi.fn<(p: string) => Promise<BranchSummary[]>>(),
	editorReadSnapshot: vi.fn<() => Promise<SnapshotDetail | null>>(),
}));

vi.mock('$lib/editor-ipc', () => ({
	editorListSaves: mocks.editorListSaves,
	editorListBranches: mocks.editorListBranches,
	editorReadSnapshot: mocks.editorReadSnapshot,
}));

// ── Fixtures ──────────────────────────────────────────────────────────────────

const saveSummary: SaveFileSummary = {
	path: '/saves/rundale.sav',
	filename: 'rundale.sav',
	file_size: '42 KB',
	branch_count: 2,
};

const branch: BranchSummary = {
	id: 1,
	name: 'main',
	parent_branch_id: null,
	parent_branch_name: null,
	created_at: '2024-01-01T00:00:00Z',
	snapshot_count: 3,
};

// ── Tests ─────────────────────────────────────────────────────────────────────

describe('SaveInspector', () => {
	beforeEach(() => {
		mocks.editorListSaves.mockResolvedValue([]);
		mocks.editorListBranches.mockResolvedValue([]);
		mocks.editorReadSnapshot.mockResolvedValue(null);
	});

	it('shows "No save files found." when saves list is empty', async () => {
		mocks.editorListSaves.mockResolvedValue([]);
		const { getByText } = render(SaveInspector);
		await waitFor(() => {
			expect(getByText('No save files found.')).toBeTruthy();
		});
	});

	it('renders save file names and branch counts when saves are loaded', async () => {
		mocks.editorListSaves.mockResolvedValue([saveSummary]);
		const { getByText } = render(SaveInspector);
		await waitFor(() => {
			expect(getByText('rundale.sav')).toBeTruthy();
		});
		expect(getByText('42 KB · 2 branches')).toBeTruthy();
	});

	it('shows "Select a save file." prompt when no save is selected', async () => {
		mocks.editorListSaves.mockResolvedValue([saveSummary]);
		const { getByText } = render(SaveInspector);
		await waitFor(() => {
			expect(getByText('rundale.sav')).toBeTruthy();
		});
		expect(getByText('Select a save file.')).toBeTruthy();
	});

	it('shows "Select a branch." prompt in the snapshot column initially', async () => {
		mocks.editorListSaves.mockResolvedValue([saveSummary]);
		mocks.editorListBranches.mockResolvedValue([branch]);
		const { getByText } = render(SaveInspector);
		await waitFor(() => {
			expect(getByText('rundale.sav')).toBeTruthy();
		});
		expect(getByText('Select a branch.')).toBeTruthy();
	});

	it('renders a Refresh button', async () => {
		const { getByText } = render(SaveInspector);
		await waitFor(() => {
			expect(getByText('Refresh')).toBeTruthy();
		});
	});

	it('shows save counts header', async () => {
		mocks.editorListSaves.mockResolvedValue([saveSummary]);
		const { getByText } = render(SaveInspector);
		await waitFor(() => {
			expect(getByText('Save Files (1)')).toBeTruthy();
		});
	});

	it('shows error message when editorListSaves rejects', async () => {
		mocks.editorListSaves.mockRejectedValue(new Error('disk error'));
		const { getByText } = render(SaveInspector);
		await waitFor(() => {
			expect(getByText(/disk error/)).toBeTruthy();
		});
	});
});
