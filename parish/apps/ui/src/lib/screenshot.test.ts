import { describe, it, expect, vi, beforeEach } from 'vitest';

// `html-to-image` is mocked so we can exercise `captureScreen`'s target
// selection and option forwarding without running the (jsdom-incompatible)
// real renderer.
const toPngMock = vi.fn(
	async (_node: HTMLElement, _opts?: Record<string, unknown>) =>
		'data:image/png;base64,FAKE',
);

vi.mock('html-to-image', () => ({
	toPng: toPngMock,
}));

beforeEach(() => {
	toPngMock.mockClear();
	document.body.innerHTML = '';
});

describe('captureScreen()', () => {
	it('targets .app-shell when present', async () => {
		const shell = document.createElement('div');
		shell.className = 'app-shell';
		document.body.appendChild(shell);

		const { captureScreen } = await import('./screenshot');
		const url = await captureScreen();
		expect(url).toBe('data:image/png;base64,FAKE');
		expect(toPngMock).toHaveBeenCalledTimes(1);
		expect(toPngMock.mock.calls[0]?.[0]).toBe(shell);
	});

	it('falls back to document.body when .app-shell is absent', async () => {
		const { captureScreen } = await import('./screenshot');
		await captureScreen();
		expect(toPngMock.mock.calls[0]?.[0]).toBe(document.body);
	});

	it('forwards cacheBust and a sensible pixelRatio to html-to-image', async () => {
		const { captureScreen } = await import('./screenshot');
		await captureScreen();
		const opts = toPngMock.mock.calls[0]?.[1] as
			| Record<string, unknown>
			| undefined;
		expect(opts?.cacheBust).toBe(true);
		expect(typeof opts?.pixelRatio).toBe('number');
		expect(opts?.pixelRatio).toBeGreaterThan(0);
	});
});
