import { test } from '@playwright/test';

throw new Error(
	'Intentional #1714 fail-closed proof: Playwright discovery must block CI gate',
);

test('intentional discovery failure marker', () => {
	// Unreachable by design in the failing proof state.
});
