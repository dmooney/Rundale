import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import SetupOverlay from './SetupOverlay.svelte';
import { LONG_WAIT_MESSAGES } from '$lib/setupWaitMessages';

const mockIpc = vi.hoisted(() => {
	type StatusCb = (payload: { message: string }) => void;
	type ProgressCb = (payload: { completed: number; total: number }) => void;
	type DoneCb = (payload: { success: boolean; error: string }) => void;
	type Snapshot = {
		current_message: string;
		messages: string[];
		completed: number;
		total: number;
		done: boolean;
		success: boolean | null;
		error: string;
	};

	const defaultSnapshot = (): Snapshot => ({
		current_message: 'Preparing the storyteller...',
		messages: ['Preparing the storyteller...'],
		completed: 0,
		total: 0,
		done: false,
		success: null,
		error: ''
	});

	const callbacks: {
		status?: StatusCb;
		progress?: ProgressCb;
		done?: DoneCb;
	} = {};

	return {
		callbacks,
		isTauri: vi.fn(() => true),
		onSetupStatus: vi.fn(async (cb: StatusCb) => {
			callbacks.status = cb;
			return vi.fn();
		}),
		onSetupProgress: vi.fn(async (cb: ProgressCb) => {
			callbacks.progress = cb;
			return vi.fn();
		}),
		onSetupDone: vi.fn(async (cb: DoneCb) => {
			callbacks.done = cb;
			return vi.fn();
		}),
		getSetupSnapshot: vi.fn(async (): Promise<Snapshot> => defaultSnapshot())
	};
});

vi.mock('$lib/ipc', () => ({
	getSetupSnapshot: mockIpc.getSetupSnapshot,
	isTauri: mockIpc.isTauri,
	onSetupStatus: mockIpc.onSetupStatus,
	onSetupProgress: mockIpc.onSetupProgress,
	onSetupDone: mockIpc.onSetupDone
}));

