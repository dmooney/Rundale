// Flat ESLint config — Svelte 5 + TypeScript.
// ESLint owns correctness (unused vars, svelte a11y, runes misuse);
// Prettier owns formatting (eslint-config-prettier disables stylistic rules).
// Starts with syntactic (non-type-checked) rules; can ratchet to
// typescript-eslint `recommendedTypeChecked` + projectService later.
import js from '@eslint/js';
import ts from 'typescript-eslint';
import svelte from 'eslint-plugin-svelte';
import prettier from 'eslint-config-prettier';
import globals from 'globals';

export default ts.config(
	{
		ignores: [
			'.svelte-kit/',
			'dist/',
			'build/',
			'node_modules/',
			'e2e/screenshots/',
		],
	},
	js.configs.recommended,
	...ts.configs.recommended,
	...svelte.configs.recommended,
	prettier,
	...svelte.configs.prettier,
	{
		languageOptions: {
			globals: { ...globals.browser, ...globals.node },
		},
		rules: {
			// Intentionally-unused bindings are marked with a leading underscore
			// (caught errors, ignored callback args, placeholder destructures).
			// Genuinely dead code is NOT exempt — it must be deleted.
			'@typescript-eslint/no-unused-vars': [
				'error',
				{
					argsIgnorePattern: '^_',
					varsIgnorePattern: '^_',
					caughtErrorsIgnorePattern: '^_',
					destructuredArrayIgnorePattern: '^_',
				},
			],
		},
	},
	{
		// TypeScript inside <script lang="ts"> blocks.
		files: ['**/*.svelte', '**/*.svelte.ts', '**/*.svelte.js'],
		languageOptions: {
			parserOptions: { parser: ts.parser },
		},
	},
	{
		// Test + e2e code deliberately feeds malformed/`any` input to exercise
		// runtime guards, and runs under vitest/node globals.
		files: ['**/*.test.ts', 'e2e/**', 'src/test-setup.ts', 'src/__mocks__/**'],
		languageOptions: {
			globals: { ...globals.node },
		},
		rules: {
			'@typescript-eslint/no-explicit-any': 'off',
		},
	},
);
