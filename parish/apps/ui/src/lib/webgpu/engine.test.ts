/**
 * Unit tests for the pure model-choice resolver in `engine.ts`.
 *
 * Locks in the precedence (localStorage > server-passed-if-HF > auto-detect)
 * so a regression that lets the server's default `model_name` (typically
 * `qwen3:14b`, an Ollama tag) pin the WebGPU model can't slip back in.
 */

import { describe, expect, it } from 'vitest';
import { isLikelyHfRepoId, resolveModelChoice } from './engine';

describe('isLikelyHfRepoId', () => {
	it('accepts org/name shapes', () => {
		expect(isLikelyHfRepoId('onnx-community/gemma-4-E2B-it-ONNX')).toBe(true);
		expect(isLikelyHfRepoId('Xenova/llama2.c-stories15M')).toBe(true);
	});

	it('rejects ollama-style tags with a colon', () => {
		expect(isLikelyHfRepoId('qwen3:14b')).toBe(false);
		expect(isLikelyHfRepoId('llama3:8b-instruct-q4_0')).toBe(false);
	});

	it('rejects empty / null / single-segment ids', () => {
		expect(isLikelyHfRepoId(null)).toBe(false);
		expect(isLikelyHfRepoId(undefined)).toBe(false);
		expect(isLikelyHfRepoId('')).toBe(false);
		expect(isLikelyHfRepoId('  ')).toBe(false);
		expect(isLikelyHfRepoId('gemma')).toBe(false);
	});

	it('rejects ids with whitespace or extra slashes', () => {
		expect(isLikelyHfRepoId('a / b')).toBe(false);
		expect(isLikelyHfRepoId('a/b/c')).toBe(false);
	});

	it('trims surrounding whitespace before testing the shape', () => {
		expect(isLikelyHfRepoId('  org/name  ')).toBe(true);
	});
});

describe('resolveModelChoice', () => {
	it('returns the localStorage override even when a server id is present', () => {
		expect(resolveModelChoice('onnx/server-pick', 'onnx/user-pick')).toEqual({
			kind: 'fixed',
			id: 'onnx/user-pick'
		});
	});

	it('uses the server-passed id when no override is set and it looks HF-shaped', () => {
		expect(resolveModelChoice('onnx-community/gemma-4-E2B-it-ONNX', null)).toEqual({
			kind: 'fixed',
			id: 'onnx-community/gemma-4-E2B-it-ONNX'
		});
	});

	it('falls back to auto-detect when the server id is the Ollama default', () => {
		// Regression guard: the bridge must never try to load `qwen3:14b`
		// (the GameConfig default for non-WebGPU providers) through
		// transformers.js — that would 404 before the loading overlay
		// could surface anything useful.
		expect(resolveModelChoice('qwen3:14b', null)).toEqual({ kind: 'detect' });
	});

	it('falls back to auto-detect when nothing is provided', () => {
		expect(resolveModelChoice(null, null)).toEqual({ kind: 'detect' });
		expect(resolveModelChoice('', null)).toEqual({ kind: 'detect' });
	});

	it('lets the override win over an unrelated server id', () => {
		// Even if the server passes something non-HF-shaped, localStorage
		// still takes precedence — the user has opted in.
		expect(resolveModelChoice('qwen3:14b', 'onnx-community/gemma-4-E2B-it-ONNX')).toEqual({
			kind: 'fixed',
			id: 'onnx-community/gemma-4-E2B-it-ONNX'
		});
	});
});
