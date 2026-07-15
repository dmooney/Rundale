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
	it('captures the complete illustrated Pixi canvas directly', async () => {
		const host = document.createElement('div');
		host.dataset.testid = 'illustrated-notebook-pixi-host';
		const canvas = document.createElement('canvas');
		canvas.width = 1440;
		canvas.height = 900;
		const toDataURL = vi.fn(() => 'data:image/png;base64,PIXIFRAME');
		canvas.toDataURL = toDataURL;
		host.appendChild(canvas);
		document.body.appendChild(host);

		const { captureScreen } = await import('./screenshot');
		expect(await captureScreen()).toBe('data:image/png;base64,PIXIFRAME');
		expect(toDataURL).toHaveBeenCalledWith('image/png');
		expect(toPngMock).not.toHaveBeenCalled();
	});

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

	it('keeps the capture cheap: no cacheBust, pixelRatio capped at 2 (#1160)', async () => {
		// cacheBust forces a refetch+inline of every cross-origin map tile, the
		// dominant cost that pushed captures past the backend deadline. It must
		// stay off, and pixelRatio must be capped so HiDPI displays don't blow up
		// the pixel count.
		Object.defineProperty(window, 'devicePixelRatio', {
			value: 3,
			configurable: true,
		});
		const { captureScreen } = await import('./screenshot');
		await captureScreen();
		const opts = toPngMock.mock.calls[0]?.[1] as
			| Record<string, unknown>
			| undefined;
		expect(opts?.cacheBust).toBeUndefined();
		expect(typeof opts?.pixelRatio).toBe('number');
		expect(opts?.pixelRatio).toBeGreaterThan(0);
		expect(opts?.pixelRatio).toBeLessThanOrEqual(2);
	});
});