describe('SetupOverlay', () => {
	beforeEach(() => {
		vi.useRealTimers();
		sessionStorage.clear();
		mockIpc.isTauri.mockReturnValue(true);
		mockIpc.onSetupStatus.mockClear();
		mockIpc.onSetupProgress.mockClear();
		mockIpc.onSetupDone.mockClear();
		mockIpc.getSetupSnapshot.mockClear();
		mockIpc.getSetupSnapshot.mockResolvedValue({
			current_message: 'Preparing the storyteller...',
			messages: ['Preparing the storyteller...'],
			completed: 0,
			total: 0,
			done: false,
			success: null,
			error: ''
		});
		mockIpc.callbacks.status = undefined;
		mockIpc.callbacks.progress = undefined;
		mockIpc.callbacks.done = undefined;
	});

	it('has a deep pool of still-loading messages', () => {
		expect(LONG_WAIT_MESSAGES[0]).toBe('Reticulating spleens...');
		expect(LONG_WAIT_MESSAGES.length).toBeGreaterThanOrEqual(500);
		expect(LONG_WAIT_MESSAGES.length).toBeLessThan(750);
		expect(new Set(LONG_WAIT_MESSAGES).size).toBe(LONG_WAIT_MESSAGES.length);
		expect(LONG_WAIT_MESSAGES.some((message) => message.includes('one-time'))).toBe(true);
	});

	it('shows an initial setup message before backend status events arrive', async () => {
		const { container, getByRole } = render(SetupOverlay);

		await waitFor(() =>
			expect(getByRole('heading', { name: 'Rundale' })).toHaveClass('game-title')
		);
		expect(container.querySelector('.current-phrase')).toHaveTextContent(
			'Preparing the storyteller...'
		);
	});

	it('shows indeterminate progress before backend percentage events arrive', async () => {
		const { getByRole, queryByText } = render(SetupOverlay);

		const progress = await waitFor(() => getByRole('progressbar', { name: 'Setup progress' }));
		expect(progress).toHaveClass('indeterminate');
		expect(progress).not.toHaveAttribute('aria-valuenow');
		expect(queryByText('0.0%')).toBeNull();
	});

	it('uses inherited color for the setup spinner strokes', async () => {
		const { container } = render(SetupOverlay);

		await waitFor(() => expect(container.querySelector('.triquetra-spinner')).toBeTruthy());
		const spinner = container.querySelector('.triquetra-spinner');
		expect(spinner?.querySelector('.knot-circle')).toHaveAttribute('stroke', 'currentColor');
		expect(spinner?.querySelector('.triquetra-path')).toHaveAttribute('stroke', 'currentColor');
	});

	it('does not show the overlay when the setup snapshot is already complete', async () => {
		mockIpc.getSetupSnapshot.mockResolvedValueOnce({
			current_message: 'The storyteller is ready.',
			messages: ['The storyteller is ready.'],
			completed: 100,
			total: 100,
			done: true,
			success: true,
			error: ''
		});

		const { container, queryByRole } = render(SetupOverlay);

		expect(queryByRole('heading', { name: 'Rundale' })).toBeNull();
		await waitFor(() => expect(mockIpc.getSetupSnapshot).toHaveBeenCalled());
		await tick();
		expect(queryByRole('heading', { name: 'Rundale' })).toBeNull();
		expect(container.querySelector('.setup-overlay')).toBeNull();
	});

	it('preserves activity history across remounts when a stale snapshot only has the fallback message', async () => {
		const first = render(SetupOverlay);

		await waitFor(() => expect(mockIpc.callbacks.status).toBeDefined());
		mockIpc.callbacks.status?.({ message: 'Starting inference provider setup...' });
		mockIpc.callbacks.status?.({ message: "  pulling manifest" });
		await tick();
		expect(first.getByText(/starting inference provider setup/)).toBeTruthy();
		expect(first.getAllByText(/pulling manifest/).length).toBeGreaterThan(0);

		first.unmount();
		mockIpc.getSetupSnapshot.mockResolvedValueOnce({
			current_message: 'Preparing the storyteller...',
			messages: ['Preparing the storyteller...'],
			completed: 0,
			total: 0,
			done: false,
			success: null,
			error: ''
		});

		const second = render(SetupOverlay);

		await waitFor(() =>
			expect(second.getByText(/starting inference provider setup/)).toBeTruthy()
		);
		expect(second.getAllByText(/pulling manifest/).length).toBeGreaterThan(0);
	});

	it('adds whimsical wait messages while manifest fetching is quiet', async () => {
		vi.useFakeTimers();
		const { getAllByText } = render(SetupOverlay);

		try {
			await Promise.resolve();
			await Promise.resolve();
			await tick();
			expect(mockIpc.callbacks.status).toBeDefined();

			mockIpc.callbacks.status?.({ message: "  pulling manifest" });
			await tick();
			expect(getAllByText(/Ollama: pulling manifest/).length).toBeGreaterThan(0);

			await vi.advanceTimersByTimeAsync(2_500);
			await tick();

			expect(getAllByText('Reticulating spleens...').length).toBeGreaterThan(0);
		} finally {
			vi.useRealTimers();
		}
	});

	it('starts whimsical wait messages from a manifest snapshot', async () => {
		vi.useFakeTimers();
		mockIpc.getSetupSnapshot.mockResolvedValueOnce({
			current_message: '  pulling manifest',
			messages: [
				"Fetching the storyteller's book of tales ('qwen3:32b')...",
				'  pulling manifest'
			],
			completed: 0,
			total: 0,
			done: false,
			success: null,
			error: ''
		});
		const { getAllByText } = render(SetupOverlay);

		try {
			await Promise.resolve();
			await Promise.resolve();
			await tick();
			await vi.advanceTimersByTimeAsync(2_500);
			await tick();

			expect(getAllByText('Reticulating spleens...').length).toBeGreaterThan(0);
		} finally {
			vi.useRealTimers();
		}
	});

	it('updates the visible setup message from setup-status events', async () => {
		const { container } = render(SetupOverlay);

		await waitFor(() => expect(mockIpc.callbacks.status).toBeDefined());
		mockIpc.callbacks.status?.({ message: 'Taking stock of what we have to work with...' });
		await tick();

		expect(container.querySelector('.current-phrase')).toHaveTextContent(
			'Taking stock of what we have to work with...'
		);
	});

	it('keeps the one-time download note visible when fetching a model', async () => {
		const { container, getAllByText } = render(SetupOverlay);

		await waitFor(() => expect(mockIpc.callbacks.status).toBeDefined());
		mockIpc.callbacks.status?.({
			message: "Fetching the storyteller's book of tales ('qwen3:32b')..."
		});
		await tick();

		expect(getAllByText(/one-time model download/).length).toBeGreaterThan(0);
		expect(getAllByText(/qwen3:32b/).length).toBeGreaterThan(0);
		expect(container.querySelectorAll('.current-phrase wbr').length).toBeGreaterThan(0);
		expect(container.querySelectorAll('.msg wbr').length).toBeGreaterThan(0);
	});

	it('hydrates missed setup activity from the backend snapshot', async () => {
		mockIpc.getSetupSnapshot.mockResolvedValueOnce({
			current_message: 'Hardware: Apple Silicon',
			messages: [
				'Starting inference provider setup...',
				'Taking stock of what we have to work with...',
				'Hardware: Apple Silicon'
			],
			completed: 0,
			total: 0,
			done: false,
			success: null,
			error: ''
		});

		const { container, getByText } = render(SetupOverlay);

		await waitFor(() =>
			expect(container.querySelector('.current-phrase')).toHaveTextContent(
				'Hardware: Apple Silicon'
			)
		);
		expect(getByText(/starting inference provider setup/)).toBeTruthy();
		expect(getByText('Taking stock of what we have to work with...')).toBeTruthy();
	});

	it('switches to determinate progress when backend progress arrives', async () => {
		const { getByLabelText, getByRole } = render(SetupOverlay);

		await waitFor(() => expect(mockIpc.callbacks.progress).toBeDefined());
		mockIpc.callbacks.progress?.({ completed: 50, total: 100 });
		await tick();

		const progress = getByRole('progressbar', { name: 'Setup progress' });
		const fill = progress.querySelector('.progress-fill');
		expect(progress).not.toHaveClass('indeterminate');
		expect(progress).toHaveAttribute('aria-valuenow', '50');
		expect(fill).toHaveStyle({ width: '50%' });
		expect(getByLabelText('50.0%')).toHaveClass('progress-label');
		expect(getByLabelText('50 B of 100 B')).toHaveClass('download-stats');
	});

	it('shows live download speed and ETA after multiple progress samples', async () => {
		const nowSpy = vi.spyOn(performance, 'now');
		const { getByLabelText } = render(SetupOverlay);

		try {
			await waitFor(() => expect(mockIpc.callbacks.progress).toBeDefined());
			nowSpy.mockReturnValueOnce(1_000);
			mockIpc.callbacks.progress?.({ completed: 25, total: 100 });
			await tick();
			nowSpy.mockReturnValueOnce(2_000);
			mockIpc.callbacks.progress?.({ completed: 50, total: 100 });
			await tick();

			expect(getByLabelText('50 B of 100 B • 25 B/s • 0:02 left')).toHaveClass(
				'download-stats'
			);
		} finally {
			nowSpy.mockRestore();
		}
	});

	it('damps download speed changes across the moving average window', async () => {
		const nowSpy = vi.spyOn(performance, 'now');
		const { getByLabelText } = render(SetupOverlay);

		try {
			await waitFor(() => expect(mockIpc.callbacks.progress).toBeDefined());
			nowSpy.mockReturnValueOnce(1_000);
			mockIpc.callbacks.progress?.({ completed: 0, total: 2000 });
			await tick();
			nowSpy.mockReturnValueOnce(2_000);
			mockIpc.callbacks.progress?.({ completed: 100, total: 2000 });
			await tick();
			expect(getByLabelText('100 B of 1.95 KB • 100 B/s • 0:19 left')).toHaveClass(
				'download-stats'
			);

			nowSpy.mockReturnValueOnce(3_000);
			mockIpc.callbacks.progress?.({ completed: 1000, total: 2000 });
			await tick();

			expect(getByLabelText('1000 B of 1.95 KB • 160 B/s • 0:06 left')).toHaveClass(
				'download-stats'
			);
		} finally {
			nowSpy.mockRestore();
		}
	});

	it('holds download speed steady between display-rate updates', async () => {
		const nowSpy = vi.spyOn(performance, 'now');
		const { getByLabelText } = render(SetupOverlay);

		try {
			await waitFor(() => expect(mockIpc.callbacks.progress).toBeDefined());
			nowSpy.mockReturnValueOnce(1_000);
			mockIpc.callbacks.progress?.({ completed: 0, total: 1000 });
			await tick();
			nowSpy.mockReturnValueOnce(2_000);
			mockIpc.callbacks.progress?.({ completed: 100, total: 1000 });
			await tick();
			expect(getByLabelText('100 B of 1000 B • 100 B/s • 0:09 left')).toHaveClass(
				'download-stats'
			);

			nowSpy.mockReturnValueOnce(2_200);
			mockIpc.callbacks.progress?.({ completed: 900, total: 1000 });
			await tick();

			expect(getByLabelText('900 B of 1000 B • 100 B/s • 0:01 left')).toHaveClass(
				'download-stats'
			);
		} finally {
			nowSpy.mockRestore();
		}
	});

	it('keeps download speed when aggregate model total grows', async () => {
		const nowSpy = vi.spyOn(performance, 'now');
		const { getByLabelText } = render(SetupOverlay);

		try {
			await waitFor(() => expect(mockIpc.callbacks.progress).toBeDefined());
			nowSpy.mockReturnValueOnce(1_000);
			mockIpc.callbacks.progress?.({ completed: 0, total: 1000 });
			await tick();
			nowSpy.mockReturnValueOnce(2_000);
			mockIpc.callbacks.progress?.({ completed: 500, total: 1000 });
			await tick();
			expect(getByLabelText('500 B of 1000 B • 500 B/s • 0:01 left')).toHaveClass(
				'download-stats'
			);

			nowSpy.mockReturnValueOnce(2_200);
			mockIpc.callbacks.progress?.({ completed: 1000, total: 1488 });
			await tick();

			expect(getByLabelText('1000 B of 1.45 KB • 500 B/s • 0:01 left')).toHaveClass(
				'download-stats'
			);
		} finally {
			nowSpy.mockRestore();
		}
	});

	it('registers all setup listeners on mount', async () => {
		render(SetupOverlay);

		await waitFor(() => {
			expect(mockIpc.onSetupStatus).toHaveBeenCalledTimes(1);
			expect(mockIpc.onSetupProgress).toHaveBeenCalledTimes(1);
			expect(mockIpc.onSetupDone).toHaveBeenCalledTimes(1);
		});
	});
});
