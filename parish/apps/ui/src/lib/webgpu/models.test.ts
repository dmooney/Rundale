/**
 * Unit tests for the GPU-tier auto-detection logic in `models.ts`.
 *
 * The tier table is intentionally exercised via `detectGpuTier` (rather than
 * by re-implementing the rules inline) so a future addition of, say, a third
 * model entry only changes the table — never the tests.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
	detectGpuTier,
	findModel,
	WEBGPU_MODELS,
	FALLBACK_MODEL
} from './models';

interface FakeAdapter {
	limits: { maxStorageBufferBindingSize: number };
}

function installFakeNavigator(adapter: FakeAdapter | null, deviceMemory?: number): void {
	const fake = {
		gpu: adapter
			? {
					requestAdapter: vi.fn().mockResolvedValue(adapter)
				}
			: undefined,
		deviceMemory
	};
	Object.defineProperty(globalThis, 'navigator', {
		value: fake,
		configurable: true,
		writable: true
	});
}

describe('WEBGPU_MODELS table invariants', () => {
	it('orders models from largest to smallest VRAM requirement', () => {
		for (let i = 1; i < WEBGPU_MODELS.length; i++) {
			expect(WEBGPU_MODELS[i].minMaxStorageBufferBindingSize).toBeLessThanOrEqual(
				WEBGPU_MODELS[i - 1].minMaxStorageBufferBindingSize
			);
		}
	});

	it('treats the smallest tier as the fallback', () => {
		expect(FALLBACK_MODEL).toBe(WEBGPU_MODELS[WEBGPU_MODELS.length - 1]);
	});

	it('finds known model ids and returns null for unknowns', () => {
		expect(findModel(WEBGPU_MODELS[0].id)?.id).toBe(WEBGPU_MODELS[0].id);
		expect(findModel('not-a-real/repo')).toBeNull();
	});
});

describe('detectGpuTier', () => {
	const originalNavigator = globalThis.navigator;

	afterEach(() => {
		Object.defineProperty(globalThis, 'navigator', {
			value: originalNavigator,
			configurable: true,
			writable: true
		});
	});

	it('warns and returns the fallback when WebGPU is unsupported', async () => {
		installFakeNavigator(null);
		const result = await detectGpuTier();
		expect(result.model).toBe(FALLBACK_MODEL);
		expect(result.warning).toMatch(/WebGPU/);
	});

	it('picks the largest model when both VRAM and RAM clear the bar', async () => {
		const big = WEBGPU_MODELS[0];
		installFakeNavigator(
			{
				limits: { maxStorageBufferBindingSize: big.minMaxStorageBufferBindingSize + 1 }
			},
			Math.max(big.minDeviceMemoryGb, 16)
		);
		const result = await detectGpuTier();
		expect(result.model.id).toBe(big.id);
		expect(result.warning).toBeNull();
	});

	it('falls to a smaller model when RAM is below the largest tier', async () => {
		const big = WEBGPU_MODELS[0];
		installFakeNavigator(
			{
				limits: { maxStorageBufferBindingSize: big.minMaxStorageBufferBindingSize + 1 }
			},
			0
		);
		const result = await detectGpuTier();
		expect(result.model.id).not.toBe(big.id);
	});

	it('warns when the GPU is below every tier minimum', async () => {
		installFakeNavigator(
			{ limits: { maxStorageBufferBindingSize: 100 } },
			0
		);
		const result = await detectGpuTier();
		expect(result.model).toBe(FALLBACK_MODEL);
		expect(result.warning).toMatch(/below the recommended/);
	});

	it('handles requestAdapter rejection gracefully', async () => {
		Object.defineProperty(globalThis, 'navigator', {
			value: {
				gpu: {
					requestAdapter: vi.fn().mockRejectedValue(new Error('nope'))
				}
			},
			configurable: true,
			writable: true
		});
		const result = await detectGpuTier();
		expect(result.model).toBe(FALLBACK_MODEL);
		expect(result.warning).not.toBeNull();
	});
});
